use crate::file_safety::{
    canonical_pdf_input, paths_are_equal, reject_control_characters, TemporaryOutput,
    ValidatedPdfPaths,
};
use crate::health::ensure_document_rewrite_acknowledged;
use crate::job_control::PdfJobExecutionControl;
#[cfg(test)]
use crate::protection::lock_pdf_changes;
use crate::protection::{lock_pdf_changes_with_control, validate_password};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_EXPORT_PAGES: usize = 50_000;
const MIN_PAGE_POINTS: f64 = 18.0;
const MAX_PAGE_POINTS: f64 = 14_400.0;
const MAX_SIGNATURE_DATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_SIGNATURE_TOTAL_DATA_BYTES: usize = 64 * 1024 * 1024;
const MAX_SIGNATURE_DIMENSION: u32 = 8_192;
const MAX_SIGNATURE_ALLOCATION: u64 = 128 * 1024 * 1024;
const MAX_EMBEDDED_SIGNATURE_DIMENSION: u32 = 2_048;
const MAX_IMPORTED_SOURCES: usize = 249;
const MAX_VISUAL_SIGNATURE_ASSETS: usize = 32;
const MAX_VISUAL_SIGNATURE_PLACEMENTS: usize = 128;
const MAX_SOURCE_PASSWORD_BYTES: usize = 1_024;
const PRIMARY_SOURCE_ID: &str = "primary";

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    #[serde(default)]
    acknowledge_certificate_signature: bool,
    pages: Vec<ExportPage>,
    signature: Option<SignaturePlacement>,
    document_lock: Option<DocumentLock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportComposedPdfRequest {
    primary_input_path: String,
    primary_input_password: Option<String>,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    acknowledge_primary_certificate_signature: bool,
    imported_sources: Vec<ExportPdfSource>,
    output_path: String,
    pages: Vec<ComposedExportPage>,
    #[serde(default)]
    signature: Option<SignaturePlacement>,
    #[serde(default)]
    visual_signature_assets: Vec<VisualSignatureAsset>,
    #[serde(default)]
    visual_signature_placements: Vec<VisualSignaturePlacement>,
    document_lock: Option<DocumentLock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfSource {
    id: String,
    input_path: String,
    input_password: Option<String>,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    acknowledge_certificate_signature: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComposedExportPage {
    Source {
        source_id: String,
        source_page: u32,
        rotation: i64,
    },
    Blank {
        width_pt: f64,
        height_pt: f64,
        rotation: i64,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignaturePlacement {
    page_number: usize,
    position: SignaturePosition,
    png_data_url: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VisualSignatureAsset {
    id: String,
    png_data_url: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VisualSignaturePlacement {
    id: String,
    asset_id: String,
    page_number: usize,
    left_ratio: f64,
    top_ratio: f64,
    width_ratio: f64,
    rotation_degrees: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignaturePosition {
    Left,
    Centre,
    Right,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLock {
    open_password: String,
    owner_password: String,
}

#[cfg(test)]
#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportPage {
    Source {
        source_page: u32,
        rotation: i64,
    },
    Blank {
        width_pt: f64,
        height_pt: f64,
        rotation: i64,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfResult {
    output_path: String,
    bytes_written: u64,
    page_count: usize,
    warnings: Vec<String>,
}

#[cfg(test)]
pub fn export_pdf(request: ExportPdfRequest) -> Result<ExportPdfResult, String> {
    validate_page_plan(&request.pages)?;
    validate_signature_request(request.signature.as_ref(), request.pages.len())?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    let organised_temporary = TemporaryOutput::new(&paths.output)?;
    let locked_temporary = request
        .document_lock
        .as_ref()
        .map(|_| TemporaryOutput::new(&paths.output))
        .transpose()?;
    let mut document = Document::load(&paths.input)
        .map_err(|error| format!("The source PDF could not be parsed: {error}"))?;
    let source_was_encrypted = document.is_encrypted() || document.was_encrypted();

    if document.is_encrypted() {
        document
            .decrypt(request.input_password.as_deref().unwrap_or_default())
            .map_err(|_| {
                "The source PDF could not be decrypted. Check its password and try again."
                    .to_string()
            })?;
    }
    ensure_document_rewrite_acknowledged(
        &document,
        &paths.input,
        request.acknowledge_certificate_signature,
    )?;

    let warnings = export_warnings(
        &document,
        &request.pages,
        source_was_encrypted,
        request.signature.is_some(),
        request.document_lock.is_some(),
    );
    let output_page_ids = apply_page_plan(&mut document, &request.pages)?;
    if let Some(signature) = request.signature.as_ref() {
        flatten_signature(
            &mut document,
            output_page_ids[signature.page_number - 1],
            signature,
        )?;
    }
    document.change_producer("Tüfekci Paperworks");

    let output_file = document
        .save(organised_temporary.path())
        .map_err(|error| format!("The organised PDF could not be written: {error}"))?;
    output_file
        .sync_all()
        .map_err(|error| format!("The organised PDF could not be flushed to storage: {error}"))?;

    let expected_signature_counts = if request.signature.is_some() {
        flattened_signature_counts(&document, &output_page_ids)?
    } else {
        BTreeMap::new()
    };
    verify_export(
        organised_temporary.path(),
        request.pages.len(),
        None,
        &expected_signature_counts,
    )?;

    let published_temporary = if let (Some(lock), Some(locked_temporary)) =
        (request.document_lock.as_ref(), locked_temporary.as_ref())
    {
        lock_pdf_changes(
            organised_temporary.path(),
            locked_temporary.path(),
            &lock.open_password,
            &lock.owner_password,
        )?;
        verify_export(
            locked_temporary.path(),
            request.pages.len(),
            Some(&lock.open_password),
            &expected_signature_counts,
        )?;
        locked_temporary
    } else {
        &organised_temporary
    };
    let bytes_written = published_temporary.persist(&paths.output)?;

    Ok(ExportPdfResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        bytes_written,
        page_count: request.pages.len(),
        warnings,
    })
}

#[cfg(test)]
pub fn export_composed_pdf(request: ExportComposedPdfRequest) -> Result<ExportPdfResult, String> {
    export_composed_pdf_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn export_composed_pdf_with_control(
    request: ExportComposedPdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfResult, String> {
    control.checkpoint(1, "Validating organised export request")?;
    validate_composed_pdf_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.primary_input_path, &request.output_path)?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
        "The primary PDF",
    )?;
    let mut verified_sources = vec![VerifiedExportSource {
        expected_modified_at_ms: request.expected_source_modified_at_ms,
        expected_size: request.expected_source_size,
        label: "The primary PDF".to_string(),
        path: paths.input.clone(),
    }];
    let organised_temporary = TemporaryOutput::new(&paths.output)?;
    let locked_temporary = request
        .document_lock
        .as_ref()
        .map(|_| TemporaryOutput::new(&paths.output))
        .transpose()?;

    control.checkpoint(6, "Opening primary PDF")?;
    let (mut document, source_was_encrypted) = load_export_source(
        &paths.input,
        request.primary_input_password.as_deref(),
        "The primary PDF",
    )?;
    ensure_document_rewrite_acknowledged(
        &document,
        &paths.input,
        request.acknowledge_primary_certificate_signature,
    )?;

    let primary_page_count = document.get_pages().len();
    let mut warnings = composed_export_warnings(
        &document,
        &request.pages,
        source_was_encrypted,
        request.signature.is_some() || !request.visual_signature_placements.is_empty(),
        request.document_lock.is_some(),
        primary_page_count,
    );
    let mut source_pages = HashMap::new();
    let primary_pages = document.get_pages();
    if primary_pages.is_empty() {
        return Err("The primary PDF does not contain any readable pages.".to_string());
    }
    let primary_snapshot_control = control.subrange(10, 19, "Primary PDF".to_string());
    source_pages.insert(
        PRIMARY_SOURCE_ID.to_string(),
        snapshot_source_pages_with_control(&document, &primary_pages, &primary_snapshot_control)?,
    );

    for (source_index, source) in request.imported_sources.iter().enumerate() {
        let source_start = progress_between(20, 48, source_index, request.imported_sources.len());
        let source_end = progress_between(20, 48, source_index + 1, request.imported_sources.len());
        control.checkpoint(
            source_start,
            format!(
                "Opening imported source {} of {}",
                source_index + 1,
                request.imported_sources.len()
            ),
        )?;
        let input = canonical_pdf_input(&source.input_path)?;
        if paths_are_equal(&input, &paths.output) {
            return Err(
                "An imported source PDF cannot be overwritten. Choose a new filename.".to_string(),
            );
        }
        let label = format!("Imported source {}", display_path_name(&input));
        verify_source_fingerprint(
            &input,
            source.expected_source_size,
            source.expected_source_modified_at_ms,
            &label,
        )?;
        verified_sources.push(VerifiedExportSource {
            expected_modified_at_ms: source.expected_source_modified_at_ms,
            expected_size: source.expected_source_size,
            label: label.clone(),
            path: input.clone(),
        });
        let (mut imported, imported_was_encrypted) =
            load_export_source(&input, source.input_password.as_deref(), &label)?;
        ensure_document_rewrite_acknowledged(
            &imported,
            &input,
            source.acknowledge_certificate_signature,
        )?;
        collect_imported_source_warnings(
            &imported,
            &input,
            imported_was_encrypted,
            request.document_lock.is_some(),
            &mut warnings,
        );

        control.ensure_not_cancelled()?;
        imported.renumber_objects_with(document.max_id.saturating_add(1));
        document.max_id = document.max_id.max(imported.max_id);
        let pages = imported.get_pages();
        if pages.is_empty() {
            return Err(format!(
                "Imported source {} does not contain any readable pages.",
                display_path_name(&input)
            ));
        }
        let snapshot_control = control.subrange(
            source_start.saturating_add(1).min(source_end),
            source_end,
            format!(
                "Imported source {} of {}",
                source_index + 1,
                request.imported_sources.len()
            ),
        );
        let snapshots = snapshot_source_pages_with_control(&imported, &pages, &snapshot_control)?;
        control.ensure_not_cancelled()?;
        document.objects.extend(imported.objects);
        source_pages.insert(source.id.clone(), snapshots);
    }

    let page_plan_control = control.subrange(50, 75, "Page plan".to_string());
    let output_page_ids = apply_composed_page_plan_with_control(
        &mut document,
        &request.pages,
        &source_pages,
        &page_plan_control,
    )?;
    if let Some(signature) = request.signature.as_ref() {
        let signature_control = control.subrange(76, 82, "Visual signature".to_string());
        flatten_signature_with_control(
            &mut document,
            output_page_ids[signature.page_number - 1],
            signature,
            &signature_control,
        )?;
    }
    if !request.visual_signature_placements.is_empty() {
        let signature_control = control.subrange(76, 82, "Visual signatures".to_string());
        flatten_visual_signatures_with_control(
            &mut document,
            &output_page_ids,
            &request.visual_signature_assets,
            &request.visual_signature_placements,
            &signature_control,
        )?;
    }
    document.change_producer("Tüfekci Paperworks");

    control.checkpoint(83, "Writing organised temporary PDF")?;
    let output_file = document
        .save(organised_temporary.path())
        .map_err(|error| format!("The organised PDF could not be written: {error}"))?;
    output_file
        .sync_all()
        .map_err(|error| format!("The organised PDF could not be flushed to storage: {error}"))?;

    let expected_signature_counts =
        if request.signature.is_some() || !request.visual_signature_placements.is_empty() {
            flattened_signature_counts(&document, &output_page_ids)?
        } else {
            BTreeMap::new()
        };
    let organised_verification_control = control.subrange(87, 92, "Organised output".to_string());
    verify_export_with_control(
        organised_temporary.path(),
        request.pages.len(),
        None,
        &expected_signature_counts,
        &organised_verification_control,
    )?;

    let published_temporary = if let (Some(lock), Some(locked_temporary)) =
        (request.document_lock.as_ref(), locked_temporary.as_ref())
    {
        control.checkpoint(93, "Applying document password and change restrictions")?;
        lock_pdf_changes_with_control(
            organised_temporary.path(),
            locked_temporary.path(),
            &lock.open_password,
            &lock.owner_password,
            control,
        )?;
        control.checkpoint(96, "Reopening locked organised PDF")?;
        let locked_verification_control = control.subrange(96, 98, "Locked output".to_string());
        verify_export_with_control(
            locked_temporary.path(),
            request.pages.len(),
            Some(&lock.open_password),
            &expected_signature_counts,
            &locked_verification_control,
        )?;
        locked_temporary
    } else {
        &organised_temporary
    };

    control.checkpoint(98, "Rechecking source PDFs before publication")?;
    for source in &verified_sources {
        control.ensure_not_cancelled()?;
        verify_source_fingerprint(
            &source.path,
            source.expected_size,
            source.expected_modified_at_ms,
            &source.label,
        )?;
    }
    control.checkpoint(99, "Publishing verified organised PDF")?;
    let bytes_written = published_temporary.persist(&paths.output)?;
    warnings.sort();
    warnings.dedup();

    Ok(ExportPdfResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        bytes_written,
        page_count: request.pages.len(),
        warnings,
    })
}

pub(crate) fn validate_composed_pdf_request(
    request: &ExportComposedPdfRequest,
) -> Result<(), String> {
    validate_composed_request(request)?;
    validate_signature_request(request.signature.as_ref(), request.pages.len())?;
    validate_visual_signature_request(
        request.signature.as_ref(),
        &request.visual_signature_assets,
        &request.visual_signature_placements,
        request.pages.len(),
    )?;
    if let Some(lock) = request.document_lock.as_ref() {
        validate_password("Opening password", &lock.open_password, false)?;
        validate_password("Administrator password", &lock.owner_password, false)?;
        if lock.open_password == lock.owner_password {
            return Err(
                "Use a different administrator password when restricting document changes."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_composed_request(request: &ExportComposedPdfRequest) -> Result<(), String> {
    reject_control_characters("Primary input path", &request.primary_input_path)?;
    reject_control_characters("Output path", &request.output_path)?;
    validate_source_password(
        "Primary PDF password",
        request.primary_input_password.as_deref(),
    )?;
    if request.imported_sources.len() > MAX_IMPORTED_SOURCES {
        return Err(format!(
            "An organised export may contain no more than {MAX_IMPORTED_SOURCES} imported PDFs."
        ));
    }
    let mut source_ids = HashSet::from([PRIMARY_SOURCE_ID.to_string()]);
    for source in &request.imported_sources {
        reject_control_characters("Imported source identifier", &source.id)?;
        reject_control_characters("Imported source path", &source.input_path)?;
        validate_source_password("Imported source password", source.input_password.as_deref())?;
        if source.id.is_empty() || source.id.len() > 256 || source.id == PRIMARY_SOURCE_ID {
            return Err("An imported PDF has an invalid source identifier.".to_string());
        }
        if !source_ids.insert(source.id.clone()) {
            return Err("Each imported PDF must have a unique source identifier.".to_string());
        }
    }

    if request.pages.is_empty() {
        return Err("The export must contain at least one page.".to_string());
    }
    if request.pages.len() > MAX_EXPORT_PAGES {
        return Err(format!(
            "The export contains more than {MAX_EXPORT_PAGES} pages. Split it into smaller jobs."
        ));
    }
    for page in &request.pages {
        let rotation = match page {
            ComposedExportPage::Source {
                source_id,
                source_page,
                rotation,
            } => {
                if !source_ids.contains(source_id) {
                    return Err("A planned page refers to an unavailable source PDF.".to_string());
                }
                if *source_page == 0 {
                    return Err("Source page numbers begin at 1.".to_string());
                }
                *rotation
            }
            ComposedExportPage::Blank {
                width_pt,
                height_pt,
                rotation,
            } => {
                validate_page_dimension("Blank-page width", *width_pt)?;
                validate_page_dimension("Blank-page height", *height_pt)?;
                *rotation
            }
        };
        if !matches!(rotation, 0 | 90 | 180 | 270) {
            return Err("Page rotation must be 0, 90, 180 or 270 degrees.".to_string());
        }
    }
    Ok(())
}

fn validate_source_password(label: &str, password: Option<&str>) -> Result<(), String> {
    let Some(password) = password else {
        return Ok(());
    };
    reject_control_characters(label, password)?;
    if password.len() > MAX_SOURCE_PASSWORD_BYTES {
        return Err(format!(
            "{label} may contain no more than {MAX_SOURCE_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }
    Ok(())
}

struct VerifiedExportSource {
    expected_modified_at_ms: Option<u64>,
    expected_size: u64,
    label: String,
    path: PathBuf,
}

fn verify_source_fingerprint(
    path: &Path,
    expected_size: u64,
    expected_modified_at_ms: Option<u64>,
    label: &str,
) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("{label} could not be inspected: {error}"))?;
    if !metadata.is_file()
        || metadata.len() != expected_size
        || modified_at_ms(&metadata) != expected_modified_at_ms
    {
        return Err(format!(
            "{label} changed after it was opened. Reopen it before exporting."
        ));
    }
    Ok(())
}

fn modified_at_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn progress_between(start: u8, end: u8, completed: usize, total: usize) -> u8 {
    if total == 0 || end <= start {
        return end;
    }
    start.saturating_add(
        (((end - start) as u128 * completed.min(total) as u128) / total as u128) as u8,
    )
}

fn load_export_source(
    path: &Path,
    password: Option<&str>,
    label: &str,
) -> Result<(Document, bool), String> {
    let mut document =
        Document::load(path).map_err(|error| format!("{label} could not be parsed: {error}"))?;
    let was_encrypted = document.is_encrypted() || document.was_encrypted();
    if document.is_encrypted() {
        document
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                format!("{label} could not be decrypted. Check its password and try again.")
            })?;
    }
    Ok((document, was_encrypted))
}

#[cfg(test)]
fn validate_page_plan(pages: &[ExportPage]) -> Result<(), String> {
    if pages.is_empty() {
        return Err("The export must contain at least one page.".to_string());
    }
    if pages.len() > MAX_EXPORT_PAGES {
        return Err(format!(
            "The export contains more than {MAX_EXPORT_PAGES} pages. Split it into smaller jobs."
        ));
    }

    for page in pages {
        let rotation = match page {
            ExportPage::Source {
                source_page,
                rotation,
            } => {
                if *source_page == 0 {
                    return Err("Source page numbers begin at 1.".to_string());
                }
                *rotation
            }
            ExportPage::Blank {
                width_pt,
                height_pt,
                rotation,
            } => {
                validate_page_dimension("Blank-page width", *width_pt)?;
                validate_page_dimension("Blank-page height", *height_pt)?;
                *rotation
            }
        };

        if !matches!(rotation, 0 | 90 | 180 | 270) {
            return Err("Page rotation must be 0, 90, 180 or 270 degrees.".to_string());
        }
    }

    Ok(())
}

fn validate_signature_request(
    signature: Option<&SignaturePlacement>,
    page_count: usize,
) -> Result<(), String> {
    let Some(signature) = signature else {
        return Ok(());
    };
    if signature.page_number == 0 || signature.page_number > page_count {
        return Err("The signature page is outside the exported page range.".to_string());
    }
    if signature.png_data_url.len() > MAX_SIGNATURE_DATA_BYTES * 2 {
        return Err("The prepared signature image is too large to export safely.".to_string());
    }
    if !signature.png_data_url.starts_with("data:image/png;base64,") {
        return Err("The prepared signature must be a transparent PNG image.".to_string());
    }
    Ok(())
}

fn validate_visual_signature_request(
    legacy_signature: Option<&SignaturePlacement>,
    assets: &[VisualSignatureAsset],
    placements: &[VisualSignaturePlacement],
    page_count: usize,
) -> Result<(), String> {
    if legacy_signature.is_some() && (!assets.is_empty() || !placements.is_empty()) {
        return Err(
            "Use either the legacy visual signature or the multi-placement visual-signature format, not both."
                .to_string(),
        );
    }
    if assets.len() > MAX_VISUAL_SIGNATURE_ASSETS {
        return Err(format!(
            "A document may use no more than {MAX_VISUAL_SIGNATURE_ASSETS} visual-signature assets."
        ));
    }
    if placements.len() > MAX_VISUAL_SIGNATURE_PLACEMENTS {
        return Err(format!(
            "A document may contain no more than {MAX_VISUAL_SIGNATURE_PLACEMENTS} visual-signature placements."
        ));
    }
    if placements.is_empty() {
        if assets.is_empty() {
            return Ok(());
        }
        return Err("Visual-signature assets must not be sent without placements.".to_string());
    }
    if assets.is_empty() {
        return Err("Visual-signature placements require at least one image asset.".to_string());
    }

    let mut asset_ids = HashSet::with_capacity(assets.len());
    let mut total_data_bytes = 0_usize;
    for asset in assets {
        validate_visual_signature_id("asset", &asset.id)?;
        if !asset_ids.insert(asset.id.as_str()) {
            return Err("Visual-signature asset identifiers must be unique.".to_string());
        }
        if asset.png_data_url.len() > MAX_SIGNATURE_DATA_BYTES * 2
            || !asset.png_data_url.starts_with("data:image/png;base64,")
        {
            return Err(
                "Every visual-signature asset must be a bounded transparent PNG image.".to_string(),
            );
        }
        total_data_bytes = total_data_bytes
            .checked_add(asset.png_data_url.len())
            .ok_or_else(|| "The visual-signature asset payload is too large.".to_string())?;
    }
    if total_data_bytes > MAX_SIGNATURE_TOTAL_DATA_BYTES * 2 {
        return Err("The combined visual-signature image payload is too large.".to_string());
    }

    let mut placement_ids = HashSet::with_capacity(placements.len());
    let mut used_asset_ids = HashSet::with_capacity(assets.len());
    for placement in placements {
        validate_visual_signature_id("placement", &placement.id)?;
        if !placement_ids.insert(placement.id.as_str()) {
            return Err("Visual-signature placement identifiers must be unique.".to_string());
        }
        if !asset_ids.contains(placement.asset_id.as_str()) {
            return Err("A visual-signature placement refers to a missing asset.".to_string());
        }
        used_asset_ids.insert(placement.asset_id.as_str());
        if placement.page_number == 0 || placement.page_number > page_count {
            return Err(
                "A visual-signature placement is outside the exported page range.".to_string(),
            );
        }
        if !placement.left_ratio.is_finite()
            || !placement.top_ratio.is_finite()
            || !placement.width_ratio.is_finite()
            || !placement.rotation_degrees.is_finite()
            || !(-1.0..=2.0).contains(&placement.left_ratio)
            || !(-1.0..=2.0).contains(&placement.top_ratio)
            || !(0.001..=0.9).contains(&placement.width_ratio)
            || !(-180.0..=180.0).contains(&placement.rotation_degrees)
        {
            return Err("A visual-signature placement has invalid bounded geometry.".to_string());
        }
    }
    if used_asset_ids.len() != assets.len() {
        return Err("Every visual-signature asset must be used by a placement.".to_string());
    }
    Ok(())
}

fn validate_visual_signature_id(label: &str, value: &str) -> Result<(), String> {
    if (1..=64).contains(&value.len())
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        Ok(())
    } else {
        Err(format!(
            "The visual-signature {label} identifier contains unsafe characters."
        ))
    }
}

fn validate_page_dimension(label: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || !(MIN_PAGE_POINTS..=MAX_PAGE_POINTS).contains(&value) {
        return Err(format!(
            "{label} must be between {MIN_PAGE_POINTS} and {MAX_PAGE_POINTS} points."
        ));
    }
    Ok(())
}

#[cfg(test)]
fn apply_page_plan(document: &mut Document, pages: &[ExportPage]) -> Result<Vec<ObjectId>, String> {
    let source_pages = document.get_pages();
    if source_pages.is_empty() {
        return Err("The source PDF does not contain any readable pages.".to_string());
    }

    let pages_root_id = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|error| format!("The source PDF has an invalid page tree: {error}"))?;
    let snapshots = snapshot_source_pages(document, &source_pages)?;
    let mut used_source_ids = HashSet::new();
    let mut output_ids = Vec::with_capacity(pages.len());

    for planned_page in pages {
        let output_id = match planned_page {
            ExportPage::Source {
                source_page,
                rotation,
            } => {
                let snapshot = snapshots.get(source_page).ok_or_else(|| {
                    format!(
                        "Source page {source_page} does not exist. The document has {} pages.",
                        source_pages.len()
                    )
                })?;
                let mut page = snapshot.dictionary.clone();
                apply_rotation(&mut page, *rotation);
                page.set("Parent", pages_root_id);

                if used_source_ids.insert(snapshot.id) {
                    document
                        .objects
                        .insert(snapshot.id, Object::Dictionary(page));
                    snapshot.id
                } else {
                    page.remove(b"StructParents");
                    document.add_object(Object::Dictionary(page))
                }
            }
            ExportPage::Blank {
                width_pt,
                height_pt,
                rotation,
            } => create_blank_page(document, pages_root_id, *width_pt, *height_pt, *rotation),
        };

        output_ids.push(output_id);
    }

    let pages_root = document
        .get_dictionary_mut(pages_root_id)
        .map_err(|error| format!("The source PDF page root could not be updated: {error}"))?;
    pages_root.set(
        "Kids",
        output_ids
            .iter()
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    pages_root.set("Count", output_ids.len() as i64);
    pages_root.set("Type", Object::Name(b"Pages".to_vec()));

    document.prune_objects();
    Ok(output_ids)
}

fn apply_composed_page_plan_with_control(
    document: &mut Document,
    pages: &[ComposedExportPage],
    source_pages: &HashMap<String, BTreeMap<u32, PageSnapshot>>,
    control: &PdfJobExecutionControl,
) -> Result<Vec<ObjectId>, String> {
    let pages_root_id = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|error| format!("The primary PDF has an invalid page tree: {error}"))?;
    let mut used_source_ids = HashSet::new();
    let mut output_ids = Vec::with_capacity(pages.len());

    for (page_index, planned_page) in pages.iter().enumerate() {
        control.ensure_not_cancelled()?;
        if page_index == 0 || page_index % 128 == 0 {
            control.checkpoint(
                progress_between(0, 94, page_index, pages.len()),
                format!("Arranging page {} of {}", page_index + 1, pages.len()),
            )?;
        }
        let output_id = match planned_page {
            ComposedExportPage::Source {
                source_id,
                source_page,
                rotation,
            } => {
                let snapshots = source_pages.get(source_id).ok_or_else(|| {
                    "A planned page refers to an unavailable source PDF.".to_string()
                })?;
                let snapshot = snapshots.get(source_page).ok_or_else(|| {
                    format!("Source page {source_page} does not exist in source {source_id}.")
                })?;
                let mut page = snapshot.dictionary.clone();
                apply_rotation(&mut page, *rotation);
                page.set("Parent", pages_root_id);
                if source_id != PRIMARY_SOURCE_ID {
                    page.remove(b"StructParents");
                }

                if used_source_ids.insert(snapshot.id) {
                    document
                        .objects
                        .insert(snapshot.id, Object::Dictionary(page));
                    snapshot.id
                } else {
                    page.remove(b"StructParents");
                    document.add_object(Object::Dictionary(page))
                }
            }
            ComposedExportPage::Blank {
                width_pt,
                height_pt,
                rotation,
            } => create_blank_page(document, pages_root_id, *width_pt, *height_pt, *rotation),
        };
        output_ids.push(output_id);
    }

    let pages_root = document
        .get_dictionary_mut(pages_root_id)
        .map_err(|error| format!("The primary PDF page root could not be updated: {error}"))?;
    pages_root.set(
        "Kids",
        output_ids
            .iter()
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    pages_root.set("Count", output_ids.len() as i64);
    pages_root.set("Type", Object::Name(b"Pages".to_vec()));

    control.checkpoint(96, "Pruning unused page objects")?;
    document.prune_objects();
    control.checkpoint(100, "Organised page plan prepared")?;
    Ok(output_ids)
}

#[derive(Clone)]
struct PageSnapshot {
    id: ObjectId,
    dictionary: Dictionary,
}

#[cfg(test)]
fn snapshot_source_pages(
    document: &Document,
    source_pages: &BTreeMap<u32, ObjectId>,
) -> Result<BTreeMap<u32, PageSnapshot>, String> {
    snapshot_source_pages_with_control(document, source_pages, &PdfJobExecutionControl::direct())
}

fn snapshot_source_pages_with_control(
    document: &Document,
    source_pages: &BTreeMap<u32, ObjectId>,
    control: &PdfJobExecutionControl,
) -> Result<BTreeMap<u32, PageSnapshot>, String> {
    let mut snapshots = BTreeMap::new();
    for (page_index, (page_number, page_id)) in source_pages.iter().enumerate() {
        control.ensure_not_cancelled()?;
        if page_index == 0 || page_index % 128 == 0 {
            control.checkpoint(
                progress_between(0, 98, page_index, source_pages.len()),
                format!("Reading page {} of {}", page_index + 1, source_pages.len()),
            )?;
        }
        let mut dictionary = document
            .get_dictionary(*page_id)
            .map_err(|error| format!("Source page {page_number} is invalid: {error}"))?
            .clone();

        for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
            if let Some(value) = inherited_page_value(document, *page_id, key)? {
                dictionary.set(key, value);
            }
        }
        dictionary.remove(b"Parent");
        dictionary.set("Type", Object::Name(b"Page".to_vec()));
        snapshots.insert(
            *page_number,
            PageSnapshot {
                id: *page_id,
                dictionary,
            },
        );
    }
    control.checkpoint(100, "Source page snapshots prepared")?;
    Ok(snapshots)
}

fn inherited_page_value(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<Object>, String> {
    let mut current_id = page_id;
    let mut visited = HashSet::new();

    for _ in 0..256 {
        if !visited.insert(current_id) {
            return Err("The source PDF contains a cyclic page tree.".to_string());
        }

        let dictionary = document
            .get_dictionary(current_id)
            .map_err(|error| format!("The source PDF page tree is invalid: {error}"))?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value.clone()));
        }

        match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent_id) => current_id = parent_id,
            Err(_) => return Ok(None),
        }
    }

    Err("The source PDF page tree is too deeply nested.".to_string())
}

fn apply_rotation(page: &mut Dictionary, planned_rotation: i64) {
    let existing = page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
    let rotation = (existing + planned_rotation).rem_euclid(360);

    if rotation == 0 {
        page.remove(b"Rotate");
    } else {
        page.set("Rotate", rotation);
    }
}

fn create_blank_page(
    document: &mut Document,
    pages_root_id: ObjectId,
    width_pt: f64,
    height_pt: f64,
    rotation: i64,
) -> ObjectId {
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_root_id,
        "MediaBox" => vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(width_pt as f32),
            Object::Real(height_pt as f32),
        ],
        "Resources" => Dictionary::new(),
        "Contents" => content_id,
    };
    apply_rotation(&mut page, rotation);
    document.add_object(Object::Dictionary(page))
}

#[cfg(test)]
fn flatten_signature(
    document: &mut Document,
    page_id: ObjectId,
    placement: &SignaturePlacement,
) -> Result<(), String> {
    flatten_signature_with_control(
        document,
        page_id,
        placement,
        &PdfJobExecutionControl::direct(),
    )
}

fn flatten_signature_with_control(
    document: &mut Document,
    page_id: ObjectId,
    placement: &SignaturePlacement,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let image_control = control.subrange(0, 82, "Visual signature image".to_string());
    let embedded =
        embed_signature_image_with_control(document, &placement.png_data_url, &image_control)?;
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("The signature page is invalid: {error}"))?;
    let (page_box, rotation) = page_geometry(document, page)?;
    let matrix = signature_matrix(
        page_box,
        rotation,
        embedded.width,
        embedded.height,
        placement.position,
    );
    let placement_control = control.subrange(83, 100, "Visual signature placement".to_string());
    place_embedded_signature_with_control(document, page_id, embedded, matrix, &placement_control)
}

#[derive(Clone, Copy, Debug)]
struct EmbeddedSignatureImage {
    height: u32,
    image_id: ObjectId,
    width: u32,
}

fn embed_signature_image_with_control(
    document: &mut Document,
    png_data_url: &str,
    control: &PdfJobExecutionControl,
) -> Result<EmbeddedSignatureImage, String> {
    control.checkpoint(2, "Decoding prepared signature")?;
    let mut signature = decode_signature(png_data_url)?;
    if signature.width() > MAX_EMBEDDED_SIGNATURE_DIMENSION
        || signature.height() > MAX_EMBEDDED_SIGNATURE_DIMENSION
    {
        signature = signature.thumbnail(
            MAX_EMBEDDED_SIGNATURE_DIMENSION,
            MAX_EMBEDDED_SIGNATURE_DIMENSION,
        );
    }

    let rgba = signature.to_rgba8();
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    let mut alpha = Vec::with_capacity(rgba.len() / 4);
    let pixel_count = u64::from(rgba.width()) * u64::from(rgba.height());
    let mut visible_ink = false;
    for (pixel_index, pixel) in rgba.pixels().enumerate() {
        if pixel_index == 0 || pixel_index % 65_536 == 0 {
            control.checkpoint(
                progress_between(25, 72, pixel_index, pixel_count as usize),
                "Preparing transparent signature pixels",
            )?;
        }
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
        visible_ink |= pixel.0[3] > 0;
    }
    if !visible_ink {
        return Err("The prepared signature does not contain any visible ink.".to_string());
    }

    control.checkpoint(76, "Compressing signature transparency")?;
    let mut alpha_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(rgba.width()),
            "Height" => i64::from(rgba.height()),
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        },
        alpha,
    );
    alpha_stream
        .compress()
        .map_err(|error| format!("The signature transparency could not be compressed: {error}"))?;
    let alpha_id = document.add_object(alpha_stream);

    control.checkpoint(84, "Compressing signature artwork")?;
    let mut image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(rgba.width()),
            "Height" => i64::from(rgba.height()),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "SMask" => alpha_id,
            "TufekciSignature" => true,
        },
        rgb,
    );
    image_stream
        .compress()
        .map_err(|error| format!("The signature image could not be compressed: {error}"))?;
    let image_id = document.add_object(image_stream);

    control.checkpoint(100, "Prepared signature image embedded")?;
    Ok(EmbeddedSignatureImage {
        height: rgba.height(),
        image_id,
        width: rgba.width(),
    })
}

fn place_embedded_signature_with_control(
    document: &mut Document,
    page_id: ObjectId,
    embedded: EmbeddedSignatureImage,
    matrix: [f64; 6],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.checkpoint(5, "Preparing visual-signature page resources")?;

    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("The signature page is invalid: {error}"))?
        .clone();

    let mut resources = match page.get(b"Resources") {
        Ok(value) => cloned_dictionary(document, value, "page resources")?,
        Err(_) => Dictionary::new(),
    };
    let mut xobjects = match resources.get(b"XObject") {
        Ok(value) => cloned_dictionary(document, value, "page image resources")?,
        Err(_) => Dictionary::new(),
    };
    let mut resource_name = format!("TufekciSignature{}", embedded.image_id.0).into_bytes();
    let mut suffix = 1_u32;
    while xobjects.has(&resource_name) {
        resource_name = format!("TufekciSignature{}_{suffix}", embedded.image_id.0).into_bytes();
        suffix += 1;
    }
    xobjects.set(resource_name.clone(), embedded.image_id);
    resources.set("XObject", xobjects);
    page.set("Resources", resources);

    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix.into_iter().map(pdf_real).collect::<Vec<_>>()),
            Operation::new("Do", vec![Object::Name(resource_name)]),
            Operation::new("Q", vec![]),
        ],
    }
    .encode()
    .map_err(|error| format!("The signature placement could not be encoded: {error}"))?;
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let existing_contents = page.get(b"Contents").ok().cloned();
    page.set(
        "Contents",
        append_content_stream(document, existing_contents, content_id)?,
    );
    document.objects.insert(page_id, Object::Dictionary(page));
    control.checkpoint(100, "Flattened signature prepared")?;
    Ok(())
}

fn flatten_visual_signatures_with_control(
    document: &mut Document,
    output_page_ids: &[ObjectId],
    assets: &[VisualSignatureAsset],
    placements: &[VisualSignaturePlacement],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let mut embedded_assets = HashMap::with_capacity(assets.len());
    for (index, asset) in assets.iter().enumerate() {
        control.ensure_not_cancelled()?;
        let start = progress_between(0, 58, index, assets.len());
        let end = progress_between(0, 58, index + 1, assets.len());
        let asset_control = control.subrange(
            start,
            end,
            format!("Visual-signature image {} of {}", index + 1, assets.len()),
        );
        let embedded =
            embed_signature_image_with_control(document, &asset.png_data_url, &asset_control)?;
        embedded_assets.insert(asset.id.as_str(), embedded);
    }

    for (index, placement) in placements.iter().enumerate() {
        control.ensure_not_cancelled()?;
        let start = progress_between(59, 99, index, placements.len());
        let end = progress_between(59, 99, index + 1, placements.len());
        let placement_control = control.subrange(
            start,
            end,
            format!(
                "Visual-signature placement {} of {}",
                index + 1,
                placements.len()
            ),
        );
        let embedded = embedded_assets
            .get(placement.asset_id.as_str())
            .ok_or_else(|| {
                "A visual-signature placement refers to an unavailable prepared asset.".to_string()
            })?;
        let page_id = *output_page_ids
            .get(placement.page_number.saturating_sub(1))
            .ok_or_else(|| {
                "A visual-signature placement page was not found during export.".to_string()
            })?;
        let page = document
            .get_dictionary(page_id)
            .map_err(|error| format!("The signature page is invalid: {error}"))?;
        let (page_box, page_rotation) = page_geometry(document, page)?;
        let matrix = visual_signature_matrix(page_box, page_rotation, *embedded, placement)?;
        place_embedded_signature_with_control(
            document,
            page_id,
            *embedded,
            matrix,
            &placement_control,
        )?;
    }
    control.checkpoint(100, "Visual signatures flattened")?;
    Ok(())
}

fn decode_signature(data_url: &str) -> Result<DynamicImage, String> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "The prepared signature must be a transparent PNG image.".to_string())?;
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| "The prepared signature image is not valid base64 data.".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_SIGNATURE_DATA_BYTES {
        return Err(
            "The prepared signature image is empty or too large to export safely.".to_string(),
        );
    }

    match catch_unwind(AssertUnwindSafe(|| -> Result<DynamicImage, String> {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_SIGNATURE_DIMENSION);
        limits.max_image_height = Some(MAX_SIGNATURE_DIMENSION);
        limits.max_alloc = Some(MAX_SIGNATURE_ALLOCATION);
        reader.limits(limits);
        reader
            .decode()
            .map_err(|error| format!("The prepared signature PNG could not be decoded: {error}"))
    })) {
        Ok(result) => result,
        Err(_) => Err("The prepared signature image was rejected safely.".to_string()),
    }
}

fn cloned_dictionary(
    document: &Document,
    object: &Object,
    label: &str,
) -> Result<Dictionary, String> {
    match object {
        Object::Dictionary(dictionary) => Ok(dictionary.clone()),
        Object::Reference(id) => document
            .get_dictionary(*id)
            .cloned()
            .map_err(|error| format!("The PDF {label} are invalid: {error}")),
        _ => Err(format!("The PDF {label} are not a dictionary.")),
    }
}

fn append_content_stream(
    document: &mut Document,
    existing: Option<Object>,
    content_id: ObjectId,
) -> Result<Object, String> {
    let new_content = Object::Reference(content_id);
    match existing {
        None | Some(Object::Null) => Ok(new_content),
        Some(Object::Reference(id)) => Ok(Object::Array(vec![Object::Reference(id), new_content])),
        Some(Object::Array(mut contents)) => {
            contents.push(new_content);
            Ok(Object::Array(contents))
        }
        Some(Object::Stream(stream)) => {
            let original_id = document.add_object(stream);
            Ok(Object::Array(vec![
                Object::Reference(original_id),
                new_content,
            ]))
        }
        Some(_) => Err("The signature page has an invalid content stream.".to_string()),
    }
}

#[derive(Clone, Copy, Debug)]
struct PageBox {
    left: f64,
    bottom: f64,
    width: f64,
    height: f64,
}

fn page_geometry(document: &Document, page: &Dictionary) -> Result<(PageBox, i64), String> {
    let page_box = page
        .get(b"CropBox")
        .or_else(|_| page.get(b"MediaBox"))
        .map_err(|_| "The signature page does not define a crop or media box.".to_string())?;
    let page_box = match page_box {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| format!("The signature page box is invalid: {error}"))?,
        value => value,
    };
    let coordinates = page_box
        .as_array()
        .map_err(|_| "The signature page box is not an array.".to_string())?;
    if coordinates.len() != 4 {
        return Err("The signature page box must contain four coordinates.".to_string());
    }
    let values = coordinates
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, _>>()?;
    let width = values[2] - values[0];
    let height = values[3] - values[1];
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("The signature page has invalid dimensions.".to_string());
    }
    let rotation = page
        .get(b"Rotate")
        .and_then(Object::as_i64)
        .unwrap_or(0)
        .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("The signature page has an unsupported rotation.".to_string());
    }
    Ok((
        PageBox {
            left: values[0],
            bottom: values[1],
            width,
            height,
        },
        rotation,
    ))
}

fn pdf_number_value(object: &Object) -> Result<f64, String> {
    match object {
        Object::Integer(value) => Ok(*value as f64),
        Object::Real(value) => Ok(f64::from(*value)),
        _ => Err("The signature page box contains a non-numeric value.".to_string()),
    }
}

fn signature_matrix(
    page: PageBox,
    rotation: i64,
    image_width: u32,
    image_height: u32,
    position: SignaturePosition,
) -> [f64; 6] {
    let (visual_width, visual_height) = if matches!(rotation, 90 | 270) {
        (page.height, page.width)
    } else {
        (page.width, page.height)
    };
    let max_width = visual_width * 0.32;
    let max_height = visual_height * 0.14;
    let scale = (max_width / f64::from(image_width)).min(max_height / f64::from(image_height));
    let placed_width = f64::from(image_width) * scale;
    let placed_height = f64::from(image_height) * scale;
    let horizontal_margin = visual_width * 0.055;
    let x = match position {
        SignaturePosition::Left => horizontal_margin,
        SignaturePosition::Centre => (visual_width - placed_width) / 2.0,
        SignaturePosition::Right => visual_width - horizontal_margin - placed_width,
    };
    let y = visual_height * 0.065;

    match rotation {
        90 => [
            0.0,
            placed_width,
            -placed_height,
            0.0,
            page.left + page.width - y,
            page.bottom + x,
        ],
        180 => [
            -placed_width,
            0.0,
            0.0,
            -placed_height,
            page.left + page.width - x,
            page.bottom + page.height - y,
        ],
        270 => [
            0.0,
            -placed_width,
            placed_height,
            0.0,
            page.left + y,
            page.bottom + page.height - x,
        ],
        _ => [
            placed_width,
            0.0,
            0.0,
            placed_height,
            page.left + x,
            page.bottom + y,
        ],
    }
}

fn visual_signature_matrix(
    page: PageBox,
    page_rotation: i64,
    image: EmbeddedSignatureImage,
    placement: &VisualSignaturePlacement,
) -> Result<[f64; 6], String> {
    let (visual_width, visual_height) = if matches!(page_rotation, 90 | 270) {
        (page.height, page.width)
    } else {
        (page.width, page.height)
    };
    let placed_width = visual_width * placement.width_ratio;
    let placed_height = placed_width * f64::from(image.height) / f64::from(image.width);
    let left = visual_width * placement.left_ratio;
    let top = visual_height * placement.top_ratio;
    let bottom = visual_height - top - placed_height;
    if !placed_width.is_finite()
        || !placed_height.is_finite()
        || placed_width <= 0.0
        || placed_height <= 0.0
    {
        return Err("A visual-signature placement has invalid image dimensions.".to_string());
    }

    // Positive editor angles are clockwise in the top-left screen coordinate system.
    let radians = -placement.rotation_degrees.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let a = placed_width * cosine;
    let b = placed_width * sine;
    let c = -placed_height * sine;
    let d = placed_height * cosine;
    let centre_x = left + placed_width / 2.0;
    let centre_y = bottom + placed_height / 2.0;
    let e = centre_x - a / 2.0 - c / 2.0;
    let f = centre_y - b / 2.0 - d / 2.0;
    let visual_matrix = [a, b, c, d, e, f];

    let tolerance = visual_width.max(visual_height) * 0.000_01;
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let visual_x = a * x + c * y + e;
        let visual_y = b * x + d * y + f;
        if visual_x < -tolerance
            || visual_x > visual_width + tolerance
            || visual_y < -tolerance
            || visual_y > visual_height + tolerance
        {
            return Err(
                "A visual-signature placement extends outside its selected page.".to_string(),
            );
        }
    }

    Ok(match page_rotation {
        90 => [
            -visual_matrix[1],
            visual_matrix[0],
            -visual_matrix[3],
            visual_matrix[2],
            page.left + page.width - visual_matrix[5],
            page.bottom + visual_matrix[4],
        ],
        180 => [
            -visual_matrix[0],
            -visual_matrix[1],
            -visual_matrix[2],
            -visual_matrix[3],
            page.left + page.width - visual_matrix[4],
            page.bottom + page.height - visual_matrix[5],
        ],
        270 => [
            visual_matrix[1],
            -visual_matrix[0],
            visual_matrix[3],
            -visual_matrix[2],
            page.left + visual_matrix[5],
            page.bottom + page.height - visual_matrix[4],
        ],
        _ => [
            visual_matrix[0],
            visual_matrix[1],
            visual_matrix[2],
            visual_matrix[3],
            page.left + visual_matrix[4],
            page.bottom + visual_matrix[5],
        ],
    })
}

fn pdf_real(value: f64) -> Object {
    Object::Real(value as f32)
}

#[cfg(test)]
fn export_warnings(
    document: &Document,
    pages: &[ExportPage],
    source_was_encrypted: bool,
    has_visual_signature: bool,
    has_document_lock: bool,
) -> Vec<String> {
    let identity_plan = pages.len() == document.get_pages().len()
        && pages.iter().enumerate().all(|(index, page)| {
            matches!(
                page,
                ExportPage::Source {
                    source_page,
                    rotation: 0
                } if *source_page as usize == index + 1
            )
        });
    common_export_warnings(
        document,
        identity_plan,
        source_was_encrypted,
        has_visual_signature,
        has_document_lock,
    )
}

fn composed_export_warnings(
    document: &Document,
    pages: &[ComposedExportPage],
    source_was_encrypted: bool,
    has_visual_signature: bool,
    has_document_lock: bool,
    primary_page_count: usize,
) -> Vec<String> {
    let identity_plan = pages.len() == primary_page_count
        && pages.iter().enumerate().all(|(index, page)| {
            matches!(
                page,
                ComposedExportPage::Source {
                    source_id,
                    source_page,
                    rotation: 0
                } if source_id == PRIMARY_SOURCE_ID && *source_page as usize == index + 1
            )
        });
    common_export_warnings(
        document,
        identity_plan,
        source_was_encrypted,
        has_visual_signature,
        has_document_lock,
    )
}

fn common_export_warnings(
    document: &Document,
    identity_plan: bool,
    source_was_encrypted: bool,
    has_visual_signature: bool,
    has_document_lock: bool,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if source_was_encrypted && has_document_lock {
        warnings.push(
            "The source was encrypted. The exported copy uses the new signing passwords and permissions."
                .to_string(),
        );
    } else if source_was_encrypted {
        warnings.push(
            "The source was encrypted. This organised copy is not password-protected.".to_string(),
        );
    }
    if document_has_certificate_signature(document) {
        warnings.push(
            "Existing certificate signatures are invalidated by structural PDF export.".to_string(),
        );
    }
    if !identity_plan
        && document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        warnings.push(
            "This PDF contains form fields. Check their appearance and behaviour in the exported copy."
                .to_string(),
        );
    }
    if !identity_plan
        && document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"Outlines"))
    {
        warnings.push(
            "This PDF contains bookmarks. Check their destinations in the exported copy."
                .to_string(),
        );
    }
    if has_visual_signature {
        warnings.push(
            "The visual signature is flattened into the selected page, but it is not a certificate-backed digital signature."
                .to_string(),
        );
    }
    if has_document_lock {
        warnings.push(
            "AES-256 reader permissions restrict changes, but permissions are advisory and do not provide cryptographic tamper evidence."
                .to_string(),
        );
    }

    warnings
}

fn collect_imported_source_warnings(
    document: &Document,
    path: &Path,
    source_was_encrypted: bool,
    has_document_lock: bool,
    warnings: &mut Vec<String>,
) {
    let name = display_path_name(path);
    if source_was_encrypted && has_document_lock {
        warnings.push(format!(
            "Imported pages from {name} use the new output passwords and permissions."
        ));
    } else if source_was_encrypted {
        warnings.push(format!(
            "Imported pages from encrypted source {name} are not password-protected in this copy."
        ));
    }
    if document_has_certificate_signature(document) {
        warnings.push(format!(
            "The certificate signature in imported source {name} cannot be preserved in the organised copy."
        ));
    }
    if document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        warnings.push(format!(
            "Imported source {name} contains form fields. Check their appearance in the organised copy."
        ));
    }
    if document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"Outlines"))
    {
        warnings.push(format!(
            "Bookmarks from imported source {name} are not added to the primary bookmark tree."
        ));
    }
}

fn display_path_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn document_has_certificate_signature(document: &Document) -> bool {
    document.objects.values().any(|object| {
        let dictionary = match object {
            Object::Dictionary(dictionary) => Some(dictionary),
            Object::Stream(stream) => Some(&stream.dict),
            _ => None,
        };

        dictionary.is_some_and(|dictionary| {
            dictionary.has(b"ByteRange")
                || dictionary
                    .get(b"FT")
                    .and_then(Object::as_name)
                    .is_ok_and(|field_type| field_type == b"Sig")
        })
    })
}

#[cfg(test)]
fn verify_export(
    path: &std::path::Path,
    expected_pages: usize,
    password: Option<&str>,
    expected_signature_counts: &BTreeMap<usize, usize>,
) -> Result<(), String> {
    verify_export_with_control(
        path,
        expected_pages,
        password,
        expected_signature_counts,
        &PdfJobExecutionControl::direct(),
    )
}

fn verify_export_with_control(
    path: &std::path::Path,
    expected_pages: usize,
    password: Option<&str>,
    expected_signature_counts: &BTreeMap<usize, usize>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.checkpoint(2, "Reopening output PDF")?;
    let mut verification = Document::load(path)
        .map_err(|error| format!("The output PDF failed verification: {error}"))?;
    if let Some(password) = password {
        if !verification.is_encrypted() {
            return Err("The locked output PDF was not encrypted.".to_string());
        }
        verification.decrypt(password).map_err(|_| {
            "The locked output PDF could not be reopened with its new password.".to_string()
        })?;
    } else if verification.is_encrypted() {
        return Err("The output PDF unexpectedly remained encrypted.".to_string());
    }

    let pages = verification.get_pages();
    if pages.len() != expected_pages {
        return Err(format!(
            "The output PDF failed verification: expected {expected_pages} pages but found {}.",
            pages.len()
        ));
    }
    for (page_index, (page_number, page_id)) in pages.iter().enumerate() {
        control.ensure_not_cancelled()?;
        if page_index == 0 || page_index % 256 == 0 {
            control.checkpoint(
                progress_between(10, 90, page_index, expected_pages),
                format!("Verifying page {} of {expected_pages}", page_index + 1),
            )?;
        }
        let page = verification.get_dictionary(*page_id).map_err(|error| {
            format!("The output PDF page {page_number} failed verification: {error}")
        })?;
        if !page.has_type(b"Page") {
            return Err(format!(
                "The output PDF page {page_number} has an invalid page object."
            ));
        }
    }
    if !expected_signature_counts.is_empty() {
        control.checkpoint(94, "Verifying flattened visual signatures")?;
        for (signature_page, expected_count) in expected_signature_counts {
            let page_id = pages.get(&(*signature_page as u32)).ok_or_else(|| {
                "A flattened visual-signature page was not found during verification.".to_string()
            })?;
            let actual_count = page_flattened_signature_count(&verification, *page_id)?;
            if actual_count != *expected_count {
                return Err(format!(
                    "The output PDF failed verification on page {signature_page}: expected {expected_count} flattened visual-signature marks but found {actual_count}."
                ));
            }
        }
    }

    control.checkpoint(100, "Output PDF verified")?;
    Ok(())
}

#[cfg(test)]
fn page_has_flattened_signature(document: &Document, page_id: ObjectId) -> Result<bool, String> {
    Ok(page_flattened_signature_count(document, page_id)? > 0)
}

fn page_flattened_signature_count(document: &Document, page_id: ObjectId) -> Result<usize, String> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("The signed output page could not be inspected: {error}"))?;
    let resources = match page.get(b"Resources") {
        Ok(resources) => resources,
        Err(_) => return Ok(0),
    };
    let resources = cloned_dictionary(document, resources, "signed page resources")?;
    let xobjects = match resources.get(b"XObject") {
        Ok(xobjects) => xobjects,
        Err(_) => return Ok(0),
    };
    let xobjects = cloned_dictionary(document, xobjects, "signed page image resources")?;

    let mut count = 0_usize;
    for (_, value) in xobjects.iter() {
        let object = match value {
            Object::Reference(id) => document.get_object(*id).map_err(|error| {
                format!("A signed page image resource could not be inspected: {error}")
            })?,
            value => value,
        };
        let dictionary = match object {
            Object::Stream(stream) => &stream.dict,
            Object::Dictionary(dictionary) => dictionary,
            _ => continue,
        };
        if dictionary
            .get(b"TufekciSignature")
            .is_ok_and(|value| matches!(value, Object::Boolean(true)))
        {
            count += 1;
        }
    }
    Ok(count)
}

fn flattened_signature_counts(
    document: &Document,
    page_ids: &[ObjectId],
) -> Result<BTreeMap<usize, usize>, String> {
    let mut counts = BTreeMap::new();
    for (index, page_id) in page_ids.iter().enumerate() {
        let count = page_flattened_signature_count(document, *page_id)?;
        if count > 0 {
            counts.insert(index + 1, count);
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_control::PDF_JOB_CANCELLED_ERROR;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn applies_reorder_rotation_duplicate_and_blank_page() {
        let mut document = sample_document(3);
        let original_pages = document.get_pages();
        let plan = vec![
            ExportPage::Source {
                source_page: 3,
                rotation: 90,
            },
            ExportPage::Source {
                source_page: 1,
                rotation: 0,
            },
            ExportPage::Source {
                source_page: 1,
                rotation: 180,
            },
            ExportPage::Blank {
                width_pt: 612.0,
                height_pt: 792.0,
                rotation: 0,
            },
        ];

        apply_page_plan(&mut document, &plan).unwrap();
        let pages = document.get_pages();
        assert_eq!(pages.len(), 4);
        assert_eq!(pages.get(&1), original_pages.get(&3));
        assert_eq!(pages.get(&2), original_pages.get(&1));
        assert_ne!(pages.get(&2), pages.get(&3));

        let first = document.get_dictionary(*pages.get(&1).unwrap()).unwrap();
        assert_eq!(first.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
        let duplicate = document.get_dictionary(*pages.get(&3).unwrap()).unwrap();
        assert_eq!(duplicate.get(b"Rotate").unwrap().as_i64().unwrap(), 180);
        let blank = document.get_dictionary(*pages.get(&4).unwrap()).unwrap();
        let media_box = blank.get(b"MediaBox").unwrap().as_array().unwrap();
        assert_eq!(media_box[2].as_float().unwrap(), 612.0);
        assert_eq!(media_box[3].as_float().unwrap(), 792.0);
    }

    #[test]
    fn rejects_invalid_page_plans() {
        let error = validate_page_plan(&[ExportPage::Source {
            source_page: 1,
            rotation: 45,
        }])
        .unwrap_err();
        assert!(error.contains("0, 90, 180 or 270"));

        let error = validate_page_plan(&[ExportPage::Blank {
            width_pt: f64::NAN,
            height_pt: 842.0,
            rotation: 0,
        }])
        .unwrap_err();
        assert!(error.contains("Blank-page width"));
    }

    #[test]
    fn rejects_missing_source_pages() {
        let mut document = sample_document(1);
        let error = apply_page_plan(
            &mut document,
            &[ExportPage::Source {
                source_page: 2,
                rotation: 0,
            }],
        )
        .unwrap_err();
        assert!(error.contains("does not exist"));
    }

    #[test]
    fn exports_to_a_new_file_and_refuses_to_overwrite_it() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("organised.pdf");
        sample_document(2).save(&input).unwrap().sync_all().unwrap();
        let request = || ExportPdfRequest {
            acknowledge_certificate_signature: false,
            document_lock: None,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            pages: vec![
                ExportPage::Source {
                    source_page: 2,
                    rotation: 90,
                },
                ExportPage::Blank {
                    width_pt: 595.0,
                    height_pt: 842.0,
                    rotation: 0,
                },
            ],
            signature: None,
        };

        let result = export_pdf(request()).unwrap();
        assert_eq!(result.page_count, 2);
        assert!(result.bytes_written > 0);
        assert_eq!(Document::load(&output).unwrap().get_pages().len(), 2);

        let error = export_pdf(request()).unwrap_err();
        assert!(error.contains("destination already exists"));
    }

    #[test]
    fn exports_primary_and_imported_pages_in_one_verified_plan() {
        let directory = TestDirectory::new();
        let primary = directory.path.join("primary.pdf");
        let imported = directory.path.join("imported.pdf");
        let output = directory.path.join("composed.pdf");
        sample_document(2)
            .save(&primary)
            .unwrap()
            .sync_all()
            .unwrap();
        let mut imported_document = sample_document(2);
        let imported_page = *imported_document.get_pages().get(&1).unwrap();
        imported_document
            .get_dictionary_mut(imported_page)
            .unwrap()
            .set("MediaBox", vec![0.into(), 0.into(), 320.into(), 480.into()]);
        imported_document
            .save(&imported)
            .unwrap()
            .sync_all()
            .unwrap();
        let primary_metadata = fs::metadata(&primary).unwrap();
        let imported_metadata = fs::metadata(&imported).unwrap();

        let result = export_composed_pdf(ExportComposedPdfRequest {
            primary_input_path: primary.to_string_lossy().into_owned(),
            primary_input_password: None,
            expected_source_size: primary_metadata.len(),
            expected_source_modified_at_ms: modified_at_ms(&primary_metadata),
            acknowledge_primary_certificate_signature: false,
            imported_sources: vec![ExportPdfSource {
                id: "second-source".to_string(),
                input_path: imported.to_string_lossy().into_owned(),
                input_password: None,
                expected_source_size: imported_metadata.len(),
                expected_source_modified_at_ms: modified_at_ms(&imported_metadata),
                acknowledge_certificate_signature: false,
            }],
            output_path: output.to_string_lossy().into_owned(),
            pages: vec![
                ComposedExportPage::Source {
                    source_id: PRIMARY_SOURCE_ID.to_string(),
                    source_page: 2,
                    rotation: 0,
                },
                ComposedExportPage::Source {
                    source_id: "second-source".to_string(),
                    source_page: 1,
                    rotation: 90,
                },
                ComposedExportPage::Source {
                    source_id: "second-source".to_string(),
                    source_page: 1,
                    rotation: 180,
                },
                ComposedExportPage::Blank {
                    width_pt: 595.0,
                    height_pt: 842.0,
                    rotation: 0,
                },
            ],
            signature: None,
            visual_signature_assets: Vec::new(),
            visual_signature_placements: Vec::new(),
            document_lock: None,
        })
        .unwrap();

        assert_eq!(result.page_count, 4);
        let document = Document::load(&output).unwrap();
        let pages = document.get_pages();
        assert_eq!(pages.len(), 4);
        assert_ne!(pages.get(&2), pages.get(&3));
        let imported_page = document.get_dictionary(*pages.get(&2).unwrap()).unwrap();
        assert_eq!(imported_page.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
        let media_box = imported_page.get(b"MediaBox").unwrap().as_array().unwrap();
        assert_eq!(media_box[2].as_float().unwrap(), 320.0);
        assert_eq!(Document::load(&primary).unwrap().get_pages().len(), 2);
        assert_eq!(Document::load(&imported).unwrap().get_pages().len(), 2);
    }

    #[test]
    fn composed_export_requires_signed_import_acknowledgement() {
        let directory = TestDirectory::new();
        let primary = directory.path.join("primary.pdf");
        let imported = directory.path.join("signed-import.pdf");
        let output = directory.path.join("composed.pdf");
        sample_document(1)
            .save(&primary)
            .unwrap()
            .sync_all()
            .unwrap();
        let mut signed = sample_document(1);
        let signature = signed.add_object(dictionary! {
            "FT" => "Sig",
            "ByteRange" => vec![0.into(), 1.into(), 2.into(), 3.into()],
        });
        signed.catalog_mut().unwrap().set(
            "AcroForm",
            dictionary! { "Fields" => vec![signature.into()] },
        );
        signed.save(&imported).unwrap().sync_all().unwrap();
        let primary_metadata = fs::metadata(&primary).unwrap();
        let imported_metadata = fs::metadata(&imported).unwrap();

        let error = export_composed_pdf(ExportComposedPdfRequest {
            primary_input_path: primary.to_string_lossy().into_owned(),
            primary_input_password: None,
            expected_source_size: primary_metadata.len(),
            expected_source_modified_at_ms: modified_at_ms(&primary_metadata),
            acknowledge_primary_certificate_signature: false,
            imported_sources: vec![ExportPdfSource {
                id: "signed-source".to_string(),
                input_path: imported.to_string_lossy().into_owned(),
                input_password: None,
                expected_source_size: imported_metadata.len(),
                expected_source_modified_at_ms: modified_at_ms(&imported_metadata),
                acknowledge_certificate_signature: false,
            }],
            output_path: output.to_string_lossy().into_owned(),
            pages: vec![ComposedExportPage::Source {
                source_id: "signed-source".to_string(),
                source_page: 1,
                rotation: 0,
            }],
            signature: None,
            visual_signature_assets: Vec::new(),
            visual_signature_placements: Vec::new(),
            document_lock: None,
        })
        .unwrap_err();

        assert!(error.contains("certificate signature"));
        assert!(error.contains("Confirm"));
        assert!(!output.exists());
    }

    #[test]
    fn composed_export_rejects_a_primary_source_changed_after_opening() {
        let directory = TestDirectory::new();
        let input = directory.path.join("primary.pdf");
        let output = directory.path.join("organised.pdf");
        sample_document(2).save(&input).unwrap().sync_all().unwrap();
        let metadata = fs::metadata(&input).unwrap();
        let mut request = composed_primary_request(&input, &output, 2);
        request.expected_source_size = metadata.len();
        request.expected_source_modified_at_ms = modified_at_ms(&metadata);
        let mut bytes = fs::read(&input).unwrap();
        bytes.extend_from_slice(b"\n% changed after opening\n");
        fs::write(&input, bytes).unwrap();

        let error = export_composed_pdf(request).unwrap_err();

        assert!(error.contains("changed after it was opened"));
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_during_composed_page_planning_never_publishes_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("primary.pdf");
        let output = directory.path.join("organised.pdf");
        sample_document(400)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_progress = Arc::clone(&observed);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                observed_for_progress.lock().unwrap().push(progress);
                if progress >= 55 {
                    cancelled_for_progress.store(true, Ordering::Release);
                }
            }),
        );

        let error = export_composed_pdf_with_control(
            composed_primary_request(&input, &output, 400),
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
        let observed = observed.lock().unwrap();
        assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn composed_export_rechecks_sources_after_verification_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("primary.pdf");
        let output = directory.path.join("organised.pdf");
        sample_document(2).save(&input).unwrap().sync_all().unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress >= 98 && !changed_for_progress.swap(true, Ordering::AcqRel) {
                    let mut bytes = fs::read(&input_for_progress).unwrap();
                    bytes.extend_from_slice(b"\n% changed before publication\n");
                    fs::write(&input_for_progress, bytes).unwrap();
                }
            }),
        );

        let error = export_composed_pdf_with_control(
            composed_primary_request(&input, &output, 2),
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed after it was opened"));
        assert!(!output.exists());
    }

    #[test]
    fn composed_request_rejects_overlong_source_password_before_file_work() {
        let mut request = composed_primary_request(
            Path::new("missing-primary.pdf"),
            Path::new("organised.pdf"),
            1,
        );
        request.primary_input_password = Some("p".repeat(MAX_SOURCE_PASSWORD_BYTES + 1));

        let error = validate_composed_pdf_request(&request).unwrap_err();

        assert!(error.contains("1024 UTF-8 bytes"));
    }

    #[test]
    fn legacy_export_command_also_guards_signed_sources() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed-primary.pdf");
        let output = directory.path.join("organised.pdf");
        let mut signed = sample_document(1);
        signed.add_object(dictionary! {
            "FT" => "Sig",
            "ByteRange" => vec![0.into(), 1.into(), 2.into(), 3.into()],
        });
        signed.save(&input).unwrap().sync_all().unwrap();

        let request = |acknowledge_certificate_signature| ExportPdfRequest {
            acknowledge_certificate_signature,
            document_lock: None,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            pages: vec![ExportPage::Source {
                source_page: 1,
                rotation: 0,
            }],
            signature: None,
        };
        let error = export_pdf(request(false)).unwrap_err();
        assert!(error.contains("certificate signature"));
        assert!(!output.exists());

        let result = export_pdf(request(true)).unwrap();
        assert_eq!(result.page_count, 1);
        assert!(output.exists());
    }

    #[test]
    fn flattens_a_transparent_signature_on_a_rotated_page() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("signed.pdf");
        sample_document(1).save(&input).unwrap().sync_all().unwrap();
        let result = export_pdf(ExportPdfRequest {
            acknowledge_certificate_signature: false,
            document_lock: None,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            pages: vec![ExportPage::Source {
                source_page: 1,
                rotation: 90,
            }],
            signature: Some(SignaturePlacement {
                page_number: 1,
                position: SignaturePosition::Right,
                png_data_url: test_signature_data_url(),
            }),
        })
        .unwrap();

        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("not a certificate-backed digital signature")));
        let document = Document::load(&output).unwrap();
        let page_id = *document.get_pages().get(&1).unwrap();
        assert!(page_has_flattened_signature(&document, page_id).unwrap());
        let page = document.get_dictionary(page_id).unwrap();
        let contents = page.get(b"Contents").unwrap().as_array().unwrap();
        assert_eq!(contents.len(), 2);
    }

    #[test]
    fn composed_export_flattens_multiple_reused_marks_and_verifies_exact_counts() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("multi-signed.pdf");
        sample_document(2).save(&input).unwrap().sync_all().unwrap();
        let mut request = composed_primary_request(&input, &output, 2);
        request.pages[1] = ComposedExportPage::Source {
            source_id: PRIMARY_SOURCE_ID.to_string(),
            source_page: 2,
            rotation: 90,
        };
        request.visual_signature_assets = vec![VisualSignatureAsset {
            id: "asset:main".to_string(),
            png_data_url: test_signature_data_url(),
        }];
        request.visual_signature_placements = vec![
            VisualSignaturePlacement {
                id: "placement:first".to_string(),
                asset_id: "asset:main".to_string(),
                page_number: 1,
                left_ratio: 0.1,
                top_ratio: 0.12,
                width_ratio: 0.28,
                rotation_degrees: 15.0,
            },
            VisualSignaturePlacement {
                id: "placement:second".to_string(),
                asset_id: "asset:main".to_string(),
                page_number: 1,
                left_ratio: 0.58,
                top_ratio: 0.72,
                width_ratio: 0.24,
                rotation_degrees: -10.0,
            },
            VisualSignaturePlacement {
                id: "placement:rotated-page".to_string(),
                asset_id: "asset:main".to_string(),
                page_number: 2,
                left_ratio: 0.36,
                top_ratio: 0.38,
                width_ratio: 0.24,
                rotation_degrees: 45.0,
            },
        ];

        let result = export_composed_pdf(request).unwrap();

        assert_eq!(result.page_count, 2);
        let document = Document::load(&output).unwrap();
        let pages = document.get_pages();
        assert_eq!(
            page_flattened_signature_count(&document, *pages.get(&1).unwrap()).unwrap(),
            2
        );
        assert_eq!(
            page_flattened_signature_count(&document, *pages.get(&2).unwrap()).unwrap(),
            1
        );
        let embedded_images = document
            .objects
            .values()
            .filter(|object| {
                matches!(
                    object,
                    Object::Stream(stream)
                        if stream
                            .dict
                            .get(b"TufekciSignature")
                            .is_ok_and(|value| matches!(value, Object::Boolean(true)))
                )
            })
            .count();
        assert_eq!(embedded_images, 1, "the reused PNG should be embedded once");
    }

    #[test]
    fn multi_placement_validation_rejects_mixed_legacy_missing_and_unused_assets() {
        let asset = VisualSignatureAsset {
            id: "asset:main".to_string(),
            png_data_url: test_signature_data_url(),
        };
        let placement = VisualSignaturePlacement {
            id: "placement:one".to_string(),
            asset_id: asset.id.clone(),
            page_number: 1,
            left_ratio: 0.1,
            top_ratio: 0.1,
            width_ratio: 0.25,
            rotation_degrees: 0.0,
        };
        let legacy = SignaturePlacement {
            page_number: 1,
            position: SignaturePosition::Right,
            png_data_url: test_signature_data_url(),
        };
        assert!(validate_visual_signature_request(
            Some(&legacy),
            std::slice::from_ref(&asset),
            std::slice::from_ref(&placement),
            1
        )
        .unwrap_err()
        .contains("not both"));

        let missing = VisualSignaturePlacement {
            asset_id: "asset:missing".to_string(),
            ..placement.clone()
        };
        assert!(validate_visual_signature_request(
            None,
            std::slice::from_ref(&asset),
            &[missing],
            1
        )
        .unwrap_err()
        .contains("missing asset"));

        let unused = VisualSignatureAsset {
            id: "asset:unused".to_string(),
            png_data_url: test_signature_data_url(),
        };
        assert!(
            validate_visual_signature_request(None, &[asset, unused], &[placement], 1)
                .unwrap_err()
                .contains("must be used")
        );
    }

    #[test]
    fn rejects_a_signature_outside_the_exported_pages() {
        let error = validate_signature_request(
            Some(&SignaturePlacement {
                page_number: 2,
                position: SignaturePosition::Left,
                png_data_url: test_signature_data_url(),
            }),
            1,
        )
        .unwrap_err();
        assert!(error.contains("outside the exported page range"));
    }

    #[test]
    fn signature_matrices_remain_upright_for_every_page_rotation() {
        let page = PageBox {
            left: 0.0,
            bottom: 0.0,
            width: 600.0,
            height: 800.0,
        };
        for rotation in [0, 90, 180, 270] {
            let [a, b, c, d, _, _] =
                signature_matrix(page, rotation, 300, 100, SignaturePosition::Centre);
            assert!(a * d - b * c > 0.0);
        }
    }

    #[test]
    fn free_geometry_matrices_remain_non_mirrored_for_every_page_rotation() {
        let page = PageBox {
            left: 12.0,
            bottom: 18.0,
            width: 600.0,
            height: 800.0,
        };
        let image = EmbeddedSignatureImage {
            height: 100,
            image_id: (1, 0),
            width: 300,
        };
        for page_rotation in [0, 90, 180, 270] {
            for mark_rotation in [-135.0, -15.0, 0.0, 45.0, 120.0] {
                let placement = VisualSignaturePlacement {
                    id: "placement:test".to_string(),
                    asset_id: "asset:test".to_string(),
                    page_number: 1,
                    left_ratio: 0.38,
                    top_ratio: 0.42,
                    width_ratio: 0.18,
                    rotation_degrees: mark_rotation,
                };
                let [a, b, c, d, _, _] =
                    visual_signature_matrix(page, page_rotation, image, &placement).unwrap();
                assert!(a * d - b * c > 0.0);
            }
        }
    }

    fn test_signature_data_url() -> String {
        let mut image = image::RgbaImage::from_pixel(120, 40, image::Rgba([0, 0, 0, 0]));
        for x in 8..112 {
            let y = 10 + ((x / 8) % 18);
            for offset in 0..3 {
                image.put_pixel(x, y + offset, image::Rgba([18, 25, 43, 255]));
            }
        }
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(encoded.into_inner())
        )
    }

    fn composed_primary_request(
        input: &Path,
        output: &Path,
        page_count: u32,
    ) -> ExportComposedPdfRequest {
        let metadata = fs::metadata(input).ok();
        ExportComposedPdfRequest {
            acknowledge_primary_certificate_signature: false,
            document_lock: None,
            expected_source_modified_at_ms: metadata.as_ref().and_then(modified_at_ms),
            expected_source_size: metadata.as_ref().map_or(0, fs::Metadata::len),
            imported_sources: Vec::new(),
            output_path: output.to_string_lossy().into_owned(),
            pages: (1..=page_count)
                .map(|source_page| ComposedExportPage::Source {
                    rotation: 0,
                    source_id: PRIMARY_SOURCE_ID.to_string(),
                    source_page,
                })
                .collect(),
            primary_input_password: None,
            primary_input_path: input.to_string_lossy().into_owned(),
            signature: None,
            visual_signature_assets: Vec::new(),
            visual_signature_placements: Vec::new(),
        }
    }

    fn sample_document(page_count: u32) -> Document {
        let mut document = Document::with_version("1.4");
        let pages_id = document.new_object_id();
        let mut kids = Vec::new();

        for _ in 0..page_count {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => Dictionary::new(),
                "Contents" => content_id,
            });
            kids.push(Object::Reference(page_id));
        }

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => page_count as i64,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "tufekci-paperworks-export-test-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
