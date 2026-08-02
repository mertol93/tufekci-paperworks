use crate::file_safety::{
    canonical_pdf_input, reject_control_characters, TemporaryOutput, ValidatedPdfPaths,
};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{DynamicImage, ImageReader, Limits};
use lopdf::content::{Content, Operation};
use lopdf::{
    dictionary, Dictionary, Document, LoadOptions, Object, ObjectId, Stream, StringFormat,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, Cursor, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::UNIX_EPOCH;

const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_PAGE_TREE_DEPTH: usize = 32;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_CONTENT_STREAM_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_CONTENT_BYTES: usize = 512 * 1024 * 1024;
const MAX_STREAMS: usize = 50_000;
const MAX_OPERATIONS_PER_STREAM: usize = 250_000;
const MAX_TOTAL_OPERATIONS: usize = 2_000_000;
const MAX_REFERENCE_SCAN_NODES: usize = 4_000_000;
const MAX_EDITABLE_TEXT_RUNS: usize = 20_000;
const MAX_EDITABLE_IMAGES: usize = 5_000;
const MAX_TEXT_EDITS: usize = 2_000;
const MAX_IMAGE_EDITS: usize = 500;
const MAX_TEXT_CHARACTERS: usize = 4_096;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 80;
const MAX_IMAGE_DATA_BYTES: usize = 24 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 8_192;
const MAX_EMBEDDED_IMAGE_DIMENSION: u32 = 4_096;
const MAX_IMAGE_ALLOCATION: u64 = 256 * 1024 * 1024;
const FILE_HASH_BUFFER_BYTES: usize = 1024 * 1024;
const MIN_RECT_SIZE: f64 = 0.002;

const STREAM_EDIT_MARKER: &[u8] = b"TufekciPaperworksContentEdit";
const STREAM_EDIT_SHA: &[u8] = b"TufekciPaperworksContentEditSha256";
const STREAM_EDIT_COUNT: &[u8] = b"TufekciPaperworksContentEditCount";
const IMAGE_EDIT_MARKER: &[u8] = b"TufekciPaperworksContentImage";
const IMAGE_EDIT_SHA: &[u8] = b"TufekciPaperworksContentImageSha256";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InspectPdfContentRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportPdfContentRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
    acknowledge_certificate_signatures: bool,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    expected_source_sha256: String,
    #[serde(default)]
    text_edits: Vec<PdfTextEdit>,
    #[serde(default)]
    image_edits: Vec<PdfImageEdit>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PdfTextEdit {
    source_id: String,
    replacement_text: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PdfImageEdit {
    source_id: String,
    delete: bool,
    replacement_image_data_url: Option<String>,
    rect: NormalisedRect,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NormalisedRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditableTextRun {
    source_id: String,
    page_number: usize,
    text: String,
    rect: NormalisedRect,
    font_label: String,
    font_size: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditableImage {
    source_id: String,
    page_number: usize,
    rect: NormalisedRect,
    pixel_width: u32,
    pixel_height: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfContentInspection {
    file_name: String,
    source_size: u64,
    source_modified_at_ms: Option<u64>,
    source_sha256: String,
    page_count: usize,
    editable_text_count: usize,
    read_only_text_count: usize,
    editable_image_count: usize,
    read_only_image_count: usize,
    editable_text_runs: Vec<PdfEditableTextRun>,
    editable_images: Vec<PdfEditableImage>,
    pages_with_unsupported_content: Vec<usize>,
    was_encrypted: bool,
    certificate_signature: bool,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfContentResult {
    output_path: String,
    page_count: usize,
    text_edit_count: usize,
    image_edit_count: usize,
    deleted_image_count: usize,
    replaced_image_count: usize,
    repositioned_image_count: usize,
    bytes_written: u64,
    output_sha256: String,
    encryption: &'static str,
    warnings: Vec<String>,
}

struct LoadedContentPdf {
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
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn transform(self, x: f64, y: f64) -> PdfPoint {
        PdfPoint {
            x: self.a * x + self.c * y + self.e,
            y: self.b * x + self.d * y + self.f,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.c * next.b,
            b: self.b * next.a + self.d * next.b,
            c: self.a * next.c + self.c * next.d,
            d: self.b * next.c + self.d * next.d,
            e: self.a * next.e + self.c * next.f + self.e,
            f: self.b * next.e + self.d * next.f + self.f,
        }
    }
}

#[derive(Clone, Debug)]
struct TextSource {
    public: PdfEditableTextRun,
    stream_id: ObjectId,
    operation_index: usize,
    font_name: Vec<u8>,
    original_bytes: Vec<u8>,
    original_stream_sha256: String,
}

#[derive(Clone, Debug)]
struct ImageSource {
    public: PdfEditableImage,
    page_id: ObjectId,
    stream_id: ObjectId,
    block_start_index: usize,
    cm_operation_index: usize,
    operation_index: usize,
    resource_name: Vec<u8>,
    original_stream_sha256: String,
}

struct ContentCatalogue {
    text_sources: Vec<TextSource>,
    image_sources: Vec<ImageSource>,
    read_only_text_count: usize,
    read_only_image_count: usize,
    pages_with_unsupported_content: Vec<usize>,
    stream_hashes: HashMap<ObjectId, String>,
}

#[derive(Clone)]
struct PreparedTextEdit {
    source: TextSource,
    encoded: Vec<u8>,
}

struct PreparedImageEdit {
    source: ImageSource,
    delete: bool,
    rect: NormalisedRect,
    replacement: Option<DynamicImage>,
    replacement_resource_name: Option<Vec<u8>>,
}

#[derive(Clone)]
struct ExpectedEditedStream {
    marker: String,
    page_number: usize,
    original_stream_id: ObjectId,
    content_sha256: String,
    edit_count: usize,
}

#[derive(Clone)]
struct ExpectedReplacementImage {
    marker: String,
    content_sha256: String,
    pixel_width: u32,
    pixel_height: u32,
}

struct ContentVerificationExpectations<'a> {
    encrypted: bool,
    page_count: usize,
    annotation_counts: &'a [usize],
    had_form_fields: bool,
    had_outlines: bool,
    original_stream_hashes: &'a HashMap<ObjectId, String>,
    edited_streams: &'a [ExpectedEditedStream],
    replacement_images: &'a [ExpectedReplacementImage],
}

#[cfg(test)]
pub fn inspect_pdf_content(
    request: InspectPdfContentRequest,
) -> Result<PdfContentInspection, String> {
    inspect_pdf_content_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_content_request(
    request: &InspectPdfContentRequest,
) -> Result<(), String> {
    reject_control_characters("Content-editing source path", &request.input_path)?;
    validate_password(request.input_password.as_deref())
}

fn inspect_pdf_content_with_control(
    request: InspectPdfContentRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfContentInspection, String> {
    control.checkpoint(2, "Validating content review")?;
    validate_inspect_pdf_content_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let opening_metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_size = opening_metadata.len();
    let source_modified_at_ms = modified_at_ms(&opening_metadata);
    control.checkpoint(8, "Hashing the source PDF")?;
    let source_sha256 = sha256_file(&input, control)?;
    control.checkpoint(20, "Opening page content")?;
    let loaded = load_pdf(&input, request.input_password.as_deref())?;
    control.checkpoint(32, "Reviewing text and image objects")?;
    let catalogue = catalogue_content(&loaded.document, control)?;
    control.checkpoint(88, "Checking document signatures")?;
    let certificate_signature = document_has_certificate_signature(&loaded.document);
    verify_source_fingerprint(
        &input,
        source_size,
        source_modified_at_ms,
        &source_sha256,
        control,
    )?;

    let editable_text_count = catalogue.text_sources.len();
    let editable_image_count = catalogue.image_sources.len();
    let mut warnings = vec![
        "Only native-reviewed page-stream objects are editable. Complex text, nested form content, shared streams, and ambiguous image placements remain visible and read-only."
            .to_string(),
        "Text bounds are an indicative selection aid; export changes the exact reviewed PDF text-show operation."
            .to_string(),
    ];
    if catalogue.read_only_text_count > 0 || catalogue.read_only_image_count > 0 {
        warnings.push(format!(
            "{} text object{} and {} image object{} could not be edited safely in this release and will be preserved unchanged.",
            catalogue.read_only_text_count,
            if catalogue.read_only_text_count == 1 { "" } else { "s" },
            catalogue.read_only_image_count,
            if catalogue.read_only_image_count == 1 { "" } else { "s" },
        ));
    }
    if certificate_signature {
        warnings.push(
            "Editing page content rewrites this certificate-signed PDF and invalidates its existing signatures."
                .to_string(),
        );
    }
    control.checkpoint(99, "Finalising content review")?;

    Ok(PdfContentInspection {
        file_name: display_name(&input),
        source_size,
        source_modified_at_ms,
        source_sha256,
        page_count: loaded.page_count,
        editable_text_count,
        read_only_text_count: catalogue.read_only_text_count,
        editable_image_count,
        read_only_image_count: catalogue.read_only_image_count,
        editable_text_runs: catalogue
            .text_sources
            .into_iter()
            .map(|source| source.public)
            .collect(),
        editable_images: catalogue
            .image_sources
            .into_iter()
            .map(|source| source.public)
            .collect(),
        pages_with_unsupported_content: catalogue.pages_with_unsupported_content,
        was_encrypted: loaded.was_encrypted,
        certificate_signature,
        warnings,
    })
}

pub(crate) fn run_pdf_content_inspection_job_with_control(
    request: InspectPdfContentRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfContentInspection, String> {
    inspect_pdf_content_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_content_inspection_error(&error)
        }
    })
}

#[cfg(test)]
pub fn export_pdf_content(
    request: ExportPdfContentRequest,
) -> Result<ExportPdfContentResult, String> {
    export_pdf_content_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_export_pdf_content_request(
    request: &ExportPdfContentRequest,
) -> Result<(), String> {
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    validate_sha256("Reviewed source hash", &request.expected_source_sha256)?;
    if request.expected_source_size == 0 {
        return Err("Review the source PDF again before editing its content.".to_string());
    }
    if request.text_edits.is_empty() && request.image_edits.is_empty() {
        return Err(
            "Change at least one reviewed text or image object before exporting.".to_string(),
        );
    }
    if request.text_edits.len() > MAX_TEXT_EDITS {
        return Err(format!(
            "A single export can change at most {MAX_TEXT_EDITS} text objects."
        ));
    }
    if request.image_edits.len() > MAX_IMAGE_EDITS {
        return Err(format!(
            "A single export can change at most {MAX_IMAGE_EDITS} image objects."
        ));
    }
    let mut identifiers = HashSet::new();
    for (index, edit) in request.text_edits.iter().enumerate() {
        validate_source_id(&edit.source_id, "text", index + 1)?;
        if !identifiers.insert(edit.source_id.as_str()) {
            return Err(
                "A reviewed content object can be changed only once per export.".to_string(),
            );
        }
        validate_replacement_text(&edit.replacement_text, index + 1)?;
    }
    for (index, edit) in request.image_edits.iter().enumerate() {
        validate_source_id(&edit.source_id, "image", index + 1)?;
        if !identifiers.insert(edit.source_id.as_str()) {
            return Err(
                "A reviewed content object can be changed only once per export.".to_string(),
            );
        }
        validate_normalised_rect(edit.rect, index + 1)?;
        if edit.delete && edit.replacement_image_data_url.is_some() {
            return Err(format!(
                "Image edit {} cannot both delete and replace the image.",
                index + 1
            ));
        }
        if edit
            .replacement_image_data_url
            .as_ref()
            .is_some_and(|value| value.len() > MAX_IMAGE_DATA_BYTES.saturating_mul(2))
        {
            return Err(format!(
                "Image edit {} contains too much image data.",
                index + 1
            ));
        }
    }
    Ok(())
}

fn export_pdf_content_with_control(
    request: ExportPdfContentRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfContentResult, String> {
    control.checkpoint(1, "Validating content edits")?;
    validate_export_pdf_content_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
        &request.expected_source_sha256,
        control,
    )?;

    control.checkpoint(10, "Opening reviewed page content")?;
    let mut loaded = load_pdf(&paths.input, request.input_password.as_deref())?;
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
    let had_outlines = loaded
        .document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"Outlines"));
    let original_annotation_counts = annotation_counts(&loaded.document)?;
    let catalogue = catalogue_content(&loaded.document, control)?;

    control.checkpoint(34, "Matching reviewed content objects")?;
    let prepared_text = prepare_text_edits(&loaded.document, &catalogue, &request.text_edits)?;
    let mut prepared_images = prepare_image_edits(&catalogue, &request.image_edits)?;
    let replaced_image_count = prepared_images
        .iter()
        .filter(|edit| edit.replacement.is_some())
        .count();
    let deleted_image_count = prepared_images.iter().filter(|edit| edit.delete).count();
    let repositioned_image_count = prepared_images
        .iter()
        .filter(|edit| !edit.delete && edit.rect != edit.source.public.rect)
        .count();

    control.checkpoint(43, "Embedding replacement images")?;
    let replacement_expectations =
        add_replacement_images(&mut loaded.document, &mut prepared_images, control)?;
    control.checkpoint(55, "Rewriting exact content streams")?;
    let edited_streams = apply_content_edits(
        &mut loaded.document,
        &prepared_text,
        &prepared_images,
        control,
    )?;
    loaded.document.change_producer("Tufekci Paperworks");

    control.checkpoint(66, "Writing prepared content-edited PDF")?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    loaded
        .document
        .save(prepared.path())
        .map_err(|error| format!("The content-edited PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| {
            format!("The content-edited PDF could not be flushed to storage: {error}")
        })?;

    let prepared_expectations = ContentVerificationExpectations {
        encrypted: false,
        page_count: loaded.page_count,
        annotation_counts: &original_annotation_counts,
        had_form_fields,
        had_outlines,
        original_stream_hashes: &catalogue.stream_hashes,
        edited_streams: &edited_streams,
        replacement_images: &replacement_expectations,
    };
    control.checkpoint(73, "Reopening and verifying edited content")?;
    verify_content_pdf(prepared.path(), None, &prepared_expectations)?;

    let protected = if let Some(protection) = request.output_protection.as_ref() {
        control.checkpoint(80, "Applying AES-256 output protection")?;
        let protected = TemporaryOutput::new(&paths.output)?;
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            control,
        )?;
        control.checkpoint(90, "Reopening protected edited content")?;
        let protected_expectations = ContentVerificationExpectations {
            encrypted: true,
            ..prepared_expectations
        };
        verify_content_pdf(
            protected.path(),
            Some(&protection.open_password),
            &protected_expectations,
        )?;
        Some(protected)
    } else {
        None
    };

    let final_output = protected.as_ref().unwrap_or(&prepared);
    control.checkpoint(95, "Rechecking the source PDF")?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
        &request.expected_source_sha256,
        control,
    )?;
    control.checkpoint(98, "Hashing verified output")?;
    let output_sha256 = sha256_file(final_output.path(), control)?;
    control.checkpoint(99, "Publishing verified content-edited PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;

    let mut warnings = vec![
        "Only the selected native-reviewed page content was changed. Unsupported content was preserved unchanged."
            .to_string(),
    ];
    if !prepared_text.is_empty() {
        warnings.push(
            "Replacement text keeps the reviewed position and font but is not reflowed. Review the edited page for overlap before sharing."
                .to_string(),
        );
    }
    if deleted_image_count > 0 || replaced_image_count > 0 {
        warnings.push(
            "Removed or replaced image resources may remain as unreachable or unused data. Run Privacy Cleaner before sharing when hidden-data removal matters."
                .to_string(),
        );
    }
    if request.output_protection.is_some() {
        warnings.push(
            "The edited copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if loaded.was_encrypted {
        warnings.push(
            "The edited copy is not password-protected. Enable output protection or use Protect to apply new encryption."
                .to_string(),
        );
    }
    if had_certificate_signature {
        warnings.push(
            "Page-content editing changed the PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }

    Ok(ExportPdfContentResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: loaded.page_count,
        text_edit_count: prepared_text.len(),
        image_edit_count: prepared_images.len(),
        deleted_image_count,
        replaced_image_count,
        repositioned_image_count,
        bytes_written,
        output_sha256,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_content_job_with_control(
    request: ExportPdfContentRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfContentResult, String> {
    export_pdf_content_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_content_export_error(&error)
        }
    })
}

fn catalogue_content(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<ContentCatalogue, String> {
    let pages = document.get_pages();
    let mut page_streams = BTreeMap::new();
    let mut stream_usage = HashMap::<ObjectId, usize>::new();
    let mut pages_with_unsupported_content = HashSet::new();

    for (page_number, page_id) in &pages {
        let Some(streams) = complete_page_content_streams(document, *page_id)? else {
            pages_with_unsupported_content.insert(*page_number as usize);
            continue;
        };
        for stream_id in &streams {
            *stream_usage.entry(*stream_id).or_default() += 1;
        }
        page_streams.insert(*page_number, streams);
    }
    if stream_usage.len() > MAX_STREAMS {
        return Err(format!(
            "This PDF contains more than {MAX_STREAMS} page content streams and is too complex to review safely."
        ));
    }
    let content_stream_ids = stream_usage.keys().copied().collect::<HashSet<_>>();
    let document_reference_counts =
        reference_counts_for_targets(document, &content_stream_ids, control)?;

    let mut text_sources = Vec::new();
    let mut image_sources = Vec::new();
    let mut read_only_text_count = 0_usize;
    let mut read_only_image_count = 0_usize;
    let mut stream_hashes = HashMap::new();
    let mut total_content_bytes = 0_usize;
    let mut total_operations = 0_usize;
    let total_pages = pages.len().max(1);

    for (page_offset, (page_number, page_id)) in pages.iter().enumerate() {
        if page_offset.is_multiple_of(4) {
            let progress =
                32_u8.saturating_add(((52_u128 * page_offset as u128) / total_pages as u128) as u8);
            control.checkpoint(progress, format!("Reviewing page {page_number}"))?;
        }
        let Some(stream_ids) = page_streams.get(page_number) else {
            continue;
        };
        let page_number = usize::try_from(*page_number)
            .map_err(|_| "A PDF page number is too large to review safely.".to_string())?;
        let Ok(geometry) = page_geometry(document, *page_id) else {
            pages_with_unsupported_content.insert(page_number);
            continue;
        };
        let fonts = document
            .get_page_fonts(*page_id)
            .map_err(|error| format!("Page {page_number} has invalid font resources: {error}"))?;
        let resources = effective_page_resources(document, *page_id)?;

        for (stream_offset, stream_id) in stream_ids.iter().enumerate() {
            if stream_offset.is_multiple_of(8) {
                control.ensure_not_cancelled()?;
            }
            let stream = document
                .get_object(*stream_id)
                .and_then(Object::as_stream)
                .map_err(|error| {
                    format!("Page {page_number} contains an invalid content stream: {error}")
                })?;
            let bytes = stream
                .decompressed_content_with_limit(MAX_CONTENT_STREAM_BYTES)
                .map_err(|error| {
                    format!(
                        "Page {page_number} contains a content stream that exceeds safe review limits: {error}"
                    )
                })?;
            total_content_bytes = total_content_bytes.saturating_add(bytes.len());
            if total_content_bytes > MAX_TOTAL_CONTENT_BYTES {
                return Err(format!(
                    "The PDF expands to more than {} MiB of page content and is too large to review safely.",
                    MAX_TOTAL_CONTENT_BYTES / (1024 * 1024)
                ));
            }
            let stream_sha256 = sha256_bytes(&bytes);
            stream_hashes.insert(*stream_id, stream_sha256.clone());
            let content = Content::decode_strict(&bytes).map_err(|error| {
                format!("Page {page_number} contains malformed PDF drawing operations: {error}")
            })?;
            if content.operations.len() > MAX_OPERATIONS_PER_STREAM {
                return Err(format!(
                    "Page {page_number} contains a content stream with too many drawing operations."
                ));
            }
            total_operations = total_operations.saturating_add(content.operations.len());
            if total_operations > MAX_TOTAL_OPERATIONS {
                return Err(format!(
                    "This PDF contains more than {MAX_TOTAL_OPERATIONS} drawing operations and is too complex to review safely."
                ));
            }
            let stream_is_shared = stream_usage.get(stream_id).copied().unwrap_or_default() != 1
                || document_reference_counts
                    .get(stream_id)
                    .copied()
                    .unwrap_or_default()
                    != 1;
            let scan = catalogue_stream(
                document,
                *page_id,
                page_number,
                geometry,
                *stream_id,
                &stream_sha256,
                &content.operations,
                &fonts,
                resources.as_ref(),
                stream_is_shared,
            )?;
            read_only_text_count = read_only_text_count.saturating_add(scan.read_only_text_count);
            read_only_image_count =
                read_only_image_count.saturating_add(scan.read_only_image_count);
            if scan.unsupported_content {
                pages_with_unsupported_content.insert(page_number);
            }
            if text_sources.len().saturating_add(scan.text_sources.len()) > MAX_EDITABLE_TEXT_RUNS {
                return Err(format!(
                    "This PDF contains more than {MAX_EDITABLE_TEXT_RUNS} editable text objects. Split it before editing page content."
                ));
            }
            if image_sources.len().saturating_add(scan.image_sources.len()) > MAX_EDITABLE_IMAGES {
                return Err(format!(
                    "This PDF contains more than {MAX_EDITABLE_IMAGES} editable image objects. Split it before editing page content."
                ));
            }
            text_sources.extend(scan.text_sources);
            image_sources.extend(scan.image_sources);
        }
    }

    let mut pages_with_unsupported_content = pages_with_unsupported_content
        .into_iter()
        .collect::<Vec<_>>();
    pages_with_unsupported_content.sort_unstable();
    Ok(ContentCatalogue {
        text_sources,
        image_sources,
        read_only_text_count,
        read_only_image_count,
        pages_with_unsupported_content,
        stream_hashes,
    })
}

struct StreamCatalogue {
    text_sources: Vec<TextSource>,
    image_sources: Vec<ImageSource>,
    read_only_text_count: usize,
    read_only_image_count: usize,
    unsupported_content: bool,
}

#[derive(Clone)]
struct TextState {
    in_text: bool,
    font_name: Option<Vec<u8>>,
    font_size: f64,
    horizontal_scale: f64,
    character_spacing: f64,
    word_spacing: f64,
    leading: f64,
    rise: f64,
    text_matrix: Matrix,
    line_matrix: Matrix,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            in_text: false,
            font_name: None,
            font_size: 0.0,
            horizontal_scale: 1.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            leading: 0.0,
            rise: 0.0,
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn catalogue_stream(
    document: &Document,
    page_id: ObjectId,
    page_number: usize,
    geometry: PageGeometry,
    stream_id: ObjectId,
    stream_sha256: &str,
    operations: &[Operation],
    fonts: &BTreeMap<Vec<u8>, &Dictionary>,
    resources: Option<&Dictionary>,
    stream_is_shared: bool,
) -> Result<StreamCatalogue, String> {
    let mut result = StreamCatalogue {
        text_sources: Vec::new(),
        image_sources: Vec::new(),
        read_only_text_count: 0,
        read_only_image_count: 0,
        unsupported_content: stream_is_shared,
    };
    let mut text = TextState::default();
    let mut current_ctm = Matrix::IDENTITY;
    let mut graphics_stack = Vec::<Matrix>::new();

    for (index, operation) in operations.iter().enumerate() {
        match operation.operator.as_str() {
            "q" => {
                graphics_stack.push(current_ctm);
                if graphics_stack.len() > 128 {
                    result.unsupported_content = true;
                }
            }
            "Q" => {
                if let Some(previous) = graphics_stack.pop() {
                    current_ctm = previous;
                } else {
                    result.unsupported_content = true;
                }
            }
            "cm" => {
                if let Some(matrix) = operation_matrix(operation) {
                    current_ctm = current_ctm.then(matrix);
                } else {
                    result.unsupported_content = true;
                }
            }
            "BT" => {
                text = TextState {
                    in_text: true,
                    ..TextState::default()
                };
            }
            "ET" => text.in_text = false,
            "Tf" if text.in_text => {
                text.font_name = operation
                    .operands
                    .first()
                    .and_then(|operand| operand.as_name().ok())
                    .map(ToOwned::to_owned);
                text.font_size = operation
                    .operands
                    .get(1)
                    .and_then(pdf_number)
                    .unwrap_or_default();
            }
            "Tz" if text.in_text => {
                text.horizontal_scale = operation
                    .operands
                    .first()
                    .and_then(pdf_number)
                    .map(|value| value / 100.0)
                    .unwrap_or(1.0);
            }
            "Tc" if text.in_text => {
                text.character_spacing = operation
                    .operands
                    .first()
                    .and_then(pdf_number)
                    .unwrap_or_default();
            }
            "Tw" if text.in_text => {
                text.word_spacing = operation
                    .operands
                    .first()
                    .and_then(pdf_number)
                    .unwrap_or_default();
            }
            "TL" if text.in_text => {
                text.leading = operation
                    .operands
                    .first()
                    .and_then(pdf_number)
                    .unwrap_or_default();
            }
            "Ts" if text.in_text => {
                text.rise = operation
                    .operands
                    .first()
                    .and_then(pdf_number)
                    .unwrap_or_default();
            }
            "Tm" if text.in_text => {
                if let Some(matrix) = operation_matrix(operation) {
                    text.text_matrix = matrix;
                    text.line_matrix = matrix;
                } else {
                    result.unsupported_content = true;
                }
            }
            "Td" | "TD" if text.in_text => {
                let tx = operation.operands.first().and_then(pdf_number);
                let ty = operation.operands.get(1).and_then(pdf_number);
                if let (Some(tx), Some(ty)) = (tx, ty) {
                    let translation = Matrix {
                        e: tx,
                        f: ty,
                        ..Matrix::IDENTITY
                    };
                    text.line_matrix = text.line_matrix.then(translation);
                    text.text_matrix = text.line_matrix;
                    if operation.operator == "TD" {
                        text.leading = -ty;
                    }
                } else {
                    result.unsupported_content = true;
                }
            }
            "T*" if text.in_text => advance_text_line(&mut text),
            "Tj" if text.in_text => {
                let source = editable_text_source(
                    document,
                    page_number,
                    geometry,
                    stream_id,
                    stream_sha256,
                    index,
                    operation,
                    fonts,
                    &text,
                    current_ctm,
                    stream_is_shared,
                );
                if let Some(source) = source {
                    result.text_sources.push(source);
                } else {
                    result.read_only_text_count += 1;
                    result.unsupported_content = true;
                }
                advance_text_for_operation(document, operation, fonts, &mut text);
            }
            "TJ" | "'" | "\"" if text.in_text => {
                result.read_only_text_count += 1;
                result.unsupported_content = true;
                if matches!(operation.operator.as_str(), "'" | "\"") {
                    advance_text_line(&mut text);
                }
                advance_text_for_operation(document, operation, fonts, &mut text);
            }
            "Do" => {
                if operation_resolves_to_image(document, resources, operation) {
                    if let Some(source) = editable_image_source(
                        document,
                        page_id,
                        page_number,
                        geometry,
                        stream_id,
                        stream_sha256,
                        operations,
                        index,
                        resources,
                        current_ctm,
                        stream_is_shared,
                    ) {
                        result.image_sources.push(source);
                    } else {
                        result.read_only_image_count += 1;
                        result.unsupported_content = true;
                    }
                } else if operation_resolves_to_form(document, resources, operation) {
                    result.unsupported_content = true;
                }
            }
            "BI" | "ID" | "EI" => {
                result.read_only_image_count += usize::from(operation.operator == "BI");
                result.unsupported_content = true;
            }
            _ => {}
        }
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn editable_text_source(
    document: &Document,
    page_number: usize,
    geometry: PageGeometry,
    stream_id: ObjectId,
    stream_sha256: &str,
    operation_index: usize,
    operation: &Operation,
    fonts: &BTreeMap<Vec<u8>, &Dictionary>,
    text_state: &TextState,
    ctm: Matrix,
    stream_is_shared: bool,
) -> Option<TextSource> {
    if stream_is_shared
        || operation.operands.len() != 1
        || !text_state.font_size.is_finite()
        || !(1.0..=512.0).contains(&text_state.font_size.abs())
    {
        return None;
    }
    let Object::String(original_bytes, _) = operation.operands.first()? else {
        return None;
    };
    if original_bytes.len() > MAX_TEXT_BYTES {
        return None;
    }
    let font_name = text_state.font_name.as_ref()?;
    let font = fonts.get(font_name)?;
    let encoding = font
        .get_font_encoding_with_limit(document, MAX_CONTENT_STREAM_BYTES)
        .ok()?;
    let decoded = Document::decode_text(&encoding, original_bytes).ok()?;
    if decoded.is_empty()
        || decoded.chars().count() > MAX_TEXT_CHARACTERS
        || decoded.contains('\u{fffd}')
        || decoded
            .chars()
            .any(|character| character.is_control() && character != '\t')
    {
        return None;
    }
    if Document::encode_text(&encoding, &decoded) != *original_bytes {
        return None;
    }
    let width = text_advance_width(font, original_bytes, &decoded, text_state).abs();
    let transform = ctm.then(text_state.text_matrix);
    let lower = -text_state.font_size.abs() * 0.25 + text_state.rise;
    let upper = text_state.font_size.abs() * 0.9 + text_state.rise;
    let points = [
        transform.transform(0.0, lower),
        transform.transform(width.max(text_state.font_size.abs() * 0.2), lower),
        transform.transform(0.0, upper),
        transform.transform(width.max(text_state.font_size.abs() * 0.2), upper),
    ];
    let rect = normalised_rect_from_pdf_points(geometry, &points)?;
    let source_id = opaque_source_id(
        "text",
        page_number,
        stream_id,
        operation_index,
        stream_sha256,
        &[font_name, original_bytes],
    );
    Some(TextSource {
        public: PdfEditableTextRun {
            source_id,
            page_number,
            text: decoded,
            rect,
            font_label: font_label(font, font_name),
            font_size: text_state.font_size.abs(),
        },
        stream_id,
        operation_index,
        font_name: font_name.clone(),
        original_bytes: original_bytes.clone(),
        original_stream_sha256: stream_sha256.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn editable_image_source(
    document: &Document,
    page_id: ObjectId,
    page_number: usize,
    geometry: PageGeometry,
    stream_id: ObjectId,
    stream_sha256: &str,
    operations: &[Operation],
    operation_index: usize,
    resources: Option<&Dictionary>,
    current_ctm: Matrix,
    stream_is_shared: bool,
) -> Option<ImageSource> {
    if stream_is_shared || operation_index < 2 || operation_index + 1 >= operations.len() {
        return None;
    }
    let block_start_index = operation_index - 2;
    if operations[block_start_index].operator != "q"
        || operations[operation_index - 1].operator != "cm"
        || operations[operation_index + 1].operator != "Q"
    {
        return None;
    }
    let matrix = operation_matrix(&operations[operation_index - 1])?;
    if !matrix_approximately_equal(current_ctm, matrix)
        || !approximately(matrix.b, 0.0)
        || !approximately(matrix.c, 0.0)
        || matrix.a <= 0.0
        || matrix.d <= 0.0
    {
        return None;
    }
    let operation = &operations[operation_index];
    let resource_name = operation.operands.first()?.as_name().ok()?.to_vec();
    let image_id = resolve_xobject_id(document, resources?, &resource_name)?;
    let image = document.get_object(image_id).ok()?.as_stream().ok()?;
    if image.dict.get(b"Subtype").and_then(Object::as_name).ok()? != b"Image" {
        return None;
    }
    let pixel_width = u32::try_from(image.dict.get(b"Width").ok()?.as_i64().ok()?).ok()?;
    let pixel_height = u32::try_from(image.dict.get(b"Height").ok()?.as_i64().ok()?).ok()?;
    if pixel_width == 0
        || pixel_height == 0
        || pixel_width > MAX_IMAGE_DIMENSION
        || pixel_height > MAX_IMAGE_DIMENSION
    {
        return None;
    }
    let points = [
        matrix.transform(0.0, 0.0),
        matrix.transform(1.0, 0.0),
        matrix.transform(0.0, 1.0),
        matrix.transform(1.0, 1.0),
    ];
    let rect = normalised_rect_from_pdf_points(geometry, &points)?;
    let source_id = opaque_source_id(
        "image",
        page_number,
        stream_id,
        operation_index,
        stream_sha256,
        &[
            &resource_name,
            &image_id.0.to_be_bytes(),
            &image_id.1.to_be_bytes(),
        ],
    );
    Some(ImageSource {
        public: PdfEditableImage {
            source_id,
            page_number,
            rect,
            pixel_width,
            pixel_height,
        },
        page_id,
        stream_id,
        block_start_index,
        cm_operation_index: operation_index - 1,
        operation_index,
        resource_name,
        original_stream_sha256: stream_sha256.to_string(),
    })
}

fn prepare_text_edits(
    document: &Document,
    catalogue: &ContentCatalogue,
    edits: &[PdfTextEdit],
) -> Result<Vec<PreparedTextEdit>, String> {
    let sources = catalogue
        .text_sources
        .iter()
        .map(|source| (source.public.source_id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let pages = document.get_pages();
    let mut prepared = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let source = sources.get(edit.source_id.as_str()).ok_or_else(|| {
            format!(
                "Text edit {} no longer matches a native-reviewed source object. Review the PDF again.",
                index + 1
            )
        })?;
        if edit.replacement_text == source.public.text {
            return Err(format!(
                "Text edit {} does not change the reviewed text.",
                index + 1
            ));
        }
        let page_number = u32::try_from(source.public.page_number)
            .map_err(|_| "A reviewed text page number is invalid.".to_string())?;
        let page_id = pages
            .get(&page_number)
            .copied()
            .ok_or_else(|| "A reviewed text page no longer exists.".to_string())?;
        let fonts = document
            .get_page_fonts(page_id)
            .map_err(|error| format!("A reviewed text font is invalid: {error}"))?;
        let font = fonts
            .get(&source.font_name)
            .ok_or_else(|| "A reviewed text font no longer matches the source PDF.".to_string())?;
        let encoding = font
            .get_font_encoding_with_limit(document, MAX_CONTENT_STREAM_BYTES)
            .map_err(|_| {
                "A reviewed text font encoding could not be reopened safely.".to_string()
            })?;
        let encoded = Document::encode_text(&encoding, &edit.replacement_text);
        let decoded = Document::decode_text(&encoding, &encoded).map_err(|_| {
            format!(
                "Text edit {} cannot be represented by the original PDF font.",
                index + 1
            )
        })?;
        if decoded != edit.replacement_text
            || (!edit.replacement_text.is_empty() && encoded.is_empty())
        {
            return Err(format!(
                "Text edit {} contains characters the original PDF font cannot reproduce exactly.",
                index + 1
            ));
        }
        prepared.push(PreparedTextEdit {
            source: (*source).clone(),
            encoded,
        });
    }
    Ok(prepared)
}

fn prepare_image_edits(
    catalogue: &ContentCatalogue,
    edits: &[PdfImageEdit],
) -> Result<Vec<PreparedImageEdit>, String> {
    let sources = catalogue
        .image_sources
        .iter()
        .map(|source| (source.public.source_id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let mut prepared = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let source = sources.get(edit.source_id.as_str()).ok_or_else(|| {
            format!(
                "Image edit {} no longer matches a native-reviewed source object. Review the PDF again.",
                index + 1
            )
        })?;
        if !edit.delete
            && edit.replacement_image_data_url.is_none()
            && edit.rect == source.public.rect
        {
            return Err(format!(
                "Image edit {} does not change the reviewed image.",
                index + 1
            ));
        }
        let replacement = edit
            .replacement_image_data_url
            .as_deref()
            .map(decode_replacement_image)
            .transpose()?;
        prepared.push(PreparedImageEdit {
            source: (*source).clone(),
            delete: edit.delete,
            rect: edit.rect,
            replacement,
            replacement_resource_name: None,
        });
    }
    Ok(prepared)
}

fn add_replacement_images(
    document: &mut Document,
    edits: &mut [PreparedImageEdit],
    control: &PdfJobExecutionControl,
) -> Result<Vec<ExpectedReplacementImage>, String> {
    let replacement_count = edits
        .iter()
        .filter(|edit| edit.replacement.is_some())
        .count();
    let mut completed = 0_usize;
    let mut expected = Vec::with_capacity(replacement_count);
    for edit in edits {
        let Some(image) = edit.replacement.take() else {
            continue;
        };
        if completed.is_multiple_of(8) {
            control.ensure_not_cancelled()?;
        }
        let marker = format!("content-image-{}", &edit.source.public.source_id[6..38]);
        let (image_id, expectation) = add_replacement_image_xobject(document, image, marker)?;
        let resource_name = add_page_image_resource(
            document,
            edit.source.page_id,
            image_id,
            &edit.source.public.source_id,
        )?;
        edit.replacement_resource_name = Some(resource_name);
        expected.push(expectation);
        completed += 1;
    }
    Ok(expected)
}

#[derive(Clone)]
enum PreparedStreamEdit {
    Text {
        operation_index: usize,
        original_bytes: Vec<u8>,
        encoded: Vec<u8>,
    },
    Image {
        block_start_index: usize,
        cm_operation_index: usize,
        operation_index: usize,
        original_resource_name: Vec<u8>,
        replacement_resource_name: Option<Vec<u8>>,
        delete: bool,
        rect: NormalisedRect,
        geometry: PageGeometry,
    },
}

impl PreparedStreamEdit {
    fn operation_index(&self) -> usize {
        match self {
            Self::Text {
                operation_index, ..
            }
            | Self::Image {
                operation_index, ..
            } => *operation_index,
        }
    }
}

struct StreamEditGroup {
    page_number: usize,
    original_sha256: String,
    edits: Vec<PreparedStreamEdit>,
}

fn apply_content_edits(
    document: &mut Document,
    text_edits: &[PreparedTextEdit],
    image_edits: &[PreparedImageEdit],
    control: &PdfJobExecutionControl,
) -> Result<Vec<ExpectedEditedStream>, String> {
    let pages = document.get_pages();
    let mut groups = HashMap::<ObjectId, StreamEditGroup>::new();
    for edit in text_edits {
        let group = groups
            .entry(edit.source.stream_id)
            .or_insert_with(|| StreamEditGroup {
                page_number: edit.source.public.page_number,
                original_sha256: edit.source.original_stream_sha256.clone(),
                edits: Vec::new(),
            });
        if group.page_number != edit.source.public.page_number
            || group.original_sha256 != edit.source.original_stream_sha256
        {
            return Err("Reviewed text objects disagree about their source stream.".to_string());
        }
        group.edits.push(PreparedStreamEdit::Text {
            operation_index: edit.source.operation_index,
            original_bytes: edit.source.original_bytes.clone(),
            encoded: edit.encoded.clone(),
        });
    }
    for edit in image_edits {
        let geometry = page_geometry(document, edit.source.page_id)?;
        let group = groups
            .entry(edit.source.stream_id)
            .or_insert_with(|| StreamEditGroup {
                page_number: edit.source.public.page_number,
                original_sha256: edit.source.original_stream_sha256.clone(),
                edits: Vec::new(),
            });
        if group.page_number != edit.source.public.page_number
            || group.original_sha256 != edit.source.original_stream_sha256
        {
            return Err("Reviewed image objects disagree about their source stream.".to_string());
        }
        group.edits.push(PreparedStreamEdit::Image {
            block_start_index: edit.source.block_start_index,
            cm_operation_index: edit.source.cm_operation_index,
            operation_index: edit.source.operation_index,
            original_resource_name: edit.source.resource_name.clone(),
            replacement_resource_name: edit.replacement_resource_name.clone(),
            delete: edit.delete,
            rect: edit.rect,
            geometry,
        });
    }

    let mut stream_ids = groups.keys().copied().collect::<Vec<_>>();
    stream_ids.sort_unstable();
    let mut expected = Vec::with_capacity(stream_ids.len());
    for (stream_offset, stream_id) in stream_ids.into_iter().enumerate() {
        control.ensure_not_cancelled()?;
        let mut group = groups
            .remove(&stream_id)
            .ok_or_else(|| "A prepared content stream edit is missing.".to_string())?;
        let page_id = pages
            .get(
                &u32::try_from(group.page_number)
                    .map_err(|_| "A prepared content stream page number is invalid.".to_string())?,
            )
            .copied()
            .ok_or_else(|| "A prepared content stream page no longer exists.".to_string())?;
        if !document.get_page_contents(page_id).contains(&stream_id) {
            return Err("A reviewed content stream no longer belongs to its page.".to_string());
        }
        let original = document
            .get_object(stream_id)
            .and_then(Object::as_stream)
            .map_err(|error| format!("A reviewed content stream is invalid: {error}"))?
            .decompressed_content_with_limit(MAX_CONTENT_STREAM_BYTES)
            .map_err(|error| format!("A reviewed content stream could not be decoded: {error}"))?;
        if sha256_bytes(&original) != group.original_sha256 {
            return Err("A reviewed content stream changed before export.".to_string());
        }
        let mut content = Content::decode_strict(&original)
            .map_err(|error| format!("A reviewed content stream is malformed: {error}"))?;
        group
            .edits
            .sort_by_key(|edit| std::cmp::Reverse(edit.operation_index()));
        for edit in &group.edits {
            apply_stream_edit(&mut content.operations, edit)?;
        }
        let encoded = content
            .encode()
            .map_err(|error| format!("The edited content stream could not be encoded: {error}"))?;
        let content_sha256 = sha256_bytes(&encoded);
        let marker = format!(
            "content-stream-{}",
            &sha256_bytes(
                format!(
                    "{}:{}:{}:{}",
                    group.page_number, stream_id.0, stream_id.1, content_sha256
                )
                .as_bytes()
            )[..32]
        );
        let stream = document
            .get_object_mut(stream_id)
            .and_then(Object::as_stream_mut)
            .map_err(|error| format!("The edited content stream could not be updated: {error}"))?;
        stream.set_plain_content(encoded);
        stream.compress().map_err(|error| {
            format!("The edited content stream could not be compressed: {error}")
        })?;
        stream.dict.set(
            STREAM_EDIT_MARKER,
            Object::String(marker.as_bytes().to_vec(), StringFormat::Literal),
        );
        stream.dict.set(
            STREAM_EDIT_SHA,
            Object::String(content_sha256.as_bytes().to_vec(), StringFormat::Literal),
        );
        stream.dict.set(
            STREAM_EDIT_COUNT,
            i64::try_from(group.edits.len())
                .map_err(|_| "A content stream contains too many edits.".to_string())?,
        );
        expected.push(ExpectedEditedStream {
            marker,
            page_number: group.page_number,
            original_stream_id: stream_id,
            content_sha256,
            edit_count: group.edits.len(),
        });
        if stream_offset.is_multiple_of(8) {
            control.ensure_not_cancelled()?;
        }
    }
    Ok(expected)
}

fn apply_stream_edit(
    operations: &mut Vec<Operation>,
    edit: &PreparedStreamEdit,
) -> Result<(), String> {
    match edit {
        PreparedStreamEdit::Text {
            operation_index,
            original_bytes,
            encoded,
        } => {
            let operation = operations.get_mut(*operation_index).ok_or_else(|| {
                "A reviewed text operation is no longer present in its stream.".to_string()
            })?;
            if operation.operator != "Tj" || operation.operands.len() != 1 {
                return Err("A reviewed text operation changed before export.".to_string());
            }
            let Object::String(bytes, _) = &mut operation.operands[0] else {
                return Err("A reviewed text value changed before export.".to_string());
            };
            if bytes != original_bytes {
                return Err("A reviewed text value changed before export.".to_string());
            }
            bytes.clone_from(encoded);
        }
        PreparedStreamEdit::Image {
            block_start_index,
            cm_operation_index,
            operation_index,
            original_resource_name,
            replacement_resource_name,
            delete,
            rect,
            geometry,
        } => {
            let valid_block = operations
                .get(*block_start_index)
                .is_some_and(|operation| operation.operator == "q")
                && operations
                    .get(*cm_operation_index)
                    .is_some_and(|operation| operation.operator == "cm")
                && operations.get(*operation_index).is_some_and(|operation| {
                    operation.operator == "Do"
                        && operation.operands.len() == 1
                        && operation.operands[0]
                            .as_name()
                            .is_ok_and(|name| name == original_resource_name)
                })
                && operations
                    .get(*operation_index + 1)
                    .is_some_and(|operation| operation.operator == "Q");
            if !valid_block {
                return Err("A reviewed image placement changed before export.".to_string());
            }
            if *delete {
                operations.drain(*block_start_index..=*operation_index + 1);
                return Ok(());
            }
            let mapped = map_rect_to_pdf(*geometry, *rect);
            operations[*cm_operation_index].operands = vec![
                pdf_real(mapped.width),
                0.into(),
                0.into(),
                pdf_real(mapped.height),
                pdf_real(mapped.left),
                pdf_real(mapped.bottom),
            ];
            if let Some(resource_name) = replacement_resource_name {
                operations[*operation_index].operands = vec![Object::Name(resource_name.clone())];
            }
        }
    }
    Ok(())
}

fn add_replacement_image_xobject(
    document: &mut Document,
    mut image: DynamicImage,
    marker: String,
) -> Result<(ObjectId, ExpectedReplacementImage), String> {
    if image.width() > MAX_EMBEDDED_IMAGE_DIMENSION || image.height() > MAX_EMBEDDED_IMAGE_DIMENSION
    {
        image = image.thumbnail(MAX_EMBEDDED_IMAGE_DIMENSION, MAX_EMBEDDED_IMAGE_DIMENSION);
    }
    let rgba = image.to_rgba8();
    if !rgba.pixels().any(|pixel| pixel.0[3] > 0) {
        return Err("A replacement image does not contain any visible pixels.".to_string());
    }
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    let mut alpha = Vec::with_capacity(rgba.len() / 4);
    let mut has_transparency = false;
    for pixel in rgba.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
        alpha.push(pixel.0[3]);
        has_transparency |= pixel.0[3] != u8::MAX;
    }
    let content_sha256 = sha256_bytes(&rgb);
    let alpha_id = if has_transparency {
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
        alpha_stream.compress().map_err(|error| {
            format!("Replacement image transparency could not be compressed: {error}")
        })?;
        Some(document.add_object(alpha_stream))
    } else {
        None
    };
    let mut dictionary = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(rgba.width()),
        "Height" => i64::from(rgba.height()),
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        IMAGE_EDIT_MARKER => Object::String(marker.as_bytes().to_vec(), StringFormat::Literal),
        IMAGE_EDIT_SHA => Object::String(content_sha256.as_bytes().to_vec(), StringFormat::Literal),
    };
    if let Some(alpha_id) = alpha_id {
        dictionary.set("SMask", alpha_id);
    }
    let mut image_stream = Stream::new(dictionary, rgb);
    image_stream
        .compress()
        .map_err(|error| format!("The replacement image could not be compressed: {error}"))?;
    let image_id = document.add_object(image_stream);
    Ok((
        image_id,
        ExpectedReplacementImage {
            marker,
            content_sha256,
            pixel_width: rgba.width(),
            pixel_height: rgba.height(),
        },
    ))
}

fn add_page_image_resource(
    document: &mut Document,
    page_id: ObjectId,
    image_id: ObjectId,
    source_id: &str,
) -> Result<Vec<u8>, String> {
    let mut resources = effective_page_resources(document, page_id)?.unwrap_or_default();
    let mut xobjects = match resources.get(b"XObject") {
        Ok(Object::Dictionary(dictionary)) => dictionary.clone(),
        Ok(Object::Reference(id)) => document
            .get_dictionary(*id)
            .cloned()
            .map_err(|error| format!("The page image resource list is invalid: {error}"))?,
        Err(_) | Ok(Object::Null) => Dictionary::new(),
        Ok(_) => return Err("The page image resource list is invalid.".to_string()),
    };
    let stem = format!("PWCE{}", &source_id[6..18]);
    let mut resource_name = stem.as_bytes().to_vec();
    for suffix in 0..1_000_usize {
        if xobjects.get(&resource_name).is_err() {
            break;
        }
        resource_name = format!("{stem}{suffix}").into_bytes();
    }
    if xobjects.get(&resource_name).is_ok() {
        return Err("A unique replacement image resource name could not be allocated.".to_string());
    }
    xobjects.set(resource_name.clone(), image_id);
    resources.set("XObject", Object::Dictionary(xobjects));
    let page = document
        .get_dictionary_mut(page_id)
        .map_err(|error| format!("The replacement image page is invalid: {error}"))?;
    page.set("Resources", Object::Dictionary(resources));
    Ok(resource_name)
}

fn decode_replacement_image(data_url: &str) -> Result<DynamicImage, String> {
    let (header, encoded) = data_url
        .split_once(',')
        .ok_or_else(|| "A replacement image is not a valid data URL.".to_string())?;
    if !header.starts_with("data:image/") || !header.ends_with(";base64") {
        return Err("Replacement images must use a base64 image data URL.".to_string());
    }
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| "A replacement image is not valid base64 data.".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_DATA_BYTES {
        return Err("A replacement image is empty or too large to export safely.".to_string());
    }
    match catch_unwind(AssertUnwindSafe(|| -> Result<DynamicImage, String> {
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| {
                format!("The replacement image format could not be detected: {error}")
            })?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_IMAGE_ALLOCATION);
        reader.limits(limits);
        reader
            .decode()
            .map_err(|error| format!("The replacement image could not be decoded: {error}"))
    })) {
        Ok(result) => result,
        Err(_) => Err("The replacement image was rejected safely.".to_string()),
    }
}

fn verify_content_pdf(
    path: &Path,
    password: Option<&str>,
    expected: &ContentVerificationExpectations<'_>,
) -> Result<(), String> {
    let mut verification = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The content-edited PDF failed its reopening check: {error}"))?;
    let encrypted = verification.is_encrypted();
    if encrypted != expected.encrypted {
        return Err(if expected.encrypted {
            "The content-edited PDF was not encrypted as requested and was not saved.".to_string()
        } else {
            "The content-edited PDF unexpectedly remained encrypted and was not saved.".to_string()
        });
    }
    if encrypted {
        verification
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "The protected content-edited PDF could not be reopened with its new password."
                    .to_string()
            })?;
    }
    let pages = verification.get_pages();
    if pages.len() != expected.page_count {
        return Err("The content-edited PDF changed the page count and was not saved.".to_string());
    }
    if expected.had_form_fields
        && !verification
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        return Err(
            "The content-edited PDF lost its form structure and was not saved.".to_string(),
        );
    }
    if expected.had_outlines
        && !verification
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"Outlines"))
    {
        return Err("The content-edited PDF lost its bookmarks and was not saved.".to_string());
    }
    if annotation_counts(&verification)? != expected.annotation_counts {
        return Err(
            "The content-edited PDF changed its annotations and was not saved.".to_string(),
        );
    }

    let expected_by_marker = expected
        .edited_streams
        .iter()
        .map(|stream| (stream.marker.as_str(), stream))
        .collect::<HashMap<_, _>>();
    let mut found_markers = HashSet::new();
    let mut actual_page_stream_hashes = HashMap::<ObjectId, String>::new();
    for (page_number, page_id) in &pages {
        let streams = complete_page_content_streams(&verification, *page_id)?.ok_or_else(|| {
            "The content-edited PDF contains an unverifiable direct page stream.".to_string()
        })?;
        for stream_id in streams {
            let stream = verification
                .get_object(stream_id)
                .and_then(Object::as_stream)
                .map_err(|error| format!("The verified page stream is invalid: {error}"))?;
            let content = stream
                .decompressed_content_with_limit(MAX_CONTENT_STREAM_BYTES)
                .map_err(|error| {
                    format!("The verified page stream could not be decoded: {error}")
                })?;
            let content_sha256 = sha256_bytes(&content);
            actual_page_stream_hashes.insert(stream_id, content_sha256.clone());
            let Some(marker) = dictionary_literal_string(&stream.dict, STREAM_EDIT_MARKER) else {
                continue;
            };
            let stream_expectation = expected_by_marker.get(marker.as_str()).ok_or_else(|| {
                "The content-edited PDF contains an unexpected native edit marker.".to_string()
            })?;
            if !found_markers.insert(marker.clone()) {
                return Err("The content-edited PDF repeats a native edit marker.".to_string());
            }
            if usize::try_from(*page_number).ok() != Some(stream_expectation.page_number) {
                return Err("An edited content stream moved to a different page.".to_string());
            }
            if content_sha256 != stream_expectation.content_sha256
                || dictionary_literal_string(&stream.dict, STREAM_EDIT_SHA).as_deref()
                    != Some(stream_expectation.content_sha256.as_str())
                || stream
                    .dict
                    .get(STREAM_EDIT_COUNT)
                    .and_then(Object::as_i64)
                    .ok()
                    .and_then(|value| usize::try_from(value).ok())
                    != Some(stream_expectation.edit_count)
            {
                return Err("An edited content stream failed its exact hash check.".to_string());
            }
        }
    }
    if actual_page_stream_hashes.len() != expected.original_stream_hashes.len() {
        return Err(
            "The content-edited PDF changed the number of page content streams and was not saved."
                .to_string(),
        );
    }
    if found_markers.len() != expected.edited_streams.len() {
        return Err("A verified content edit marker is missing from the output.".to_string());
    }

    let edited_original_ids = expected
        .edited_streams
        .iter()
        .map(|stream| stream.original_stream_id)
        .collect::<HashSet<_>>();
    if !expected.encrypted {
        for (stream_id, original_sha256) in expected.original_stream_hashes {
            if edited_original_ids.contains(stream_id) {
                continue;
            }
            if actual_page_stream_hashes.get(stream_id) != Some(original_sha256) {
                return Err(
                    "An untouched page content stream changed during export and the PDF was not saved."
                        .to_string(),
                );
            }
        }
    } else {
        let mut actual_hash_counts = HashMap::<&str, usize>::new();
        for hash in actual_page_stream_hashes.values() {
            *actual_hash_counts.entry(hash.as_str()).or_default() += 1;
        }
        for (stream_id, original_sha256) in expected.original_stream_hashes {
            if edited_original_ids.contains(stream_id) {
                continue;
            }
            let count = actual_hash_counts
                .entry(original_sha256.as_str())
                .or_default();
            if *count == 0 {
                return Err(
                    "An untouched page content stream was not preserved through output protection."
                        .to_string(),
                );
            }
            *count -= 1;
        }
    }
    verify_replacement_images(&verification, expected.replacement_images)?;
    Ok(())
}

fn verify_replacement_images(
    document: &Document,
    expected: &[ExpectedReplacementImage],
) -> Result<(), String> {
    let expected_by_marker = expected
        .iter()
        .map(|image| (image.marker.as_str(), image))
        .collect::<HashMap<_, _>>();
    let mut found = HashSet::new();
    for object in document.objects.values() {
        let Ok(stream) = object.as_stream() else {
            continue;
        };
        let Some(marker) = dictionary_literal_string(&stream.dict, IMAGE_EDIT_MARKER) else {
            continue;
        };
        let expectation = expected_by_marker.get(marker.as_str()).ok_or_else(|| {
            "The content-edited PDF contains an unexpected replacement image marker.".to_string()
        })?;
        if !found.insert(marker.clone()) {
            return Err("The content-edited PDF repeats a replacement image marker.".to_string());
        }
        let width = stream
            .dict
            .get(b"Width")
            .and_then(Object::as_i64)
            .ok()
            .and_then(|value| u32::try_from(value).ok());
        let height = stream
            .dict
            .get(b"Height")
            .and_then(Object::as_i64)
            .ok()
            .and_then(|value| u32::try_from(value).ok());
        let content = stream
            .decompressed_content_with_limit(MAX_IMAGE_ALLOCATION as usize)
            .map_err(|error| format!("A replacement image failed its reopening check: {error}"))?;
        if width != Some(expectation.pixel_width)
            || height != Some(expectation.pixel_height)
            || sha256_bytes(&content) != expectation.content_sha256
            || dictionary_literal_string(&stream.dict, IMAGE_EDIT_SHA).as_deref()
                != Some(expectation.content_sha256.as_str())
            || stream.dict.get(b"Subtype").and_then(Object::as_name).ok()
                != Some(b"Image".as_slice())
        {
            return Err("A replacement image failed its exact reopening check.".to_string());
        }
    }
    if found.len() != expected.len() {
        return Err("A verified replacement image is missing from the output.".to_string());
    }
    Ok(())
}

fn complete_page_content_streams(
    document: &Document,
    page_id: ObjectId,
) -> Result<Option<Vec<ObjectId>>, String> {
    let page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("The PDF page tree is invalid: {error}"))?;
    let contents = match page.get(b"Contents") {
        Err(_) | Ok(Object::Null) => return Ok(Some(Vec::new())),
        Ok(contents) => contents,
    };
    let mut streams = Vec::new();
    let mut visited = HashSet::new();
    if collect_content_stream_ids(document, contents, &mut streams, &mut visited, 0)? {
        Ok(Some(streams))
    } else {
        Ok(None)
    }
}

fn reference_counts_for_targets(
    document: &Document,
    targets: &HashSet<ObjectId>,
    control: &PdfJobExecutionControl,
) -> Result<HashMap<ObjectId, usize>, String> {
    let mut counts = HashMap::new();
    let mut inspected_nodes = 0_usize;
    for object in document.objects.values() {
        count_target_references(
            object,
            targets,
            &mut counts,
            &mut inspected_nodes,
            control,
            0,
        )?;
    }
    count_target_references(
        &Object::Dictionary(document.trailer.clone()),
        targets,
        &mut counts,
        &mut inspected_nodes,
        control,
        0,
    )?;
    Ok(counts)
}

fn count_target_references(
    value: &Object,
    targets: &HashSet<ObjectId>,
    counts: &mut HashMap<ObjectId, usize>,
    inspected_nodes: &mut usize,
    control: &PdfJobExecutionControl,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PAGE_TREE_DEPTH {
        return Err("The PDF contains excessively nested direct objects.".to_string());
    }
    *inspected_nodes = inspected_nodes.saturating_add(1);
    if inspected_nodes.is_multiple_of(4_096) {
        control.ensure_not_cancelled()?;
    }
    if *inspected_nodes > MAX_REFERENCE_SCAN_NODES {
        return Err(
            "The PDF contains too many direct objects to review content-stream ownership safely."
                .to_string(),
        );
    }
    match value {
        Object::Reference(id) => {
            if targets.contains(id) {
                *counts.entry(*id).or_default() += 1;
            }
        }
        Object::Array(values) => {
            for item in values {
                count_target_references(
                    item,
                    targets,
                    counts,
                    inspected_nodes,
                    control,
                    depth + 1,
                )?;
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, item) in dictionary.iter() {
                count_target_references(
                    item,
                    targets,
                    counts,
                    inspected_nodes,
                    control,
                    depth + 1,
                )?;
            }
        }
        Object::Stream(stream) => {
            for (_, item) in stream.dict.iter() {
                count_target_references(
                    item,
                    targets,
                    counts,
                    inspected_nodes,
                    control,
                    depth + 1,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn collect_content_stream_ids(
    document: &Document,
    value: &Object,
    streams: &mut Vec<ObjectId>,
    visited: &mut HashSet<ObjectId>,
    depth: usize,
) -> Result<bool, String> {
    if depth > MAX_PAGE_TREE_DEPTH {
        return Err("A page content reference chain is too deeply nested.".to_string());
    }
    match value {
        Object::Reference(id) => {
            if !visited.insert(*id) {
                return Err("A page content reference chain contains a cycle.".to_string());
            }
            let referenced = document
                .get_object(*id)
                .map_err(|error| format!("A page content reference is invalid: {error}"))?;
            let result = match referenced {
                Object::Stream(_) => {
                    streams.push(*id);
                    true
                }
                other => collect_content_stream_ids(document, other, streams, visited, depth + 1)?,
            };
            visited.remove(id);
            Ok(result)
        }
        Object::Array(values) => {
            for item in values {
                if !collect_content_stream_ids(document, item, streams, visited, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Object::Null => Ok(true),
        Object::Stream(_) => Ok(false),
        _ => Ok(false),
    }
}

fn effective_page_resources(
    document: &Document,
    start_id: ObjectId,
) -> Result<Option<Dictionary>, String> {
    let mut current_id = start_id;
    let mut visited = HashSet::new();
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        if !visited.insert(current_id) {
            return Err("The PDF page tree contains a cycle.".to_string());
        }
        let dictionary = document
            .get_dictionary(current_id)
            .map_err(|error| format!("The PDF page tree is invalid: {error}"))?;
        if let Ok(resources) = dictionary.get(b"Resources") {
            return match resources {
                Object::Dictionary(resources) => Ok(Some(resources.clone())),
                Object::Reference(id) => document
                    .get_dictionary(*id)
                    .cloned()
                    .map(Some)
                    .map_err(|error| format!("The PDF page resources are invalid: {error}")),
                Object::Null => Ok(None),
                _ => Err("The PDF page resources are invalid.".to_string()),
            };
        }
        match dictionary.get(b"Parent") {
            Ok(Object::Reference(parent_id)) => current_id = *parent_id,
            Err(_) => return Ok(None),
            _ => return Err("The PDF page tree has an invalid parent reference.".to_string()),
        }
    }
    Err("The PDF page tree is too deeply nested.".to_string())
}

fn resolve_xobject_id(
    document: &Document,
    resources: &Dictionary,
    name: &[u8],
) -> Option<ObjectId> {
    let xobjects = match resources.get(b"XObject").ok()? {
        Object::Dictionary(dictionary) => dictionary,
        Object::Reference(id) => document.get_dictionary(*id).ok()?,
        _ => return None,
    };
    match xobjects.get(name).ok()? {
        Object::Reference(id) => Some(*id),
        _ => None,
    }
}

fn operation_resolves_to_image(
    document: &Document,
    resources: Option<&Dictionary>,
    operation: &Operation,
) -> bool {
    operation_xobject_subtype(document, resources, operation) == Some(b"Image".as_slice())
}

fn operation_resolves_to_form(
    document: &Document,
    resources: Option<&Dictionary>,
    operation: &Operation,
) -> bool {
    operation_xobject_subtype(document, resources, operation) == Some(b"Form".as_slice())
}

fn operation_xobject_subtype<'a>(
    document: &'a Document,
    resources: Option<&Dictionary>,
    operation: &Operation,
) -> Option<&'a [u8]> {
    let name = operation.operands.first()?.as_name().ok()?;
    let id = resolve_xobject_id(document, resources?, name)?;
    document
        .get_object(id)
        .ok()?
        .as_stream()
        .ok()?
        .dict
        .get(b"Subtype")
        .ok()?
        .as_name()
        .ok()
}

fn advance_text_for_operation(
    _document: &Document,
    operation: &Operation,
    fonts: &BTreeMap<Vec<u8>, &Dictionary>,
    text: &mut TextState,
) {
    if operation.operator == "\"" {
        text.word_spacing = operation
            .operands
            .first()
            .and_then(pdf_number)
            .unwrap_or(text.word_spacing);
        text.character_spacing = operation
            .operands
            .get(1)
            .and_then(pdf_number)
            .unwrap_or(text.character_spacing);
    }
    let Some(font) = text.font_name.as_ref().and_then(|name| fonts.get(name)) else {
        return;
    };
    let mut advance = 0.0_f64;
    match operation.operator.as_str() {
        "Tj" => {
            if let Some(Object::String(bytes, _)) = operation.operands.first() {
                advance += text_advance_width(font, bytes, "", text);
            }
        }
        "TJ" => {
            if let Some(Object::Array(items)) = operation.operands.first() {
                for item in items {
                    match item {
                        Object::String(bytes, _) => {
                            advance += text_advance_width(font, bytes, "", text);
                        }
                        Object::Integer(value) => {
                            advance -=
                                *value as f64 / 1_000.0 * text.font_size * text.horizontal_scale;
                        }
                        Object::Real(value) => {
                            advance -= f64::from(*value) / 1_000.0
                                * text.font_size
                                * text.horizontal_scale;
                        }
                        _ => {}
                    }
                }
            }
        }
        "'" => {
            if let Some(Object::String(bytes, _)) = operation.operands.first() {
                advance += text_advance_width(font, bytes, "", text);
            }
        }
        "\"" => {
            if let Some(Object::String(bytes, _)) = operation.operands.get(2) {
                advance += text_advance_width(font, bytes, "", text);
            }
        }
        _ => {}
    }
    if advance.is_finite() {
        text.text_matrix = text.text_matrix.then(Matrix {
            e: advance,
            ..Matrix::IDENTITY
        });
    }
}

fn advance_text_line(text: &mut TextState) {
    text.line_matrix = text.line_matrix.then(Matrix {
        f: -text.leading,
        ..Matrix::IDENTITY
    });
    text.text_matrix = text.line_matrix;
}

fn text_advance_width(font: &Dictionary, bytes: &[u8], decoded: &str, text: &TextState) -> f64 {
    let glyph_count = if decoded.is_empty() {
        bytes.len()
    } else {
        decoded.chars().count()
    };
    let units = simple_font_width_units(font, bytes).unwrap_or(glyph_count as f64 * 500.0);
    let spaces = bytes.iter().filter(|value| **value == b' ').count() as f64;
    ((units / 1_000.0) * text.font_size
        + glyph_count as f64 * text.character_spacing
        + spaces * text.word_spacing)
        * text.horizontal_scale
}

fn simple_font_width_units(font: &Dictionary, bytes: &[u8]) -> Option<f64> {
    let first = font.get(b"FirstChar").ok()?.as_i64().ok()?;
    let widths = font.get(b"Widths").ok()?.as_array().ok()?;
    let mut total = 0.0_f64;
    for byte in bytes {
        let index = i64::from(*byte).checked_sub(first)?;
        let width = widths
            .get(usize::try_from(index).ok()?)
            .and_then(pdf_number)?;
        total += width;
    }
    Some(total)
}

fn operation_matrix(operation: &Operation) -> Option<Matrix> {
    if operation.operands.len() != 6 {
        return None;
    }
    let values = operation
        .operands
        .iter()
        .map(pdf_number)
        .collect::<Option<Vec<_>>>()?;
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(Matrix {
            a: values[0],
            b: values[1],
            c: values[2],
            d: values[3],
            e: values[4],
            f: values[5],
        })
}

fn font_label(font: &Dictionary, resource_name: &[u8]) -> String {
    let value = font
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .unwrap_or(resource_name);
    let label = String::from_utf8_lossy(value)
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect::<String>();
    if label.is_empty() {
        "PDF font".to_string()
    } else {
        label
    }
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<PageGeometry, String> {
    let page_box = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "A content-editing page does not define a crop or media box.".to_string())?;
    let page_box = dereference_object(document, &page_box, "page box")?;
    let coordinates = page_box
        .as_array()
        .map_err(|_| "A content-editing page box is not an array.".to_string())?;
    if coordinates.len() != 4 {
        return Err("A content-editing page box must contain four coordinates.".to_string());
    }
    let values = coordinates
        .iter()
        .map(|object| {
            pdf_number(object)
                .ok_or_else(|| "A content-editing page box contains a non-number.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let width = values[2] - values[0];
    let height = values[3] - values[1];
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("A content-editing page has invalid dimensions.".to_string());
    }
    let rotation = match inherited_page_value(document, page_id, b"Rotate")? {
        Some(value) => dereference_object(document, &value, "page rotation")?
            .as_i64()
            .map_err(|_| "A content-editing page has an invalid rotation.".to_string())?,
        None => 0,
    }
    .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("A content-editing page has an unsupported rotation.".to_string());
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
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let right = points
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let top = points
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let bottom = points
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let rect = NormalisedRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    (rect.width >= MIN_RECT_SIZE && rect.height >= MIN_RECT_SIZE).then_some(rect)
}

fn normalise_pdf_point(geometry: PageGeometry, point: PdfPoint) -> Option<(f64, f64)> {
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
    Some((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
}

fn map_rect_to_pdf(geometry: PageGeometry, rect: NormalisedRect) -> PageBox {
    let corners = [
        map_visual_point(
            geometry,
            rect.x * geometry.visual_width,
            rect.y * geometry.visual_height,
        ),
        map_visual_point(
            geometry,
            (rect.x + rect.width) * geometry.visual_width,
            rect.y * geometry.visual_height,
        ),
        map_visual_point(
            geometry,
            rect.x * geometry.visual_width,
            (rect.y + rect.height) * geometry.visual_height,
        ),
        map_visual_point(
            geometry,
            (rect.x + rect.width) * geometry.visual_width,
            (rect.y + rect.height) * geometry.visual_height,
        ),
    ];
    let left = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let top = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PageBox {
        left,
        bottom,
        width: right - left,
        height: top - bottom,
    }
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

fn annotation_counts(document: &Document) -> Result<Vec<usize>, String> {
    document
        .get_pages()
        .values()
        .map(|page_id| {
            let page = document
                .get_dictionary(*page_id)
                .map_err(|error| format!("A PDF page is invalid: {error}"))?;
            match page.get(b"Annots") {
                Err(_) | Ok(Object::Null) => Ok(0),
                Ok(Object::Array(values)) => Ok(values.len()),
                Ok(Object::Reference(id)) => document
                    .get_object(*id)
                    .and_then(Object::as_array)
                    .map(Vec::len)
                    .map_err(|error| format!("A PDF annotation list is invalid: {error}")),
                Ok(_) => Err("A PDF annotation list is invalid.".to_string()),
            }
        })
        .collect()
}

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedContentPdf, String> {
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
                "The PDF could not be decrypted for page-content editing. Check its password."
                    .to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    Ok(LoadedContentPdf {
        document,
        page_count,
        was_encrypted,
    })
}

fn validate_password(password: Option<&str>) -> Result<(), String> {
    if password.is_some_and(|value| value.len() > MAX_PASSWORD_BYTES) {
        return Err("The source password is too long to process safely.".to_string());
    }
    Ok(())
}

fn validate_source_id(source_id: &str, kind: &str, number: usize) -> Result<(), String> {
    let prefix = format!("{kind}-");
    if source_id.len() != prefix.len() + 64
        || source_id.len() > MAX_IDENTIFIER_BYTES
        || !source_id.starts_with(&prefix)
        || !source_id[prefix.len()..]
            .bytes()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
    {
        return Err(format!(
            "Reviewed {kind} object {number} has an invalid native identity."
        ));
    }
    Ok(())
}

fn validate_replacement_text(value: &str, number: usize) -> Result<(), String> {
    if value.chars().count() > MAX_TEXT_CHARACTERS || value.len() > MAX_TEXT_BYTES {
        return Err(format!("Text edit {number} is too long."));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "Text edit {number} contains unsupported control characters."
        ));
    }
    Ok(())
}

fn validate_normalised_rect(rect: NormalisedRect, number: usize) -> Result<(), String> {
    let values = [rect.x, rect.y, rect.width, rect.height];
    if values.iter().any(|value| !value.is_finite())
        || rect.x < 0.0
        || rect.y < 0.0
        || rect.width < MIN_RECT_SIZE
        || rect.height < MIN_RECT_SIZE
        || rect.x + rect.width > 1.0 + f64::EPSILON
        || rect.y + rect.height > 1.0 + f64::EPSILON
    {
        return Err(format!(
            "Image edit {number} has bounds outside its reviewed page."
        ));
    }
    Ok(())
}

fn validate_sha256(label: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("{label} is invalid. Review the source PDF again."));
    }
    Ok(())
}

fn verify_source_fingerprint(
    path: &Path,
    expected_size: u64,
    expected_modified_at_ms: Option<u64>,
    expected_sha256: &str,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("The source PDF could not be rechecked: {error}"))?;
    if metadata.len() != expected_size || modified_at_ms(&metadata) != expected_modified_at_ms {
        return Err(
            "The source PDF changed on disk after its content was reviewed. Review it again before exporting."
                .to_string(),
        );
    }
    if sha256_file(path, control)? != expected_sha256 {
        return Err(
            "The source PDF bytes changed after its content was reviewed. Review it again before exporting."
                .to_string(),
        );
    }
    Ok(())
}

fn sha256_file(path: &Path, control: &PdfJobExecutionControl) -> Result<String, String> {
    let file = File::open(path)
        .map_err(|error| format!("A PDF could not be opened for its integrity check: {error}"))?;
    let mut reader = BufReader::with_capacity(FILE_HASH_BUFFER_BYTES, file);
    let mut buffer = vec![0_u8; FILE_HASH_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut chunks = 0_usize;
    loop {
        if chunks.is_multiple_of(16) {
            control.ensure_not_cancelled()?;
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("A PDF integrity check could not complete: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        chunks += 1;
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn opaque_source_id(
    kind: &str,
    page_number: usize,
    stream_id: ObjectId,
    operation_index: usize,
    stream_sha256: &str,
    extras: &[&[u8]],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"TufekciPaperworksContentSourceV1\0");
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(page_number.to_be_bytes());
    hasher.update(stream_id.0.to_be_bytes());
    hasher.update(stream_id.1.to_be_bytes());
    hasher.update(operation_index.to_be_bytes());
    hasher.update(stream_sha256.as_bytes());
    for extra in extras {
        hasher.update(extra.len().to_be_bytes());
        hasher.update(extra);
    }
    format!("{kind}-{}", hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[(byte >> 4) as usize]));
        value.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    value
}

fn dictionary_literal_string(dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    let Object::String(bytes, _) = dictionary.get(key).ok()? else {
        return None;
    };
    std::str::from_utf8(bytes).ok().map(ToOwned::to_owned)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document.pdf")
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

fn pdf_number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn pdf_real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn approximately(left: f64, right: f64) -> bool {
    (left - right).abs() <= 0.000_001
}

fn matrix_approximately_equal(left: Matrix, right: Matrix) -> bool {
    approximately(left.a, right.a)
        && approximately(left.b, right.b)
        && approximately(left.c, right.c)
        && approximately(left.d, right.d)
        && approximately(left.e, right.e)
        && approximately(left.f, right.f)
}

fn safe_content_inspection_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed") {
        return "The source PDF changed during content review. Open it again before editing."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The content-editing PDF could not be opened with the supplied password."
            .to_string();
    }
    if normalised.contains("more than") || normalised.contains("too complex") {
        return error.to_string();
    }
    "The page-content review failed a bounded structural safety check. The PDF was not changed."
        .to_string()
}

fn safe_content_export_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed") || normalised.contains("no longer matches") {
        return "The source PDF changed after review. Review its page content again before exporting."
            .to_string();
    }
    if normalised.contains("font") || normalised.contains("cannot reproduce") {
        return error.to_string();
    }
    if normalised.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before exporting page-content edits."
            .to_string();
    }
    if error.contains("QPDF") {
        return "AES-256 protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The page-content PDF could not be opened or protected with the supplied passwords."
            .to_string();
    }
    if normalised.contains("destination already exists") {
        return "The destination already exists. Choose a new filename.".to_string();
    }
    if normalised.contains("cannot be overwritten") {
        return "The source PDF cannot be overwritten. Choose a new filename.".to_string();
    }
    "The page-content export failed an exact structural verification. No destination PDF was published."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::path::PathBuf;

    #[test]
    fn inspects_exact_editable_text_and_image_objects() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect(&input);

        assert_eq!(inspection.page_count, 1);
        assert_eq!(inspection.editable_text_count, 1);
        assert_eq!(inspection.editable_image_count, 1);
        assert_eq!(inspection.read_only_text_count, 0);
        assert_eq!(inspection.read_only_image_count, 0);
        assert_eq!(inspection.editable_text_runs[0].text, "Hello");
        assert!(inspection.editable_text_runs[0]
            .source_id
            .starts_with("text-"));
        assert!(inspection.editable_images[0]
            .source_id
            .starts_with("image-"));
        assert_eq!(inspection.source_sha256.len(), 64);
    }

    #[test]
    fn replaces_exact_text_and_reopens_verified_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("edited.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(&input);
        let text = &inspection.editable_text_runs[0];

        let result = export_pdf_content(request(
            &input,
            &output,
            &inspection,
            vec![PdfTextEdit {
                source_id: text.source_id.clone(),
                replacement_text: "World".to_string(),
            }],
            Vec::new(),
        ))
        .unwrap();

        assert_eq!(result.text_edit_count, 1);
        assert_eq!(result.image_edit_count, 0);
        assert_eq!(result.encryption, "None");
        assert_eq!(result.output_sha256.len(), 64);
        let reopened = Document::load(&output).unwrap();
        assert_eq!(reopened.extract_text(&[1]).unwrap().trim(), "World");
        assert!(edited_stream_markers(&reopened).len() == 1);
    }

    #[test]
    fn replaces_and_repositions_one_exact_image_placement() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("image-edited.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(&input);
        let image = &inspection.editable_images[0];
        let new_rect = NormalisedRect {
            x: 0.2,
            y: 0.25,
            width: 0.3,
            height: 0.2,
        };

        let result = export_pdf_content(request(
            &input,
            &output,
            &inspection,
            Vec::new(),
            vec![PdfImageEdit {
                source_id: image.source_id.clone(),
                delete: false,
                replacement_image_data_url: Some(test_png_data_url()),
                rect: new_rect,
            }],
        ))
        .unwrap();

        assert_eq!(result.image_edit_count, 1);
        assert_eq!(result.replaced_image_count, 1);
        assert_eq!(result.repositioned_image_count, 1);
        let reopened = Document::load(&output).unwrap();
        assert_eq!(replacement_image_markers(&reopened).len(), 1);
        let second_inspection = inspect(&output);
        assert!(second_inspection
            .editable_images
            .iter()
            .any(|candidate| rect_close(candidate.rect, new_rect)));
    }

    #[test]
    fn removes_only_the_selected_image_painting_block() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("image-removed.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(&input);
        let image = &inspection.editable_images[0];

        let result = export_pdf_content(request(
            &input,
            &output,
            &inspection,
            Vec::new(),
            vec![PdfImageEdit {
                source_id: image.source_id.clone(),
                delete: true,
                replacement_image_data_url: None,
                rect: image.rect,
            }],
        ))
        .unwrap();

        assert_eq!(result.deleted_image_count, 1);
        let second_inspection = inspect(&output);
        assert_eq!(second_inspection.editable_image_count, 0);
        assert_eq!(second_inspection.editable_text_count, 1);
        assert_eq!(second_inspection.editable_text_runs[0].text, "Hello");
    }

    #[test]
    fn rejects_text_the_original_font_cannot_round_trip() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("unsupported.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(&input);

        let error = export_pdf_content(request(
            &input,
            &output,
            &inspection,
            vec![PdfTextEdit {
                source_id: inspection.editable_text_runs[0].source_id.clone(),
                replacement_text: "Merhaba \u{0130}stanbul".to_string(),
            }],
            Vec::new(),
        ))
        .unwrap_err();

        assert!(error.contains("cannot reproduce exactly"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_stale_full_source_hash_even_when_metadata_is_claimed() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("stale.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(&input);
        let mut request = request(
            &input,
            &output,
            &inspection,
            vec![PdfTextEdit {
                source_id: inspection.editable_text_runs[0].source_id.clone(),
                replacement_text: "World".to_string(),
            }],
            Vec::new(),
        );
        request.expected_source_sha256 = "0".repeat(64);

        let error = export_pdf_content(request).unwrap_err();

        assert!(error.contains("bytes changed"));
        assert!(!output.exists());
    }

    #[test]
    fn shared_page_streams_are_preserved_read_only() {
        let directory = TestDirectory::new();
        let input = directory.path.join("shared.pdf");
        fixture(true).save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect(&input);

        assert_eq!(inspection.page_count, 2);
        assert_eq!(inspection.editable_text_count, 0);
        assert_eq!(inspection.editable_image_count, 0);
        assert!(inspection.read_only_text_count >= 2);
        assert!(inspection.read_only_image_count >= 2);
        assert_eq!(inspection.pages_with_unsupported_content, vec![1, 2]);
    }

    #[test]
    fn content_streams_referenced_outside_the_page_tree_are_preserved_read_only() {
        let directory = TestDirectory::new();
        let input = directory.path.join("multiply-referenced.pdf");
        let mut document = fixture(false);
        let page_id = document.get_pages()[&1];
        let content_id = document.get_page_contents(page_id)[0];
        let catalog_id = document
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .unwrap();
        document
            .get_dictionary_mut(catalog_id)
            .unwrap()
            .set("ContentBackup", content_id);
        document.save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect(&input);

        assert_eq!(inspection.editable_text_count, 0);
        assert_eq!(inspection.editable_image_count, 0);
        assert_eq!(inspection.read_only_text_count, 1);
        assert_eq!(inspection.read_only_image_count, 1);
        assert_eq!(inspection.pages_with_unsupported_content, vec![1]);
    }

    #[test]
    fn malformed_direct_page_content_fails_closed_without_guessing_an_identity() {
        let directory = TestDirectory::new();
        let input = directory.path.join("direct.pdf");
        let mut document = fixture(false);
        let page_id = document.get_pages()[&1];
        document
            .get_dictionary_mut(page_id)
            .unwrap()
            .set("Contents", Object::Integer(42));
        document.save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect(&input);

        assert_eq!(inspection.editable_text_count, 0);
        assert_eq!(inspection.editable_image_count, 0);
        assert_eq!(inspection.pages_with_unsupported_content, vec![1]);
    }

    fn inspect(path: &Path) -> PdfContentInspection {
        inspect_pdf_content(InspectPdfContentRequest {
            input_path: path.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap()
    }

    fn request(
        input: &Path,
        output: &Path,
        inspection: &PdfContentInspection,
        text_edits: Vec<PdfTextEdit>,
        image_edits: Vec<PdfImageEdit>,
    ) -> ExportPdfContentRequest {
        ExportPdfContentRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            expected_source_sha256: inspection.source_sha256.clone(),
            text_edits,
            image_edits,
        }
    }

    fn fixture(shared_stream: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let widths = vec![Object::Integer(500); 95];
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
            "FirstChar" => 32,
            "LastChar" => 126,
            "Widths" => widths,
        });
        let mut image = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        );
        image.compress().unwrap();
        let image_id = document.add_object(image);
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        120.into(),
                        0.into(),
                        0.into(),
                        80.into(),
                        72.into(),
                        610.into(),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
                Operation::new("Q", vec![]),
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 18.into()]),
                Operation::new(
                    "Tm",
                    vec![
                        1.into(),
                        0.into(),
                        0.into(),
                        1.into(),
                        72.into(),
                        740.into(),
                    ],
                ),
                Operation::new(
                    "Tj",
                    vec![Object::String(b"Hello".to_vec(), StringFormat::Literal)],
                ),
                Operation::new("ET", vec![]),
            ],
        }
        .encode()
        .unwrap();
        let mut content_stream = Stream::new(dictionary! {}, content);
        content_stream.compress().unwrap();
        let content_id = document.add_object(content_stream);
        let page_count = if shared_stream { 2 } else { 1 };
        let mut page_ids = Vec::new();
        for _ in 0..page_count {
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => font_id },
                    "XObject" => dictionary! { "Im1" => image_id },
                },
                "Contents" => content_id,
            });
            page_ids.push(page_id);
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => i64::from(page_count),
                "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn test_png_data_url() -> String {
        let image = ImageBuffer::from_fn(10, 6, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([35_u8, 93, 216, 255])
            } else {
                Rgba([255_u8, 255, 255, 160])
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

    fn edited_stream_markers(document: &Document) -> Vec<String> {
        document
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter_map(|stream| dictionary_literal_string(&stream.dict, STREAM_EDIT_MARKER))
            .collect()
    }

    fn replacement_image_markers(document: &Document) -> Vec<String> {
        document
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter_map(|stream| dictionary_literal_string(&stream.dict, IMAGE_EDIT_MARKER))
            .collect()
    }

    fn rect_close(left: NormalisedRect, right: NormalisedRect) -> bool {
        approximately(left.x, right.x)
            && approximately(left.y, right.y)
            && approximately(left.width, right.width)
            && approximately(left.height, right.height)
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path =
                crate::test_support::create_unique_test_directory("paperworks-content-editor-test");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
