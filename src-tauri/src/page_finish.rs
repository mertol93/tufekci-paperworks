use crate::file_safety::{
    canonical_pdf_input, reject_control_characters, TemporaryOutput, ValidatedPdfPaths,
};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use lopdf::content::{Content, Operation};
use lopdf::{
    dictionary, Dictionary, Document, LoadOptions, Object, ObjectId, Stream, StringFormat,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PAGE_TREE_DEPTH: usize = 32;
const MAX_PAGES: usize = 20_000;
const MAX_RANGE_BYTES: usize = 4_096;
const MAX_TEXT_CHARACTERS: usize = 512;
const MAX_TEXT_BYTES: usize = 2_048;
const MAX_EXPANDED_TEXT_CHARACTERS: usize = 1_024;
const MIN_VISIBLE_PAGE_POINTS: f64 = 36.0;
const MAX_PAGE_DIMENSION_POINTS: f64 = 14_400.0;
const MAX_MARGIN_POINTS: f64 = 7_200.0;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfFinishingRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfFinishingRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    acknowledge_certificate_signatures: bool,
    page_range: String,
    crop: Option<CropMargins>,
    resize: Option<PageResize>,
    watermark: Option<TextWatermark>,
    header_footer: Option<HeaderFooter>,
    bates: Option<BatesNumbering>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CropMargins {
    top_pt: f64,
    right_pt: f64,
    bottom_pt: f64,
    left_pt: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageResize {
    width_pt: f64,
    height_pt: f64,
    margin_pt: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextWatermark {
    text: String,
    font_size_pt: f64,
    opacity: f64,
    angle_degrees: f64,
    colour: [f32; 3],
    over_content: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TextAlignment {
    Left,
    Centre,
    Right,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeaderFooter {
    header_text: String,
    footer_text: String,
    header_alignment: TextAlignment,
    footer_alignment: TextAlignment,
    font_size_pt: f64,
    margin_pt: f64,
    colour: [f32; 3],
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum BatesPosition {
    TopLeft,
    TopCentre,
    TopRight,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatesNumbering {
    prefix: String,
    suffix: String,
    start_number: u64,
    digits: usize,
    position: BatesPosition,
    font_size_pt: f64,
    margin_pt: f64,
    colour: [f32; 3],
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfPageFinishInspection {
    page_number: usize,
    width_pt: f64,
    height_pt: f64,
    rotation: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfFinishingInspection {
    file_name: String,
    source_size: u64,
    source_modified_at_ms: Option<u64>,
    page_count: usize,
    annotation_count: usize,
    has_forms: bool,
    has_bookmarks: bool,
    certificate_signature: bool,
    was_encrypted: bool,
    pages: Vec<PdfPageFinishInspection>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfFinishingResult {
    output_path: String,
    page_count: usize,
    changed_page_count: usize,
    cropped_page_count: usize,
    resized_page_count: usize,
    marked_page_count: usize,
    bates_number_count: usize,
    adjusted_annotation_count: usize,
    bytes_written: u64,
    encryption: &'static str,
    warnings: Vec<String>,
}

struct LoadedPdf {
    document: Document,
    page_count: usize,
    was_encrypted: bool,
    certificate_signature: bool,
    has_forms: bool,
    has_bookmarks: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PageBox {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

#[derive(Clone, Copy, Debug)]
struct PageGeometry {
    page: PageBox,
    rotation: i64,
    visual_width: f64,
    visual_height: f64,
}

#[derive(Clone, Copy, Debug)]
struct AffineTransform {
    scale: f64,
    translate_x: f64,
    translate_y: f64,
}

#[derive(Clone)]
struct ExpectedPageFinish {
    page_number: usize,
    crop_box: PageBox,
    media_box: Option<PageBox>,
    marker: String,
    selected_index: usize,
    cropped: bool,
    resized: bool,
    marked: bool,
}

struct FinishVerificationExpectations<'a> {
    encrypted: bool,
    page_count: usize,
    forms: bool,
    bookmarks: bool,
    original_annotation_counts: &'a [usize],
    expected_pages: &'a [ExpectedPageFinish],
    expected_finish_forms: usize,
    marker: &'a str,
}

struct PageFinishStats {
    cropped_pages: usize,
    resized_pages: usize,
    marked_pages: usize,
    bates_numbers: usize,
    adjusted_annotations: usize,
    substituted_appearances: usize,
    finish_form_count: usize,
}

impl PageFinishStats {
    fn new() -> Self {
        Self {
            cropped_pages: 0,
            resized_pages: 0,
            marked_pages: 0,
            bates_numbers: 0,
            adjusted_annotations: 0,
            substituted_appearances: 0,
            finish_form_count: 0,
        }
    }
}

#[cfg(test)]
pub fn inspect_pdf_finishing(
    request: InspectPdfFinishingRequest,
) -> Result<PdfFinishingInspection, String> {
    inspect_pdf_finishing_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_finishing_request(
    request: &InspectPdfFinishingRequest,
) -> Result<(), String> {
    reject_control_characters("Page Finish source path", &request.input_path)?;
    validate_password(request.input_password.as_deref())
}

fn inspect_pdf_finishing_with_control(
    request: InspectPdfFinishingRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfFinishingInspection, String> {
    control.checkpoint(2, "Validating Page Finish review")?;
    validate_inspect_pdf_finishing_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_size = metadata.len();
    let source_modified_at_ms = modified_at_ms(&metadata);
    control.checkpoint(18, "Opening Page Finish structure")?;
    let loaded = load_pdf(&input, request.input_password.as_deref())?;
    let page_map = loaded.document.get_pages();
    let mut pages = Vec::with_capacity(page_map.len());
    for (index, (page_number, page_id)) in page_map.into_iter().enumerate() {
        checkpoint_finishing_inspection_loop(
            control,
            index,
            loaded.page_count,
            30,
            70,
            "Inspecting Page Finish page",
        )?;
        let geometry = page_geometry(&loaded.document, page_id)?;
        pages.push(PdfPageFinishInspection {
            page_number: page_number as usize,
            width_pt: geometry.visual_width,
            height_pt: geometry.visual_height,
            rotation: geometry.rotation,
        });
    }
    let annotation_counts = page_annotation_counts_for_inspection(&loaded.document, control)?;
    let mut warnings = Vec::new();
    if loaded.was_encrypted {
        warnings.push(
            "The finished copy will be unlocked. Use Protect afterwards to apply new AES-256 encryption."
                .to_string(),
        );
    }
    if loaded.certificate_signature {
        warnings.push(
            "Page finishing rewrites the PDF and invalidates its existing certificate signatures."
                .to_string(),
        );
    }
    if loaded.has_forms {
        warnings.push(
            "Interactive form structures are preserved. Resizing carries widget rectangles with their pages, but the finished copy should be reviewed."
                .to_string(),
        );
    }
    if loaded.has_bookmarks {
        warnings.push(
            "Bookmarks are preserved. Page resizing can change specialist destination coordinates, so navigation should be reviewed."
                .to_string(),
        );
    }

    control.checkpoint(94, "Rechecking Page Finish source")?;
    verify_source_fingerprint(&input, source_size, source_modified_at_ms)?;
    control.checkpoint(99, "Finalising Page Finish review")?;

    Ok(PdfFinishingInspection {
        file_name: display_name(&input),
        source_size,
        source_modified_at_ms,
        page_count: loaded.page_count,
        annotation_count: annotation_counts.into_iter().sum(),
        has_forms: loaded.has_forms,
        has_bookmarks: loaded.has_bookmarks,
        certificate_signature: loaded.certificate_signature,
        was_encrypted: loaded.was_encrypted,
        pages,
        warnings,
    })
}

pub(crate) fn run_pdf_finishing_inspection_job_with_control(
    request: InspectPdfFinishingRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfFinishingInspection, String> {
    inspect_pdf_finishing_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_finishing_inspection_job_error(&error)
        }
    })
}

fn safe_finishing_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during Page Finish review. Open it again before editing."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The Page Finish PDF could not be opened with the supplied password.".to_string();
    }
    "The Page Finish review failed a structural safety check. Review the source PDF and try again."
        .to_string()
}

#[cfg(test)]
pub fn export_pdf_finishing(
    request: ExportPdfFinishingRequest,
) -> Result<ExportPdfFinishingResult, String> {
    export_pdf_finishing_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_export_pdf_finishing_request(
    request: &ExportPdfFinishingRequest,
) -> Result<(), String> {
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    let mut validated = request.clone();
    validate_finish_options(&mut validated)?;
    parse_page_range(&validated.page_range, MAX_PAGES)?;
    Ok(())
}

pub(crate) fn export_pdf_finishing_with_control(
    mut request: ExportPdfFinishingRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfFinishingResult, String> {
    control.checkpoint(1, "Validating page-finishing export")?;
    validate_export_pdf_finishing_request(&request)?;
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
    control.checkpoint(22, "Validating page-finishing settings")?;
    validate_finish_options(&mut request)?;
    let selected_pages = parse_page_range(&request.page_range, loaded.page_count)?;
    validate_per_page_options_with_control(
        &loaded.document,
        &selected_pages,
        request.crop,
        request.resize,
        control,
    )?;
    let original_annotation_counts =
        page_annotation_counts_with_control(&loaded.document, control)?;
    let marker = finish_marker()?;
    let file_name = display_name(&paths.input);
    let has_text_marks = request.watermark.is_some()
        || request
            .header_footer
            .as_ref()
            .is_some_and(|value| !value.header_text.is_empty() || !value.footer_text.is_empty())
        || request.bates.is_some();
    let font_id = has_text_marks.then(|| {
        loaded.document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        })
    });
    let mut stats = PageFinishStats::new();
    let mut expected_pages = Vec::with_capacity(selected_pages.len());
    let pages = loaded.document.get_pages();

    for (selected_index, page_number) in selected_pages.iter().copied().enumerate() {
        checkpoint_finish_loop(
            control,
            selected_index,
            selected_pages.len(),
            36,
            66,
            "Applying reviewed page finishing",
        )?;
        let page_id = *pages
            .get(&(page_number as u32))
            .ok_or_else(|| format!("Page {page_number} disappeared before export."))?;
        let original_geometry = page_geometry(&loaded.document, page_id)?;
        let source_box = request
            .crop
            .map(|crop| crop_page_box(original_geometry, crop))
            .transpose()?
            .unwrap_or(original_geometry.page);
        if request.crop.is_some() {
            stats.cropped_pages += 1;
        }

        let (final_geometry, media_box, transform) = if let Some(resize) = request.resize {
            let (geometry, transform) =
                resize_geometry(original_geometry.rotation, source_box, resize)?;
            wrap_page_content(&mut loaded.document, page_id, source_box, transform)?;
            stats.adjusted_annotations +=
                transform_page_annotations(&mut loaded.document, page_id, transform)?;
            set_resized_page_boxes(&mut loaded.document, page_id, geometry.page)?;
            stats.resized_pages += 1;
            (geometry, Some(geometry.page), Some(transform))
        } else {
            if request.crop.is_some() {
                set_page_crop_box(&mut loaded.document, page_id, source_box)?;
            }
            (
                geometry_for_box(original_geometry.rotation, source_box),
                None,
                None,
            )
        };

        let rendered = add_page_marks(
            &mut loaded.document,
            page_id,
            final_geometry,
            font_id,
            &marker,
            &file_name,
            page_number,
            loaded.page_count,
            selected_index,
            request.watermark.as_ref(),
            request.header_footer.as_ref(),
            request.bates.as_ref(),
        )?;
        stats.marked_pages += usize::from(rendered.marked);
        stats.bates_numbers += usize::from(rendered.bates_numbered);
        stats.substituted_appearances += rendered.substitutions;
        stats.finish_form_count += rendered.form_count;
        mark_finished_page(
            &mut loaded.document,
            page_id,
            &marker,
            selected_index,
            request.crop.is_some(),
            transform.is_some(),
            rendered.marked,
        )?;
        expected_pages.push(ExpectedPageFinish {
            page_number,
            crop_box: final_geometry.page,
            media_box,
            marker: marker.clone(),
            selected_index,
            cropped: request.crop.is_some(),
            resized: transform.is_some(),
            marked: rendered.marked,
        });
    }

    loaded.document.prune_objects();
    loaded.document.change_producer("Tüfekci Paperworks");
    control.checkpoint(68, "Writing prepared finished PDF")?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    loaded
        .document
        .save(prepared.path())
        .map_err(|error| format!("The finished PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| format!("The finished PDF could not be flushed to storage: {error}"))?;

    control.checkpoint(74, "Verifying prepared page-finishing structure")?;
    let prepared_expectations = FinishVerificationExpectations {
        encrypted: false,
        page_count: loaded.page_count,
        forms: loaded.has_forms,
        bookmarks: loaded.has_bookmarks,
        original_annotation_counts: &original_annotation_counts,
        expected_pages: &expected_pages,
        expected_finish_forms: stats.finish_form_count,
        marker: &marker,
    };
    verify_finished_pdf_path(
        prepared.path(),
        None,
        &prepared_expectations,
        control,
        75,
        80,
    )?;

    let protected = if let Some(protection) = request.output_protection.as_ref() {
        control.checkpoint(82, "Applying AES-256 output protection")?;
        let protected = TemporaryOutput::new(&paths.output)?;
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            control,
        )?;
        control.checkpoint(89, "Verifying protected page-finishing structure")?;
        let protected_expectations = FinishVerificationExpectations {
            encrypted: true,
            ..prepared_expectations
        };
        verify_finished_pdf_path(
            protected.path(),
            Some(&protection.open_password),
            &protected_expectations,
            control,
            90,
            93,
        )?;
        Some(protected)
    } else {
        None
    };
    let final_output = protected.as_ref().unwrap_or(&prepared);
    control.checkpoint(95, "Rechecking source PDF before publication")?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(99, "Publishing verified finished PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let warnings = finishing_warnings(&loaded, &request, &stats, selected_pages.len());
    Ok(ExportPdfFinishingResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: loaded.page_count,
        changed_page_count: selected_pages.len(),
        cropped_page_count: stats.cropped_pages,
        resized_page_count: stats.resized_pages,
        marked_page_count: stats.marked_pages,
        bates_number_count: stats.bates_numbers,
        adjusted_annotation_count: stats.adjusted_annotations,
        bytes_written,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_finishing_job_with_control(
    request: ExportPdfFinishingRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfFinishingResult, String> {
    export_pdf_finishing_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_finishing_job_error(&error)
        }
    })
}

fn safe_finishing_job_error(error: &str) -> String {
    if error.contains("changed on disk") {
        return "The source PDF changed after review. Review page finishing again before exporting."
            .to_string();
    }
    if error.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before exporting page finishing."
            .to_string();
    }
    if error.contains("QPDF") {
        return "AES-256 page-finishing protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if error.to_ascii_lowercase().contains("password")
        || error.to_ascii_lowercase().contains("decrypt")
    {
        return "The page-finishing PDF could not be opened or protected with the supplied passwords."
            .to_string();
    }
    if error.contains("destination already exists") {
        return "The destination already exists. Choose a new filename.".to_string();
    }
    if error.contains("cannot be overwritten") {
        return "The source PDF cannot be overwritten. Choose a new filename.".to_string();
    }
    "Page finishing failed a structural safety check. Review the settings and try again."
        .to_string()
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
            "The source PDF changed on disk after page finishing was reviewed. Review it again before exporting."
                .to_string(),
        );
    }
    Ok(())
}

struct RenderedMarks {
    marked: bool,
    bates_numbered: bool,
    substitutions: usize,
    form_count: usize,
}

fn validate_password(password: Option<&str>) -> Result<(), String> {
    if password.is_some_and(|value| value.len() > MAX_PASSWORD_BYTES) {
        return Err("The source password is too long to process safely.".to_string());
    }
    Ok(())
}

fn validate_finish_options(request: &mut ExportPdfFinishingRequest) -> Result<(), String> {
    if request.page_range.len() > MAX_RANGE_BYTES {
        return Err("The page selection is too long to process safely.".to_string());
    }
    if let Some(crop) = request.crop {
        for (label, value) in [
            ("Top crop", crop.top_pt),
            ("Right crop", crop.right_pt),
            ("Bottom crop", crop.bottom_pt),
            ("Left crop", crop.left_pt),
        ] {
            validate_number(label, value, 0.0, MAX_MARGIN_POINTS)?;
        }
        if crop.top_pt == 0.0
            && crop.right_pt == 0.0
            && crop.bottom_pt == 0.0
            && crop.left_pt == 0.0
        {
            request.crop = None;
        }
    }
    if let Some(resize) = request.resize {
        validate_number(
            "Page width",
            resize.width_pt,
            MIN_VISIBLE_PAGE_POINTS,
            MAX_PAGE_DIMENSION_POINTS,
        )?;
        validate_number(
            "Page height",
            resize.height_pt,
            MIN_VISIBLE_PAGE_POINTS,
            MAX_PAGE_DIMENSION_POINTS,
        )?;
        validate_number("Page margin", resize.margin_pt, 0.0, MAX_MARGIN_POINTS)?;
        if resize.margin_pt * 2.0 + MIN_VISIBLE_PAGE_POINTS > resize.width_pt
            || resize.margin_pt * 2.0 + MIN_VISIBLE_PAGE_POINTS > resize.height_pt
        {
            return Err("The resize margin leaves too little visible page area.".to_string());
        }
    }
    if let Some(watermark) = request.watermark.as_mut() {
        watermark.text = validate_single_line_text("Watermark", &watermark.text, false)?;
        validate_number("Watermark font size", watermark.font_size_pt, 12.0, 240.0)?;
        validate_number("Watermark opacity", watermark.opacity, 0.05, 0.9)?;
        validate_number("Watermark angle", watermark.angle_degrees, -180.0, 180.0)?;
        validate_colour(watermark.colour, "Watermark")?;
    }
    if let Some(header_footer) = request.header_footer.as_mut() {
        header_footer.header_text =
            validate_single_line_text("Header", &header_footer.header_text, true)?;
        header_footer.footer_text =
            validate_single_line_text("Footer", &header_footer.footer_text, true)?;
        validate_number(
            "Header and footer font size",
            header_footer.font_size_pt,
            6.0,
            36.0,
        )?;
        validate_number(
            "Header and footer margin",
            header_footer.margin_pt,
            0.0,
            144.0,
        )?;
        validate_colour(header_footer.colour, "Header and footer")?;
        if header_footer.header_text.is_empty() && header_footer.footer_text.is_empty() {
            request.header_footer = None;
        }
    }
    if let Some(bates) = request.bates.as_mut() {
        bates.prefix = validate_single_line_text("Bates prefix", &bates.prefix, true)?;
        bates.suffix = validate_single_line_text("Bates suffix", &bates.suffix, true)?;
        if !(1..=12).contains(&bates.digits) {
            return Err("Bates numbering must use between 1 and 12 digits.".to_string());
        }
        validate_number("Bates font size", bates.font_size_pt, 6.0, 36.0)?;
        validate_number("Bates margin", bates.margin_pt, 0.0, 144.0)?;
        validate_colour(bates.colour, "Bates")?;
        if bates.prefix.chars().count() + bates.suffix.chars().count() + bates.digits
            > MAX_TEXT_CHARACTERS
        {
            return Err("The Bates number format is too long.".to_string());
        }
    }
    if request.crop.is_none()
        && request.resize.is_none()
        && request.watermark.is_none()
        && request.header_footer.is_none()
        && request.bates.is_none()
    {
        return Err("Enable at least one page-finishing operation before exporting.".to_string());
    }
    Ok(())
}

fn validate_number(label: &str, value: f64, minimum: f64, maximum: f64) -> Result<(), String> {
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(format!(
            "{label} must be between {minimum:.2} and {maximum:.2} points."
        ));
    }
    Ok(())
}

fn validate_colour(colour: [f32; 3], label: &str) -> Result<(), String> {
    if colour
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(format!("{label} has an invalid colour."));
    }
    Ok(())
}

fn validate_single_line_text(
    label: &str,
    value: &str,
    allow_empty: bool,
) -> Result<String, String> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(format!(
            "{label} cannot contain line breaks or null characters."
        ));
    }
    let value = value.trim().to_string();
    if !allow_empty && value.is_empty() {
        return Err(format!("{label} text cannot be empty."));
    }
    if value.chars().count() > MAX_TEXT_CHARACTERS || value.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "{label} can contain at most {MAX_TEXT_CHARACTERS} characters and {MAX_TEXT_BYTES} bytes."
        ));
    }
    Ok(value)
}

fn validate_per_page_options_with_control(
    document: &Document,
    selected_pages: &[usize],
    crop: Option<CropMargins>,
    resize: Option<PageResize>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let pages = document.get_pages();
    for (index, page_number) in selected_pages.iter().enumerate() {
        checkpoint_finish_loop(
            control,
            index,
            selected_pages.len(),
            24,
            34,
            "Checking selected page geometry",
        )?;
        let page_id = pages
            .get(&(*page_number as u32))
            .ok_or_else(|| format!("Page {page_number} is not present in this PDF."))?;
        let geometry = page_geometry(document, *page_id)?;
        let source = crop
            .map(|margins| crop_page_box(geometry, margins))
            .transpose()?
            .unwrap_or(geometry.page);
        if let Some(resize) = resize {
            resize_geometry(geometry.rotation, source, resize)?;
        }
    }
    Ok(())
}

fn checkpoint_finish_loop(
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
    let span = usize::from(end.saturating_sub(start));
    let progress = usize::from(start).saturating_add(span.saturating_mul(index) / total.max(1));
    control.checkpoint(u8::try_from(progress).unwrap_or(end).min(end), stage)
}

fn checkpoint_finishing_inspection_loop(
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
    let span = usize::from(end.saturating_sub(start));
    let progress = usize::from(start).saturating_add(span.saturating_mul(index + 1) / total.max(1));
    control.checkpoint(
        u8::try_from(progress).unwrap_or(end).min(end),
        format!("{stage} {} of {total}", index + 1),
    )
}

fn parse_page_range(value: &str, page_count: usize) -> Result<Vec<usize>, String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("all") {
        return Ok((1..=page_count).collect());
    }
    let mut pages = BTreeSet::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err("The page selection contains an empty item.".to_string());
        }
        if part.eq_ignore_ascii_case("odd") || part.eq_ignore_ascii_case("even") {
            let parity = usize::from(part.eq_ignore_ascii_case("odd"));
            pages.extend((1..=page_count).filter(|page| page % 2 == parity));
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            let start = parse_page_number(start, page_count)?;
            let end = parse_page_number(end, page_count)?;
            if start <= end {
                pages.extend(start..=end);
            } else {
                pages.extend(end..=start);
            }
        } else {
            pages.insert(parse_page_number(part, page_count)?);
        }
        if pages.len() > MAX_PAGES {
            return Err(format!(
                "A page-finishing operation can include at most {MAX_PAGES} pages."
            ));
        }
    }
    if pages.is_empty() {
        return Err("Choose at least one page to finish.".to_string());
    }
    Ok(pages.into_iter().collect())
}

fn parse_page_number(value: &str, page_count: usize) -> Result<usize, String> {
    let page = value
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("“{}” is not a valid page number.", value.trim()))?;
    if page == 0 || page > page_count {
        return Err(format!(
            "Page {page} is outside this {page_count}-page PDF."
        ));
    }
    Ok(page)
}

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedPdf, String> {
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
                "The PDF could not be decrypted for page finishing. Check its password.".to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    if page_count > MAX_PAGES {
        return Err(format!(
            "Page finishing supports at most {MAX_PAGES} pages in one PDF."
        ));
    }
    let certificate_signature = document_has_certificate_signature(&document);
    let has_forms = document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"AcroForm"));
    let has_bookmarks = document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"Outlines"));
    Ok(LoadedPdf {
        document,
        page_count,
        was_encrypted,
        certificate_signature,
        has_forms,
        has_bookmarks,
    })
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<PageGeometry, String> {
    let page_box = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "A PDF page does not define a crop or media box.".to_string())?;
    let page_box = parse_page_box(document, &page_box, "page box")?;
    let rotation = match inherited_page_value(document, page_id, b"Rotate")? {
        Some(value) => dereference_object(document, &value, "page rotation")?
            .as_i64()
            .map_err(|_| "A PDF page has an invalid rotation.".to_string())?,
        None => 0,
    }
    .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("A PDF page has an unsupported rotation.".to_string());
    }
    Ok(geometry_for_box(rotation, page_box))
}

fn geometry_for_box(rotation: i64, page: PageBox) -> PageGeometry {
    let (visual_width, visual_height) = if matches!(rotation, 90 | 270) {
        (page.height(), page.width())
    } else {
        (page.width(), page.height())
    };
    PageGeometry {
        page,
        rotation,
        visual_width,
        visual_height,
    }
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

fn parse_page_box(document: &Document, value: &Object, label: &str) -> Result<PageBox, String> {
    let value = dereference_object(document, value, label)?;
    let values = value
        .as_array()
        .map_err(|_| format!("The PDF {label} is not an array."))?;
    if values.len() != 4 {
        return Err(format!("The PDF {label} must contain four coordinates."));
    }
    let coordinates = values
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, _>>()?;
    let page = PageBox {
        left: coordinates[0].min(coordinates[2]),
        bottom: coordinates[1].min(coordinates[3]),
        right: coordinates[0].max(coordinates[2]),
        top: coordinates[1].max(coordinates[3]),
    };
    if !page.valid(MIN_VISIBLE_PAGE_POINTS) {
        return Err(format!("The PDF {label} has invalid dimensions."));
    }
    Ok(page)
}

fn dereference_object<'a>(
    document: &'a Document,
    value: &'a Object,
    label: &str,
) -> Result<&'a Object, String> {
    match value {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| format!("The PDF {label} is invalid: {error}")),
        value => Ok(value),
    }
}

fn pdf_number_value(value: &Object) -> Result<f64, String> {
    match value {
        Object::Integer(value) => Ok(*value as f64),
        Object::Real(value) => Ok(f64::from(*value)),
        _ => Err("A PDF coordinate is not numeric.".to_string()),
    }
}

impl PageBox {
    fn width(self) -> f64 {
        self.right - self.left
    }

    fn height(self) -> f64 {
        self.top - self.bottom
    }

    fn valid(self, minimum: f64) -> bool {
        [self.left, self.bottom, self.right, self.top]
            .iter()
            .all(|value| value.is_finite())
            && self.width() >= minimum
            && self.height() >= minimum
    }
}

fn crop_page_box(geometry: PageGeometry, margins: CropMargins) -> Result<PageBox, String> {
    let left = margins.left_pt;
    let top = margins.top_pt;
    let right = geometry.visual_width - margins.right_pt;
    let bottom = geometry.visual_height - margins.bottom_pt;
    if right - left < MIN_VISIBLE_PAGE_POINTS || bottom - top < MIN_VISIBLE_PAGE_POINTS {
        return Err(
            "The crop margins leave less than 36 points of visible width or height on a selected page."
                .to_string(),
        );
    }
    let corners = [
        visual_to_pdf(geometry, left, top),
        visual_to_pdf(geometry, right, top),
        visual_to_pdf(geometry, left, bottom),
        visual_to_pdf(geometry, right, bottom),
    ];
    let page = PageBox {
        left: corners
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min),
        bottom: corners
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min),
        right: corners
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max),
        top: corners
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max),
    };
    if !page.valid(MIN_VISIBLE_PAGE_POINTS) {
        return Err("The crop settings produced an invalid page box.".to_string());
    }
    Ok(page)
}

fn visual_to_pdf(geometry: PageGeometry, visual_x: f64, visual_y: f64) -> (f64, f64) {
    let visual_bottom = geometry.visual_height - visual_y;
    let (x, y) = match geometry.rotation {
        90 => (geometry.page.width() - visual_bottom, visual_x),
        180 => (
            geometry.page.width() - visual_x,
            geometry.page.height() - visual_bottom,
        ),
        270 => (visual_bottom, geometry.page.height() - visual_x),
        _ => (visual_x, visual_bottom),
    };
    (geometry.page.left + x, geometry.page.bottom + y)
}

fn resize_geometry(
    rotation: i64,
    source: PageBox,
    resize: PageResize,
) -> Result<(PageGeometry, AffineTransform), String> {
    let (raw_width, raw_height) = if matches!(rotation, 90 | 270) {
        (resize.height_pt, resize.width_pt)
    } else {
        (resize.width_pt, resize.height_pt)
    };
    let available_width = raw_width - resize.margin_pt * 2.0;
    let available_height = raw_height - resize.margin_pt * 2.0;
    let scale = (available_width / source.width()).min(available_height / source.height());
    if !scale.is_finite() || scale <= 0.0 {
        return Err("The selected page cannot be fitted to that paper size.".to_string());
    }
    let placed_width = source.width() * scale;
    let placed_height = source.height() * scale;
    let transform = AffineTransform {
        scale,
        translate_x: (raw_width - placed_width) / 2.0 - source.left * scale,
        translate_y: (raw_height - placed_height) / 2.0 - source.bottom * scale,
    };
    let page = PageBox {
        left: 0.0,
        bottom: 0.0,
        right: raw_width,
        top: raw_height,
    };
    Ok((geometry_for_box(rotation, page), transform))
}

impl AffineTransform {
    fn point(self, x: f64, y: f64) -> (f64, f64) {
        (
            x * self.scale + self.translate_x,
            y * self.scale + self.translate_y,
        )
    }
}

fn set_page_crop_box(
    document: &mut Document,
    page_id: ObjectId,
    page_box: PageBox,
) -> Result<(), String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected PDF page is invalid: {error}"))?
        .clone();
    page.set("CropBox", page_box_object(page_box));
    document.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

fn set_resized_page_boxes(
    document: &mut Document,
    page_id: ObjectId,
    page_box: PageBox,
) -> Result<(), String> {
    let secondary_boxes = [b"BleedBox".as_slice(), b"TrimBox", b"ArtBox"]
        .into_iter()
        .map(|key| inherited_page_value(document, page_id, key).map(|value| (key, value.is_some())))
        .collect::<Result<Vec<_>, _>>()?;
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected PDF page is invalid: {error}"))?
        .clone();
    page.set("MediaBox", page_box_object(page_box));
    page.set("CropBox", page_box_object(page_box));
    for (key, present) in secondary_boxes {
        if present {
            page.set(key, page_box_object(page_box));
        } else {
            page.remove(key);
        }
    }
    document.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

fn wrap_page_content(
    document: &mut Document,
    page_id: ObjectId,
    source_box: PageBox,
    transform: AffineTransform,
) -> Result<(), String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected PDF page is invalid: {error}"))?
        .clone();
    let existing = page.get(b"Contents").ok().cloned();
    let mut streams = content_streams(document, existing, 0)?;
    let prefix = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    pdf_real(transform.scale),
                    0.into(),
                    0.into(),
                    pdf_real(transform.scale),
                    pdf_real(transform.translate_x),
                    pdf_real(transform.translate_y),
                ],
            ),
            Operation::new(
                "re",
                vec![
                    pdf_real(source_box.left),
                    pdf_real(source_box.bottom),
                    pdf_real(source_box.width()),
                    pdf_real(source_box.height()),
                ],
            ),
            Operation::new("W", vec![]),
            Operation::new("n", vec![]),
        ],
    }
    .encode()
    .map_err(|error| format!("The page resize transform could not be encoded: {error}"))?;
    let suffix = Content {
        operations: vec![Operation::new("Q", vec![])],
    }
    .encode()
    .map_err(|error| format!("The page resize transform could not be encoded: {error}"))?;
    let prefix_id = document.add_object(Stream::new(dictionary! {}, prefix));
    let suffix_id = document.add_object(Stream::new(dictionary! {}, suffix));
    streams.insert(0, Object::Reference(prefix_id));
    streams.push(Object::Reference(suffix_id));
    page.set("Contents", Object::Array(streams));
    document.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

fn content_streams(
    document: &mut Document,
    value: Option<Object>,
    depth: usize,
) -> Result<Vec<Object>, String> {
    if depth > 8 {
        return Err("A PDF page content list is too deeply nested.".to_string());
    }
    match value {
        None | Some(Object::Null) => Ok(Vec::new()),
        Some(Object::Array(values)) => {
            let mut result = Vec::new();
            for value in values {
                result.extend(content_streams(document, Some(value), depth + 1)?);
            }
            Ok(result)
        }
        Some(Object::Reference(id)) => match document.get_object(id) {
            Ok(Object::Array(values)) => {
                content_streams(document, Some(Object::Array(values.clone())), depth + 1)
            }
            Ok(Object::Stream(_)) => Ok(vec![Object::Reference(id)]),
            Ok(_) => Err("A PDF page has an invalid content reference.".to_string()),
            Err(error) => Err(format!("A PDF page content stream is invalid: {error}")),
        },
        Some(Object::Stream(stream)) => {
            let id = document.add_object(stream);
            Ok(vec![Object::Reference(id)])
        }
        Some(_) => Err("A PDF page has an invalid content stream.".to_string()),
    }
}

fn transform_page_annotations(
    document: &mut Document,
    page_id: ObjectId,
    transform: AffineTransform,
) -> Result<usize, String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected PDF page is invalid: {error}"))?
        .clone();
    let Some(annots) = page.get(b"Annots").ok().cloned() else {
        return Ok(0);
    };
    let (array_id, mut values) = match annots {
        Object::Array(values) => (None, values),
        Object::Reference(id) => {
            let values = document
                .get_object(id)
                .and_then(Object::as_array)
                .map_err(|error| format!("A PDF page annotation list is invalid: {error}"))?
                .clone();
            (Some(id), values)
        }
        Object::Null => return Ok(0),
        _ => return Err("A PDF page annotation list is invalid.".to_string()),
    };
    let mut visited = HashSet::new();
    let mut adjusted = 0_usize;
    for value in &mut values {
        match value {
            Object::Reference(id) => {
                adjusted += transform_annotation_by_id(document, *id, transform, &mut visited)?;
            }
            Object::Dictionary(dictionary) => {
                transform_annotation_dictionary(document, dictionary, transform, &mut visited)?;
                adjusted += 1;
            }
            _ => return Err("A PDF page annotation entry is invalid.".to_string()),
        }
    }
    if let Some(array_id) = array_id {
        document.objects.insert(array_id, Object::Array(values));
    } else {
        page.set("Annots", Object::Array(values));
        document.objects.insert(page_id, Object::Dictionary(page));
    }
    Ok(adjusted)
}

fn transform_annotation_by_id(
    document: &mut Document,
    id: ObjectId,
    transform: AffineTransform,
    visited: &mut HashSet<ObjectId>,
) -> Result<usize, String> {
    if !visited.insert(id) {
        return Ok(0);
    }
    let mut dictionary = document
        .get_dictionary(id)
        .map_err(|error| format!("A PDF annotation is invalid: {error}"))?
        .clone();
    transform_annotation_dictionary(document, &mut dictionary, transform, visited)?;
    document.objects.insert(id, Object::Dictionary(dictionary));
    Ok(1)
}

fn transform_annotation_dictionary(
    document: &mut Document,
    dictionary: &mut Dictionary,
    transform: AffineTransform,
    visited: &mut HashSet<ObjectId>,
) -> Result<(), String> {
    if let Ok(value) = dictionary.get(b"Rect").cloned() {
        dictionary.set("Rect", transform_rect_object(&value, transform)?);
    }
    for key in [b"QuadPoints".as_slice(), b"L", b"Vertices", b"CL"] {
        if let Ok(value) = dictionary.get(key).cloned() {
            dictionary.set(key, transform_coordinate_array(&value, transform)?);
        }
    }
    if let Ok(value) = dictionary.get(b"InkList").cloned() {
        let lines = value
            .as_array()
            .map_err(|_| "A PDF ink annotation has an invalid point list.".to_string())?
            .iter()
            .map(|line| transform_coordinate_array(line, transform))
            .collect::<Result<Vec<_>, _>>()?;
        dictionary.set("InkList", Object::Array(lines));
    }
    if let Ok(value) = dictionary.get(b"RD").cloned() {
        let values = value
            .as_array()
            .map_err(|_| "A PDF annotation has invalid rectangle differences.".to_string())?
            .iter()
            .map(|value| pdf_number_value(value).map(|value| pdf_real(value * transform.scale)))
            .collect::<Result<Vec<_>, _>>()?;
        dictionary.set("RD", Object::Array(values));
    }
    if let Ok(Object::Reference(popup_id)) = dictionary.get(b"Popup") {
        transform_annotation_by_id(document, *popup_id, transform, visited)?;
    }
    Ok(())
}

fn transform_rect_object(value: &Object, transform: AffineTransform) -> Result<Object, String> {
    let values = value
        .as_array()
        .map_err(|_| "A PDF annotation rectangle is invalid.".to_string())?;
    if values.len() != 4 {
        return Err("A PDF annotation rectangle must contain four coordinates.".to_string());
    }
    let first = transform.point(pdf_number_value(&values[0])?, pdf_number_value(&values[1])?);
    let second = transform.point(pdf_number_value(&values[2])?, pdf_number_value(&values[3])?);
    Ok(Object::Array(vec![
        pdf_real(first.0.min(second.0)),
        pdf_real(first.1.min(second.1)),
        pdf_real(first.0.max(second.0)),
        pdf_real(first.1.max(second.1)),
    ]))
}

fn transform_coordinate_array(
    value: &Object,
    transform: AffineTransform,
) -> Result<Object, String> {
    let values = value
        .as_array()
        .map_err(|_| "A PDF annotation coordinate list is invalid.".to_string())?;
    if values.len() < 2 || values.len() % 2 != 0 {
        return Err("A PDF annotation coordinate list is incomplete.".to_string());
    }
    let mut transformed = Vec::with_capacity(values.len());
    for pair in values.chunks_exact(2) {
        let point = transform.point(pdf_number_value(&pair[0])?, pdf_number_value(&pair[1])?);
        transformed.push(pdf_real(point.0));
        transformed.push(pdf_real(point.1));
    }
    Ok(Object::Array(transformed))
}

#[allow(clippy::too_many_arguments)]
fn add_page_marks(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    font_id: Option<ObjectId>,
    marker: &str,
    file_name: &str,
    page_number: usize,
    page_count: usize,
    selected_index: usize,
    watermark: Option<&TextWatermark>,
    header_footer: Option<&HeaderFooter>,
    bates: Option<&BatesNumbering>,
) -> Result<RenderedMarks, String> {
    let mut result = RenderedMarks {
        marked: false,
        bates_numbered: false,
        substitutions: 0,
        form_count: 0,
    };
    let Some(font_id) = font_id else {
        return Ok(result);
    };
    if let Some(watermark) = watermark.filter(|watermark| !watermark.over_content) {
        let built = build_marks_form(
            document,
            geometry,
            font_id,
            marker,
            file_name,
            page_number,
            page_count,
            selected_index,
            Some(watermark),
            None,
            None,
            "Underlay",
        )?;
        attach_marks_form(document, page_id, geometry, built.form_id, false)?;
        result.marked = true;
        result.substitutions += built.substitutions;
        result.form_count += 1;
    }

    let overlay_watermark = watermark.filter(|watermark| watermark.over_content);
    if overlay_watermark.is_some() || header_footer.is_some() || bates.is_some() {
        let built = build_marks_form(
            document,
            geometry,
            font_id,
            marker,
            file_name,
            page_number,
            page_count,
            selected_index,
            overlay_watermark,
            header_footer,
            bates,
            "Overlay",
        )?;
        attach_marks_form(document, page_id, geometry, built.form_id, true)?;
        result.marked = true;
        result.bates_numbered = bates.is_some();
        result.substitutions += built.substitutions;
        result.form_count += 1;
    }
    Ok(result)
}

struct BuiltMarksForm {
    form_id: ObjectId,
    substitutions: usize,
}

#[allow(clippy::too_many_arguments)]
fn build_marks_form(
    document: &mut Document,
    geometry: PageGeometry,
    font_id: ObjectId,
    marker: &str,
    file_name: &str,
    page_number: usize,
    page_count: usize,
    selected_index: usize,
    watermark: Option<&TextWatermark>,
    header_footer: Option<&HeaderFooter>,
    bates: Option<&BatesNumbering>,
    layer: &str,
) -> Result<BuiltMarksForm, String> {
    let mut operations = Vec::new();
    let mut substitutions = 0_usize;
    if let Some(watermark) = watermark {
        substitutions += usize::from(draw_watermark(&mut operations, geometry, watermark)?);
    }
    let mut has_header = false;
    let mut has_footer = false;
    if let Some(header_footer) = header_footer {
        if !header_footer.header_text.is_empty() {
            let text = expand_template(
                &header_footer.header_text,
                file_name,
                page_number,
                page_count,
            )?;
            substitutions += usize::from(draw_aligned_text(
                &mut operations,
                &text,
                geometry.visual_width,
                geometry.visual_height,
                geometry.visual_height - header_footer.margin_pt - header_footer.font_size_pt,
                header_footer.font_size_pt,
                header_footer.margin_pt,
                header_footer.header_alignment,
                header_footer.colour,
            )?);
            has_header = true;
        }
        if !header_footer.footer_text.is_empty() {
            let text = expand_template(
                &header_footer.footer_text,
                file_name,
                page_number,
                page_count,
            )?;
            substitutions += usize::from(draw_aligned_text(
                &mut operations,
                &text,
                geometry.visual_width,
                geometry.visual_height,
                header_footer.margin_pt,
                header_footer.font_size_pt,
                header_footer.margin_pt,
                header_footer.footer_alignment,
                header_footer.colour,
            )?);
            has_footer = true;
        }
    }
    if let Some(bates) = bates {
        let number = bates
            .start_number
            .checked_add(selected_index as u64)
            .ok_or_else(|| "The Bates number sequence is too large.".to_string())?;
        let text = format!(
            "{}{:0width$}{}",
            bates.prefix,
            number,
            bates.suffix,
            width = bates.digits
        );
        if text.chars().count() > MAX_EXPANDED_TEXT_CHARACTERS {
            return Err("An expanded Bates number is too long.".to_string());
        }
        let top = matches!(
            bates.position,
            BatesPosition::TopLeft | BatesPosition::TopCentre | BatesPosition::TopRight
        );
        let alignment = match bates.position {
            BatesPosition::TopLeft | BatesPosition::BottomLeft => TextAlignment::Left,
            BatesPosition::TopCentre | BatesPosition::BottomCentre => TextAlignment::Centre,
            BatesPosition::TopRight | BatesPosition::BottomRight => TextAlignment::Right,
        };
        let stacked_offset = if (top && has_header) || (!top && has_footer) {
            bates.font_size_pt + 3.0
        } else {
            0.0
        };
        let y = if top {
            geometry.visual_height - bates.margin_pt - bates.font_size_pt - stacked_offset
        } else {
            bates.margin_pt + stacked_offset
        };
        substitutions += usize::from(draw_aligned_text(
            &mut operations,
            &text,
            geometry.visual_width,
            geometry.visual_height,
            y,
            bates.font_size_pt,
            bates.margin_pt,
            alignment,
            bates.colour,
        )?);
    }

    let mut fonts = Dictionary::new();
    fonts.set("TufekciFinishFont", font_id);
    let mut resources = Dictionary::new();
    resources.set("Font", fonts);
    if let Some(watermark) = watermark {
        let mut states = Dictionary::new();
        states.set(
            "TufekciFinishOpacity",
            dictionary! {
                "Type" => "ExtGState",
                "ca" => pdf_real(watermark.opacity),
                "CA" => pdf_real(watermark.opacity),
            },
        );
        resources.set("ExtGState", states);
    }
    let content = Content { operations }
        .encode()
        .map_err(|error| format!("The page marks could not be encoded: {error}"))?;
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => Object::Array(vec![
                0.into(),
                0.into(),
                pdf_real(geometry.visual_width),
                pdf_real(geometry.visual_height),
            ]),
            "Resources" => resources,
            "TufekciPageFinishMarker" => Object::String(marker.as_bytes().to_vec(), StringFormat::Literal),
            "TufekciPageFinishLayer" => layer,
        },
        content,
    );
    stream
        .compress()
        .map_err(|error| format!("The page marks could not be compressed: {error}"))?;
    Ok(BuiltMarksForm {
        form_id: document.add_object(stream),
        substitutions,
    })
}

fn draw_watermark(
    operations: &mut Vec<Operation>,
    geometry: PageGeometry,
    watermark: &TextWatermark,
) -> Result<bool, String> {
    let (encoded, substituted) = encode_win_ansi(&watermark.text);
    let estimated_units = encoded.len().max(1) as f64 * 0.54;
    let diagonal = geometry
        .visual_width
        .hypot(geometry.visual_height)
        .max(MIN_VISIBLE_PAGE_POINTS);
    let font_size = watermark
        .font_size_pt
        .min(diagonal * 0.82 / estimated_units)
        .max(6.0);
    let text_width = estimated_units * font_size;
    if text_width > diagonal * 0.95 {
        return Err("The watermark text is too long for a selected page.".to_string());
    }
    let angle = watermark.angle_degrees.to_radians();
    let cosine = angle.cos();
    let sine = angle.sin();
    let centre_x = geometry.visual_width / 2.0;
    let centre_y = geometry.visual_height / 2.0;
    let translate_x = centre_x - cosine * text_width / 2.0 + sine * font_size * 0.35;
    let translate_y = centre_y - sine * text_width / 2.0 - cosine * font_size * 0.35;
    operations.extend([
        Operation::new("q", vec![]),
        Operation::new("gs", vec![Object::Name(b"TufekciFinishOpacity".to_vec())]),
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![
                Object::Name(b"TufekciFinishFont".to_vec()),
                pdf_real(font_size),
            ],
        ),
        Operation::new(
            "rg",
            watermark
                .colour
                .into_iter()
                .map(|value| pdf_real(f64::from(value)))
                .collect(),
        ),
        Operation::new(
            "Tm",
            vec![
                pdf_real(cosine),
                pdf_real(sine),
                pdf_real(-sine),
                pdf_real(cosine),
                pdf_real(translate_x),
                pdf_real(translate_y),
            ],
        ),
        Operation::new("Tj", vec![Object::String(encoded, StringFormat::Literal)]),
        Operation::new("ET", vec![]),
        Operation::new("Q", vec![]),
    ]);
    Ok(substituted)
}

#[allow(clippy::too_many_arguments)]
fn draw_aligned_text(
    operations: &mut Vec<Operation>,
    text: &str,
    page_width: f64,
    page_height: f64,
    y: f64,
    requested_font_size: f64,
    margin: f64,
    alignment: TextAlignment,
    colour: [f32; 3],
) -> Result<bool, String> {
    let (encoded, substituted) = encode_win_ansi(text);
    let units = encoded.len().max(1) as f64 * 0.52;
    let available_width = (page_width - margin * 2.0).max(1.0);
    let font_size = requested_font_size.min(available_width / units);
    if font_size < 4.0 {
        return Err(format!(
            "The expanded page text “{}” is too long for a selected page.",
            truncate(text, 80)
        ));
    }
    if y < 0.0 || y + font_size > page_height {
        return Err(format!(
            "The page margin leaves no room for “{}” on a selected page.",
            truncate(text, 80)
        ));
    }
    let text_width = units * font_size;
    let x = match alignment {
        TextAlignment::Left => margin,
        TextAlignment::Centre => (page_width - text_width) / 2.0,
        TextAlignment::Right => page_width - margin - text_width,
    };
    operations.extend([
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![
                Object::Name(b"TufekciFinishFont".to_vec()),
                pdf_real(font_size),
            ],
        ),
        Operation::new(
            "rg",
            colour
                .into_iter()
                .map(|value| pdf_real(f64::from(value)))
                .collect(),
        ),
        Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                pdf_real(x.max(0.0)),
                pdf_real(y),
            ],
        ),
        Operation::new("Tj", vec![Object::String(encoded, StringFormat::Literal)]),
        Operation::new("ET", vec![]),
    ]);
    Ok(substituted)
}

fn expand_template(
    template: &str,
    file_name: &str,
    page_number: usize,
    page_count: usize,
) -> Result<String, String> {
    let text = template
        .replace("{page}", &page_number.to_string())
        .replace("{pages}", &page_count.to_string())
        .replace("{file}", file_name);
    if text.chars().count() > MAX_EXPANDED_TEXT_CHARACTERS || text.len() > MAX_TEXT_BYTES * 2 {
        return Err("Expanded header or footer text is too long.".to_string());
    }
    Ok(text)
}

fn attach_marks_form(
    document: &mut Document,
    page_id: ObjectId,
    geometry: PageGeometry,
    form_id: ObjectId,
    overlay: bool,
) -> Result<(), String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected PDF page is invalid: {error}"))?
        .clone();
    let inherited_resources = inherited_page_value(document, page_id, b"Resources")?;
    let mut resources = match inherited_resources {
        Some(value) => resolved_dictionary(document, &value, "page resources")?,
        None => Dictionary::new(),
    };
    let mut xobjects = match resources.get(b"XObject") {
        Ok(value) => resolved_dictionary(document, value, "page XObject resources")?,
        Err(_) => Dictionary::new(),
    };
    let mut resource_name = format!("TufekciPageFinish{}", form_id.0).into_bytes();
    let mut suffix = 1_u32;
    while xobjects.has(&resource_name) {
        resource_name = format!("TufekciPageFinish{}_{}", form_id.0, suffix).into_bytes();
        suffix += 1;
    }
    xobjects.set(resource_name.clone(), form_id);
    resources.set("XObject", xobjects);
    page.set("Resources", resources);
    let matrix = visual_overlay_matrix(geometry);
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", matrix.into_iter().map(pdf_real).collect::<Vec<_>>()),
            Operation::new("Do", vec![Object::Name(resource_name)]),
            Operation::new("Q", vec![]),
        ],
    }
    .encode()
    .map_err(|error| format!("The page mark placement could not be encoded: {error}"))?;
    let content_id = document.add_object(Stream::new(dictionary! {}, content));
    let existing = page.get(b"Contents").ok().cloned();
    let mut streams = content_streams(document, existing, 0)?;
    if overlay {
        streams.push(Object::Reference(content_id));
    } else {
        streams.insert(0, Object::Reference(content_id));
    }
    page.set("Contents", Object::Array(streams));
    document.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

fn visual_overlay_matrix(geometry: PageGeometry) -> [f64; 6] {
    match geometry.rotation {
        90 => [
            0.0,
            1.0,
            -1.0,
            0.0,
            geometry.page.right,
            geometry.page.bottom,
        ],
        180 => [-1.0, 0.0, 0.0, -1.0, geometry.page.right, geometry.page.top],
        270 => [0.0, -1.0, 1.0, 0.0, geometry.page.left, geometry.page.top],
        _ => [1.0, 0.0, 0.0, 1.0, geometry.page.left, geometry.page.bottom],
    }
}

fn resolved_dictionary(
    document: &Document,
    value: &Object,
    label: &str,
) -> Result<Dictionary, String> {
    match value {
        Object::Dictionary(dictionary) => Ok(dictionary.clone()),
        Object::Reference(id) => document
            .get_dictionary(*id)
            .cloned()
            .map_err(|error| format!("The PDF {label} are invalid: {error}")),
        _ => Err(format!("The PDF {label} are not a dictionary.")),
    }
}

fn mark_finished_page(
    document: &mut Document,
    page_id: ObjectId,
    marker: &str,
    selected_index: usize,
    cropped: bool,
    resized: bool,
    marked: bool,
) -> Result<(), String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected PDF page is invalid: {error}"))?
        .clone();
    page.set(
        "TufekciPageFinish",
        dictionary! {
            "Version" => 1,
            "Marker" => Object::String(marker.as_bytes().to_vec(), StringFormat::Literal),
            "SelectedIndex" => selected_index as i64,
            "Cropped" => cropped,
            "Resized" => resized,
            "Marked" => marked,
        },
    );
    document.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

fn verify_finished_pdf_path(
    path: &Path,
    password: Option<&str>,
    expected: &FinishVerificationExpectations<'_>,
    control: &PdfJobExecutionControl,
    progress_start: u8,
    progress_end: u8,
) -> Result<(), String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The finished PDF failed its reopening check: {error}"))?;
    let encrypted = document.is_encrypted();
    if encrypted != expected.encrypted {
        return Err(if expected.encrypted {
            "The finished PDF was not encrypted as requested and was not saved.".to_string()
        } else {
            "The finished PDF unexpectedly remained encrypted and was not saved.".to_string()
        });
    }
    if encrypted {
        document
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "The protected finished PDF could not be reopened with its new password."
                    .to_string()
            })?;
    }
    verify_finished_pdf(&document, expected, control, progress_start, progress_end)
}

fn verify_finished_pdf(
    document: &Document,
    expected: &FinishVerificationExpectations<'_>,
    control: &PdfJobExecutionControl,
    progress_start: u8,
    progress_end: u8,
) -> Result<(), String> {
    let pages = document.get_pages();
    if pages.len() != expected.page_count {
        return Err("The finished PDF changed the page count and was not saved.".to_string());
    }
    if expected.forms
        && !document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        return Err("The finished PDF lost its form structure and was not saved.".to_string());
    }
    if expected.bookmarks
        && !document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"Outlines"))
    {
        return Err("The finished PDF lost its bookmarks and was not saved.".to_string());
    }
    let annotation_counts = page_annotation_counts_with_control(document, control)?;
    if annotation_counts != expected.original_annotation_counts {
        return Err("The finished PDF changed its annotation count and was not saved.".to_string());
    }
    for (index, expected_page) in expected.expected_pages.iter().enumerate() {
        checkpoint_finish_loop(
            control,
            index,
            expected.expected_pages.len(),
            progress_start,
            progress_end,
            "Verifying finished page structure",
        )?;
        let page_id = pages
            .get(&(expected_page.page_number as u32))
            .ok_or_else(|| "A finished page disappeared during verification.".to_string())?;
        let crop = inherited_page_value(document, *page_id, b"CropBox")?
            .or(inherited_page_value(document, *page_id, b"MediaBox")?)
            .ok_or_else(|| "A finished page lost its visible page box.".to_string())?;
        let crop = parse_page_box(document, &crop, "finished crop box")?;
        if !boxes_close(crop, expected_page.crop_box) {
            return Err(format!(
                "Page {} did not retain its reviewed crop or paper size.",
                expected_page.page_number
            ));
        }
        if let Some(expected_media) = expected_page.media_box {
            let media = inherited_page_value(document, *page_id, b"MediaBox")?
                .ok_or_else(|| "A resized page lost its media box.".to_string())?;
            let media = parse_page_box(document, &media, "finished media box")?;
            if !boxes_close(media, expected_media) {
                return Err(format!(
                    "Page {} did not retain its reviewed paper size.",
                    expected_page.page_number
                ));
            }
        }
        let page = document
            .get_dictionary(*page_id)
            .map_err(|error| format!("A finished page is invalid: {error}"))?;
        let finish = page
            .get(b"TufekciPageFinish")
            .map_err(|_| "A finished page lost its verification marker.".to_string())?;
        let finish = dereference_object(document, finish, "page-finishing marker")?
            .as_dict()
            .map_err(|_| "A finished page lost its verification marker.".to_string())?;
        let saved_marker = finish
            .get(b"Marker")
            .and_then(Object::as_str)
            .map_err(|_| "A finished page has an invalid verification marker.".to_string())?;
        if saved_marker != expected_page.marker.as_bytes() {
            return Err("A finished page marker did not match the reviewed export.".to_string());
        }
        let selected_index = finish
            .get(b"SelectedIndex")
            .and_then(Object::as_i64)
            .map_err(|_| "A finished page has an invalid sequence marker.".to_string())?;
        let cropped = finish
            .get(b"Cropped")
            .and_then(Object::as_bool)
            .map_err(|_| "A finished page has an invalid crop marker.".to_string())?;
        let resized = finish
            .get(b"Resized")
            .and_then(Object::as_bool)
            .map_err(|_| "A finished page has an invalid resize marker.".to_string())?;
        let marked = finish
            .get(b"Marked")
            .and_then(Object::as_bool)
            .map_err(|_| "A finished page has an invalid page-mark marker.".to_string())?;
        if selected_index != expected_page.selected_index as i64
            || cropped != expected_page.cropped
            || resized != expected_page.resized
            || marked != expected_page.marked
        {
            return Err("A finished page did not match its reviewed operations.".to_string());
        }
    }
    if finish_form_count_with_control(document, expected.marker, control)?
        != expected.expected_finish_forms
    {
        return Err(
            "One or more verified page-mark layers were lost before publication.".to_string(),
        );
    }
    Ok(())
}

fn boxes_close(left: PageBox, right: PageBox) -> bool {
    [
        (left.left, right.left),
        (left.bottom, right.bottom),
        (left.right, right.right),
        (left.top, right.top),
    ]
    .into_iter()
    .all(|(left, right)| (left - right).abs() <= 0.05)
}

fn finish_form_count_with_control(
    document: &Document,
    marker: &str,
    control: &PdfJobExecutionControl,
) -> Result<usize, String> {
    let mut count = 0_usize;
    for (index, object) in document.objects.values().enumerate() {
        if index.is_multiple_of(1_024) {
            control.ensure_not_cancelled()?;
        }
        let Object::Stream(stream) = object else {
            continue;
        };
        if stream
            .dict
            .get(b"TufekciPageFinishMarker")
            .and_then(Object::as_str)
            .is_ok_and(|value| value == marker.as_bytes())
        {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
fn page_annotation_counts(document: &Document) -> Result<Vec<usize>, String> {
    document
        .get_pages()
        .values()
        .map(|page_id| page_annotation_count(document, *page_id))
        .collect()
}

fn page_annotation_counts_with_control(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<Vec<usize>, String> {
    let pages = document.get_pages();
    let mut counts = Vec::with_capacity(pages.len());
    for (index, page_id) in pages.values().enumerate() {
        if index.is_multiple_of(32) {
            control.ensure_not_cancelled()?;
        }
        counts.push(page_annotation_count(document, *page_id)?);
    }
    Ok(counts)
}

fn page_annotation_counts_for_inspection(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<Vec<usize>, String> {
    let pages = document.get_pages();
    let mut counts = Vec::with_capacity(pages.len());
    for (index, page_id) in pages.values().enumerate() {
        checkpoint_finishing_inspection_loop(
            control,
            index,
            pages.len(),
            72,
            88,
            "Inspecting Page Finish annotations on page",
        )?;
        counts.push(page_annotation_count(document, *page_id)?);
    }
    Ok(counts)
}

fn page_annotation_count(document: &Document, page_id: ObjectId) -> Result<usize, String> {
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
            .map_err(|error| format!("A PDF page annotation list is invalid: {error}")),
        Ok(_) => Err("A PDF page annotation list is invalid.".to_string()),
    }
}

fn finishing_warnings(
    loaded: &LoadedPdf,
    request: &ExportPdfFinishingRequest,
    stats: &PageFinishStats,
    selected_page_count: usize,
) -> Vec<String> {
    let mut warnings = vec![format!(
        "Page finishing was applied to {selected_page_count} selected page{}. The source PDF was not changed.",
        if selected_page_count == 1 { "" } else { "s" }
    )];
    if request.crop.is_some() {
        warnings.push(
            "Cropping changes the visible page box only. Hidden content remains in the PDF and this is not redaction."
                .to_string(),
        );
    }
    if request.resize.is_some() {
        warnings.push(format!(
            "{} page{} were fitted to the selected paper size. Standard annotation and form-widget coordinates were adjusted with the page content.",
            stats.resized_pages,
            if stats.resized_pages == 1 { "" } else { "s" }
        ));
        if loaded.has_forms {
            warnings.push(
                "Interactive form fields were preserved after resizing; review their appearance and behaviour in the finished copy."
                    .to_string(),
            );
        }
        if loaded.has_bookmarks {
            warnings.push(
                "Bookmarks were preserved, but specialist zoom and coordinate destinations may still refer to their original view."
                    .to_string(),
            );
        }
    }
    if stats.marked_pages > 0 {
        warnings.push(
            "Watermarks, headers, footers, and Bates numbers are visual page content. They do not authenticate the document or replace a certificate signature."
                .to_string(),
        );
    }
    if stats.substituted_appearances > 0 {
        warnings.push(format!(
            "{} page-mark appearance{} contained characters outside the built-in Windows Latin font. Unsupported glyphs were replaced with question marks.",
            stats.substituted_appearances,
            if stats.substituted_appearances == 1 { "" } else { "s" }
        ));
    }
    if request.output_protection.is_some() {
        warnings.push(
            "The finished copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if loaded.was_encrypted {
        warnings.push(
            "The finished copy is not password-protected. Use Protect to apply new AES-256 encryption."
                .to_string(),
        );
    }
    if loaded.certificate_signature {
        warnings.push(
            "Page finishing changed the PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }
    warnings
}

fn page_box_object(page: PageBox) -> Object {
    Object::Array(vec![
        pdf_real(page.left),
        pdf_real(page.bottom),
        pdf_real(page.right),
        pdf_real(page.top),
    ])
}

fn finish_marker() -> Result<String, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("The system clock is invalid: {error}"))?
        .as_nanos();
    Ok(format!("TufekciPageFinish-{}-{nonce}", std::process::id()))
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

fn pdf_real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn truncate(value: &str, limit: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() <= limit {
        value
    } else {
        let mut result = value
            .chars()
            .take(limit.saturating_sub(3))
            .collect::<String>();
        result.push_str("...");
        result
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::text_string;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn parses_ranges_with_correct_odd_even_and_reverse_semantics() {
        assert_eq!(parse_page_range("odd", 6).unwrap(), vec![1, 3, 5]);
        assert_eq!(parse_page_range("even", 6).unwrap(), vec![2, 4, 6]);
        assert_eq!(parse_page_range("5-3,1", 6).unwrap(), vec![1, 3, 4, 5]);
        assert!(parse_page_range("0", 6).unwrap_err().contains("outside"));
    }

    #[test]
    fn inspects_inherited_boxes_rotations_forms_bookmarks_and_annotations() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect_pdf_finishing(InspectPdfFinishingRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(inspection.page_count, 2);
        assert_eq!(inspection.annotation_count, 2);
        assert!(inspection.has_forms);
        assert!(inspection.has_bookmarks);
        assert_eq!(inspection.pages[0].width_pt, 600.0);
        assert_eq!(inspection.pages[0].height_pt, 800.0);
        assert_eq!(inspection.pages[1].width_pt, 800.0);
        assert_eq!(inspection.pages[1].height_pt, 600.0);
        assert_eq!(inspection.pages[1].rotation, 90);
    }

    #[test]
    fn controlled_finishing_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let stages = Arc::new(Mutex::new(Vec::new()));
        let stages_for_progress = Arc::clone(&stages);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            stages_for_progress.lock().unwrap().push((value, stage));
        });

        let inspection = run_pdf_finishing_inspection_job_with_control(
            InspectPdfFinishingRequest {
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
            .any(|(_, stage)| stage == "Inspecting Page Finish page 1 of 2"));
        assert!(stages
            .iter()
            .any(|(_, stage)| { stage == "Inspecting Page Finish annotations on page 1 of 2" }));
        drop(stages);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let cancel_progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Inspecting Page Finish page 1 of 2" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = run_pdf_finishing_inspection_job_with_control(
            InspectPdfFinishingRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(cancelled, cancel_progress),
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn finishing_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-finishing-review.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking Page Finish source"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during Page Finish review\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });

        let error = run_pdf_finishing_inspection_job_with_control(
            InspectPdfFinishingRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert_eq!(
            error,
            "The source PDF changed during Page Finish review. Open it again before editing."
        );
        assert!(!error.contains("private-finishing-review.pdf"));
    }

    #[test]
    fn crops_and_marks_rotated_pages_with_verified_layers() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("finished.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let mut request = request(&input, &output, &inspection);
        request.page_range = "all".to_string();
        request.crop = Some(CropMargins {
            top_pt: 10.0,
            right_pt: 20.0,
            bottom_pt: 30.0,
            left_pt: 40.0,
        });
        request.watermark = Some(TextWatermark {
            text: "Reviewed 🗎".to_string(),
            font_size_pt: 72.0,
            opacity: 0.2,
            angle_degrees: -35.0,
            colour: [0.25, 0.3, 0.4],
            over_content: true,
        });
        request.header_footer = Some(HeaderFooter {
            header_text: "{file}".to_string(),
            footer_text: "Page {page} of {pages}".to_string(),
            header_alignment: TextAlignment::Left,
            footer_alignment: TextAlignment::Centre,
            font_size_pt: 9.0,
            margin_pt: 18.0,
            colour: [0.1, 0.1, 0.1],
        });
        request.bates = Some(BatesNumbering {
            prefix: "TF-".to_string(),
            suffix: String::new(),
            start_number: 7,
            digits: 4,
            position: BatesPosition::BottomRight,
            font_size_pt: 8.0,
            margin_pt: 18.0,
            colour: [0.1, 0.1, 0.1],
        });

        let result = export_pdf_finishing(request).unwrap();
        assert_eq!(result.changed_page_count, 2);
        assert_eq!(result.cropped_page_count, 2);
        assert_eq!(result.marked_page_count, 2);
        assert_eq!(result.bates_number_count, 2);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("not redaction")));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("question marks")));

        let reopened = Document::load(&output).unwrap();
        let pages = reopened.get_pages();
        assert!(boxes_close(
            page_geometry(&reopened, pages[&1]).unwrap().page,
            PageBox {
                left: 50.0,
                bottom: 50.0,
                right: 590.0,
                top: 810.0,
            }
        ));
        assert!(boxes_close(
            page_geometry(&reopened, pages[&2]).unwrap().page,
            PageBox {
                left: 20.0,
                bottom: 60.0,
                right: 580.0,
                top: 800.0,
            }
        ));
        assert_eq!(page_annotation_counts(&reopened).unwrap(), vec![1, 1]);
        assert!(reopened.catalog().unwrap().has(b"AcroForm"));
        assert!(reopened.catalog().unwrap().has(b"Outlines"));
        assert_eq!(
            reopened
                .objects
                .values()
                .filter(|object| {
                    matches!(object, Object::Stream(stream) if stream.dict.has(b"TufekciPageFinishMarker"))
                })
                .count(),
            2
        );
    }

    #[test]
    fn resizes_rotated_page_content_and_widget_geometry() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("resized.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspection(&input);
        let mut request = request(&input, &output, &inspection);
        request.page_range = "2".to_string();
        request.resize = Some(PageResize {
            width_pt: 595.0,
            height_pt: 842.0,
            margin_pt: 36.0,
        });

        let result = export_pdf_finishing(request).unwrap();
        assert_eq!(result.resized_page_count, 1);
        assert_eq!(result.adjusted_annotation_count, 1);
        let reopened = Document::load(&output).unwrap();
        let page_id = reopened.get_pages()[&2];
        let geometry = page_geometry(&reopened, page_id).unwrap();
        assert_eq!(geometry.rotation, 90);
        assert!((geometry.visual_width - 595.0).abs() < 0.05);
        assert!((geometry.visual_height - 842.0).abs() < 0.05);
        let page = reopened.get_dictionary(page_id).unwrap();
        let annots = page
            .get(b"Annots")
            .and_then(Object::as_reference)
            .and_then(|id| reopened.get_object(id))
            .and_then(Object::as_array)
            .unwrap();
        let widget_id = annots[0].as_reference().unwrap();
        let rect = reopened
            .get_dictionary(widget_id)
            .unwrap()
            .get(b"Rect")
            .unwrap();
        let rect = test_rect(rect);
        let scale = 523.0 / 800.0;
        let transform = AffineTransform {
            scale,
            translate_x: (842.0 - 600.0 * scale) / 2.0 - 10.0 * scale,
            translate_y: (595.0 - 800.0 * scale) / 2.0 - 20.0 * scale,
        };
        let first = transform.point(100.0, 120.0);
        let second = transform.point(200.0, 150.0);
        assert!((rect.left - first.0).abs() < 0.05);
        assert!((rect.bottom - first.1).abs() < 0.05);
        assert!((rect.right - second.0).abs() < 0.05);
        assert!((rect.top - second.1).abs() < 0.05);
        assert_eq!(page_annotation_counts(&reopened).unwrap(), vec![1, 1]);
        assert!(reopened.catalog().unwrap().has(b"AcroForm"));
        assert!(reopened.catalog().unwrap().has(b"Outlines"));
    }

    #[test]
    fn requires_signature_acknowledgement_and_rejects_changed_sources() {
        let directory = TestDirectory::new();
        let signed_input = directory.path.join("signed.pdf");
        let signed_output = directory.path.join("signed-finished.pdf");
        fixture(true)
            .save(&signed_input)
            .unwrap()
            .sync_all()
            .unwrap();
        let signed_inspection = inspection(&signed_input);
        let mut signed_request = request(&signed_input, &signed_output, &signed_inspection);
        signed_request.crop = Some(CropMargins {
            top_pt: 1.0,
            right_pt: 1.0,
            bottom_pt: 1.0,
            left_pt: 1.0,
        });
        let error = export_pdf_finishing(signed_request).unwrap_err();
        assert!(error.contains("certificate"));
        assert!(!signed_output.exists());

        let input = directory.path.join("changed.pdf");
        let output = directory.path.join("changed-finished.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspected = inspection(&input);
        let mut file = fs::OpenOptions::new().append(true).open(&input).unwrap();
        file.write_all(b" ").unwrap();
        file.sync_all().unwrap();
        let mut changed_request = request(&input, &output, &inspected);
        changed_request.crop = Some(CropMargins {
            top_pt: 1.0,
            right_pt: 1.0,
            bottom_pt: 1.0,
            left_pt: 1.0,
        });
        let error = export_pdf_finishing(changed_request).unwrap_err();
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_finishing_source_changed_during_export_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("finished.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspected = inspection(&input);
        let mut finishing_request = request(&input, &output, &inspected);
        finishing_request.crop = Some(CropMargins {
            top_pt: 1.0,
            right_pt: 1.0,
            bottom_pt: 1.0,
            left_pt: 1.0,
        });
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking source PDF before publication"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during page finishing\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });
        let control = PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress);

        let error = export_pdf_finishing_with_control(finishing_request, &control).unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn never_overwrites_source_or_existing_destination() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspected = inspection(&input);
        let mut source_request = request(&input, &input, &inspected);
        source_request.crop = Some(CropMargins {
            top_pt: 1.0,
            right_pt: 1.0,
            bottom_pt: 1.0,
            left_pt: 1.0,
        });
        let error = export_pdf_finishing(source_request).unwrap_err();
        assert!(error.contains("already exists") || error.contains("cannot be overwritten"));

        let existing = directory.path.join("existing.pdf");
        fixture(false).save(&existing).unwrap().sync_all().unwrap();
        let mut existing_request = request(&input, &existing, &inspected);
        existing_request.crop = Some(CropMargins {
            top_pt: 1.0,
            right_pt: 1.0,
            bottom_pt: 1.0,
            left_pt: 1.0,
        });
        assert!(export_pdf_finishing(existing_request)
            .unwrap_err()
            .contains("already exists"));
    }

    fn inspection(path: &Path) -> PdfFinishingInspection {
        inspect_pdf_finishing(InspectPdfFinishingRequest {
            input_path: path.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap()
    }

    fn request(
        input: &Path,
        output: &Path,
        inspection: &PdfFinishingInspection,
    ) -> ExportPdfFinishingRequest {
        ExportPdfFinishingRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            acknowledge_certificate_signatures: false,
            page_range: "all".to_string(),
            crop: None,
            resize: None,
            watermark: None,
            header_footer: None,
            bates: None,
        }
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
                "Contents" => content_id,
            };
            if page_number == 2 {
                page.set("Rotate", 90);
            }
            page_ids.push(document.add_object(page));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => 2,
                "MediaBox" => vec![10.into(), 20.into(), 610.into(), 820.into()],
                "Resources" => Dictionary::new(),
            }),
        );

        let direct_annotation = Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![20.into(), 30.into(), 40.into(), 50.into()],
            "Contents" => text_string("Direct note"),
        });
        let mut first_page = document.get_dictionary(page_ids[0]).unwrap().clone();
        first_page.set("Annots", vec![direct_annotation]);
        document
            .objects
            .insert(page_ids[0], Object::Dictionary(first_page));

        let widget_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => text_string("reference"),
            "V" => text_string("Value"),
            "Rect" => vec![100.into(), 120.into(), 200.into(), 150.into()],
            "P" => page_ids[1],
        });
        let annots_id = document.add_object(Object::Array(vec![Object::Reference(widget_id)]));
        let mut second_page = document.get_dictionary(page_ids[1]).unwrap().clone();
        second_page.set("Annots", annots_id);
        document
            .objects
            .insert(page_ids[1], Object::Dictionary(second_page));

        let mut fields = vec![Object::Reference(widget_id)];
        if signed {
            let signature_id = document.add_object(dictionary! {
                "FT" => "Sig",
                "T" => text_string("certificate"),
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
            "Outlines" => dictionary! { "Type" => "Outlines", "Count" => 0 },
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn test_rect(value: &Object) -> PageBox {
        let values = value.as_array().unwrap();
        let coordinates = values
            .iter()
            .map(pdf_number_value)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        PageBox {
            left: coordinates[0].min(coordinates[2]),
            bottom: coordinates[1].min(coordinates[3]),
            right: coordinates[0].max(coordinates[2]),
            top: coordinates[1].max(coordinates[3]),
        }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-page-finish-test",
            );
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
