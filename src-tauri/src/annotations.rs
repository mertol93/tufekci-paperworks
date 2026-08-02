use crate::file_safety::{
    canonical_pdf_input, reject_control_characters, TemporaryOutput, ValidatedPdfPaths,
};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use lopdf::content::{Content, Operation};
use lopdf::{
    decode_text_string, dictionary, text_string, Dictionary, Document, LoadOptions, Object,
    ObjectId, Stream, StringFormat,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::UNIX_EPOCH;

const MAX_ANNOTATIONS: usize = 2_000;
const MAX_ANNOTATIONS_PER_PAGE: usize = 500;
const MAX_POINTS_PER_ANNOTATION: usize = 10_000;
const MAX_TOTAL_POINTS: usize = 250_000;
const MAX_TEXT_CHARACTERS: usize = 4_096;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 96;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_IMAGE_DATA_BYTES: usize = 12 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 8_192;
const MAX_EMBEDDED_IMAGE_DIMENSION: u32 = 2_048;
const MAX_IMAGE_ALLOCATION: u64 = 128 * 1024 * 1024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PAGE_TREE_DEPTH: usize = 32;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfAnnotationsRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfAnnotationsRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
    acknowledge_certificate_signatures: bool,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    #[serde(default)]
    annotations: Vec<PdfAnnotationDraft>,
    #[serde(default)]
    updated_annotations: Vec<PdfAnnotationDraft>,
    #[serde(default)]
    removed_existing_annotation_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum AnnotationKind {
    Text,
    Highlight,
    Stamp,
    Freehand,
    Rectangle,
    Ellipse,
    Line,
    Image,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalisedPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalisedRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfAnnotationDraft {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_annotation_id: Option<String>,
    page_number: usize,
    kind: AnnotationKind,
    rect: Option<NormalisedRect>,
    start: Option<NormalisedPoint>,
    end: Option<NormalisedPoint>,
    #[serde(default)]
    points: Vec<NormalisedPoint>,
    colour: [f32; 3],
    fill_colour: Option<[f32; 3]>,
    opacity: f32,
    line_width: f32,
    font_size: f32,
    text: Option<String>,
    stamp: Option<String>,
    image_data_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EditablePdfAnnotation {
    #[serde(flatten)]
    annotation: PdfAnnotationDraft,
    viewer_annotation_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfAnnotationInspection {
    file_name: String,
    source_size: u64,
    source_modified_at_ms: Option<u64>,
    page_count: usize,
    existing_annotation_count: usize,
    annotations_per_page: Vec<usize>,
    editable_annotation_count: usize,
    editable_annotations_per_page: Vec<usize>,
    read_only_annotation_count: usize,
    read_only_annotations_per_page: Vec<usize>,
    editable_annotations: Vec<EditablePdfAnnotation>,
    was_encrypted: bool,
    certificate_signature: bool,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfAnnotationsResult {
    output_path: String,
    page_count: usize,
    added_annotation_count: usize,
    updated_annotation_count: usize,
    removed_annotation_count: usize,
    total_annotation_count: usize,
    bytes_written: u64,
    encryption: &'static str,
    warnings: Vec<String>,
}

struct LoadedAnnotationsPdf {
    document: Document,
    page_count: usize,
    was_encrypted: bool,
}

#[derive(Clone, Copy, Debug)]
struct PageBox {
    left: f64,
    bottom: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Copy, Debug)]
struct PageGeometry {
    page: PageBox,
    rotation: i64,
    visual_width: f64,
    visual_height: f64,
}

#[derive(Clone, Copy, Debug)]
struct PdfPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct PdfRect {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

#[derive(Clone, Copy, Debug)]
struct MappedRect {
    pdf: PdfRect,
    visual_left: f64,
    visual_top: f64,
    visual_width: f64,
    visual_height: f64,
}

#[derive(Debug)]
struct ExpectedAnnotation {
    marker: String,
    page_number: usize,
    subtype: &'static [u8],
    image: bool,
}

#[derive(Clone, Debug)]
struct ExistingAnnotationSource {
    source_id: String,
    viewer_annotation_id: String,
    page_number: usize,
    annotation_index: usize,
    dictionary: Dictionary,
    editable: Option<PdfAnnotationDraft>,
}

struct AnnotationVerificationExpectations<'a> {
    encrypted: bool,
    page_count: usize,
    form_fields: bool,
    original_counts: &'a [usize],
    added_per_page: &'a [usize],
    removed_per_page: &'a [usize],
    expected_annotations: &'a [ExpectedAnnotation],
}

#[cfg(test)]
pub fn inspect_pdf_annotations(
    request: InspectPdfAnnotationsRequest,
) -> Result<PdfAnnotationInspection, String> {
    inspect_pdf_annotations_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_annotations_request(
    request: &InspectPdfAnnotationsRequest,
) -> Result<(), String> {
    reject_control_characters("Annotation source path", &request.input_path)?;
    validate_password(request.input_password.as_deref())
}

fn inspect_pdf_annotations_with_control(
    request: InspectPdfAnnotationsRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfAnnotationInspection, String> {
    control.checkpoint(2, "Validating annotation review")?;
    validate_inspect_pdf_annotations_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_size = metadata.len();
    let source_modified_at_ms = modified_at_ms(&metadata);
    control.checkpoint(18, "Opening annotation structure")?;
    let loaded = load_pdf(&input, request.input_password.as_deref())?;
    control.checkpoint(42, "Inspecting page annotations")?;
    let annotations_per_page = annotation_counts_with_control(&loaded.document, control)?;
    let existing_annotation_count: usize = annotations_per_page.iter().sum();
    control.checkpoint(85, "Identifying editable annotations")?;
    let existing_sources = editable_existing_annotation_sources(&loaded.document, control)?;
    let editable_annotations = existing_sources
        .iter()
        .filter_map(|source| {
            source
                .editable
                .clone()
                .map(|annotation| EditablePdfAnnotation {
                    annotation,
                    viewer_annotation_id: source.viewer_annotation_id.clone(),
                })
        })
        .collect::<Vec<_>>();
    let editable_annotation_count = editable_annotations.len();
    let mut editable_annotations_per_page = vec![0_usize; loaded.page_count];
    for annotation in &editable_annotations {
        editable_annotations_per_page[annotation.annotation.page_number - 1] += 1;
    }
    let read_only_annotations_per_page = annotations_per_page
        .iter()
        .zip(&editable_annotations_per_page)
        .map(|(total, editable)| total.saturating_sub(*editable))
        .collect::<Vec<_>>();
    let read_only_annotation_count =
        existing_annotation_count.saturating_sub(editable_annotation_count);
    control.checkpoint(86, "Checking certificate signatures")?;
    let certificate_signature = document_has_certificate_signature(&loaded.document);
    let mut warnings = Vec::new();
    if editable_annotation_count > 0 {
        warnings.push(format!(
            "{editable_annotation_count} existing standard annotation{} can be moved, restyled, duplicated, or deleted in this workspace.",
            if editable_annotation_count == 1 { "" } else { "s" }
        ));
    }
    if read_only_annotation_count > 0 {
        warnings.push(format!(
            "{read_only_annotation_count} existing annotation{} {} unsupported, structurally complex, or beyond workspace limits. {} remain visible and are preserved read-only.",
            if read_only_annotation_count == 1 { "" } else { "s" },
            if read_only_annotation_count == 1 { "is" } else { "are" },
            if read_only_annotation_count == 1 { "It" } else { "They" }
        ));
    }
    if certificate_signature {
        warnings.push(
            "Editing annotations rewrites this certificate-signed PDF and invalidates its existing signatures."
                .to_string(),
        );
    }

    verify_source_fingerprint(&input, source_size, source_modified_at_ms)?;
    control.checkpoint(99, "Finalising annotation review")?;
    Ok(PdfAnnotationInspection {
        file_name: display_name(&input),
        source_size,
        source_modified_at_ms,
        page_count: loaded.page_count,
        existing_annotation_count,
        annotations_per_page,
        editable_annotation_count,
        editable_annotations_per_page,
        read_only_annotation_count,
        read_only_annotations_per_page,
        editable_annotations,
        was_encrypted: loaded.was_encrypted,
        certificate_signature,
        warnings,
    })
}

pub(crate) fn run_pdf_annotation_inspection_job_with_control(
    request: InspectPdfAnnotationsRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfAnnotationInspection, String> {
    inspect_pdf_annotations_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_annotation_inspection_job_error(&error)
        }
    })
}

fn safe_annotation_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during annotation review. Open it again before editing."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The annotation PDF could not be opened with the supplied password.".to_string();
    }
    "The annotation review failed a structural safety check. Review the source PDF and try again."
        .to_string()
}

#[cfg(test)]
pub fn export_pdf_annotations(
    request: ExportPdfAnnotationsRequest,
) -> Result<ExportPdfAnnotationsResult, String> {
    export_pdf_annotations_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_export_pdf_annotations_request(
    request: &ExportPdfAnnotationsRequest,
) -> Result<(), String> {
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    validate_annotation_request_bounds(
        &request.annotations,
        &request.updated_annotations,
        &request.removed_existing_annotation_ids,
    )
}

pub(crate) fn export_pdf_annotations_with_control(
    request: ExportPdfAnnotationsRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfAnnotationsResult, String> {
    control.checkpoint(1, "Validating annotation export")?;
    validate_export_pdf_annotations_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;

    control.checkpoint(8, "Opening and decrypting source PDF")?;
    let mut loaded = load_pdf(&paths.input, request.input_password.as_deref())?;
    control.checkpoint(16, "Checking document rewrite safety")?;
    ensure_document_rewrite_acknowledged(
        &loaded.document,
        &paths.input,
        request.acknowledge_certificate_signatures,
    )?;
    let had_certificate_signature = document_has_certificate_signature(&loaded.document);
    let had_form_fields = loaded
        .document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"AcroForm"));
    let original_counts = annotation_counts(&loaded.document)?;
    control.checkpoint(22, "Validating annotation changes")?;
    let added_annotation_count = request.annotations.len();
    let updated_annotation_count = request.updated_annotations.len();
    let removed_annotation_count = request.removed_existing_annotation_ids.len();
    let mut combined_annotations = request.annotations;
    combined_annotations.extend(request.updated_annotations);
    let mut updated_annotations =
        validate_annotations_with_control(combined_annotations, loaded.page_count, control)?;
    let annotations = updated_annotations
        .drain(..added_annotation_count)
        .collect::<Vec<_>>();
    let existing_sources = editable_existing_annotation_sources(&loaded.document, control)?;
    let existing_by_id = existing_sources
        .iter()
        .filter(|source| source.editable.is_some())
        .map(|source| (source.source_id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let removal_targets = validate_existing_annotation_changes(
        &annotations,
        &updated_annotations,
        &request.removed_existing_annotation_ids,
        &existing_by_id,
    )?;
    let pages = loaded.document.get_pages();
    remove_existing_annotations(&mut loaded.document, &pages, &removal_targets)?;

    let needs_font = annotations
        .iter()
        .chain(&updated_annotations)
        .any(|annotation| {
            matches!(
                annotation.kind,
                AnnotationKind::Text | AnnotationKind::Stamp
            )
        });
    let font_id = needs_font.then(|| {
        loaded.document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        })
    });
    let operation_count = annotations.len() + updated_annotations.len();
    let mut expected = Vec::with_capacity(operation_count);
    let mut substituted_text_appearances = 0_usize;

    for (index, annotation) in annotations.iter().chain(&updated_annotations).enumerate() {
        checkpoint_annotation_loop(
            control,
            index,
            operation_count,
            34,
            58,
            "Building annotation appearances",
        )?;
        let page_id = pages
            .get(&u32::try_from(annotation.page_number).map_err(|_| {
                "An annotation page number is too large to process safely.".to_string()
            })?)
            .copied()
            .ok_or_else(|| "An annotation refers to a missing page.".to_string())?;
        let geometry = page_geometry(&loaded.document, page_id)?;
        let annotation_id = loaded.document.new_object_id();
        let marker = format!(
            "TufekciPaperworksAnnotation:{}:{}",
            annotation_id.0,
            index + 1
        );
        let (mut dictionary, subtype, substituted) = build_annotation(
            &mut loaded.document,
            page_id,
            geometry,
            annotation,
            &marker,
            font_id,
        )?;
        if let Some(source_id) = annotation.source_annotation_id.as_deref() {
            let source = existing_by_id.get(source_id).ok_or_else(|| {
                "An annotation update no longer matches the reviewed source.".to_string()
            })?;
            preserve_annotation_metadata(&mut dictionary, &source.dictionary);
        }
        substituted_text_appearances += usize::from(substituted);
        loaded
            .document
            .objects
            .insert(annotation_id, Object::Dictionary(dictionary));
        append_page_annotation(&mut loaded.document, page_id, annotation_id)?;
        expected.push(ExpectedAnnotation {
            marker,
            page_number: annotation.page_number,
            subtype,
            image: annotation.kind == AnnotationKind::Image,
        });
    }

    let mut added_per_page = vec![0_usize; loaded.page_count];
    for annotation in &annotations {
        added_per_page[annotation.page_number - 1] += 1;
    }
    let mut removed_per_page = vec![0_usize; loaded.page_count];
    for source_id in &request.removed_existing_annotation_ids {
        let source = existing_by_id.get(source_id.as_str()).ok_or_else(|| {
            "An annotation removal no longer matches the reviewed source.".to_string()
        })?;
        removed_per_page[source.page_number - 1] += 1;
    }

    loaded.document.prune_objects();
    loaded.document.change_producer("Tüfekci Paperworks");
    control.checkpoint(60, "Writing prepared annotated PDF")?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    loaded
        .document
        .save(prepared.path())
        .map_err(|error| format!("The annotated PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| format!("The annotated PDF could not be flushed to storage: {error}"))?;

    control.checkpoint(68, "Verifying prepared annotation structure")?;
    let prepared_expectations = AnnotationVerificationExpectations {
        encrypted: false,
        page_count: loaded.page_count,
        form_fields: had_form_fields,
        original_counts: &original_counts,
        added_per_page: &added_per_page,
        removed_per_page: &removed_per_page,
        expected_annotations: &expected,
    };
    let prepared_counts = verify_annotated_pdf(prepared.path(), None, &prepared_expectations)?;
    let protected = if let Some(protection) = request.output_protection.as_ref() {
        control.checkpoint(76, "Applying AES-256 output protection")?;
        let protected = TemporaryOutput::new(&paths.output)?;
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            control,
        )?;
        control.checkpoint(88, "Verifying protected annotation structure")?;
        let protected_expectations = AnnotationVerificationExpectations {
            encrypted: true,
            ..prepared_expectations
        };
        verify_annotated_pdf(
            protected.path(),
            Some(&protection.open_password),
            &protected_expectations,
        )?;
        Some(protected)
    } else {
        None
    };
    let final_output = protected.as_ref().unwrap_or(&prepared);
    control.checkpoint(94, "Rechecking source PDF before publication")?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(99, "Publishing verified annotated PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let total_annotation_count = prepared_counts.iter().sum();
    let mut warnings = vec![
        "New and updated items use standard PDF annotations and remain editable in compatible readers. Unsupported existing annotations are preserved unchanged."
            .to_string(),
    ];
    if substituted_text_appearances > 0 {
        warnings.push(format!(
            "{substituted_text_appearances} text appearance{} contained characters outside the built-in Windows Latin font. The full Unicode text remains in the annotation contents, while unsupported appearance glyphs use question marks.",
            if substituted_text_appearances == 1 { "" } else { "s" }
        ));
    }
    if request.output_protection.is_some() {
        warnings.push(
            "The annotated copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if loaded.was_encrypted {
        warnings.push(
            "The annotated copy is not password-protected. Use Protect to apply new encryption."
                .to_string(),
        );
    }
    if had_certificate_signature {
        warnings.push(
            "Annotation editing changed the PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }

    Ok(ExportPdfAnnotationsResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: loaded.page_count,
        added_annotation_count,
        updated_annotation_count,
        removed_annotation_count,
        total_annotation_count,
        bytes_written,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_annotations_job_with_control(
    request: ExportPdfAnnotationsRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfAnnotationsResult, String> {
    export_pdf_annotations_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_annotation_job_error(&error)
        }
    })
}

fn safe_annotation_job_error(error: &str) -> String {
    if error.contains("changed on disk") {
        return "The source PDF changed after review. Review its annotations again before exporting."
            .to_string();
    }
    if error.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before exporting annotations."
            .to_string();
    }
    if error.contains("QPDF") {
        return "AES-256 annotation protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if error.to_ascii_lowercase().contains("password")
        || error.to_ascii_lowercase().contains("decrypt")
    {
        return "The annotation PDF could not be opened or protected with the supplied passwords."
            .to_string();
    }
    if error.contains("destination already exists") {
        return "The destination already exists. Choose a new filename.".to_string();
    }
    if error.contains("cannot be overwritten") {
        return "The source PDF cannot be overwritten. Choose a new filename.".to_string();
    }
    "The annotation export failed a structural safety check. Review the annotations and try again."
        .to_string()
}

fn verify_annotated_pdf(
    path: &Path,
    password: Option<&str>,
    expected: &AnnotationVerificationExpectations<'_>,
) -> Result<Vec<usize>, String> {
    let mut verification = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The annotated PDF failed its reopening check: {error}"))?;
    let encrypted = verification.is_encrypted();
    if encrypted != expected.encrypted {
        return Err(if expected.encrypted {
            "The annotated PDF was not encrypted as requested and was not saved.".to_string()
        } else {
            "The annotated PDF unexpectedly remained encrypted and was not saved.".to_string()
        });
    }
    if encrypted {
        verification
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "The protected annotated PDF could not be reopened with its new password."
                    .to_string()
            })?;
    }
    if verification.get_pages().len() != expected.page_count {
        return Err("The annotated PDF changed the page count and was not saved.".to_string());
    }
    if expected.form_fields
        && !verification
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        return Err("The annotated PDF lost its form structure and was not saved.".to_string());
    }
    let verified_counts = annotation_counts(&verification)?;
    for (page_index, original_count) in expected.original_counts.iter().enumerate() {
        let expected_count = original_count
            .saturating_add(expected.added_per_page[page_index])
            .saturating_sub(expected.removed_per_page[page_index]);
        if verified_counts[page_index] != expected_count {
            return Err(format!(
                "Page {} contains {} annotations after export; {} were expected. The PDF was not saved.",
                page_index + 1,
                verified_counts[page_index],
                expected_count,
            ));
        }
    }
    verify_expected_annotations(&verification, expected.expected_annotations)?;
    Ok(verified_counts)
}

fn verify_source_fingerprint(
    path: &Path,
    expected_size: u64,
    expected_modified_at_ms: Option<u64>,
) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("The source PDF could not be rechecked: {error}"))?;
    if metadata.len() != expected_size || modified_at_ms(&metadata) != expected_modified_at_ms {
        return Err(
            "The source PDF changed on disk after its annotations were reviewed. Review it again before exporting."
                .to_string(),
        );
    }
    Ok(())
}

fn validate_annotation_request_bounds(
    annotations: &[PdfAnnotationDraft],
    updated_annotations: &[PdfAnnotationDraft],
    removed_existing_annotation_ids: &[String],
) -> Result<(), String> {
    let annotation_operations = annotations.len().saturating_add(updated_annotations.len());
    let total_operations =
        annotation_operations.saturating_add(removed_existing_annotation_ids.len());
    if total_operations == 0 {
        return Err("Change at least one annotation before exporting a new copy.".to_string());
    }
    if total_operations > MAX_ANNOTATIONS {
        return Err(format!(
            "An annotation export can contain at most {MAX_ANNOTATIONS} changes."
        ));
    }
    let mut total_points = 0_usize;
    for annotation in annotations.iter().chain(updated_annotations) {
        if annotation.points.len() > MAX_POINTS_PER_ANNOTATION {
            return Err(format!(
                "A freehand annotation can contain at most {MAX_POINTS_PER_ANNOTATION} points."
            ));
        }
        total_points = total_points.saturating_add(annotation.points.len());
        if total_points > MAX_TOTAL_POINTS {
            return Err(format!(
                "The annotation set can contain at most {MAX_TOTAL_POINTS} freehand points."
            ));
        }
        if annotation
            .image_data_url
            .as_ref()
            .is_some_and(|data| data.len() > MAX_IMAGE_DATA_BYTES * 2)
        {
            return Err("An annotation image is too large.".to_string());
        }
    }
    for source_id in removed_existing_annotation_ids {
        validate_source_annotation_id(source_id)?;
    }
    Ok(())
}

fn validate_source_annotation_id(source_id: &str) -> Result<(), String> {
    if source_id.is_empty()
        || source_id.len() > MAX_IDENTIFIER_BYTES
        || !source_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        return Err("An existing annotation has an invalid source identity.".to_string());
    }
    Ok(())
}

fn validate_password(password: Option<&str>) -> Result<(), String> {
    if password.is_some_and(|value| value.len() > MAX_PASSWORD_BYTES) {
        return Err("The source password is too long to process safely.".to_string());
    }
    Ok(())
}

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedAnnotationsPdf, String> {
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
                "The PDF could not be decrypted for annotation editing. Check its password."
                    .to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    Ok(LoadedAnnotationsPdf {
        document,
        page_count,
        was_encrypted,
    })
}

#[cfg(test)]
fn validate_annotations(
    annotations: Vec<PdfAnnotationDraft>,
    page_count: usize,
) -> Result<Vec<PdfAnnotationDraft>, String> {
    validate_annotations_with_control(annotations, page_count, &PdfJobExecutionControl::direct())
}

fn validate_annotations_with_control(
    annotations: Vec<PdfAnnotationDraft>,
    page_count: usize,
    control: &PdfJobExecutionControl,
) -> Result<Vec<PdfAnnotationDraft>, String> {
    if annotations.len() > MAX_ANNOTATIONS {
        return Err(format!(
            "A document can receive at most {MAX_ANNOTATIONS} new annotations in one export."
        ));
    }
    let mut identifiers = HashSet::new();
    let mut per_page = vec![0_usize; page_count];
    let mut total_points = 0_usize;
    let mut validated = Vec::with_capacity(annotations.len());

    let annotation_count = annotations.len();
    for (index, mut annotation) in annotations.into_iter().enumerate() {
        checkpoint_annotation_loop(
            control,
            index,
            annotation_count,
            24,
            32,
            "Validating annotation data",
        )?;
        let number = index + 1;
        annotation.id = annotation.id.trim().to_string();
        if annotation.id.is_empty()
            || annotation.id.len() > MAX_IDENTIFIER_BYTES
            || !annotation
                .id
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
        {
            return Err(format!(
                "Annotation {number} has an invalid local identifier."
            ));
        }
        if !identifiers.insert(annotation.id.clone()) {
            return Err(format!(
                "Annotation {number} repeats another annotation identifier."
            ));
        }
        if let Some(source_id) = annotation.source_annotation_id.as_mut() {
            *source_id = source_id.trim().to_string();
            validate_source_annotation_id(source_id)?;
        }
        if annotation.page_number == 0 || annotation.page_number > page_count {
            return Err(format!(
                "Annotation {number} refers to a page outside this PDF."
            ));
        }
        per_page[annotation.page_number - 1] += 1;
        if per_page[annotation.page_number - 1] > MAX_ANNOTATIONS_PER_PAGE {
            return Err(format!(
                "Page {} can receive at most {MAX_ANNOTATIONS_PER_PAGE} new annotations in one export.",
                annotation.page_number
            ));
        }
        validate_colour(annotation.colour, number, "outline")?;
        if let Some(fill) = annotation.fill_colour {
            validate_colour(fill, number, "fill")?;
        }
        if !annotation.opacity.is_finite() || !(0.05..=1.0).contains(&annotation.opacity) {
            return Err(format!(
                "Annotation {number} opacity must be between 5% and 100%."
            ));
        }
        if !annotation.line_width.is_finite() || !(0.5..=20.0).contains(&annotation.line_width) {
            return Err(format!(
                "Annotation {number} line width must be between 0.5 and 20 points."
            ));
        }
        if !annotation.font_size.is_finite() || !(6.0..=96.0).contains(&annotation.font_size) {
            return Err(format!(
                "Annotation {number} font size must be between 6 and 96 points."
            ));
        }

        match annotation.kind {
            AnnotationKind::Text => {
                validate_rect(annotation.rect, number)?;
                annotation.text = Some(validate_text(annotation.text, number, "text")?);
            }
            AnnotationKind::Highlight => validate_rect(annotation.rect, number)?,
            AnnotationKind::Stamp => {
                validate_rect(annotation.rect, number)?;
                let label = validate_text(annotation.stamp, number, "stamp label")?;
                if label.chars().count() > 64 || label.len() > 256 {
                    return Err(format!("Annotation {number} stamp label is too long."));
                }
                annotation.stamp = Some(label);
            }
            AnnotationKind::Freehand => {
                if annotation.points.len() < 2
                    || annotation.points.len() > MAX_POINTS_PER_ANNOTATION
                {
                    return Err(format!(
                        "Annotation {number} freehand stroke needs between 2 and {MAX_POINTS_PER_ANNOTATION} points."
                    ));
                }
                for (point_index, point) in annotation.points.iter().enumerate() {
                    if point_index % 512 == 0 {
                        control.ensure_not_cancelled()?;
                    }
                    validate_point(*point, number)?;
                }
                total_points = total_points.saturating_add(annotation.points.len());
            }
            AnnotationKind::Rectangle | AnnotationKind::Ellipse | AnnotationKind::Image => {
                validate_rect(annotation.rect, number)?;
                if annotation.kind == AnnotationKind::Image {
                    let data_url = annotation
                        .image_data_url
                        .as_deref()
                        .ok_or_else(|| format!("Annotation {number} is missing its image data."))?;
                    if data_url.len() > MAX_IMAGE_DATA_BYTES * 2 {
                        return Err(format!("Annotation {number} image is too large."));
                    }
                }
            }
            AnnotationKind::Line => {
                validate_point(
                    annotation.start.ok_or_else(|| {
                        format!("Annotation {number} is missing its start point.")
                    })?,
                    number,
                )?;
                validate_point(
                    annotation
                        .end
                        .ok_or_else(|| format!("Annotation {number} is missing its end point."))?,
                    number,
                )?;
            }
        }
        if total_points > MAX_TOTAL_POINTS {
            return Err(format!(
                "The annotation set can contain at most {MAX_TOTAL_POINTS} freehand points."
            ));
        }
        validated.push(annotation);
    }
    Ok(validated)
}

fn checkpoint_annotation_loop(
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
    let span = end.saturating_sub(start);
    let progress = start.saturating_add(
        ((u16::from(span) * u16::try_from(index).unwrap_or(u16::MAX))
            / u16::try_from(total.max(1)).unwrap_or(u16::MAX)) as u8,
    );
    control.checkpoint(progress.min(end), stage)
}

fn validate_colour(colour: [f32; 3], number: usize, label: &str) -> Result<(), String> {
    if colour
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(format!(
            "Annotation {number} has an invalid {label} colour."
        ));
    }
    Ok(())
}

fn validate_rect(rect: Option<NormalisedRect>, number: usize) -> Result<(), String> {
    let rect = rect.ok_or_else(|| format!("Annotation {number} is missing its area."))?;
    let values = [rect.x, rect.y, rect.width, rect.height];
    if values.iter().any(|value| !value.is_finite())
        || rect.x < 0.0
        || rect.y < 0.0
        || rect.width < 0.002
        || rect.height < 0.002
        || rect.x + rect.width > 1.000_001
        || rect.y + rect.height > 1.000_001
    {
        return Err(format!("Annotation {number} has an invalid page area."));
    }
    Ok(())
}

fn validate_point(point: NormalisedPoint, number: usize) -> Result<(), String> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || !(0.0..=1.0).contains(&point.x)
        || !(0.0..=1.0).contains(&point.y)
    {
        return Err(format!(
            "Annotation {number} contains a point outside the page."
        ));
    }
    Ok(())
}

fn validate_text(value: Option<String>, number: usize, label: &str) -> Result<String, String> {
    let value = value.unwrap_or_default().trim().to_string();
    if value.is_empty() {
        return Err(format!("Annotation {number} needs {label}."));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(format!(
            "Annotation {number} {label} contains unsupported control characters."
        ));
    }
    if value.chars().count() > MAX_TEXT_CHARACTERS || value.len() > MAX_TEXT_BYTES {
        return Err(format!("Annotation {number} {label} is too long."));
    }
    Ok(value)
}

fn editable_existing_annotation_sources(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<Vec<ExistingAnnotationSource>, String> {
    let pages = document.get_pages();
    let mut sources = Vec::new();
    let mut inspected_entries = 0_usize;
    for (page_offset, (page_number, page_id)) in pages.iter().enumerate() {
        if page_offset.is_multiple_of(8) {
            control.ensure_not_cancelled()?;
        }
        let Ok(page_number) = usize::try_from(*page_number) else {
            continue;
        };
        let Ok(geometry) = page_geometry(document, *page_id) else {
            continue;
        };
        let values = page_annotation_values(document, *page_id)?;
        for (annotation_index, value) in values.iter().take(MAX_ANNOTATIONS_PER_PAGE).enumerate() {
            if inspected_entries >= MAX_ANNOTATIONS {
                return Ok(sources);
            }
            inspected_entries += 1;
            if annotation_index.is_multiple_of(32) {
                control.ensure_not_cancelled()?;
            }
            let Object::Reference(object_id) = value else {
                continue;
            };
            let Ok(dictionary) = document.get_dictionary(*object_id) else {
                continue;
            };
            let source_id = format!(
                "source-p{page_number}-a{}-o{}-g{}",
                annotation_index + 1,
                object_id.0,
                object_id.1
            );
            let local_id = format!("existing-p{page_number}-a{}", annotation_index + 1);
            let Some(editable) = parse_existing_annotation(
                document,
                dictionary,
                &source_id,
                &local_id,
                page_number,
                geometry,
            ) else {
                continue;
            };
            sources.push(ExistingAnnotationSource {
                source_id,
                viewer_annotation_id: if object_id.1 == 0 {
                    format!("{}R", object_id.0)
                } else {
                    format!("{}R{}", object_id.0, object_id.1)
                },
                page_number,
                annotation_index,
                dictionary: dictionary.clone(),
                editable: Some(editable),
            });
        }
    }
    Ok(sources)
}

fn parse_existing_annotation(
    document: &Document,
    dictionary: &Dictionary,
    source_id: &str,
    local_id: &str,
    page_number: usize,
    geometry: PageGeometry,
) -> Option<PdfAnnotationDraft> {
    if annotation_has_any(
        dictionary,
        &[
            b"IRT",
            b"RT",
            b"Popup",
            b"Parent",
            b"A",
            b"AA",
            b"Dest",
            b"StructParent",
            b"OC",
        ],
    ) || !annotation_flags_are_editable(document, dictionary)
    {
        return None;
    }
    let subtype = dictionary.get(b"Subtype").and_then(Object::as_name).ok()?;
    let kind = match subtype {
        b"FreeText" => {
            if annotation_has_any(
                dictionary,
                &[b"RC", b"DS", b"CL", b"LE", b"RD", b"IC", b"BE"],
            ) || !free_text_is_plain(document, dictionary)
                || !annotation_border_is_plain(document, dictionary)
            {
                return None;
            }
            AnnotationKind::Text
        }
        b"Highlight" => AnnotationKind::Highlight,
        b"Stamp" => {
            if dictionary.has(b"TufekciAnnotationKind")
                || !stamp_appearance_is_representable(dictionary)
            {
                return None;
            }
            AnnotationKind::Stamp
        }
        b"Ink" => {
            if !annotation_border_is_plain(document, dictionary) {
                return None;
            }
            AnnotationKind::Freehand
        }
        b"Square" => {
            if annotation_has_any(dictionary, &[b"RD", b"BE"])
                || !annotation_border_is_plain(document, dictionary)
            {
                return None;
            }
            AnnotationKind::Rectangle
        }
        b"Circle" => {
            if annotation_has_any(dictionary, &[b"RD", b"BE"])
                || !annotation_border_is_plain(document, dictionary)
            {
                return None;
            }
            AnnotationKind::Ellipse
        }
        b"Line" => {
            if annotation_has_any(dictionary, &[b"LL", b"LLE", b"LLO", b"Cap", b"Measure"])
                || !line_endings_are_plain(document, dictionary)
                || !annotation_border_is_plain(document, dictionary)
            {
                return None;
            }
            AnnotationKind::Line
        }
        _ => return None,
    };

    let (font_size, text_colour) = if kind == AnnotationKind::Text {
        annotation_default_appearance(document, dictionary)?
    } else {
        (14.0, None)
    };
    let mut colour = annotation_colour(document, dictionary, kind)?;
    if let Some(text_colour) = text_colour {
        if dictionary.has(b"C") && !colours_are_close(colour, text_colour) {
            return None;
        }
        colour = text_colour;
    }
    let fill_colour = if matches!(kind, AnnotationKind::Rectangle | AnnotationKind::Ellipse) {
        optional_annotation_colour(document, dictionary, b"IC")?
    } else {
        None
    };
    let opacity = annotation_number(
        document,
        dictionary,
        b"CA",
        default_opacity(kind),
        0.05,
        1.0,
    )?;
    let line_width = if matches!(kind, AnnotationKind::Highlight | AnnotationKind::Stamp) {
        2.0
    } else {
        annotation_line_width(document, dictionary)?
    };
    let mut draft = PdfAnnotationDraft {
        id: local_id.to_string(),
        source_annotation_id: Some(source_id.to_string()),
        page_number,
        kind,
        rect: None,
        start: None,
        end: None,
        points: Vec::new(),
        colour,
        fill_colour,
        opacity,
        line_width,
        font_size,
        text: None,
        stamp: None,
        image_data_url: None,
    };

    match kind {
        AnnotationKind::Text => {
            draft.rect = Some(annotation_rect(document, dictionary, geometry)?);
            draft.text = Some(annotation_contents(dictionary)?);
        }
        AnnotationKind::Highlight => {
            let quad_points = dictionary.get(b"QuadPoints").ok()?;
            let points = pdf_points(document, quad_points)?;
            if points.len() != 4 {
                return None;
            }
            draft.rect = Some(normalised_axis_aligned_quad(geometry, &points)?);
        }
        AnnotationKind::Stamp => {
            draft.rect = Some(annotation_rect(document, dictionary, geometry)?);
            let label = annotation_contents(dictionary).or_else(|| annotation_name(dictionary))?;
            if label.chars().count() > 64 || label.len() > 256 {
                return None;
            }
            draft.stamp = Some(label);
        }
        AnnotationKind::Freehand => {
            let ink_list =
                dereference_object(document, dictionary.get(b"InkList").ok()?, "ink list")
                    .ok()?
                    .as_array()
                    .ok()?;
            if ink_list.len() != 1 {
                return None;
            }
            let points = pdf_points(document, &ink_list[0])?;
            if !(2..=MAX_POINTS_PER_ANNOTATION).contains(&points.len()) {
                return None;
            }
            draft.points = points
                .into_iter()
                .map(|point| normalise_pdf_point(geometry, point))
                .collect::<Option<Vec<_>>>()?;
        }
        AnnotationKind::Rectangle | AnnotationKind::Ellipse => {
            draft.rect = Some(annotation_rect(document, dictionary, geometry)?);
        }
        AnnotationKind::Line => {
            let points = pdf_points(document, dictionary.get(b"L").ok()?)?;
            if points.len() != 2 {
                return None;
            }
            draft.start = Some(normalise_pdf_point(geometry, points[0])?);
            draft.end = Some(normalise_pdf_point(geometry, points[1])?);
        }
        AnnotationKind::Image => return None,
    }
    Some(draft)
}

fn annotation_has_any(dictionary: &Dictionary, keys: &[&[u8]]) -> bool {
    keys.iter().any(|key| dictionary.has(key))
}

fn annotation_flags_are_editable(document: &Document, dictionary: &Dictionary) -> bool {
    let Ok(value) = dictionary.get(b"F") else {
        return true;
    };
    dereference_object(document, value, "annotation flags")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .is_some_and(|flags| flags >= 0 && flags & !4 == 0)
}

fn free_text_is_plain(document: &Document, dictionary: &Dictionary) -> bool {
    let alignment_is_left = match dictionary.get(b"Q") {
        Err(_) => true,
        Ok(value) => {
            dereference_object(document, value, "free-text alignment")
                .ok()
                .and_then(|value| value.as_i64().ok())
                == Some(0)
        }
    };
    let intent_is_plain = match dictionary.get(b"IT") {
        Err(_) => true,
        Ok(value) => dereference_object(document, value, "free-text intent")
            .ok()
            .and_then(|value| value.as_name().ok())
            .is_some_and(|name| name == b"FreeText"),
    };
    alignment_is_left && intent_is_plain
}

fn annotation_border_is_plain(document: &Document, dictionary: &Dictionary) -> bool {
    if let Ok(value) = dictionary.get(b"BS") {
        let Some(border) = dereference_object(document, value, "annotation border")
            .ok()
            .and_then(|value| value.as_dict().ok())
        else {
            return false;
        };
        if border.has(b"D")
            || border.get(b"S").is_ok_and(|style| {
                dereference_object(document, style, "annotation border style")
                    .ok()
                    .and_then(|style| style.as_name().ok())
                    .is_none_or(|style| style != b"S")
            })
        {
            return false;
        }
    }
    if let Ok(value) = dictionary.get(b"Border") {
        let Some(border) = dereference_object(document, value, "annotation border")
            .ok()
            .and_then(|value| value.as_array().ok())
        else {
            return false;
        };
        if border.len() < 3
            || pdf_number(&border[0]).is_none_or(|value| value != 0.0)
            || pdf_number(&border[1]).is_none_or(|value| value != 0.0)
            || border.get(3).is_some_and(|dash| {
                dereference_object(document, dash, "annotation border dash")
                    .ok()
                    .and_then(|dash| dash.as_array().ok())
                    .is_none_or(|dash| !dash.is_empty())
            })
        {
            return false;
        }
    }
    true
}

fn stamp_appearance_is_representable(dictionary: &Dictionary) -> bool {
    if dictionary
        .get(b"NM")
        .ok()
        .and_then(|value| decode_text_string(value).ok())
        .is_some_and(|value| value.starts_with("TufekciPaperworksAnnotation:"))
    {
        return true;
    }
    let Some(name) = dictionary
        .get(b"Name")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|value| {
            String::from_utf8_lossy(value)
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
    else {
        return false;
    };
    matches!(
        name.as_str(),
        "approved"
            | "experimental"
            | "notapproved"
            | "asis"
            | "expired"
            | "notforpublicrelease"
            | "confidential"
            | "final"
            | "sold"
            | "departmental"
            | "forcomment"
            | "topsecret"
            | "draft"
            | "forpublicrelease"
    )
}

fn colours_are_close(left: [f32; 3], right: [f32; 3]) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(left, right)| (left - right).abs() <= 0.005)
}

fn page_annotation_values(document: &Document, page_id: ObjectId) -> Result<Vec<Object>, String> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A PDF page is invalid: {error}"))?;
    match page.get(b"Annots") {
        Err(_) | Ok(Object::Null) => Ok(Vec::new()),
        Ok(Object::Array(values)) => Ok(values.clone()),
        Ok(Object::Reference(id)) => document
            .get_object(*id)
            .and_then(Object::as_array)
            .cloned()
            .map_err(|error| format!("A PDF page has an invalid annotation list: {error}")),
        Ok(_) => Err("A PDF page has an invalid annotation list.".to_string()),
    }
}

fn annotation_contents(dictionary: &Dictionary) -> Option<String> {
    let value = dictionary.get(b"Contents").ok()?;
    if value
        .as_str()
        .ok()
        .is_none_or(|bytes| bytes.len() > MAX_TEXT_BYTES)
    {
        return None;
    }
    let value = decode_text_string(value).ok()?.trim().to_string();
    if value.is_empty()
        || value.chars().count() > MAX_TEXT_CHARACTERS
        || value.len() > MAX_TEXT_BYTES
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return None;
    }
    Some(value)
}

fn annotation_name(dictionary: &Dictionary) -> Option<String> {
    let value = dictionary.get(b"Name").and_then(Object::as_name).ok()?;
    if value.len() > 256 {
        return None;
    }
    let value = String::from_utf8_lossy(value).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn annotation_rect(
    document: &Document,
    dictionary: &Dictionary,
    geometry: PageGeometry,
) -> Option<NormalisedRect> {
    let value = dictionary.get(b"Rect").ok()?;
    let points = pdf_rectangle_points(document, value)?;
    normalised_rect_from_pdf_points(geometry, &points)
}

fn pdf_rectangle_points(document: &Document, value: &Object) -> Option<[PdfPoint; 4]> {
    let values = dereference_object(document, value, "annotation rectangle")
        .ok()?
        .as_array()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    let left = pdf_number(&values[0])?;
    let bottom = pdf_number(&values[1])?;
    let right = pdf_number(&values[2])?;
    let top = pdf_number(&values[3])?;
    let (left, right) = (left.min(right), left.max(right));
    let (bottom, top) = (bottom.min(top), bottom.max(top));
    Some([
        PdfPoint { x: left, y: bottom },
        PdfPoint {
            x: right,
            y: bottom,
        },
        PdfPoint { x: left, y: top },
        PdfPoint { x: right, y: top },
    ])
}

fn pdf_points(document: &Document, value: &Object) -> Option<Vec<PdfPoint>> {
    let values = dereference_object(document, value, "annotation points")
        .ok()?
        .as_array()
        .ok()?;
    if values.len() % 2 != 0 || values.len() > MAX_POINTS_PER_ANNOTATION * 2 {
        return None;
    }
    values
        .chunks_exact(2)
        .map(|coordinates| {
            Some(PdfPoint {
                x: pdf_number(&coordinates[0])?,
                y: pdf_number(&coordinates[1])?,
            })
        })
        .collect()
}

fn normalise_pdf_point(geometry: PageGeometry, point: PdfPoint) -> Option<NormalisedPoint> {
    let (visual_x, visual_y_from_top) = match geometry.rotation {
        90 => (point.y - geometry.page.bottom, point.x - geometry.page.left),
        180 => (
            geometry.page.left + geometry.page.width - point.x,
            point.y - geometry.page.bottom,
        ),
        270 => (
            geometry.page.bottom + geometry.page.height - point.y,
            geometry.page.left + geometry.page.height - point.x,
        ),
        _ => (
            point.x - geometry.page.left,
            geometry.page.bottom + geometry.page.height - point.y,
        ),
    };
    let x = visual_x / geometry.visual_width;
    let y = visual_y_from_top / geometry.visual_height;
    if !x.is_finite()
        || !y.is_finite()
        || !(-0.000_001..=1.000_001).contains(&x)
        || !(-0.000_001..=1.000_001).contains(&y)
    {
        return None;
    }
    Some(NormalisedPoint {
        x: x.clamp(0.0, 1.0),
        y: y.clamp(0.0, 1.0),
    })
}

fn normalised_rect_from_pdf_points(
    geometry: PageGeometry,
    points: &[PdfPoint],
) -> Option<NormalisedRect> {
    let points = points
        .iter()
        .copied()
        .map(|point| normalise_pdf_point(geometry, point))
        .collect::<Option<Vec<_>>>()?;
    let left = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let right = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let top = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let bottom = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let rect = NormalisedRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    (rect.width >= 0.002 && rect.height >= 0.002).then_some(rect)
}

fn normalised_axis_aligned_quad(
    geometry: PageGeometry,
    points: &[PdfPoint],
) -> Option<NormalisedRect> {
    let rect = normalised_rect_from_pdf_points(geometry, points)?;
    let mut corners = HashSet::new();
    for point in points {
        let point = normalise_pdf_point(geometry, *point)?;
        let x_side = if (point.x - rect.x).abs() <= 0.000_5 {
            0_u8
        } else if (point.x - rect.x - rect.width).abs() <= 0.000_5 {
            1
        } else {
            return None;
        };
        let y_side = if (point.y - rect.y).abs() <= 0.000_5 {
            0_u8
        } else if (point.y - rect.y - rect.height).abs() <= 0.000_5 {
            1
        } else {
            return None;
        };
        corners.insert(y_side * 2 + x_side);
    }
    (corners.len() == 4).then_some(rect)
}

fn annotation_colour(
    document: &Document,
    dictionary: &Dictionary,
    kind: AnnotationKind,
) -> Option<[f32; 3]> {
    match dictionary.get(b"C") {
        Ok(value) => parse_pdf_colour(document, value),
        Err(_) => Some(default_colour(kind)),
    }
}

fn optional_annotation_colour(
    document: &Document,
    dictionary: &Dictionary,
    key: &[u8],
) -> Option<Option<[f32; 3]>> {
    match dictionary.get(key) {
        Ok(value) => parse_pdf_colour(document, value).map(Some),
        Err(_) => Some(None),
    }
}

fn parse_pdf_colour(document: &Document, value: &Object) -> Option<[f32; 3]> {
    let values = dereference_object(document, value, "annotation colour")
        .ok()?
        .as_array()
        .ok()?;
    let components = values.iter().map(pdf_number).collect::<Option<Vec<_>>>()?;
    if components
        .iter()
        .any(|component| !(0.0..=1.0).contains(component))
    {
        return None;
    }
    let rgb = match components.as_slice() {
        [grey] => [*grey, *grey, *grey],
        [red, green, blue] => [*red, *green, *blue],
        [cyan, magenta, yellow, black] => [
            1.0 - (cyan + black).min(1.0),
            1.0 - (magenta + black).min(1.0),
            1.0 - (yellow + black).min(1.0),
        ],
        _ => return None,
    };
    Some([rgb[0] as f32, rgb[1] as f32, rgb[2] as f32])
}

fn default_colour(kind: AnnotationKind) -> [f32; 3] {
    match kind {
        AnnotationKind::Highlight => [0.94, 0.79, 0.16],
        AnnotationKind::Stamp => [0.78, 0.20, 0.29],
        _ => [0.14, 0.36, 0.85],
    }
}

fn default_opacity(kind: AnnotationKind) -> f32 {
    if kind == AnnotationKind::Highlight {
        0.45
    } else {
        0.9
    }
}

fn annotation_number(
    document: &Document,
    dictionary: &Dictionary,
    key: &[u8],
    default: f32,
    minimum: f64,
    maximum: f64,
) -> Option<f32> {
    match dictionary.get(key) {
        Ok(value) => {
            let value = pdf_number(dereference_object(document, value, "annotation number").ok()?)?;
            (value.is_finite() && (minimum..=maximum).contains(&value)).then_some(value as f32)
        }
        Err(_) => Some(default),
    }
}

fn annotation_line_width(document: &Document, dictionary: &Dictionary) -> Option<f32> {
    if let Ok(value) = dictionary.get(b"BS") {
        let border = dereference_object(document, value, "annotation border")
            .ok()?
            .as_dict()
            .ok()?;
        return annotation_number(document, border, b"W", 1.0, 0.5, 20.0);
    }
    if let Ok(value) = dictionary.get(b"Border") {
        let border = dereference_object(document, value, "annotation border")
            .ok()?
            .as_array()
            .ok()?;
        if border.len() >= 3 {
            let width = pdf_number(&border[2])?;
            return (width.is_finite() && (0.5..=20.0).contains(&width)).then_some(width as f32);
        }
    }
    Some(2.0)
}

fn annotation_default_appearance(
    document: &Document,
    dictionary: &Dictionary,
) -> Option<(f32, Option<[f32; 3]>)> {
    let Ok(value) = dictionary.get(b"DA") else {
        return Some((14.0, None));
    };
    let value = dereference_object(document, value, "annotation default appearance").ok()?;
    let Object::String(bytes, _) = value else {
        return None;
    };
    if bytes.len() > MAX_TEXT_BYTES {
        return None;
    }
    let content = Content::decode(bytes).ok()?;
    let size = content.operations.iter().rev().find_map(|operation| {
        (operation.operator == "Tf" && operation.operands.len() >= 2)
            .then(|| pdf_number(&operation.operands[1]))
            .flatten()
    })?;
    if !size.is_finite() || !(6.0..=96.0).contains(&size) {
        return None;
    }
    let mut colour = None;
    for operation in &content.operations {
        colour = match operation.operator.as_str() {
            "g" if operation.operands.len() == 1 => {
                let grey = pdf_colour_component(&operation.operands[0])?;
                Some([grey, grey, grey])
            }
            "rg" if operation.operands.len() == 3 => Some([
                pdf_colour_component(&operation.operands[0])?,
                pdf_colour_component(&operation.operands[1])?,
                pdf_colour_component(&operation.operands[2])?,
            ]),
            "k" if operation.operands.len() == 4 => {
                let cyan = pdf_colour_component(&operation.operands[0])?;
                let magenta = pdf_colour_component(&operation.operands[1])?;
                let yellow = pdf_colour_component(&operation.operands[2])?;
                let black = pdf_colour_component(&operation.operands[3])?;
                Some([
                    1.0 - (cyan + black).min(1.0),
                    1.0 - (magenta + black).min(1.0),
                    1.0 - (yellow + black).min(1.0),
                ])
            }
            _ => colour,
        };
    }
    Some((size as f32, colour))
}

fn pdf_colour_component(value: &Object) -> Option<f32> {
    let value = pdf_number(value)?;
    (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(value as f32)
}

fn line_endings_are_plain(document: &Document, dictionary: &Dictionary) -> bool {
    let Ok(value) = dictionary.get(b"LE") else {
        return true;
    };
    let Some(values) = dereference_object(document, value, "line endings")
        .ok()
        .and_then(|value| value.as_array().ok())
    else {
        return false;
    };
    values.len() == 2
        && values.iter().all(|value| {
            dereference_object(document, value, "line ending")
                .ok()
                .and_then(|value| value.as_name().ok())
                .is_some_and(|name| name == b"None")
        })
}

fn pdf_number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn validate_existing_annotation_changes<'a>(
    annotations: &[PdfAnnotationDraft],
    updated_annotations: &[PdfAnnotationDraft],
    removed_existing_annotation_ids: &[String],
    existing_by_id: &HashMap<&'a str, &'a ExistingAnnotationSource>,
) -> Result<HashMap<usize, HashSet<usize>>, String> {
    if annotations
        .iter()
        .any(|annotation| annotation.source_annotation_id.is_some())
    {
        return Err("A new annotation unexpectedly refers to an existing source item.".to_string());
    }
    let mut changed_source_ids = HashSet::new();
    let mut targets = HashMap::<usize, HashSet<usize>>::new();
    for annotation in updated_annotations {
        let source_id = annotation.source_annotation_id.as_deref().ok_or_else(|| {
            "An annotation update is missing its reviewed source identity.".to_string()
        })?;
        validate_source_annotation_id(source_id)?;
        if !changed_source_ids.insert(source_id.to_string()) {
            return Err("An existing annotation is changed more than once.".to_string());
        }
        let source = existing_by_id.get(source_id).ok_or_else(|| {
            "An annotation update does not match an editable reviewed item.".to_string()
        })?;
        let original = source.editable.as_ref().expect("editable source");
        if annotation.page_number != source.page_number || annotation.kind != original.kind {
            return Err(
                "An existing annotation changed its page or annotation type unexpectedly."
                    .to_string(),
            );
        }
        targets
            .entry(source.page_number)
            .or_default()
            .insert(source.annotation_index);
    }
    for source_id in removed_existing_annotation_ids {
        if !changed_source_ids.insert(source_id.clone()) {
            return Err("An existing annotation cannot be both updated and removed.".to_string());
        }
        let source = existing_by_id.get(source_id.as_str()).ok_or_else(|| {
            "An annotation removal does not match an editable reviewed item.".to_string()
        })?;
        targets
            .entry(source.page_number)
            .or_default()
            .insert(source.annotation_index);
    }
    Ok(targets)
}

fn remove_existing_annotations(
    document: &mut Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    targets: &HashMap<usize, HashSet<usize>>,
) -> Result<(), String> {
    for (page_number, indexes) in targets {
        let page_key = u32::try_from(*page_number)
            .map_err(|_| "An annotation page number is too large to process safely.".to_string())?;
        let page_id = pages
            .get(&page_key)
            .copied()
            .ok_or_else(|| "An annotation source page no longer exists.".to_string())?;
        let values = page_annotation_values(document, page_id)?;
        if indexes.iter().any(|index| *index >= values.len()) {
            return Err("An existing annotation position changed before export.".to_string());
        }
        let retained = values
            .into_iter()
            .enumerate()
            .filter_map(|(index, value)| (!indexes.contains(&index)).then_some(value))
            .collect::<Vec<_>>();
        let mut page = document
            .get_dictionary(page_id)
            .map_err(|error| format!("The annotation page is invalid: {error}"))?
            .clone();
        page.set("Annots", Object::Array(retained));
        document.objects.insert(page_id, Object::Dictionary(page));
    }
    Ok(())
}

fn preserve_annotation_metadata(target: &mut Dictionary, source: &Dictionary) {
    for key in [
        b"T".as_slice(),
        b"Subj".as_slice(),
        b"CreationDate".as_slice(),
    ] {
        if let Ok(value @ Object::String(bytes, _)) = source.get(key) {
            if bytes.len() <= MAX_TEXT_BYTES && decode_text_string(value).is_ok() {
                target.set(key, value.clone());
            }
        }
    }
}

fn annotation_counts(document: &Document) -> Result<Vec<usize>, String> {
    document
        .get_pages()
        .values()
        .map(|page_id| annotation_count(document, *page_id))
        .collect()
}

fn annotation_counts_with_control(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<Vec<usize>, String> {
    let pages = document.get_pages();
    let total = pages.len().max(1);
    let mut counts = Vec::with_capacity(pages.len());
    for (index, page_id) in pages.values().enumerate() {
        control.checkpoint(
            42 + (((index + 1) * 42 / total).min(42)) as u8,
            format!(
                "Inspecting annotation page {} of {}",
                index + 1,
                pages.len()
            ),
        )?;
        counts.push(annotation_count(document, *page_id)?);
    }
    Ok(counts)
}

fn annotation_count(document: &Document, page_id: ObjectId) -> Result<usize, String> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A PDF page is invalid: {error}"))?;
    match page.get(b"Annots") {
        Err(_) | Ok(Object::Null) => Ok(0),
        Ok(Object::Array(values)) => Ok(values.len()),
        Ok(Object::Reference(id)) => document
            .get_object(*id)
            .and_then(Object::as_array)
            .map(Vec::len)
            .map_err(|error| format!("A PDF page has an invalid annotation list: {error}")),
        Ok(_) => Err("A PDF page has an invalid annotation list.".to_string()),
    }
}

fn append_page_annotation(
    document: &mut Document,
    page_id: ObjectId,
    annotation_id: ObjectId,
) -> Result<(), String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("The annotation page is invalid: {error}"))?
        .clone();
    let mut annotations = match page.get(b"Annots") {
        Err(_) | Ok(Object::Null) => Vec::new(),
        Ok(Object::Array(values)) => values.clone(),
        Ok(Object::Reference(id)) => document
            .get_object(*id)
            .and_then(Object::as_array)
            .cloned()
            .map_err(|error| format!("The annotation page list is invalid: {error}"))?,
        Ok(_) => return Err("The annotation page list is invalid.".to_string()),
    };
    annotations.push(Object::Reference(annotation_id));
    page.set("Annots", Object::Array(annotations));
    document.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<PageGeometry, String> {
    let page_box = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "The annotation page does not define a crop or media box.".to_string())?;
    let page_box = dereference_object(document, &page_box, "annotation page box")?;
    let coordinates = page_box
        .as_array()
        .map_err(|_| "The annotation page box is not an array.".to_string())?;
    if coordinates.len() != 4 {
        return Err("The annotation page box must contain four coordinates.".to_string());
    }
    let values = coordinates
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, _>>()?;
    let width = values[2] - values[0];
    let height = values[3] - values[1];
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("The annotation page has invalid dimensions.".to_string());
    }
    let rotation = match inherited_page_value(document, page_id, b"Rotate")? {
        Some(value) => dereference_object(document, &value, "page rotation")?
            .as_i64()
            .map_err(|_| "The annotation page has an invalid rotation.".to_string())?,
        None => 0,
    }
    .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("The annotation page has an unsupported rotation.".to_string());
    }
    let (visual_width, visual_height) = if matches!(rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    };
    Ok(PageGeometry {
        page: PageBox {
            left: values[0],
            bottom: values[1],
            width,
            height,
        },
        rotation,
        visual_width,
        visual_height,
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

fn dereference_object<'a>(
    document: &'a Document,
    object: &'a Object,
    label: &str,
) -> Result<&'a Object, String> {
    match object {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| format!("The PDF {label} is invalid: {error}")),
        value => Ok(value),
    }
}

fn pdf_number_value(object: &Object) -> Result<f64, String> {
    match object {
        Object::Integer(value) => Ok(*value as f64),
        Object::Real(value) => Ok(f64::from(*value)),
        _ => Err("The annotation page box contains a non-numeric value.".to_string()),
    }
}

fn map_point(geometry: PageGeometry, point: NormalisedPoint) -> PdfPoint {
    map_visual_point(
        geometry,
        point.x * geometry.visual_width,
        point.y * geometry.visual_height,
    )
}

fn map_visual_point(geometry: PageGeometry, visual_x: f64, visual_y_from_top: f64) -> PdfPoint {
    let visual_y = geometry.visual_height - visual_y_from_top;
    match geometry.rotation {
        90 => PdfPoint {
            x: geometry.page.left + geometry.page.width - visual_y,
            y: geometry.page.bottom + visual_x,
        },
        180 => PdfPoint {
            x: geometry.page.left + geometry.page.width - visual_x,
            y: geometry.page.bottom + geometry.page.height - visual_y,
        },
        270 => PdfPoint {
            x: geometry.page.left + visual_y,
            y: geometry.page.bottom + geometry.page.height - visual_x,
        },
        _ => PdfPoint {
            x: geometry.page.left + visual_x,
            y: geometry.page.bottom + visual_y,
        },
    }
}

fn map_rect(geometry: PageGeometry, rect: NormalisedRect) -> MappedRect {
    let points = [
        map_point(
            geometry,
            NormalisedPoint {
                x: rect.x,
                y: rect.y,
            },
        ),
        map_point(
            geometry,
            NormalisedPoint {
                x: rect.x + rect.width,
                y: rect.y,
            },
        ),
        map_point(
            geometry,
            NormalisedPoint {
                x: rect.x,
                y: rect.y + rect.height,
            },
        ),
        map_point(
            geometry,
            NormalisedPoint {
                x: rect.x + rect.width,
                y: rect.y + rect.height,
            },
        ),
    ];
    MappedRect {
        pdf: bounding_rect(&points, 0.0, geometry.page),
        visual_left: rect.x * geometry.visual_width,
        visual_top: rect.y * geometry.visual_height,
        visual_width: rect.width * geometry.visual_width,
        visual_height: rect.height * geometry.visual_height,
    }
}

fn bounding_rect(points: &[PdfPoint], padding: f64, page: PageBox) -> PdfRect {
    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect {
        left: (min_x - padding).max(page.left),
        bottom: (min_y - padding).max(page.bottom),
        right: (max_x + padding).min(page.left + page.width),
        top: (max_y + padding).min(page.bottom + page.height),
    }
    .ensure_visible()
}

impl PdfRect {
    fn width(self) -> f64 {
        self.right - self.left
    }

    fn height(self) -> f64 {
        self.top - self.bottom
    }

    fn ensure_visible(mut self) -> Self {
        if self.width() < 0.5 {
            self.right = self.left + 0.5;
        }
        if self.height() < 0.5 {
            self.top = self.bottom + 0.5;
        }
        self
    }

    fn object(self) -> Object {
        Object::Array(vec![
            pdf_real(self.left),
            pdf_real(self.bottom),
            pdf_real(self.right),
            pdf_real(self.top),
        ])
    }
}

fn build_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
    font_id: Option<ObjectId>,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    match annotation.kind {
        AnnotationKind::Text => build_text_annotation(
            document,
            page_id,
            geometry,
            annotation,
            marker,
            font_id.ok_or_else(|| "The annotation font is unavailable.".to_string())?,
        ),
        AnnotationKind::Highlight => {
            build_highlight_annotation(document, page_id, geometry, annotation, marker)
        }
        AnnotationKind::Stamp => build_stamp_annotation(
            document,
            page_id,
            geometry,
            annotation,
            marker,
            font_id.ok_or_else(|| "The annotation font is unavailable.".to_string())?,
        ),
        AnnotationKind::Freehand => {
            build_ink_annotation(document, page_id, geometry, annotation, marker)
        }
        AnnotationKind::Rectangle => {
            build_shape_annotation(document, page_id, geometry, annotation, marker, false)
        }
        AnnotationKind::Ellipse => {
            build_shape_annotation(document, page_id, geometry, annotation, marker, true)
        }
        AnnotationKind::Line => {
            build_line_annotation(document, page_id, geometry, annotation, marker)
        }
        AnnotationKind::Image => {
            build_image_annotation(document, page_id, geometry, annotation, marker)
        }
    }
}

fn base_annotation(
    subtype: &'static [u8],
    rect: PdfRect,
    page_id: ObjectId,
    marker: &str,
    colour: [f32; 3],
    opacity: f32,
    appearance_id: ObjectId,
) -> Dictionary {
    let mut appearance = Dictionary::new();
    appearance.set("N", appearance_id);
    dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.to_vec()),
        "Rect" => rect.object(),
        "P" => page_id,
        "F" => 4,
        "NM" => Object::String(marker.as_bytes().to_vec(), StringFormat::Literal),
        "C" => colour_object(colour),
        "CA" => pdf_real(f64::from(opacity)),
        "Border" => Object::Array(vec![0.into(), 0.into(), 0.into()]),
        "AP" => Object::Dictionary(appearance),
    }
}

fn build_text_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
    font_id: ObjectId,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let mapped = map_rect(geometry, annotation.rect.expect("validated rectangle"));
    let text = annotation.text.as_deref().expect("validated text");
    let (appearance_id, substituted) = text_appearance(
        document,
        geometry,
        mapped,
        text,
        annotation.colour,
        annotation.opacity,
        annotation.font_size,
        font_id,
        true,
    )?;
    let mut dictionary = base_annotation(
        b"FreeText",
        mapped.pdf,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set("Contents", text_string(text));
    dictionary.set(
        "DA",
        Object::String(
            format!(
                "/Helv {} Tf {} {} {} rg",
                annotation.font_size,
                annotation.colour[0],
                annotation.colour[1],
                annotation.colour[2]
            )
            .into_bytes(),
            StringFormat::Literal,
        ),
    );
    dictionary.set("Q", 0);
    dictionary.set("IT", "FreeText");
    dictionary.set(
        "BS",
        dictionary! { "Type" => "Border", "W" => pdf_real(f64::from(annotation.line_width)), "S" => "S" },
    );
    Ok((dictionary, b"FreeText", substituted))
}

fn build_highlight_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let rect = annotation.rect.expect("validated rectangle");
    let mapped = map_rect(geometry, rect);
    let appearance_id = filled_rect_appearance(
        document,
        mapped.pdf,
        annotation.colour,
        annotation.opacity,
        true,
    )?;
    let top_left = map_point(
        geometry,
        NormalisedPoint {
            x: rect.x,
            y: rect.y,
        },
    );
    let top_right = map_point(
        geometry,
        NormalisedPoint {
            x: rect.x + rect.width,
            y: rect.y,
        },
    );
    let bottom_left = map_point(
        geometry,
        NormalisedPoint {
            x: rect.x,
            y: rect.y + rect.height,
        },
    );
    let bottom_right = map_point(
        geometry,
        NormalisedPoint {
            x: rect.x + rect.width,
            y: rect.y + rect.height,
        },
    );
    let mut dictionary = base_annotation(
        b"Highlight",
        mapped.pdf,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set(
        "QuadPoints",
        point_array(&[top_left, top_right, bottom_left, bottom_right]),
    );
    dictionary.set("Contents", text_string("Highlight"));
    Ok((dictionary, b"Highlight", false))
}

fn build_stamp_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
    font_id: ObjectId,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let mapped = map_rect(geometry, annotation.rect.expect("validated rectangle"));
    let label = annotation.stamp.as_deref().expect("validated stamp label");
    let (appearance_id, substituted) = stamp_appearance(
        document,
        geometry,
        mapped,
        label,
        annotation.colour,
        annotation.opacity,
        font_id,
    )?;
    let mut dictionary = base_annotation(
        b"Stamp",
        mapped.pdf,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set("Contents", text_string(label));
    dictionary.set("Name", stamp_name(label));
    Ok((dictionary, b"Stamp", substituted))
}

fn build_ink_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let points = annotation
        .points
        .iter()
        .copied()
        .map(|point| map_point(geometry, point))
        .collect::<Vec<_>>();
    let rect = bounding_rect(
        &points,
        f64::from(annotation.line_width) * 1.5,
        geometry.page,
    );
    let local = points
        .iter()
        .map(|point| PdfPoint {
            x: point.x - rect.left,
            y: point.y - rect.bottom,
        })
        .collect::<Vec<_>>();
    let appearance_id = path_appearance(
        document,
        rect,
        &local,
        annotation.colour,
        annotation.opacity,
        annotation.line_width,
    )?;
    let mut dictionary = base_annotation(
        b"Ink",
        rect,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set("InkList", Object::Array(vec![point_array(&points)]));
    dictionary.set(
        "BS",
        dictionary! { "Type" => "Border", "W" => pdf_real(f64::from(annotation.line_width)), "S" => "S" },
    );
    dictionary.set("Contents", text_string("Freehand annotation"));
    Ok((dictionary, b"Ink", false))
}

fn build_shape_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
    ellipse: bool,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let mapped = map_rect(geometry, annotation.rect.expect("validated rectangle"));
    let appearance_id = shape_appearance(
        document,
        mapped.pdf,
        annotation.colour,
        annotation.fill_colour,
        annotation.opacity,
        annotation.line_width,
        ellipse,
    )?;
    let subtype = if ellipse { b"Circle" } else { b"Square" };
    let mut dictionary = base_annotation(
        subtype,
        mapped.pdf,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set(
        "BS",
        dictionary! { "Type" => "Border", "W" => pdf_real(f64::from(annotation.line_width)), "S" => "S" },
    );
    if let Some(fill) = annotation.fill_colour {
        dictionary.set("IC", colour_object(fill));
    }
    dictionary.set(
        "Contents",
        text_string(if ellipse {
            "Ellipse annotation"
        } else {
            "Rectangle annotation"
        }),
    );
    Ok((dictionary, subtype, false))
}

fn build_line_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let start = map_point(geometry, annotation.start.expect("validated start point"));
    let end = map_point(geometry, annotation.end.expect("validated end point"));
    let rect = bounding_rect(
        &[start, end],
        f64::from(annotation.line_width) * 1.5,
        geometry.page,
    );
    let local = [
        PdfPoint {
            x: start.x - rect.left,
            y: start.y - rect.bottom,
        },
        PdfPoint {
            x: end.x - rect.left,
            y: end.y - rect.bottom,
        },
    ];
    let appearance_id = path_appearance(
        document,
        rect,
        &local,
        annotation.colour,
        annotation.opacity,
        annotation.line_width,
    )?;
    let mut dictionary = base_annotation(
        b"Line",
        rect,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set("L", point_array(&[start, end]));
    dictionary.set(
        "LE",
        Object::Array(vec![
            Object::Name(b"None".to_vec()),
            Object::Name(b"None".to_vec()),
        ]),
    );
    dictionary.set(
        "BS",
        dictionary! { "Type" => "Border", "W" => pdf_real(f64::from(annotation.line_width)), "S" => "S" },
    );
    dictionary.set("Contents", text_string("Line annotation"));
    Ok((dictionary, b"Line", false))
}

fn build_image_annotation(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    annotation: &PdfAnnotationDraft,
    marker: &str,
) -> Result<(Dictionary, &'static [u8], bool), String> {
    let mapped = map_rect(geometry, annotation.rect.expect("validated rectangle"));
    let image = decode_annotation_image(
        annotation
            .image_data_url
            .as_deref()
            .expect("validated image data"),
    )?;
    let image_id = add_image_xobject(document, image)?;
    let appearance_id = image_appearance(
        document,
        mapped,
        geometry.rotation,
        annotation.opacity,
        image_id,
    )?;
    let mut dictionary = base_annotation(
        b"Stamp",
        mapped.pdf,
        page_id,
        marker,
        annotation.colour,
        annotation.opacity,
        appearance_id,
    );
    dictionary.set("Contents", text_string("Image annotation"));
    dictionary.set("Name", "Image");
    dictionary.set("TufekciAnnotationKind", "Image");
    Ok((dictionary, b"Stamp", false))
}

fn appearance_resources(
    opacity: f32,
    blend_multiply: bool,
    font_id: Option<ObjectId>,
    image_id: Option<ObjectId>,
) -> Dictionary {
    let mut graphics_state = dictionary! {
        "Type" => "ExtGState",
        "CA" => pdf_real(f64::from(opacity)),
        "ca" => pdf_real(f64::from(opacity)),
    };
    if blend_multiply {
        graphics_state.set("BM", "Multiply");
    }
    let mut states = Dictionary::new();
    states.set("GS0", graphics_state);
    let mut resources = Dictionary::new();
    resources.set("ExtGState", states);
    if let Some(font_id) = font_id {
        let mut fonts = Dictionary::new();
        fonts.set("Helv", font_id);
        resources.set("Font", fonts);
    }
    if let Some(image_id) = image_id {
        let mut images = Dictionary::new();
        images.set("Im0", image_id);
        resources.set("XObject", images);
    }
    resources
}

fn add_appearance(
    document: &mut Document,
    rect: PdfRect,
    resources: Dictionary,
    operations: Vec<Operation>,
) -> Result<ObjectId, String> {
    let content = Content { operations }
        .encode()
        .map_err(|error| format!("An annotation appearance could not be encoded: {error}"))?;
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => Object::Array(vec![0.into(), 0.into(), pdf_real(rect.width()), pdf_real(rect.height())]),
            "Resources" => resources,
            "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceRGB", "I" => true },
        },
        content,
    );
    stream
        .compress()
        .map_err(|error| format!("An annotation appearance could not be compressed: {error}"))?;
    Ok(document.add_object(stream))
}

fn filled_rect_appearance(
    document: &mut Document,
    rect: PdfRect,
    colour: [f32; 3],
    opacity: f32,
    multiply: bool,
) -> Result<ObjectId, String> {
    let operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"GS0".to_vec())]),
        colour_operation("rg", colour),
        Operation::new(
            "re",
            vec![
                0.into(),
                0.into(),
                pdf_real(rect.width()),
                pdf_real(rect.height()),
            ],
        ),
        Operation::new("f", vec![]),
        Operation::new("Q", vec![]),
    ];
    add_appearance(
        document,
        rect,
        appearance_resources(opacity, multiply, None, None),
        operations,
    )
}

fn path_appearance(
    document: &mut Document,
    rect: PdfRect,
    points: &[PdfPoint],
    colour: [f32; 3],
    opacity: f32,
    line_width: f32,
) -> Result<ObjectId, String> {
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"GS0".to_vec())]),
        colour_operation("RG", colour),
        Operation::new("w", vec![pdf_real(f64::from(line_width))]),
        Operation::new("J", vec![1.into()]),
        Operation::new("j", vec![1.into()]),
        Operation::new("m", vec![pdf_real(points[0].x), pdf_real(points[0].y)]),
    ];
    operations.extend(
        points
            .iter()
            .skip(1)
            .map(|point| Operation::new("l", vec![pdf_real(point.x), pdf_real(point.y)])),
    );
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    add_appearance(
        document,
        rect,
        appearance_resources(opacity, false, None, None),
        operations,
    )
}

fn shape_appearance(
    document: &mut Document,
    rect: PdfRect,
    stroke: [f32; 3],
    fill: Option<[f32; 3]>,
    opacity: f32,
    line_width: f32,
    ellipse: bool,
) -> Result<ObjectId, String> {
    let inset = (f64::from(line_width) / 2.0)
        .min(rect.width() / 4.0)
        .min(rect.height() / 4.0);
    let left = inset;
    let bottom = inset;
    let width = (rect.width() - inset * 2.0).max(0.1);
    let height = (rect.height() - inset * 2.0).max(0.1);
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"GS0".to_vec())]),
        colour_operation("RG", stroke),
        Operation::new("w", vec![pdf_real(f64::from(line_width))]),
    ];
    if let Some(fill) = fill {
        operations.push(colour_operation("rg", fill));
    }
    if ellipse {
        let kappa = 0.552_284_749_8;
        let centre_x = left + width / 2.0;
        let centre_y = bottom + height / 2.0;
        let radius_x = width / 2.0;
        let radius_y = height / 2.0;
        operations.extend([
            Operation::new("m", vec![pdf_real(centre_x + radius_x), pdf_real(centre_y)]),
            Operation::new(
                "c",
                vec![
                    pdf_real(centre_x + radius_x),
                    pdf_real(centre_y + kappa * radius_y),
                    pdf_real(centre_x + kappa * radius_x),
                    pdf_real(centre_y + radius_y),
                    pdf_real(centre_x),
                    pdf_real(centre_y + radius_y),
                ],
            ),
            Operation::new(
                "c",
                vec![
                    pdf_real(centre_x - kappa * radius_x),
                    pdf_real(centre_y + radius_y),
                    pdf_real(centre_x - radius_x),
                    pdf_real(centre_y + kappa * radius_y),
                    pdf_real(centre_x - radius_x),
                    pdf_real(centre_y),
                ],
            ),
            Operation::new(
                "c",
                vec![
                    pdf_real(centre_x - radius_x),
                    pdf_real(centre_y - kappa * radius_y),
                    pdf_real(centre_x - kappa * radius_x),
                    pdf_real(centre_y - radius_y),
                    pdf_real(centre_x),
                    pdf_real(centre_y - radius_y),
                ],
            ),
            Operation::new(
                "c",
                vec![
                    pdf_real(centre_x + kappa * radius_x),
                    pdf_real(centre_y - radius_y),
                    pdf_real(centre_x + radius_x),
                    pdf_real(centre_y - kappa * radius_y),
                    pdf_real(centre_x + radius_x),
                    pdf_real(centre_y),
                ],
            ),
        ]);
    } else {
        operations.push(Operation::new(
            "re",
            vec![
                pdf_real(left),
                pdf_real(bottom),
                pdf_real(width),
                pdf_real(height),
            ],
        ));
    }
    operations.push(Operation::new(
        if fill.is_some() { "B" } else { "S" },
        vec![],
    ));
    operations.push(Operation::new("Q", vec![]));
    add_appearance(
        document,
        rect,
        appearance_resources(opacity, false, None, None),
        operations,
    )
}

#[allow(clippy::too_many_arguments)]
fn text_appearance(
    document: &mut Document,
    geometry: PageGeometry,
    mapped: MappedRect,
    text: &str,
    colour: [f32; 3],
    opacity: f32,
    font_size: f32,
    font_id: ObjectId,
    draw_box: bool,
) -> Result<(ObjectId, bool), String> {
    let padding = (f64::from(font_size) * 0.35).max(2.0);
    let line_height = f64::from(font_size) * 1.2;
    let available_width = (mapped.visual_width - padding * 2.0).max(1.0);
    let available_height = (mapped.visual_height - padding * 2.0).max(1.0);
    let max_lines = ((available_height / line_height).floor() as usize).clamp(1, 32);
    let lines = wrap_text(text, available_width, f64::from(font_size), max_lines);
    let mut substituted = false;
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"GS0".to_vec())]),
    ];
    if draw_box {
        operations.extend([
            colour_operation("rg", [1.0, 1.0, 0.96]),
            colour_operation("RG", colour),
            Operation::new("w", vec![pdf_real(0.8)]),
            Operation::new(
                "re",
                vec![
                    pdf_real(0.4),
                    pdf_real(0.4),
                    pdf_real((mapped.pdf.width() - 0.8).max(0.1)),
                    pdf_real((mapped.pdf.height() - 0.8).max(0.1)),
                ],
            ),
            Operation::new("B", vec![]),
        ]);
    }
    operations.extend([
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![
                Object::Name(b"Helv".to_vec()),
                pdf_real(f64::from(font_size)),
            ],
        ),
        colour_operation("rg", colour),
    ]);
    let [a, b, c, d] = text_orientation(geometry.rotation);
    for (line_index, line) in lines.iter().enumerate() {
        let baseline = map_visual_point(
            geometry,
            mapped.visual_left + padding,
            mapped.visual_top + padding + f64::from(font_size) + line_index as f64 * line_height,
        );
        let (encoded, line_substituted) = encode_win_ansi(line);
        substituted |= line_substituted;
        operations.push(Operation::new(
            "Tm",
            vec![
                pdf_real(a),
                pdf_real(b),
                pdf_real(c),
                pdf_real(d),
                pdf_real(baseline.x - mapped.pdf.left),
                pdf_real(baseline.y - mapped.pdf.bottom),
            ],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(encoded, StringFormat::Literal)],
        ));
    }
    operations.extend([Operation::new("ET", vec![]), Operation::new("Q", vec![])]);
    let appearance_id = add_appearance(
        document,
        mapped.pdf,
        appearance_resources(opacity, false, Some(font_id), None),
        operations,
    )?;
    Ok((appearance_id, substituted))
}

fn stamp_appearance(
    document: &mut Document,
    geometry: PageGeometry,
    mapped: MappedRect,
    label: &str,
    colour: [f32; 3],
    opacity: f32,
    font_id: ObjectId,
) -> Result<(ObjectId, bool), String> {
    let character_count = label.chars().count().max(1) as f64;
    let font_size = (mapped.visual_height * 0.42)
        .min(mapped.visual_width / (character_count * 0.62))
        .clamp(6.0, 48.0);
    let baseline = map_visual_point(
        geometry,
        mapped.visual_left + (mapped.visual_width - character_count * font_size * 0.52) / 2.0,
        mapped.visual_top + (mapped.visual_height + font_size * 0.66) / 2.0,
    );
    let (encoded, substituted) = encode_win_ansi(label);
    let [a, b, c, d] = text_orientation(geometry.rotation);
    let operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"GS0".to_vec())]),
        colour_operation("rg", [1.0, 1.0, 1.0]),
        colour_operation("RG", colour),
        Operation::new("w", vec![pdf_real(2.2)]),
        Operation::new(
            "re",
            vec![
                pdf_real(1.1),
                pdf_real(1.1),
                pdf_real((mapped.pdf.width() - 2.2).max(0.1)),
                pdf_real((mapped.pdf.height() - 2.2).max(0.1)),
            ],
        ),
        Operation::new("B", vec![]),
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"Helv".to_vec()), pdf_real(font_size)],
        ),
        colour_operation("rg", colour),
        Operation::new(
            "Tm",
            vec![
                pdf_real(a),
                pdf_real(b),
                pdf_real(c),
                pdf_real(d),
                pdf_real(baseline.x - mapped.pdf.left),
                pdf_real(baseline.y - mapped.pdf.bottom),
            ],
        ),
        Operation::new("Tj", vec![Object::String(encoded, StringFormat::Literal)]),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ];
    let appearance_id = add_appearance(
        document,
        mapped.pdf,
        appearance_resources(opacity, false, Some(font_id), None),
        operations,
    )?;
    Ok((appearance_id, substituted))
}

fn image_appearance(
    document: &mut Document,
    mapped: MappedRect,
    rotation: i64,
    opacity: f32,
    image_id: ObjectId,
) -> Result<ObjectId, String> {
    let width = mapped.visual_width;
    let height = mapped.visual_height;
    let matrix = match rotation {
        90 => [0.0, width, -height, 0.0, height, 0.0],
        180 => [-width, 0.0, 0.0, -height, width, height],
        270 => [0.0, -width, height, 0.0, 0.0, width],
        _ => [width, 0.0, 0.0, height, 0.0, 0.0],
    };
    let operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"GS0".to_vec())]),
        Operation::new("cm", matrix.into_iter().map(pdf_real).collect()),
        Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
        Operation::new("Q", vec![]),
    ];
    add_appearance(
        document,
        mapped.pdf,
        appearance_resources(opacity, false, None, Some(image_id)),
        operations,
    )
}

fn add_image_xobject(document: &mut Document, mut image: DynamicImage) -> Result<ObjectId, String> {
    if image.width() > MAX_EMBEDDED_IMAGE_DIMENSION || image.height() > MAX_EMBEDDED_IMAGE_DIMENSION
    {
        image = image.thumbnail(MAX_EMBEDDED_IMAGE_DIMENSION, MAX_EMBEDDED_IMAGE_DIMENSION);
    }
    let rgba = image.to_rgba8();
    if !rgba.pixels().any(|pixel| pixel.0[3] > 0) {
        return Err("The annotation image does not contain any visible pixels.".to_string());
    }
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    let mut alpha = Vec::with_capacity(rgba.len() / 4);
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
    }
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
        .map_err(|error| format!("The annotation transparency could not be compressed: {error}"))?;
    let alpha_id = document.add_object(alpha_stream);
    let mut image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(rgba.width()),
            "Height" => i64::from(rgba.height()),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "SMask" => alpha_id,
            "TufekciAnnotationImage" => true,
        },
        rgb,
    );
    image_stream
        .compress()
        .map_err(|error| format!("The annotation image could not be compressed: {error}"))?;
    Ok(document.add_object(image_stream))
}

fn decode_annotation_image(data_url: &str) -> Result<DynamicImage, String> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "Annotation images must be prepared as PNG data.".to_string())?;
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| "The annotation image is not valid base64 data.".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_DATA_BYTES {
        return Err("The annotation image is empty or too large to export safely.".to_string());
    }
    match catch_unwind(AssertUnwindSafe(|| -> Result<DynamicImage, String> {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_IMAGE_ALLOCATION);
        reader.limits(limits);
        reader
            .decode()
            .map_err(|error| format!("The annotation PNG could not be decoded: {error}"))
    })) {
        Ok(result) => result,
        Err(_) => Err("The annotation image was rejected safely.".to_string()),
    }
}

fn text_orientation(rotation: i64) -> [f64; 4] {
    match rotation {
        90 => [0.0, 1.0, -1.0, 0.0],
        180 => [-1.0, 0.0, 0.0, -1.0],
        270 => [0.0, -1.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0, 1.0],
    }
}

fn wrap_text(text: &str, width: f64, font_size: f64, max_lines: usize) -> Vec<String> {
    let max_characters = (width / (font_size * 0.55)).floor().max(1.0) as usize;
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max_characters
            {
                lines.push(line);
                line = String::new();
                if lines.len() >= max_lines {
                    return lines;
                }
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() || paragraph.is_empty() {
            lines.push(line);
        }
        if lines.len() >= max_lines {
            break;
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines.truncate(max_lines);
    lines
}

fn encode_win_ansi(value: &str) -> (Vec<u8>, bool) {
    let mut substituted = false;
    let bytes = value
        .chars()
        .map(|character| match character as u32 {
            0x20..=0x7e | 0xa0..=0xff => character as u8,
            0x20ac => 0x80,
            0x201a => 0x82,
            0x0192 => 0x83,
            0x201e => 0x84,
            0x2026 => 0x85,
            0x2020 => 0x86,
            0x2021 => 0x87,
            0x02c6 => 0x88,
            0x2030 => 0x89,
            0x0160 => 0x8a,
            0x2039 => 0x8b,
            0x0152 => 0x8c,
            0x017d => 0x8e,
            0x2018 => 0x91,
            0x2019 => 0x92,
            0x201c => 0x93,
            0x201d => 0x94,
            0x2022 => 0x95,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x02dc => 0x98,
            0x2122 => 0x99,
            0x0161 => 0x9a,
            0x203a => 0x9b,
            0x0153 => 0x9c,
            0x017e => 0x9e,
            0x0178 => 0x9f,
            _ => {
                substituted = true;
                b'?'
            }
        })
        .collect();
    (bytes, substituted)
}

fn stamp_name(label: &str) -> Object {
    let compact = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    Object::Name(
        if compact.is_empty() {
            "Custom".to_string()
        } else {
            compact
        }
        .into_bytes(),
    )
}

fn verify_expected_annotations(
    document: &Document,
    expected: &[ExpectedAnnotation],
) -> Result<(), String> {
    let pages = document.get_pages();
    for expected_annotation in expected {
        let page_id = pages
            .get(
                &u32::try_from(expected_annotation.page_number).map_err(|_| {
                    "An annotation verification page number is invalid.".to_string()
                })?,
            )
            .copied()
            .ok_or_else(|| "An annotation page disappeared during verification.".to_string())?;
        let found = document
            .get_page_annotations(page_id)
            .map_err(|error| format!("Annotations could not be verified: {error}"))?
            .into_iter()
            .any(|dictionary| {
                let marker_matches = dictionary
                    .get(b"NM")
                    .ok()
                    .and_then(|value| decode_text_string(value).ok())
                    .is_some_and(|marker| marker == expected_annotation.marker);
                let subtype_matches = dictionary
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|subtype| subtype == expected_annotation.subtype);
                let appearance_present = annotation_appearance_is_complete(
                    document,
                    dictionary,
                    expected_annotation.image,
                );
                marker_matches && subtype_matches && appearance_present
            });
        if !found {
            return Err(format!(
                "Annotation verification failed on page {} and the PDF was not saved.",
                expected_annotation.page_number
            ));
        }
    }
    Ok(())
}

fn annotation_appearance_is_complete(
    document: &Document,
    annotation: &Dictionary,
    image_expected: bool,
) -> bool {
    let Some(normal_appearance) = annotation
        .get(b"AP")
        .ok()
        .and_then(|value| dereference_object(document, value, "annotation appearance").ok())
        .and_then(|value| value.as_dict().ok())
        .and_then(|appearance| appearance.get(b"N").ok())
        .and_then(|value| dereference_object(document, value, "annotation appearance stream").ok())
    else {
        return false;
    };
    let Ok(stream) = normal_appearance.as_stream() else {
        return false;
    };
    if !image_expected {
        return true;
    }
    let annotation_kind_matches = annotation
        .get(b"TufekciAnnotationKind")
        .and_then(Object::as_name)
        .is_ok_and(|value| value == b"Image");
    let image_stream_present = stream
        .dict
        .get(b"Resources")
        .ok()
        .and_then(|value| dereference_object(document, value, "annotation resources").ok())
        .and_then(|value| value.as_dict().ok())
        .and_then(|resources| resources.get(b"XObject").ok())
        .and_then(|value| dereference_object(document, value, "annotation image resources").ok())
        .and_then(|value| value.as_dict().ok())
        .is_some_and(|images| {
            images.iter().any(|(_, value)| {
                dereference_object(document, value, "annotation image")
                    .ok()
                    .and_then(|value| value.as_stream().ok())
                    .is_some_and(|image| {
                        image
                            .dict
                            .get(b"TufekciAnnotationImage")
                            .and_then(Object::as_bool)
                            .is_ok_and(|value| value)
                    })
            })
        });
    annotation_kind_matches && image_stream_present
}

fn point_array(points: &[PdfPoint]) -> Object {
    Object::Array(
        points
            .iter()
            .flat_map(|point| [pdf_real(point.x), pdf_real(point.y)])
            .collect(),
    )
}

fn colour_object(colour: [f32; 3]) -> Object {
    Object::Array(
        colour
            .into_iter()
            .map(|value| pdf_real(f64::from(value)))
            .collect(),
    )
}

fn colour_operation(operator: &str, colour: [f32; 3]) -> Operation {
    Operation::new(
        operator,
        colour
            .into_iter()
            .map(|value| pdf_real(f64::from(value)))
            .collect(),
    )
}

fn pdf_real(value: f64) -> Object {
    Object::Real(value as f32)
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn exports_every_supported_annotation_and_preserves_existing_content() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("annotated.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        assert_eq!(inspection.page_count, 2);
        assert_eq!(inspection.existing_annotation_count, 1);
        assert_eq!(inspection.annotations_per_page, vec![1, 0]);

        let result = export_pdf_annotations(ExportPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            annotations: all_annotation_types(),
            updated_annotations: Vec::new(),
            removed_existing_annotation_ids: Vec::new(),
        })
        .unwrap();

        assert_eq!(result.page_count, 2);
        assert_eq!(result.added_annotation_count, 8);
        assert_eq!(result.total_annotation_count, 9);
        assert!(result.bytes_written > 0);
        let reopened = Document::load(&output).unwrap();
        assert_eq!(reopened.get_pages().len(), 2);
        assert!(reopened.catalog().unwrap().has(b"AcroForm"));
        let subtypes = reopened
            .get_pages()
            .values()
            .flat_map(|page_id| reopened.get_page_annotations(*page_id).unwrap())
            .filter_map(|dictionary| {
                dictionary
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .ok()
                    .map(|value| String::from_utf8_lossy(value).into_owned())
            })
            .collect::<Vec<_>>();
        for expected in [
            "Text",
            "FreeText",
            "Highlight",
            "Stamp",
            "Ink",
            "Square",
            "Circle",
            "Line",
        ] {
            assert!(subtypes.iter().any(|subtype| subtype == expected));
        }
        assert_eq!(
            subtypes
                .iter()
                .filter(|subtype| *subtype == "Stamp")
                .count(),
            2
        );
        let reopened_inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: output.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        assert_eq!(reopened_inspection.existing_annotation_count, 9);
        assert_eq!(reopened_inspection.editable_annotation_count, 7);
        assert_eq!(reopened_inspection.read_only_annotation_count, 2);
    }

    #[test]
    fn controlled_annotation_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_progress = Arc::clone(&observed);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            observed_for_progress.lock().unwrap().push((value, stage));
        });
        let result = inspect_pdf_annotations_with_control(
            InspectPdfAnnotationsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap();
        assert_eq!(result.page_count, 2);
        let progress = observed.lock().unwrap();
        assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert_eq!(progress.last().map(|entry| entry.0), Some(99));
        assert!(progress
            .iter()
            .any(|(_, stage)| stage == "Inspecting annotation page 1 of 2"));

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let cancel_progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Inspecting annotation page 1 of 2" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = run_pdf_annotation_inspection_job_with_control(
            InspectPdfAnnotationsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(cancelled, cancel_progress),
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn annotation_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-annotation-review.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Checking certificate signatures"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during annotation review\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });
        let error = run_pdf_annotation_inspection_job_with_control(
            InspectPdfAnnotationsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert_eq!(
            error,
            "The source PDF changed during annotation review. Open it again before editing."
        );
        assert!(!error.contains("private-annotation-review.pdf"));
    }

    #[test]
    fn maps_visual_coordinates_across_all_page_rotations() {
        let page = PageBox {
            left: 10.0,
            bottom: 20.0,
            width: 100.0,
            height: 200.0,
        };
        let cases = [
            (0, 100.0, 200.0, PdfPoint { x: 10.0, y: 220.0 }),
            (90, 200.0, 100.0, PdfPoint { x: 10.0, y: 20.0 }),
            (180, 100.0, 200.0, PdfPoint { x: 110.0, y: 20.0 }),
            (270, 200.0, 100.0, PdfPoint { x: 110.0, y: 220.0 }),
        ];
        for (rotation, visual_width, visual_height, expected) in cases {
            let actual = map_point(
                PageGeometry {
                    page,
                    rotation,
                    visual_width,
                    visual_height,
                },
                NormalisedPoint { x: 0.0, y: 0.0 },
            );
            assert!((actual.x - expected.x).abs() < 0.001);
            assert!((actual.y - expected.y).abs() < 0.001);
        }
    }

    #[test]
    fn reviews_supported_existing_annotations_and_reports_read_only_items() {
        let directory = TestDirectory::new();
        let input = directory.path.join("existing-annotations.pdf");
        fixture_with_editable_annotations()
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();

        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(inspection.existing_annotation_count, 3);
        assert_eq!(inspection.editable_annotation_count, 2);
        assert_eq!(inspection.read_only_annotation_count, 1);
        assert_eq!(inspection.annotations_per_page, vec![2, 1]);
        assert_eq!(inspection.editable_annotations_per_page, vec![1, 1]);
        assert_eq!(inspection.read_only_annotations_per_page, vec![1, 0]);
        assert!(inspection.editable_annotations.iter().all(|annotation| {
            annotation.annotation.source_annotation_id.is_some()
                && annotation.viewer_annotation_id.ends_with('R')
        }));
        let text = inspection
            .editable_annotations
            .iter()
            .find(|annotation| annotation.annotation.kind == AnnotationKind::Text)
            .unwrap();
        assert_eq!(
            text.annotation.text.as_deref(),
            Some("Editable source note")
        );
        assert!(text.annotation.rect.is_some_and(|rect| rect.width > 0.2));
        let rectangle = inspection
            .editable_annotations
            .iter()
            .find(|annotation| annotation.annotation.kind == AnnotationKind::Rectangle)
            .unwrap();
        assert_eq!(rectangle.annotation.page_number, 2);
        assert!(rectangle
            .annotation
            .rect
            .is_some_and(|rect| rect.width > 0.1 && rect.height > 0.1));
    }

    #[test]
    fn bounds_existing_annotation_extraction_without_hiding_total_counts() {
        let directory = TestDirectory::new();
        let input = directory.path.join("many-existing-annotations.pdf");
        let mut document = fixture(false);
        let first_page = *document.get_pages().get(&1).unwrap();
        let mut values = page_annotation_values(&document, first_page).unwrap();
        for index in 0..600 {
            let contents = format!("Editable note {index}");
            let annotation_id = document.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "FreeText",
                "Rect" => vec![100.into(), 650.into(), 260.into(), 710.into()],
                "Contents" => text_string(&contents),
                "C" => vec![Object::Real(0.2), Object::Real(0.4), Object::Real(0.8)],
                "CA" => Object::Real(0.85),
                "DA" => Object::String(b"/Helv 13 Tf 0.2 0.4 0.8 rg".to_vec(), StringFormat::Literal),
                "BS" => dictionary! { "W" => Object::Real(1.5), "S" => "S" },
            });
            values.push(Object::Reference(annotation_id));
        }
        let mut page = document.get_dictionary(first_page).unwrap().clone();
        page.set("Annots", Object::Array(values));
        document
            .objects
            .insert(first_page, Object::Dictionary(page));
        document.save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(inspection.existing_annotation_count, 601);
        assert_eq!(inspection.editable_annotation_count, 499);
        assert_eq!(inspection.read_only_annotation_count, 102);
        assert!(inspection
            .warnings
            .iter()
            .any(|warning| warning.contains("beyond workspace limits")));
    }

    #[test]
    fn keeps_hidden_rich_or_visually_complex_annotations_read_only() {
        let document = fixture(false);
        let first_page = *document.get_pages().get(&1).unwrap();
        let geometry = page_geometry(&document, first_page).unwrap();
        let mut hidden = dictionary! {
            "Type" => "Annot",
            "Subtype" => "FreeText",
            "Rect" => vec![100.into(), 650.into(), 260.into(), 710.into()],
            "Contents" => text_string("Hidden note"),
            "F" => 2,
            "DA" => Object::String(b"/Helv 13 Tf 0.2 0.4 0.8 rg".to_vec(), StringFormat::Literal),
            "C" => vec![Object::Real(0.2), Object::Real(0.4), Object::Real(0.8)],
            "BS" => dictionary! { "W" => 1, "S" => "S" },
        };
        assert!(parse_existing_annotation(
            &document,
            &hidden,
            "source-p1-a1-o10-g0",
            "existing-p1-a1",
            1,
            geometry,
        )
        .is_none());

        hidden.remove(b"F");
        hidden.set("RC", text_string("Rich text"));
        assert!(parse_existing_annotation(
            &document,
            &hidden,
            "source-p1-a1-o10-g0",
            "existing-p1-a1",
            1,
            geometry,
        )
        .is_none());

        let skewed_highlight = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
            "Rect" => vec![100.into(), 650.into(), 260.into(), 710.into()],
            "QuadPoints" => vec![100.into(), 700.into(), 260.into(), 710.into(), 100.into(), 650.into(), 260.into(), 660.into()],
            "C" => vec![Object::Real(1.0), Object::Real(0.8), Object::Real(0.1)],
        };
        assert!(parse_existing_annotation(
            &document,
            &skewed_highlight,
            "source-p1-a2-o11-g0",
            "existing-p1-a2",
            1,
            geometry,
        )
        .is_none());

        let custom_stamp = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Stamp",
            "Rect" => vec![100.into(), 650.into(), 260.into(), 710.into()],
            "Contents" => text_string("Company seal"),
            "Name" => "CompanySeal",
        };
        assert!(parse_existing_annotation(
            &document,
            &custom_stamp,
            "source-p1-a3-o12-g0",
            "existing-p1-a3",
            1,
            geometry,
        )
        .is_none());
    }

    #[test]
    fn updates_and_removes_reviewed_annotations_with_exact_reopening_checks() {
        let directory = TestDirectory::new();
        let input = directory.path.join("existing-annotations.pdf");
        let output = directory.path.join("edited-annotations.pdf");
        fixture_with_editable_annotations()
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        let mut updated = inspection
            .editable_annotations
            .iter()
            .find(|annotation| annotation.annotation.kind == AnnotationKind::Text)
            .unwrap()
            .annotation
            .clone();
        updated.text = Some("Updated existing note".to_string());
        updated.colour = [0.1, 0.5, 0.2];
        let removed_id = inspection
            .editable_annotations
            .iter()
            .find(|annotation| annotation.annotation.kind == AnnotationKind::Rectangle)
            .unwrap()
            .annotation
            .source_annotation_id
            .clone()
            .unwrap();

        let result = export_pdf_annotations(ExportPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            annotations: vec![draft(AnnotationKind::Highlight, 1, "new-highlight")],
            updated_annotations: vec![updated],
            removed_existing_annotation_ids: vec![removed_id],
        })
        .unwrap();

        assert_eq!(result.added_annotation_count, 1);
        assert_eq!(result.updated_annotation_count, 1);
        assert_eq!(result.removed_annotation_count, 1);
        assert_eq!(result.total_annotation_count, 3);
        let reopened = Document::load(&output).unwrap();
        assert_eq!(annotation_counts(&reopened).unwrap(), vec![3, 0]);
        let first_page = *reopened.get_pages().get(&1).unwrap();
        let annotations = reopened.get_page_annotations(first_page).unwrap();
        assert!(annotations.iter().any(|annotation| {
            annotation
                .get(b"Subtype")
                .and_then(Object::as_name)
                .is_ok_and(|subtype| subtype == b"Text")
                && annotation
                    .get(b"Contents")
                    .ok()
                    .and_then(|value| decode_text_string(value).ok())
                    .is_some_and(|contents| contents == "Existing note")
        }));
        let updated = annotations
            .iter()
            .find(|annotation| {
                annotation
                    .get(b"Contents")
                    .ok()
                    .and_then(|value| decode_text_string(value).ok())
                    .is_some_and(|contents| contents == "Updated existing note")
            })
            .unwrap();
        assert_eq!(
            updated
                .get(b"T")
                .ok()
                .and_then(|value| decode_text_string(value).ok())
                .as_deref(),
            Some("Review author")
        );
        let reopened_inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: output.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        assert_eq!(reopened_inspection.editable_annotation_count, 2);
        assert_eq!(reopened_inspection.read_only_annotation_count, 1);
    }

    #[test]
    fn rejects_an_unreviewed_existing_annotation_identity() {
        let directory = TestDirectory::new();
        let input = directory.path.join("existing-annotations.pdf");
        let output = directory.path.join("should-not-exist.pdf");
        fixture_with_editable_annotations()
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        let mut update = draft(AnnotationKind::Text, 1, "invented-update");
        update.source_annotation_id = Some("source-p1-a99-o999-g0".to_string());
        update.text = Some("Not authorised".to_string());

        let error = export_pdf_annotations(ExportPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            annotations: Vec::new(),
            updated_annotations: vec![update],
            removed_existing_annotation_ids: Vec::new(),
        })
        .unwrap_err();

        assert!(error.contains("does not match"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_oversized_or_out_of_bounds_annotation_data() {
        let mut oversized = draft(AnnotationKind::Freehand, 1, "ink");
        oversized.points = vec![NormalisedPoint { x: 0.5, y: 0.5 }; MAX_POINTS_PER_ANNOTATION + 1];
        assert!(validate_annotations(vec![oversized], 2)
            .unwrap_err()
            .contains("freehand stroke"));

        let mut outside = draft(AnnotationKind::Rectangle, 1, "outside");
        outside.rect = Some(NormalisedRect {
            x: 0.9,
            y: 0.1,
            width: 0.2,
            height: 0.2,
        });
        assert!(validate_annotations(vec![outside], 2)
            .unwrap_err()
            .contains("invalid page area"));
    }

    #[test]
    fn requires_acknowledgement_before_annotating_a_signed_pdf() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed.pdf");
        let output = directory.path.join("annotated.pdf");
        fixture(true).save(&input).unwrap().sync_all().unwrap();
        let metadata = fs::metadata(&input).unwrap();

        let error = export_pdf_annotations(ExportPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: metadata.len(),
            expected_source_modified_at_ms: modified_at_ms(&metadata),
            annotations: vec![draft(AnnotationKind::Highlight, 1, "highlight")],
            updated_annotations: Vec::new(),
            removed_existing_annotation_ids: Vec::new(),
        })
        .unwrap_err();

        assert!(error.contains("certificate signature"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_source_that_changed_after_annotation_review() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("annotated.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        let mut bytes = fs::read(&input).unwrap();
        bytes.extend_from_slice(b"\n% changed after review\n");
        fs::write(&input, bytes).unwrap();

        let error = export_pdf_annotations(ExportPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            annotations: vec![draft(AnnotationKind::Highlight, 1, "highlight")],
            updated_annotations: Vec::new(),
            removed_existing_annotation_ids: Vec::new(),
        })
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_an_annotation_source_changed_during_export_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("annotated.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect_pdf_annotations(InspectPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking source PDF before publication"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during annotation export\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });
        let control = PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress);

        let error = export_pdf_annotations_with_control(
            ExportPdfAnnotationsRequest {
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                output_protection: None,
                acknowledge_certificate_signatures: false,
                expected_source_size: inspection.source_size,
                expected_source_modified_at_ms: inspection.source_modified_at_ms,
                annotations: vec![draft(AnnotationKind::Highlight, 1, "highlight")],
                updated_annotations: Vec::new(),
                removed_existing_annotation_ids: Vec::new(),
            },
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn never_overwrites_the_source_pdf() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let original = fs::read(&input).unwrap();
        let metadata = fs::metadata(&input).unwrap();

        let error = export_pdf_annotations(ExportPdfAnnotationsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: input.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: metadata.len(),
            expected_source_modified_at_ms: modified_at_ms(&metadata),
            annotations: vec![draft(AnnotationKind::Highlight, 1, "highlight")],
            updated_annotations: Vec::new(),
            removed_existing_annotation_ids: Vec::new(),
        })
        .unwrap_err();

        assert!(error.contains("destination") || error.contains("overwritten"));
        assert_eq!(fs::read(&input).unwrap(), original);
    }

    fn all_annotation_types() -> Vec<PdfAnnotationDraft> {
        let mut text = draft(AnnotationKind::Text, 1, "text");
        text.text = Some("Tüfekci review note".to_string());
        let highlight = draft(AnnotationKind::Highlight, 1, "highlight");
        let mut stamp = draft(AnnotationKind::Stamp, 1, "stamp");
        stamp.stamp = Some("APPROVED".to_string());
        let mut ink = draft(AnnotationKind::Freehand, 1, "ink");
        ink.rect = None;
        ink.points = vec![
            NormalisedPoint { x: 0.1, y: 0.6 },
            NormalisedPoint { x: 0.2, y: 0.55 },
            NormalisedPoint { x: 0.3, y: 0.62 },
        ];
        let rectangle = draft(AnnotationKind::Rectangle, 2, "rectangle");
        let mut ellipse = draft(AnnotationKind::Ellipse, 2, "ellipse");
        ellipse.fill_colour = Some([0.8, 0.9, 1.0]);
        let mut line = draft(AnnotationKind::Line, 2, "line");
        line.rect = None;
        line.start = Some(NormalisedPoint { x: 0.2, y: 0.2 });
        line.end = Some(NormalisedPoint { x: 0.8, y: 0.75 });
        let mut image = draft(AnnotationKind::Image, 2, "image");
        image.image_data_url = Some(test_png_data_url());
        vec![text, highlight, stamp, ink, rectangle, ellipse, line, image]
    }

    fn draft(kind: AnnotationKind, page_number: usize, id: &str) -> PdfAnnotationDraft {
        PdfAnnotationDraft {
            id: id.to_string(),
            source_annotation_id: None,
            page_number,
            kind,
            rect: Some(NormalisedRect {
                x: 0.12,
                y: 0.12,
                width: 0.28,
                height: 0.12,
            }),
            start: None,
            end: None,
            points: Vec::new(),
            colour: [0.85, 0.12, 0.12],
            fill_colour: None,
            opacity: 0.8,
            line_width: 2.0,
            font_size: 12.0,
            text: None,
            stamp: None,
            image_data_url: None,
        }
    }

    fn test_png_data_url() -> String {
        let image = ImageBuffer::from_fn(8, 5, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([35_u8, 93, 216, 255])
            } else {
                Rgba([255_u8, 255, 255, 180])
            }
        });
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(bytes.into_inner())
        )
    }

    fn fixture(signed: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let mut page_ids = Vec::new();
        for page_number in 1..=2 {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let mut page = dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Resources" => Dictionary::new(),
                "Contents" => content_id,
            };
            if page_number == 2 {
                page.set("Rotate", 90);
            }
            let page_id = document.add_object(page);
            page_ids.push(page_id);
        }
        let existing_annotation_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![20.into(), 20.into(), 40.into(), 40.into()],
            "Contents" => text_string("Existing note"),
        });
        let mut first_page = document.get_dictionary(page_ids[0]).unwrap().clone();
        first_page.set("Annots", vec![Object::Reference(existing_annotation_id)]);
        document
            .objects
            .insert(page_ids[0], Object::Dictionary(first_page));
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => 2,
                "MediaBox" => vec![10.into(), 20.into(), 605.into(), 862.into()],
            }),
        );
        let mut fields = Vec::new();
        if signed {
            let signature_id = document.add_object(dictionary! {
                "FT" => "Sig",
                "V" => dictionary! {
                    "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                    "Contents" => Object::String(vec![1, 2, 3], StringFormat::Hexadecimal),
                },
            });
            fields.push(Object::Reference(signature_id));
        }
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => dictionary! { "Fields" => fields },
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn fixture_with_editable_annotations() -> Document {
        let mut document = fixture(false);
        let pages = document.get_pages();
        let first_page = *pages.get(&1).unwrap();
        let second_page = *pages.get(&2).unwrap();
        let text_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "FreeText",
            "Rect" => vec![100.into(), 650.into(), 260.into(), 710.into()],
            "Contents" => text_string("Editable source note"),
            "T" => text_string("Review author"),
            "C" => vec![Object::Real(0.2), Object::Real(0.4), Object::Real(0.8)],
            "CA" => Object::Real(0.85),
            "DA" => Object::String(b"/Helv 13 Tf 0.2 0.4 0.8 rg".to_vec(), StringFormat::Literal),
            "BS" => dictionary! { "W" => Object::Real(1.5), "S" => "S" },
        });
        append_page_annotation(&mut document, first_page, text_id).unwrap();
        let rectangle_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Square",
            "Rect" => vec![390.into(), 150.into(), 510.into(), 310.into()],
            "Contents" => text_string("Editable rotated rectangle"),
            "C" => vec![Object::Real(0.8), Object::Real(0.1), Object::Real(0.2)],
            "IC" => vec![Object::Real(0.9), Object::Real(0.95), Object::Real(1.0)],
            "CA" => Object::Real(0.7),
            "BS" => dictionary! { "W" => 2, "S" => "S" },
        });
        append_page_annotation(&mut document, second_page, rectangle_id).unwrap();
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
                "tufekci-paperworks-annotation-test-{}-{nonce}",
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
