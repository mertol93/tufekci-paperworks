use crate::file_safety::{
    canonical_pdf_input, reject_control_characters, TemporaryOutput, ValidatedPdfPaths,
};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::privacy::{sanitise_document_for_redaction, verify_redaction_sanitised};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat, ImageReader, Limits, RgbaImage};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, LoadOptions, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PAGE_TREE_DEPTH: usize = 32;
const MAX_PAGES: usize = 20_000;
const MAX_REDACTED_PAGES: usize = 256;
const MAX_REDACTIONS_PER_PAGE: usize = 10_000;
const MAX_TOTAL_REDACTIONS: usize = 100_000;
const MAX_IMAGE_DATA_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_IMAGE_DATA_BYTES: usize = 256 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 8_192;
const MIN_IMAGE_DIMENSION: u32 = 32;
const MAX_IMAGE_ALLOCATION: u64 = 192 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_TOTAL_IMAGE_PIXELS: u64 = 300_000_000;
const MAX_MASK_PIXEL_WRITES_PER_PAGE: u64 = 160_000_000;
const MAX_TOTAL_MASK_PIXEL_WRITES: u64 = 400_000_000;
const MIN_PAGE_DIMENSION_POINTS: f64 = 1.0;
const MAX_PAGE_DIMENSION_POINTS: f64 = 14_400.0;
const MAX_USER_UNIT: f64 = 75_000.0;
const IMAGE_ASPECT_TOLERANCE: f64 = 0.02;
const MIN_REDACTION_REGION_SIZE: f64 = 0.002;
const REDACTION_REGION_BOUND_TOLERANCE: f64 = 1e-9;
const REDACTION_MASK_BLEED_PIXELS: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfRedactionRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfRedactionRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    acknowledge_certificate_signatures: bool,
    pages: Vec<RedactedPageInput>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RedactedPageInput {
    page_number: usize,
    png_data_url: String,
    regions: Vec<RedactionRegionInput>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RedactionRegionInput {
    colour: RedactionRegionColour,
    height: f64,
    width: f64,
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RedactionRegionColour {
    Black,
    White,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfRedactionPageInspection {
    page_number: usize,
    width_pt: f64,
    height_pt: f64,
    rotation: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfRedactionInspection {
    file_name: String,
    source_size: u64,
    source_modified_at_ms: Option<u64>,
    page_count: usize,
    annotation_count: usize,
    has_forms: bool,
    has_bookmarks: bool,
    tagged_pdf: bool,
    certificate_signature: bool,
    was_encrypted: bool,
    pages: Vec<PdfRedactionPageInspection>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfRedactionResult {
    output_path: String,
    page_count: usize,
    redacted_page_count: usize,
    redaction_count: usize,
    raster_pixel_count: u64,
    privacy_structures_removed: usize,
    unreachable_objects_pruned: usize,
    bytes_written: u64,
    encryption: &'static str,
    warnings: Vec<String>,
}

struct LoadedRedactionPdf {
    document: Document,
    page_count: usize,
    was_encrypted: bool,
    certificate_signature: bool,
    has_forms: bool,
    has_bookmarks: bool,
    tagged_pdf: bool,
}

#[derive(Clone, Copy, Debug)]
struct PageGeometry {
    visual_width: f64,
    visual_height: f64,
    rotation: i64,
}

#[derive(Debug)]
struct ExpectedRedactedPage {
    page_number: usize,
    width_pt: f64,
    height_pt: f64,
    image_width: u32,
    image_height: u32,
    image_sha256: [u8; 32],
    redaction_count: usize,
    marker: String,
}

struct PreparedRedactionRaster {
    image_sha256: [u8; 32],
    mask_pixel_writes: u64,
}

struct RedactionRasterPreparation<'a> {
    geometry: PageGeometry,
    image: DynamicImage,
    marker: &'a str,
    regions: &'a [RedactionRegionInput],
    remaining_mask_pixel_writes: u64,
}

#[derive(Clone, Copy, Debug)]
struct PixelRedactionMask {
    bottom: u32,
    colour: [u8; 4],
    left: u32,
    right: u32,
    top: u32,
}

#[cfg(test)]
pub fn inspect_pdf_redaction(
    request: InspectPdfRedactionRequest,
) -> Result<PdfRedactionInspection, String> {
    inspect_pdf_redaction_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_redaction_request(
    request: &InspectPdfRedactionRequest,
) -> Result<(), String> {
    reject_control_characters("Redaction source path", &request.input_path)?;
    validate_password(request.input_password.as_deref())
}

fn inspect_pdf_redaction_with_control(
    request: InspectPdfRedactionRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfRedactionInspection, String> {
    control.checkpoint(2, "Validating redaction review")?;
    validate_inspect_pdf_redaction_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_size = metadata.len();
    let source_modified_at_ms = modified_at_ms(&metadata);
    control.checkpoint(18, "Opening redaction structure")?;
    let loaded = load_pdf(&input, request.input_password.as_deref())?;
    let page_map = loaded.document.get_pages();
    let mut pages = Vec::with_capacity(page_map.len());
    for (index, (page_number, page_id)) in page_map.into_iter().enumerate() {
        checkpoint_redaction_inspection_loop(
            control,
            index,
            loaded.page_count,
            30,
            70,
            "Inspecting redaction page",
        )?;
        let geometry = page_geometry(&loaded.document, page_id)?;
        pages.push(PdfRedactionPageInspection {
            page_number: page_number as usize,
            width_pt: geometry.visual_width,
            height_pt: geometry.visual_height,
            rotation: geometry.rotation,
        });
    }
    let annotation_count = annotation_count_with_control(&loaded.document, control)?;
    let mut warnings = vec![
        "Permanent redaction rasterises every marked page. Searchable text, links, form controls, comments, and accessibility tagging on those pages will be removed."
            .to_string(),
        "The exported copy is privacy-cleaned: metadata, actions, attachments, annotations, forms, bookmarks, named destinations, thumbnails, and document structure are removed throughout the PDF."
            .to_string(),
    ];
    if loaded.was_encrypted {
        warnings.push(
            "Choose AES-256 output protection during export if the redacted copy must remain encrypted."
                .to_string(),
        );
    }
    if loaded.certificate_signature {
        warnings.push(
            "Redaction rewrites this certificate-signed PDF and invalidates its existing signatures."
                .to_string(),
        );
    }

    control.checkpoint(94, "Rechecking redaction source")?;
    verify_source_fingerprint_values(&input, source_size, source_modified_at_ms)?;
    control.checkpoint(99, "Finalising redaction review")?;

    Ok(PdfRedactionInspection {
        file_name: display_name(&input),
        source_size,
        source_modified_at_ms,
        page_count: loaded.page_count,
        annotation_count,
        has_forms: loaded.has_forms,
        has_bookmarks: loaded.has_bookmarks,
        tagged_pdf: loaded.tagged_pdf,
        certificate_signature: loaded.certificate_signature,
        was_encrypted: loaded.was_encrypted,
        pages,
        warnings,
    })
}

pub(crate) fn run_pdf_redaction_inspection_job_with_control(
    request: InspectPdfRedactionRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfRedactionInspection, String> {
    inspect_pdf_redaction_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_redaction_inspection_job_error(&error)
        }
    })
}

fn safe_redaction_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during redaction review. Open it again before marking content."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The redaction PDF could not be opened with the supplied password.".to_string();
    }
    "The redaction review failed a structural safety check. Review the source PDF and try again."
        .to_string()
}

#[cfg(test)]
pub fn export_pdf_redaction(
    request: ExportPdfRedactionRequest,
) -> Result<ExportPdfRedactionResult, String> {
    export_pdf_redaction_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn export_pdf_redaction_with_control(
    mut request: ExportPdfRedactionRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfRedactionResult, String> {
    control.checkpoint(1, "Validating permanent redaction request")?;
    validate_export_pdf_redaction_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    verify_source_fingerprint(&paths.input, &request)?;

    control.checkpoint(7, "Opening reviewed source PDF")?;
    let mut loaded = load_pdf(&paths.input, request.input_password.as_deref())?;
    ensure_document_rewrite_acknowledged(
        &loaded.document,
        &paths.input,
        request.acknowledge_certificate_signatures,
    )?;
    validate_redacted_page_inputs(&mut request.pages, loaded.page_count)?;

    let marker_prefix = redaction_marker()?;
    let page_ids = loaded.document.get_pages();
    let mut expected = Vec::with_capacity(request.pages.len());
    let mut total_pixels = 0_u64;
    let mut total_image_bytes = 0_usize;
    let mut total_mask_pixel_writes = 0_u64;
    let mut total_redactions = 0_usize;

    for (index, page) in request.pages.iter().enumerate() {
        let page_start = progress_between(14, 70, index, request.pages.len());
        let page_end = progress_between(14, 70, index + 1, request.pages.len());
        let page_control = control.subrange(
            page_start,
            page_end,
            format!("Redacted page {}", page.page_number),
        );
        page_control.checkpoint(2, "Checking reviewed page geometry")?;
        let page_number = u32::try_from(page.page_number)
            .map_err(|_| "A redaction page number is too large to process safely.".to_string())?;
        let page_id = *page_ids
            .get(&page_number)
            .ok_or_else(|| format!("Page {} disappeared before export.", page.page_number))?;
        let geometry = page_geometry(&loaded.document, page_id)?;
        page_control.checkpoint(12, "Decoding reviewed lossless page image")?;
        let (image, image_bytes) = decode_redaction_png(&page.png_data_url, page.page_number)?;
        page_control.ensure_not_cancelled()?;
        total_image_bytes = total_image_bytes
            .checked_add(image_bytes)
            .ok_or_else(|| "The redaction images are too large to process safely.".to_string())?;
        if total_image_bytes > MAX_TOTAL_IMAGE_DATA_BYTES {
            return Err(format!(
                "Redaction images can contain at most {} MiB of PNG data in one export.",
                MAX_TOTAL_IMAGE_DATA_BYTES / (1024 * 1024)
            ));
        }
        let image_width = image.width();
        let image_height = image.height();
        let pixels = u64::from(image_width) * u64::from(image_height);
        total_pixels = total_pixels
            .checked_add(pixels)
            .ok_or_else(|| "The redaction raster is too large to process safely.".to_string())?;
        if total_pixels > MAX_TOTAL_IMAGE_PIXELS {
            return Err(format!(
                "Redaction images can contain at most {MAX_TOTAL_IMAGE_PIXELS} pixels in one export. Reduce the raster quality or redact fewer pages at once."
            ));
        }
        validate_image_aspect(image_width, image_height, geometry, page.page_number)?;

        let marker = format!("{marker_prefix}-{}-{}", page.page_number, index + 1);
        let raster_control = page_control.subrange(20, 96, "Image-only replacement".to_string());
        let prepared_raster = replace_page_with_raster(
            &mut loaded.document,
            page_id,
            RedactionRasterPreparation {
                geometry,
                image,
                marker: &marker,
                regions: &page.regions,
                remaining_mask_pixel_writes: MAX_TOTAL_MASK_PIXEL_WRITES
                    .saturating_sub(total_mask_pixel_writes),
            },
            &raster_control,
        )?;
        total_mask_pixel_writes = total_mask_pixel_writes
            .checked_add(prepared_raster.mask_pixel_writes)
            .ok_or_else(|| "The redaction mask work is too large to process safely.".to_string())?;
        total_redactions += page.regions.len();
        expected.push(ExpectedRedactedPage {
            page_number: page.page_number,
            width_pt: geometry.visual_width,
            height_pt: geometry.visual_height,
            image_width,
            image_height,
            image_sha256: prepared_raster.image_sha256,
            redaction_count: page.regions.len(),
            marker,
        });
        page_control.checkpoint(100, "Reviewed page replaced")?;
    }

    control.checkpoint(73, "Removing private and interactive document structures")?;
    let sanitised = sanitise_document_for_redaction(&mut loaded.document);
    control.checkpoint(80, "Writing prepared redacted PDF")?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    loaded
        .document
        .save(prepared.path())
        .map_err(|error| format!("The redacted PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| format!("The redacted PDF could not be flushed to storage: {error}"))?;

    control.checkpoint(83, "Reopening prepared redacted PDF")?;
    let verification = Document::load_with_options(
        prepared.path(),
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The redacted PDF failed its reopening check: {error}"))?;
    let verification_control =
        control.subrange(84, 89, "Prepared redaction verification".to_string());
    verify_redacted_pdf_with_control(
        &verification,
        loaded.page_count,
        &expected,
        &verification_control,
    )?;

    let protected = if let Some(protection) = request.output_protection.as_ref() {
        let protected = TemporaryOutput::new(&paths.output)?;
        let protection_control =
            control.subrange(90, 95, "Applying AES-256 output protection".to_string());
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            &protection_control,
        )?;
        control.checkpoint(95, "Opening protected redacted PDF for verification")?;
        let mut protected_verification = Document::load_with_options(
            protected.path(),
            LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
        )
        .map_err(|error| format!("The protected redacted PDF could not be reopened: {error}"))?;
        if !protected_verification.is_encrypted() {
            return Err(
                "The protected redacted PDF did not contain AES-256 encryption and was not saved."
                    .to_string(),
            );
        }
        protected_verification
            .decrypt(&protection.open_password)
            .map_err(|_| {
                "The protected redacted PDF could not be decrypted for verification.".to_string()
            })?;
        let protected_control =
            control.subrange(96, 98, "Protected redaction verification".to_string());
        verify_redacted_pdf_with_control(
            &protected_verification,
            loaded.page_count,
            &expected,
            &protected_control,
        )?;
        Some(protected)
    } else {
        None
    };
    let final_output = protected.as_ref().unwrap_or(&prepared);

    control.checkpoint(98, "Rechecking reviewed source before publication")?;
    verify_source_fingerprint(&paths.input, &request)?;
    control.checkpoint(99, "Publishing verified redacted PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let mut warnings = vec![
        format!(
            "{} page{} {} flattened to reviewed raster artwork with {} native-applied permanent redaction region{}.",
            expected.len(),
            if expected.len() == 1 { "" } else { "s" },
            if expected.len() == 1 { "was" } else { "were" },
            total_redactions,
            if total_redactions == 1 { "" } else { "s" }
        ),
        "Searchable text and accessibility information are intentionally absent from redacted pages. Run OCR only if you accept recreating text outside the covered regions."
            .to_string(),
        "Privacy cleaning removed interactive and hidden document structures from the entire exported copy."
            .to_string(),
    ];
    if request.output_protection.is_some() {
        warnings.push(
            "The redacted copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if loaded.was_encrypted {
        warnings.push(
            "The redacted copy is not password-protected. Use Protect to apply new encryption."
                .to_string(),
        );
    }
    if loaded.certificate_signature {
        warnings.push(
            "Redaction changed the PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }

    Ok(ExportPdfRedactionResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: loaded.page_count,
        redacted_page_count: expected.len(),
        redaction_count: total_redactions,
        raster_pixel_count: total_pixels,
        privacy_structures_removed: sanitised.removed_structures,
        unreachable_objects_pruned: sanitised.pruned_objects,
        bytes_written,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_redaction_job_with_control(
    request: ExportPdfRedactionRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfRedactionResult, String> {
    export_pdf_redaction_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_redaction_job_error(&error)
        }
    })
}

fn safe_redaction_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed after review. Review its redactions again before exporting."
            .to_string();
    }
    if normalised.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before exporting permanent redaction."
            .to_string();
    }
    if normalised.contains("qpdf") || normalised.contains("aes-256") {
        return "AES-256 redaction protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The redaction PDF could not be opened or protected with the supplied passwords."
            .to_string();
    }
    "Permanent redaction failed a structural or privacy safety check. Review the redactions and try again."
        .to_string()
}

pub(crate) fn validate_export_pdf_redaction_request(
    request: &ExportPdfRedactionRequest,
) -> Result<(), String> {
    reject_control_characters("Input path", &request.input_path)?;
    reject_control_characters("Output path", &request.output_path)?;
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    validate_redacted_page_input_shape(&request.pages)
}

fn verify_source_fingerprint(
    input: &Path,
    request: &ExportPdfRedactionRequest,
) -> Result<(), String> {
    verify_source_fingerprint_values(
        input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )
}

fn verify_source_fingerprint_values(
    input: &Path,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
) -> Result<(), String> {
    let metadata = fs::metadata(input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    if !metadata.is_file()
        || metadata.len() != expected_source_size
        || modified_at_ms(&metadata) != expected_source_modified_at_ms
    {
        return Err(
            "The source PDF changed on disk after its redactions were reviewed. Review it again before exporting."
                .to_string(),
        );
    }
    Ok(())
}

fn validate_password(password: Option<&str>) -> Result<(), String> {
    if password.is_some_and(|value| value.len() > MAX_PASSWORD_BYTES) {
        return Err("The source password is too long to process safely.".to_string());
    }
    Ok(())
}

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedRedactionPdf, String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The source PDF could not be parsed: {error}"))?;
    let was_encrypted = document.is_encrypted() || document.was_encrypted();
    if document.is_encrypted() {
        document
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "The PDF could not be decrypted for redaction. Check its password.".to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    if page_count > MAX_PAGES {
        return Err(format!(
            "Redaction supports at most {MAX_PAGES} pages in one PDF."
        ));
    }
    let catalogue = document.catalog().ok();
    let has_forms = catalogue.is_some_and(|value| value.has(b"AcroForm"));
    let has_bookmarks = catalogue.is_some_and(|value| value.has(b"Outlines"));
    let tagged_pdf =
        catalogue.is_some_and(|value| value.has(b"StructTreeRoot") || value.has(b"MarkInfo"));
    let certificate_signature = document_has_certificate_signature(&document);
    Ok(LoadedRedactionPdf {
        document,
        page_count,
        was_encrypted,
        certificate_signature,
        has_forms,
        has_bookmarks,
        tagged_pdf,
    })
}

fn validate_redacted_page_inputs(
    pages: &mut [RedactedPageInput],
    page_count: usize,
) -> Result<(), String> {
    validate_redacted_page_input_shape(pages)?;
    pages.sort_by_key(|page| page.page_number);
    for page in pages {
        if page.page_number > page_count {
            return Err(format!(
                "Redaction page {} is outside this PDF.",
                page.page_number
            ));
        }
    }
    Ok(())
}

fn validate_redacted_page_input_shape(pages: &[RedactedPageInput]) -> Result<(), String> {
    if pages.is_empty() {
        return Err("Add at least one redaction region before exporting.".to_string());
    }
    if pages.len() > MAX_REDACTED_PAGES {
        return Err(format!(
            "One export can rasterise at most {MAX_REDACTED_PAGES} redacted pages."
        ));
    }
    let mut seen = HashSet::with_capacity(pages.len());
    let mut total_redactions = 0_usize;
    let mut encoded_bytes = 0_usize;
    for page in pages {
        if page.page_number == 0 {
            return Err("Redaction page numbers begin at 1.".to_string());
        }
        if !seen.insert(page.page_number) {
            return Err(format!(
                "Page {} was supplied more than once for redaction.",
                page.page_number
            ));
        }
        if page.regions.is_empty() || page.regions.len() > MAX_REDACTIONS_PER_PAGE {
            return Err(format!(
                "Page {} must contain between 1 and {MAX_REDACTIONS_PER_PAGE} redaction regions.",
                page.page_number
            ));
        }
        for (region_index, region) in page.regions.iter().enumerate() {
            validate_redaction_region(region, page.page_number, region_index + 1)?;
        }
        total_redactions = total_redactions
            .checked_add(page.regions.len())
            .ok_or_else(|| "The redaction count is too large to process safely.".to_string())?;
        if total_redactions > MAX_TOTAL_REDACTIONS {
            return Err(format!(
                "One export can contain at most {MAX_TOTAL_REDACTIONS} redaction regions."
            ));
        }
        encoded_bytes = encoded_bytes
            .checked_add(page.png_data_url.len())
            .ok_or_else(|| "The redaction images are too large to process safely.".to_string())?;
        if encoded_bytes > MAX_TOTAL_IMAGE_DATA_BYTES.saturating_mul(4) / 3 + 1_024 {
            return Err(format!(
                "Redaction images can contain at most {} MiB of PNG data in one export.",
                MAX_TOTAL_IMAGE_DATA_BYTES / (1024 * 1024)
            ));
        }
    }
    Ok(())
}

fn validate_redaction_region(
    region: &RedactionRegionInput,
    page_number: usize,
    region_number: usize,
) -> Result<(), String> {
    if ![region.x, region.y, region.width, region.height]
        .into_iter()
        .all(f64::is_finite)
    {
        return Err(format!(
            "Redaction region {region_number} on page {page_number} contains a non-finite coordinate."
        ));
    }
    if region.x < 0.0
        || region.y < 0.0
        || region.width < MIN_REDACTION_REGION_SIZE
        || region.height < MIN_REDACTION_REGION_SIZE
        || region.x > 1.0
        || region.y > 1.0
        || region.x + region.width > 1.0 + REDACTION_REGION_BOUND_TOLERANCE
        || region.y + region.height > 1.0 + REDACTION_REGION_BOUND_TOLERANCE
    {
        return Err(format!(
            "Redaction region {region_number} on page {page_number} must be a bounded normalised rectangle at least {MIN_REDACTION_REGION_SIZE} wide and high."
        ));
    }
    Ok(())
}

fn decode_redaction_png(
    data_url: &str,
    page_number: usize,
) -> Result<(DynamicImage, usize), String> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| format!("The reviewed image for page {page_number} is not PNG data."))?;
    if encoded.is_empty() || encoded.len() > MAX_IMAGE_DATA_BYTES.saturating_mul(4) / 3 + 8 {
        return Err(format!(
            "The reviewed image for page {page_number} is empty or too large to export safely."
        ));
    }
    let bytes = BASE64_STANDARD.decode(encoded).map_err(|_| {
        format!("The reviewed image for page {page_number} is not valid base64 data.")
    })?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_DATA_BYTES {
        return Err(format!(
            "The reviewed image for page {page_number} is empty or too large to export safely."
        ));
    }
    let byte_count = bytes.len();
    let image = match catch_unwind(AssertUnwindSafe(|| -> Result<DynamicImage, String> {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_IMAGE_ALLOCATION);
        reader.limits(limits);
        reader.decode().map_err(|error| {
            format!("The reviewed PNG for page {page_number} could not be decoded: {error}")
        })
    })) {
        Ok(result) => result?,
        Err(_) => {
            return Err(format!(
                "The reviewed PNG for page {page_number} was rejected safely."
            ))
        }
    };
    if image.width() < MIN_IMAGE_DIMENSION || image.height() < MIN_IMAGE_DIMENSION {
        return Err(format!(
            "The reviewed image for page {page_number} is too small for dependable redaction."
        ));
    }
    let pixels = u64::from(image.width()) * u64::from(image.height());
    if pixels > MAX_IMAGE_PIXELS {
        return Err(format!(
            "The reviewed image for page {page_number} exceeds the {MAX_IMAGE_PIXELS}-pixel safety limit."
        ));
    }
    Ok((image, byte_count))
}

fn validate_image_aspect(
    width: u32,
    height: u32,
    geometry: PageGeometry,
    page_number: usize,
) -> Result<(), String> {
    let image_ratio = f64::from(width) / f64::from(height);
    let page_ratio = geometry.visual_width / geometry.visual_height;
    let difference = ((image_ratio / page_ratio) - 1.0).abs();
    if difference > IMAGE_ASPECT_TOLERANCE {
        return Err(format!(
            "The reviewed image for page {page_number} does not match the page shape. Review the PDF again before exporting."
        ));
    }
    Ok(())
}

fn replace_page_with_raster(
    document: &mut Document,
    page_id: ObjectId,
    preparation: RedactionRasterPreparation<'_>,
    control: &PdfJobExecutionControl,
) -> Result<PreparedRedactionRaster, String> {
    let RedactionRasterPreparation {
        geometry,
        image,
        marker,
        regions,
        remaining_mask_pixel_writes,
    } = preparation;
    control.checkpoint(2, "Preparing image-only page")?;
    let parent = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A redacted page dictionary is invalid: {error}"))?
        .get(b"Parent")
        .cloned()
        .map_err(|_| "A redacted page has no valid page-tree parent.".to_string())?;
    let mut rgba = image.to_rgba8();
    let mask_pixel_writes =
        apply_redaction_masks(&mut rgba, regions, remaining_mask_pixel_writes, control)?;
    let mut rgb = Vec::with_capacity(rgba.width() as usize * rgba.height() as usize * 3);
    let pixel_count = u64::from(rgba.width()) * u64::from(rgba.height());
    for (pixel_index, pixel) in rgba.pixels().enumerate() {
        if pixel_index == 0 || pixel_index % 65_536 == 0 {
            control.checkpoint(
                progress_between(48, 82, pixel_index, pixel_count as usize),
                "Flattening reviewed page pixels",
            )?;
        }
        let alpha = u32::from(pixel[3]);
        for channel in &pixel.0[..3] {
            let flattened = (u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255;
            rgb.push(flattened as u8);
        }
    }
    let image_sha256 = Sha256::digest(&rgb).into();
    control.checkpoint(86, "Compressing reviewed page image")?;
    let mut image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(rgba.width()),
            "Height" => i64::from(rgba.height()),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Interpolate" => false,
            "TufekciRedactionImage" => Object::string_literal(marker),
        },
        rgb,
    );
    image_stream
        .compress()
        .map_err(|error| format!("A redacted page image could not be compressed: {error}"))?;
    let image_id = document.add_object(image_stream);
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    pdf_real(geometry.visual_width),
                    0.into(),
                    0.into(),
                    pdf_real(geometry.visual_height),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new(
                "Do",
                vec![Object::Name(b"TufekciRedactedPageImage".to_vec())],
            ),
            Operation::new("Q", vec![]),
        ],
    }
    .encode()
    .map_err(|error| format!("A redacted page could not be encoded: {error}"))?;
    let content_id = document.add_object(Stream::new(
        dictionary! {
            "TufekciRedactionContent" => Object::string_literal(marker),
        },
        content,
    ));
    let mut xobjects = Dictionary::new();
    xobjects.set("TufekciRedactedPageImage", image_id);
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => parent,
            "MediaBox" => vec![0.into(), 0.into(), pdf_real(geometry.visual_width), pdf_real(geometry.visual_height)],
            "CropBox" => vec![0.into(), 0.into(), pdf_real(geometry.visual_width), pdf_real(geometry.visual_height)],
            "Resources" => dictionary! { "XObject" => xobjects },
            "Contents" => content_id,
            "TufekciRedactedPage" => Object::string_literal(marker),
            "TufekciRedactionCount" => regions.len() as i64,
        }),
    );
    control.checkpoint(100, "Image-only page prepared")?;
    Ok(PreparedRedactionRaster {
        image_sha256,
        mask_pixel_writes,
    })
}

fn apply_redaction_masks(
    image: &mut RgbaImage,
    regions: &[RedactionRegionInput],
    remaining_mask_pixel_writes: u64,
    control: &PdfJobExecutionControl,
) -> Result<u64, String> {
    let mut masks = Vec::with_capacity(regions.len());
    let mut mask_pixel_writes = 0_u64;
    for (index, region) in regions.iter().enumerate() {
        if index == 0 || index % 256 == 0 {
            control.checkpoint(
                progress_between(4, 12, index, regions.len()),
                "Validating native redaction masks",
            )?;
        }
        let mask = pixel_redaction_mask(region, image.width(), image.height());
        let mask_pixels = u64::from(mask.right - mask.left)
            .checked_mul(u64::from(mask.bottom - mask.top))
            .ok_or_else(|| "A redaction mask is too large to process safely.".to_string())?;
        mask_pixel_writes = mask_pixel_writes
            .checked_add(mask_pixels)
            .ok_or_else(|| "The redaction mask work is too large to process safely.".to_string())?;
        if mask_pixel_writes > MAX_MASK_PIXEL_WRITES_PER_PAGE {
            return Err(format!(
                "One page can contain at most {MAX_MASK_PIXEL_WRITES_PER_PAGE} redaction mask pixel writes. Reduce overlapping regions or the raster resolution."
            ));
        }
        if mask_pixel_writes > remaining_mask_pixel_writes {
            return Err(format!(
                "One export can contain at most {MAX_TOTAL_MASK_PIXEL_WRITES} redaction mask pixel writes. Reduce overlapping regions or the raster resolution."
            ));
        }
        masks.push(mask);
    }

    let width = image.width() as usize;
    let pixels = image.as_mut();
    for (mask_index, mask) in masks.iter().enumerate() {
        if mask_index == 0 || mask_index % 64 == 0 {
            control.checkpoint(
                progress_between(14, 46, mask_index, masks.len()),
                "Applying native redaction masks",
            )?;
        }
        for y in mask.top..mask.bottom {
            if y == mask.top || y % 512 == 0 {
                control.ensure_not_cancelled()?;
            }
            let row = y as usize * width;
            for x in mask.left..mask.right {
                let offset = (row + x as usize) * 4;
                pixels[offset..offset + 4].copy_from_slice(&mask.colour);
            }
        }
    }
    control.checkpoint(47, "Native redaction masks applied")?;
    Ok(mask_pixel_writes)
}

fn pixel_redaction_mask(
    region: &RedactionRegionInput,
    image_width: u32,
    image_height: u32,
) -> PixelRedactionMask {
    let width = f64::from(image_width);
    let height = f64::from(image_height);
    let left = ((region.x * width).floor() as u32).saturating_sub(REDACTION_MASK_BLEED_PIXELS);
    let top = ((region.y * height).floor() as u32).saturating_sub(REDACTION_MASK_BLEED_PIXELS);
    let right = (((region.x + region.width).min(1.0) * width).ceil() as u32)
        .saturating_add(REDACTION_MASK_BLEED_PIXELS)
        .min(image_width);
    let bottom = (((region.y + region.height).min(1.0) * height).ceil() as u32)
        .saturating_add(REDACTION_MASK_BLEED_PIXELS)
        .min(image_height);
    PixelRedactionMask {
        bottom: bottom.max((top + 1).min(image_height)),
        colour: match region.colour {
            RedactionRegionColour::Black => [0, 0, 0, 255],
            RedactionRegionColour::White => [255, 255, 255, 255],
        },
        left,
        right: right.max((left + 1).min(image_width)),
        top,
    }
}

fn verify_redacted_pdf_with_control(
    document: &Document,
    expected_page_count: usize,
    expected: &[ExpectedRedactedPage],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.checkpoint(2, "Checking redacted document structure")?;
    if document.is_encrypted() {
        return Err(
            "The redacted PDF unexpectedly remained encrypted and was not saved.".to_string(),
        );
    }
    let pages = document.get_pages();
    if pages.len() != expected_page_count {
        return Err("The redacted PDF changed the page count and was not saved.".to_string());
    }
    verify_redaction_sanitised(document)?;

    for (page_index, page) in expected.iter().enumerate() {
        control.checkpoint(
            progress_between(8, 96, page_index, expected.len()),
            format!(
                "Verifying redacted page {} of {}",
                page_index + 1,
                expected.len()
            ),
        )?;
        let page_id = *pages.get(&(page.page_number as u32)).ok_or_else(|| {
            format!(
                "Redacted page {} was lost during verification.",
                page.page_number
            )
        })?;
        let dictionary = document
            .get_dictionary(page_id)
            .map_err(|error| format!("Redacted page {} is invalid: {error}", page.page_number))?;
        if !dictionary_string_matches(dictionary, b"TufekciRedactedPage", &page.marker)
            || dictionary
                .get(b"TufekciRedactionCount")
                .and_then(Object::as_i64)
                .ok()
                != Some(page.redaction_count as i64)
        {
            return Err(format!(
                "Redacted page {} lost its verification marker and was not saved.",
                page.page_number
            ));
        }
        for key in [
            b"Annots".as_slice(),
            b"AA".as_slice(),
            b"Metadata".as_slice(),
            b"PieceInfo".as_slice(),
            b"Rotate".as_slice(),
            b"StructParent".as_slice(),
            b"StructParents".as_slice(),
            b"Thumb".as_slice(),
            b"UserUnit".as_slice(),
        ] {
            if dictionary.has(key) {
                return Err(format!(
                    "Redacted page {} retained private page data and was not saved.",
                    page.page_number
                ));
            }
        }
        verify_page_box(
            document,
            dictionary,
            b"MediaBox",
            page.width_pt,
            page.height_pt,
            page.page_number,
        )?;
        verify_page_box(
            document,
            dictionary,
            b"CropBox",
            page.width_pt,
            page.height_pt,
            page.page_number,
        )?;

        let content = resolve_dictionary_value(document, dictionary, b"Contents")?;
        let Object::Stream(content_stream) = content else {
            return Err(format!(
                "Redacted page {} does not contain one verified content stream.",
                page.page_number
            ));
        };
        if !dictionary_string_matches(
            &content_stream.dict,
            b"TufekciRedactionContent",
            &page.marker,
        ) {
            return Err(format!(
                "Redacted page {} retained an unverified content stream.",
                page.page_number
            ));
        }
        verify_image_only_content(content_stream, page.page_number)?;

        let resources = resolve_dictionary_value(document, dictionary, b"Resources")?
            .as_dict()
            .map_err(|_| format!("Redacted page {} has invalid resources.", page.page_number))?;
        if resources.iter().count() != 1 || !resources.has(b"XObject") {
            return Err(format!(
                "Redacted page {} retained non-image resources and was not saved.",
                page.page_number
            ));
        }
        let xobjects = resolve_dictionary_value(document, resources, b"XObject")?
            .as_dict()
            .map_err(|_| {
                format!(
                    "Redacted page {} has invalid image resources.",
                    page.page_number
                )
            })?;
        if xobjects.iter().count() != 1 {
            return Err(format!(
                "Redacted page {} does not contain exactly one reviewed image.",
                page.page_number
            ));
        }
        let image = resolve_dictionary_value(document, xobjects, b"TufekciRedactedPageImage")?;
        let Object::Stream(image_stream) = image else {
            return Err(format!(
                "Redacted page {} has an invalid reviewed image.",
                page.page_number
            ));
        };
        if !dictionary_string_matches(&image_stream.dict, b"TufekciRedactionImage", &page.marker)
            || image_stream
                .dict
                .get(b"Subtype")
                .and_then(Object::as_name)
                .ok()
                != Some(b"Image")
            || image_stream
                .dict
                .get(b"Width")
                .and_then(Object::as_i64)
                .ok()
                != Some(i64::from(page.image_width))
            || image_stream
                .dict
                .get(b"Height")
                .and_then(Object::as_i64)
                .ok()
                != Some(i64::from(page.image_height))
        {
            return Err(format!(
                "Redacted page {} failed its reviewed-image verification.",
                page.page_number
            ));
        }
        let expected_bytes = u64::from(page.image_width) * u64::from(page.image_height) * 3;
        let image_bytes = image_stream.decompressed_content().map_err(|error| {
            format!(
                "The reviewed image on page {} could not be verified: {error}",
                page.page_number
            )
        })?;
        if image_bytes.len() as u64 != expected_bytes {
            return Err(format!(
                "The reviewed image on page {} has an unexpected size and was not saved.",
                page.page_number
            ));
        }
        let image_sha256: [u8; 32] = Sha256::digest(&image_bytes).into();
        if image_sha256 != page.image_sha256 {
            return Err(format!(
                "The reviewed image pixels on page {} changed after native redaction and the PDF was not saved.",
                page.page_number
            ));
        }
        let extracted = document
            .extract_text(&[page.page_number as u32])
            .map_err(|error| {
                format!(
                    "Redacted page {} failed its searchable-text check: {error}",
                    page.page_number
                )
            })?;
        if !extracted.trim().is_empty() {
            return Err(format!(
                "Redacted page {} still exposes searchable text and was not saved.",
                page.page_number
            ));
        }
    }
    control.checkpoint(100, "Permanent redaction verified")?;
    Ok(())
}

fn verify_image_only_content(stream: &Stream, page_number: usize) -> Result<(), String> {
    let bytes = stream
        .decompressed_content()
        .map_err(|error| format!("Redacted page {page_number} content is invalid: {error}"))?;
    let content = Content::decode(&bytes)
        .map_err(|error| format!("Redacted page {page_number} content is invalid: {error}"))?;
    let operators = content
        .operations
        .iter()
        .map(|operation| operation.operator.as_str())
        .collect::<Vec<_>>();
    if operators != ["q", "cm", "Do", "Q"]
        || content.operations[2].operands.as_slice()
            != [Object::Name(b"TufekciRedactedPageImage".to_vec())]
    {
        return Err(format!(
            "Redacted page {page_number} contains content other than its reviewed image."
        ));
    }
    Ok(())
}

fn verify_page_box(
    document: &Document,
    dictionary: &Dictionary,
    key: &[u8],
    width: f64,
    height: f64,
    page_number: usize,
) -> Result<(), String> {
    let value = resolve_dictionary_value(document, dictionary, key)?;
    let values = value
        .as_array()
        .map_err(|_| format!("Redacted page {page_number} has an invalid page box."))?;
    if values.len() != 4 {
        return Err(format!(
            "Redacted page {page_number} has an invalid page box."
        ));
    }
    let coordinates = values
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, String>>()?;
    if coordinates[0].abs() > 0.05
        || coordinates[1].abs() > 0.05
        || (coordinates[2] - width).abs() > 0.05
        || (coordinates[3] - height).abs() > 0.05
    {
        return Err(format!(
            "Redacted page {page_number} changed shape during verification."
        ));
    }
    Ok(())
}

fn dictionary_string_matches(dictionary: &Dictionary, key: &[u8], expected: &str) -> bool {
    dictionary.get(key).ok().is_some_and(
        |value| matches!(value, Object::String(bytes, _) if bytes == expected.as_bytes()),
    )
}

fn resolve_dictionary_value<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Result<&'a Object, String> {
    let value = dictionary.get(key).map_err(|_| {
        format!(
            "The PDF is missing the {} entry.",
            String::from_utf8_lossy(key)
        )
    })?;
    resolve_object(document, value)
}

fn resolve_object<'a>(document: &'a Document, value: &'a Object) -> Result<&'a Object, String> {
    match value {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| format!("The PDF contains a broken object reference: {error}")),
        _ => Ok(value),
    }
}

fn annotation_count_with_control(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<usize, String> {
    let pages = document.get_pages();
    let page_count = pages.len();
    let mut count = 0_usize;
    for (index, page_id) in pages.into_values().enumerate() {
        checkpoint_redaction_inspection_loop(
            control,
            index,
            page_count,
            72,
            88,
            "Inspecting redaction annotations on page",
        )?;
        let page = document
            .get_dictionary(page_id)
            .map_err(|error| format!("A PDF page dictionary is invalid: {error}"))?;
        let Some(annots) = page.get(b"Annots").ok() else {
            continue;
        };
        let page_annotation_count = resolve_object(document, annots)?
            .as_array()
            .map(|values| values.len())
            .map_err(|_| "A PDF page contains an invalid annotation array.".to_string())?;
        count = count.checked_add(page_annotation_count).ok_or_else(|| {
            "The PDF annotation count is too large to inspect safely.".to_string()
        })?;
    }
    Ok(count)
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<PageGeometry, String> {
    let page_box = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "A PDF page does not define a crop or media box.".to_string())?;
    let values = resolve_object(document, &page_box)?
        .as_array()
        .map_err(|_| "A PDF page has an invalid page box.".to_string())?;
    if values.len() != 4 {
        return Err("A PDF page box must contain four coordinates.".to_string());
    }
    let coordinates = values
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, String>>()?;
    let user_unit = match inherited_page_value(document, page_id, b"UserUnit")? {
        Some(value) => pdf_number_value(resolve_object(document, &value)?)?,
        None => 1.0,
    };
    if !user_unit.is_finite() || user_unit <= 0.0 || user_unit > MAX_USER_UNIT {
        return Err("A PDF page has an unsupported user-unit scale.".to_string());
    }
    let width = (coordinates[2] - coordinates[0]).abs() * user_unit;
    let height = (coordinates[3] - coordinates[1]).abs() * user_unit;
    let rotation = match inherited_page_value(document, page_id, b"Rotate")? {
        Some(value) => resolve_object(document, &value)?
            .as_i64()
            .map_err(|_| "A PDF page has an invalid rotation.".to_string())?,
        None => 0,
    }
    .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("A PDF page has an unsupported rotation.".to_string());
    }
    let (visual_width, visual_height) = if matches!(rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    };
    for dimension in [visual_width, visual_height] {
        if !dimension.is_finite()
            || !(MIN_PAGE_DIMENSION_POINTS..=MAX_PAGE_DIMENSION_POINTS).contains(&dimension)
        {
            return Err(format!(
                "A PDF page dimension is outside the supported {MIN_PAGE_DIMENSION_POINTS} to {MAX_PAGE_DIMENSION_POINTS} point range."
            ));
        }
    }
    Ok(PageGeometry {
        visual_width,
        visual_height,
        rotation,
    })
}

fn inherited_page_value(
    document: &Document,
    start_id: ObjectId,
    key: &[u8],
) -> Result<Option<Object>, String> {
    let mut current_id = start_id;
    let mut visited = HashSet::new();
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        if !visited.insert(current_id) {
            return Err("The PDF page tree contains a cycle.".to_string());
        }
        let dictionary = document
            .get_dictionary(current_id)
            .map_err(|error| format!("The PDF page tree is invalid: {error}"))?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value.clone()));
        }
        match dictionary.get(b"Parent") {
            Ok(Object::Reference(parent_id)) => current_id = *parent_id,
            Err(_) => return Ok(None),
            _ => return Err("The PDF page tree has an invalid parent reference.".to_string()),
        }
    }
    Err("The PDF page tree is too deeply nested.".to_string())
}

fn pdf_number_value(value: &Object) -> Result<f64, String> {
    match value {
        Object::Integer(value) => Ok(*value as f64),
        Object::Real(value) => Ok(f64::from(*value)),
        _ => Err("The PDF contains a non-numeric page coordinate.".to_string()),
    }
}

fn redaction_marker() -> Result<String, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("The system clock is invalid: {error}"))?
        .as_nanos();
    Ok(format!("TufekciRedaction-{}-{nonce}", std::process::id()))
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Document.pdf")
        .to_string()
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

fn checkpoint_redaction_inspection_loop(
    control: &PdfJobExecutionControl,
    index: usize,
    total: usize,
    start: u8,
    end: u8,
    stage: &str,
) -> Result<(), String> {
    if !index.is_multiple_of(16) && index + 1 != total {
        return control.ensure_not_cancelled();
    }
    control.checkpoint(
        progress_between(start, end, index + 1, total),
        format!("{stage} {} of {total}", index + 1),
    )
}

fn pdf_real(value: f64) -> Object {
    Object::Real(value as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_control::PDF_JOB_CANCELLED_ERROR;
    use image::{ImageBuffer, Rgba};
    use lopdf::{text_string, StringFormat};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn inspects_rotated_pages_and_private_structures() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect_pdf_redaction(InspectPdfRedactionRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(inspection.page_count, 2);
        assert_eq!(inspection.pages[0].width_pt, 800.0);
        assert_eq!(inspection.pages[0].height_pt, 600.0);
        assert_eq!(inspection.pages[0].rotation, 90);
        assert!(inspection.has_forms);
        assert!(inspection.has_bookmarks);
        assert!(inspection.tagged_pdf);
        assert_eq!(inspection.annotation_count, 1);
    }

    #[test]
    fn controlled_redaction_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let stages = Arc::new(Mutex::new(Vec::new()));
        let stages_for_progress = Arc::clone(&stages);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            stages_for_progress.lock().unwrap().push((value, stage));
        });

        let inspection = run_pdf_redaction_inspection_job_with_control(
            InspectPdfRedactionRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap();

        assert_eq!(inspection.page_count, 2);
        let stages = stages.lock().unwrap();
        assert!(stages
            .iter()
            .any(|(_, stage)| stage == "Inspecting redaction page 1 of 2"));
        assert!(stages
            .iter()
            .any(|(_, stage)| stage == "Inspecting redaction annotations on page 1 of 2"));
        drop(stages);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let cancel_progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Inspecting redaction page 1 of 2" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = run_pdf_redaction_inspection_job_with_control(
            InspectPdfRedactionRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(cancelled, cancel_progress),
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn redaction_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-redaction-review.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking redaction source"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during redaction review\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });

        let error = run_pdf_redaction_inspection_job_with_control(
            InspectPdfRedactionRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert_eq!(
            error,
            "The source PDF changed during redaction review. Open it again before marking content."
        );
        assert!(!error.contains("private-redaction-review.pdf"));
    }

    #[test]
    fn applies_native_masks_then_verifies_and_scrubs_the_selected_page() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("redacted.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);

        let result = export_pdf_redaction(request(
            &input,
            &output,
            &inspection,
            vec![RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(80, 60),
                regions: test_regions(2),
            }],
        ))
        .unwrap();

        assert_eq!(result.redacted_page_count, 1);
        assert_eq!(result.redaction_count, 2);
        assert_eq!(result.raster_pixel_count, 4_800);
        assert_eq!(result.encryption, "None");
        assert!(result.privacy_structures_removed > 0);
        let reopened = Document::load(&output).unwrap();
        verify_redaction_sanitised(&reopened).unwrap();
        let pages = reopened.get_pages();
        let first = reopened.get_dictionary(pages[&1]).unwrap();
        assert!(first.has(b"TufekciRedactedPage"));
        assert!(!first.has(b"Rotate"));
        assert!(reopened.extract_text(&[1]).unwrap().trim().is_empty());
        let (image_width, image_height, image_rgb) = redacted_image_rgb(&reopened, 1);
        assert_eq!((image_width, image_height), (80, 60));
        assert_eq!(rgb_pixel(&image_rgb, image_width, 10, 10), [0, 0, 0]);
        assert_eq!(rgb_pixel(&image_rgb, image_width, 50, 40), [255, 255, 255]);
        let second = reopened.get_dictionary(pages[&2]).unwrap();
        assert!(!second.has(b"TufekciRedactedPage"));
        assert_eq!(Document::load(&input).unwrap().get_pages().len(), 2);
    }

    #[test]
    fn rejects_duplicate_pages_and_wrong_raster_shape() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);

        let duplicate_output = directory.path.join("duplicate.pdf");
        let error = export_pdf_redaction(request(
            &input,
            &duplicate_output,
            &inspection,
            vec![
                RedactedPageInput {
                    page_number: 1,
                    png_data_url: test_png(80, 60),
                    regions: test_regions(1),
                },
                RedactedPageInput {
                    page_number: 1,
                    png_data_url: test_png(80, 60),
                    regions: test_regions(1),
                },
            ],
        ))
        .unwrap_err();
        assert!(error.contains("more than once"));

        let shape_output = directory.path.join("shape.pdf");
        let error = export_pdf_redaction(request(
            &input,
            &shape_output,
            &inspection,
            vec![RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(60, 80),
                regions: test_regions(1),
            }],
        ))
        .unwrap_err();
        assert!(error.contains("does not match the page shape"));
    }

    #[test]
    fn rejects_empty_non_finite_tiny_and_out_of_bounds_native_regions() {
        let empty = [RedactedPageInput {
            page_number: 1,
            png_data_url: test_png(80, 60),
            regions: Vec::new(),
        }];
        assert!(validate_redacted_page_input_shape(&empty)
            .unwrap_err()
            .contains("between 1"));

        for region in [
            RedactionRegionInput {
                x: f64::NAN,
                ..test_region(RedactionRegionColour::Black, 0.1, 0.1, 0.2, 0.2)
            },
            test_region(RedactionRegionColour::Black, 0.1, 0.1, 0.001, 0.2),
            test_region(RedactionRegionColour::Black, 0.9, 0.1, 0.2, 0.2),
        ] {
            let pages = [RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(80, 60),
                regions: vec![region],
            }];
            assert!(validate_redacted_page_input_shape(&pages).is_err());
        }

        let legacy_error = serde_json::from_value::<RedactedPageInput>(serde_json::json!({
            "pageNumber": 1,
            "pngDataUrl": test_png(80, 60),
            "redactionCount": 1
        }))
        .unwrap_err();
        assert!(legacy_error.to_string().contains("unknown field"));
    }

    #[test]
    fn native_masks_expand_one_pixel_and_later_regions_win_overlap() {
        let mut image = RgbaImage::from_pixel(100, 100, Rgba([220, 30, 60, 255]));
        let regions = vec![
            test_region(RedactionRegionColour::Black, 0.2, 0.2, 0.4, 0.4),
            test_region(RedactionRegionColour::White, 0.3, 0.3, 0.1, 0.1),
        ];
        let writes = apply_redaction_masks(
            &mut image,
            &regions,
            MAX_TOTAL_MASK_PIXEL_WRITES,
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();

        assert!(writes > 40 * 40);
        assert_eq!(image.get_pixel(19, 19).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(25, 25).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(35, 35).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(10, 10).0, [220, 30, 60, 255]);
    }

    #[test]
    fn reopened_verifier_rejects_a_single_changed_raster_byte() {
        let mut document = fixture(false);
        let page_count = document.get_pages().len();
        let page_id = document.get_pages()[&1];
        let geometry = page_geometry(&document, page_id).unwrap();
        let regions = test_regions(2);
        let marker = "native-redaction-digest-test".to_string();
        let (image, _) = decode_redaction_png(&test_png(80, 60), 1).unwrap();
        let prepared = replace_page_with_raster(
            &mut document,
            page_id,
            RedactionRasterPreparation {
                geometry,
                image,
                marker: &marker,
                regions: &regions,
                remaining_mask_pixel_writes: MAX_TOTAL_MASK_PIXEL_WRITES,
            },
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();
        sanitise_document_for_redaction(&mut document);
        let expected = [ExpectedRedactedPage {
            page_number: 1,
            width_pt: geometry.visual_width,
            height_pt: geometry.visual_height,
            image_width: 80,
            image_height: 60,
            image_sha256: prepared.image_sha256,
            redaction_count: regions.len(),
            marker,
        }];
        verify_redacted_pdf_with_control(
            &document,
            page_count,
            &expected,
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();

        let image_id = redacted_image_id(&document, 1);
        let image_stream = document
            .get_object_mut(image_id)
            .unwrap()
            .as_stream_mut()
            .unwrap();
        let mut pixels = image_stream.decompressed_content().unwrap();
        pixels[0] ^= 1;
        image_stream.set_plain_content(pixels);

        let error = verify_redacted_pdf_with_control(
            &document,
            page_count,
            &expected,
            &PdfJobExecutionControl::direct(),
        )
        .unwrap_err();
        assert!(error.contains("image pixels"));
    }

    #[test]
    fn requires_certificate_acknowledgement() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed.pdf");
        let output = directory.path.join("redacted.pdf");
        fixture(true).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let mut export_request = request(
            &input,
            &output,
            &inspection,
            vec![RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(80, 60),
                regions: test_regions(1),
            }],
        );
        let error = export_pdf_redaction(export_request).unwrap_err();
        assert!(error.contains("certificate signature"));
        assert!(!output.exists());

        export_request = request(
            &input,
            &output,
            &inspection,
            vec![RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(80, 60),
                regions: test_regions(1),
            }],
        );
        export_request.acknowledge_certificate_signatures = true;
        assert!(export_pdf_redaction(export_request).is_ok());
    }

    #[test]
    fn rejects_changed_source() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("redacted.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let mut file = fs::OpenOptions::new().append(true).open(&input).unwrap();
        file.write_all(b" ").unwrap();
        file.sync_all().unwrap();

        let error = export_pdf_redaction(request(
            &input,
            &output,
            &inspection,
            vec![RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(80, 60),
                regions: test_regions(1),
            }],
        ))
        .unwrap_err();
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_during_page_flattening_never_publishes_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("redacted.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_progress = Arc::clone(&observed);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                observed_for_progress.lock().unwrap().push(progress);
                if progress >= 30 {
                    cancelled_for_progress.store(true, Ordering::Release);
                }
            }),
        );

        let error = export_pdf_redaction_with_control(
            request(
                &input,
                &output,
                &inspection,
                vec![RedactedPageInput {
                    page_number: 1,
                    png_data_url: test_png(1_600, 1_200),
                    regions: test_regions(1),
                }],
            ),
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
        let observed = observed.lock().unwrap();
        assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn source_is_rechecked_after_redaction_verification_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("redacted.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress >= 97 && !changed_for_progress.swap(true, Ordering::AcqRel) {
                    let mut file = fs::OpenOptions::new()
                        .append(true)
                        .open(&input_for_progress)
                        .unwrap();
                    file.write_all(b" changed before publication").unwrap();
                    file.sync_all().unwrap();
                }
            }),
        );

        let error = export_pdf_redaction_with_control(
            request(
                &input,
                &output,
                &inspection,
                vec![RedactedPageInput {
                    page_number: 1,
                    png_data_url: test_png(80, 60),
                    regions: test_regions(1),
                }],
            ),
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn never_overwrites_source_or_existing_destination() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let pages = || {
            vec![RedactedPageInput {
                page_number: 1,
                png_data_url: test_png(80, 60),
                regions: test_regions(1),
            }]
        };

        let error =
            export_pdf_redaction(request(&input, &input, &inspection, pages())).unwrap_err();
        assert!(error.contains("already exists") || error.contains("cannot be overwritten"));

        let existing = directory.path.join("existing.pdf");
        fixture(false).save(&existing).unwrap().sync_all().unwrap();
        let error =
            export_pdf_redaction(request(&input, &existing, &inspection, pages())).unwrap_err();
        assert!(error.contains("already exists"));
    }

    fn inspection(path: &Path) -> PdfRedactionInspection {
        inspect_pdf_redaction(InspectPdfRedactionRequest {
            input_path: path.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap()
    }

    fn request(
        input: &Path,
        output: &Path,
        inspection: &PdfRedactionInspection,
        pages: Vec<RedactedPageInput>,
    ) -> ExportPdfRedactionRequest {
        ExportPdfRedactionRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            acknowledge_certificate_signatures: false,
            pages,
            output_protection: None,
        }
    }

    fn test_png(width: u32, height: u32) -> String {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(width, height, |x, y| {
            if x > width / 3 && y > height / 3 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        }));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(bytes.into_inner())
        )
    }

    fn test_regions(count: usize) -> Vec<RedactionRegionInput> {
        [
            test_region(RedactionRegionColour::Black, 0.1, 0.1, 0.2, 0.2),
            test_region(RedactionRegionColour::White, 0.55, 0.55, 0.25, 0.25),
        ]
        .into_iter()
        .cycle()
        .take(count)
        .collect()
    }

    fn test_region(
        colour: RedactionRegionColour,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> RedactionRegionInput {
        RedactionRegionInput {
            colour,
            height,
            width,
            x,
            y,
        }
    }

    fn redacted_image_rgb(document: &Document, page_number: u32) -> (u32, u32, Vec<u8>) {
        let pages = document.get_pages();
        let page = document.get_dictionary(pages[&page_number]).unwrap();
        let resources = resolve_dictionary_value(document, page, b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let xobjects = resolve_dictionary_value(document, resources, b"XObject")
            .unwrap()
            .as_dict()
            .unwrap();
        let image = resolve_dictionary_value(document, xobjects, b"TufekciRedactedPageImage")
            .unwrap()
            .as_stream()
            .unwrap();
        let width = image.dict.get(b"Width").unwrap().as_i64().unwrap() as u32;
        let height = image.dict.get(b"Height").unwrap().as_i64().unwrap() as u32;
        (width, height, image.decompressed_content().unwrap())
    }

    fn redacted_image_id(document: &Document, page_number: u32) -> ObjectId {
        let pages = document.get_pages();
        let page = document.get_dictionary(pages[&page_number]).unwrap();
        let resources = resolve_dictionary_value(document, page, b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let xobjects = resolve_dictionary_value(document, resources, b"XObject")
            .unwrap()
            .as_dict()
            .unwrap();
        match xobjects.get(b"TufekciRedactedPageImage").unwrap() {
            Object::Reference(id) => *id,
            _ => panic!("The redacted image was not stored indirectly."),
        }
    }

    fn rgb_pixel(bytes: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
        let offset = ((y * width + x) * 3) as usize;
        bytes[offset..offset + 3].try_into().unwrap()
    }

    fn fixture(signed: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let first_content = document.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 72 720 Td (TOP SECRET) Tj ET".to_vec(),
        ));
        let second_content = document.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 72 720 Td (PUBLIC) Tj ET".to_vec(),
        ));
        let first_page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => first_content,
            "Rotate" => 90,
            "StructParents" => 0,
        });
        let annotation = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
            "Contents" => text_string("private note"),
        });
        let second_page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => second_content,
            "Annots" => vec![Object::Reference(annotation)],
        });
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![first_page.into(), second_page.into()],
                "Count" => 2,
                "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            }),
        );

        let metadata = document.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<private>history</private>".to_vec(),
        ));
        let field = if signed {
            document.add_object(dictionary! {
                "FT" => "Sig",
                "T" => text_string("certificate"),
                "V" => dictionary! {
                    "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                    "Contents" => Object::String(vec![1, 2, 3], StringFormat::Hexadecimal),
                },
            })
        } else {
            document.add_object(dictionary! { "FT" => "Tx", "T" => text_string("name") })
        };
        let structure_root = document.add_object(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => vec![Object::Dictionary(dictionary! { "S" => "P", "Alt" => text_string("hidden text") })],
        });
        let catalogue_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => dictionary! { "Fields" => vec![Object::Reference(field)] },
            "Outlines" => dictionary! { "Type" => "Outlines", "Count" => 0 },
            "Names" => dictionary! { "Dests" => dictionary! { "Names" => Vec::<Object>::new() } },
            "Metadata" => metadata,
            "MarkInfo" => dictionary! { "Marked" => true },
            "StructTreeRoot" => structure_root,
            "OpenAction" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert('private')") },
        });
        let info = document.add_object(dictionary! { "Author" => text_string("Private Author") });
        document.trailer.set("Root", catalogue_id);
        document.trailer.set("Info", info);
        document.trailer.set(
            "ID",
            vec![
                Object::string_literal("private-one"),
                Object::string_literal("private-two"),
            ],
        );
        document
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory("tufekci-redaction-test");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
