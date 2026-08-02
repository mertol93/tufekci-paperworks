use crate::child_process::ManagedChild;
use crate::file_safety::{reject_control_characters, validated_new_pdf_output, TemporaryOutput};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::ocr::{
    analyse_raster_with_tesseract_with_cancellation, ensure_ocr_ready, validate_ocr_language,
    OcrConfidenceResult, OCR_REVIEW_CANCELLED_ERROR,
};
use crate::ocr_progress::{is_progress_line, stage_for_ocr, OcrProgressParser};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use crate::scan_cleanup::{clean_scan_image, ScanCleanupOptions, ScanCleanupReport};
use crate::temporary_cleanup::{register_temporary_path, TemporaryKind, TemporaryLease};
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader, Limits, Rgb, RgbImage};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, LoadOptions, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const MAX_SCAN_PAGES: usize = 1_000;
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 30_000;
const MAX_DECODE_ALLOCATION: u64 = 384 * 1024 * 1024;
const MAX_TOOL_OUTPUT_BYTES: usize = 1024 * 1024;
const SCAN_PREVIEW_MAX_DIMENSION: f64 = 900.0;
const MAX_OCR_USER_WORDS: usize = 250;
const MAX_OCR_USER_WORD_BYTES: usize = 128;
const MAX_OCR_USER_WORD_TOTAL_BYTES: usize = 16 * 1024;
const MAX_OCR_TEXT_LAYER_CONTENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_OCR_FORM_DEPTH: usize = 8;
const MAX_OCR_FORM_VISITS: usize = 64;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const OCR_PROGRESS_PLUGIN_SOURCE: &[u8] = include_bytes!("ocr_progress_plugin.py");
pub(crate) const SCAN_JOB_CANCELLED_ERROR: &str = "The scan PDF job was cancelled.";

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanColourMode {
    Colour,
    Greyscale,
    Monochrome,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScanPdfRequest {
    input_paths: Vec<String>,
    output_path: String,
    paper_width_pt: f64,
    paper_height_pt: f64,
    margin_pt: f64,
    dpi: u32,
    jpeg_quality: u8,
    colour_mode: ScanColourMode,
    auto_orient: bool,
    #[serde(default)]
    auto_crop: bool,
    #[serde(default)]
    correct_perspective: bool,
    #[serde(default)]
    remove_shadows: bool,
    recognise_text: bool,
    straighten: bool,
    ocr_language: String,
    #[serde(default)]
    ocr_user_words: Vec<String>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
}

impl CreateScanPdfRequest {
    pub(crate) fn requires_desktop_services(&self) -> bool {
        self.recognise_text || self.output_protection.is_some()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScanPdfResult {
    output_path: String,
    bytes_written: u64,
    page_count: usize,
    ocr_applied: bool,
    used_image_magick: bool,
    pages_cropped: usize,
    pages_perspective_corrected: usize,
    pages_shadow_cleaned: usize,
    searchable_text_pages: usize,
    pages_without_searchable_text: Vec<u32>,
    ocr_hints_applied: usize,
    encryption: &'static str,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreviewScanImageRequest {
    pub(crate) input_path: String,
    pub(crate) colour_mode: ScanColourMode,
    pub(crate) auto_orient: bool,
    pub(crate) auto_crop: bool,
    pub(crate) correct_perspective: bool,
    pub(crate) remove_shadows: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewScanImageResult {
    bytes: Vec<u8>,
    mime_type: &'static str,
    width: u32,
    height: u32,
    page_boundary_detected: bool,
    cropped: bool,
    perspective_corrected: bool,
    shadow_removed: bool,
    used_image_magick: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviewScanOcrRequest {
    pub(crate) input_path: String,
    pub(crate) colour_mode: ScanColourMode,
    pub(crate) auto_orient: bool,
    pub(crate) auto_crop: bool,
    pub(crate) correct_perspective: bool,
    pub(crate) remove_shadows: bool,
    pub(crate) language: String,
}

#[derive(Default)]
struct ScanCleanupSummary {
    page_boundaries_detected: usize,
    pages_cropped: usize,
    pages_perspective_corrected: usize,
    pages_shadow_cleaned: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ImageSourceFingerprint {
    path: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
}

impl ScanCleanupSummary {
    fn record(&mut self, report: ScanCleanupReport) {
        self.page_boundaries_detected += usize::from(report.page_boundary_detected);
        self.pages_cropped += usize::from(report.cropped);
        self.pages_perspective_corrected += usize::from(report.perspective_corrected);
        self.pages_shadow_cleaned += usize::from(report.shadow_removed);
    }
}

#[derive(Clone)]
pub(crate) struct ScanExecutionControl {
    cancelled: Arc<AtomicBool>,
    progress: Arc<dyn Fn(u8, String) + Send + Sync>,
}

impl ScanExecutionControl {
    #[cfg(test)]
    pub(crate) fn direct() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_, _| {}),
        }
    }

    pub(crate) fn new(
        cancelled: Arc<AtomicBool>,
        progress: Arc<dyn Fn(u8, String) + Send + Sync>,
    ) -> Self {
        Self {
            cancelled,
            progress,
        }
    }

    fn checkpoint(&self, progress: u8, stage: impl Into<String>) -> Result<(), String> {
        self.ensure_not_cancelled()?;
        self.report(progress, stage);
        self.ensure_not_cancelled()
    }

    fn report(&self, progress: u8, stage: impl Into<String>) {
        (self.progress)(progress.min(100), stage.into());
    }

    fn ensure_not_cancelled(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(SCAN_JOB_CANCELLED_ERROR.to_string())
        } else {
            Ok(())
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn pdf_subrange(
        &self,
        start: u8,
        end: u8,
        prefix: impl Into<String>,
    ) -> PdfJobExecutionControl {
        PdfJobExecutionControl::new(Arc::clone(&self.cancelled), Arc::clone(&self.progress))
            .subrange(start, end, prefix.into())
    }
}

trait CancellableExecutionControl {
    fn cancellation_error(&self) -> &'static str;
    fn ensure_not_cancelled(&self) -> Result<(), String>;
    fn is_cancelled(&self) -> bool;
}

impl CancellableExecutionControl for ScanExecutionControl {
    fn cancellation_error(&self) -> &'static str {
        SCAN_JOB_CANCELLED_ERROR
    }

    fn ensure_not_cancelled(&self) -> Result<(), String> {
        ScanExecutionControl::ensure_not_cancelled(self)
    }

    fn is_cancelled(&self) -> bool {
        ScanExecutionControl::is_cancelled(self)
    }
}

impl CancellableExecutionControl for PdfJobExecutionControl {
    fn cancellation_error(&self) -> &'static str {
        PDF_JOB_CANCELLED_ERROR
    }

    fn ensure_not_cancelled(&self) -> Result<(), String> {
        PdfJobExecutionControl::ensure_not_cancelled(self)
    }

    fn is_cancelled(&self) -> bool {
        PdfJobExecutionControl::is_cancelled(self)
    }
}

#[cfg(test)]
pub fn create_scan_pdf(request: CreateScanPdfRequest) -> Result<CreateScanPdfResult, String> {
    create_scan_pdf_with_control(request, &ScanExecutionControl::direct())
}

#[cfg(test)]
fn preview_scan_image_blocking(
    request: PreviewScanImageRequest,
    workspace: &Path,
) -> Result<PreviewScanImageResult, String> {
    preview_scan_image_with_control(request, workspace, &ScanExecutionControl::direct())
}

pub(crate) fn validate_preview_scan_image_request(
    request: &PreviewScanImageRequest,
) -> Result<(), String> {
    validate_image_input(&request.input_path)?;
    Ok(())
}

pub(crate) fn run_scan_preview_job_with_control(
    request: PreviewScanImageRequest,
    control: &ScanExecutionControl,
) -> Result<PreviewScanImageResult, String> {
    preview_scan_image_with_control(request, &std::env::temp_dir(), control).map_err(|error| {
        if error == SCAN_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_scan_preview_job_error(&error)
        }
    })
}

fn preview_scan_image_with_control(
    request: PreviewScanImageRequest,
    workspace: &Path,
    control: &ScanExecutionControl,
) -> Result<PreviewScanImageResult, String> {
    control.checkpoint(2, "Checking scan clean-up preview settings")?;
    validate_preview_scan_image_request(&request)?;
    let input = validate_image_input(&request.input_path)?;
    let source_fingerprint = image_source_fingerprint(&input)?;
    control.checkpoint(10, "Decoding the selected scan page")?;
    let preview_anchor = workspace.join("preview.pdf");
    let (image, used_image_magick) =
        decode_scan_image(&input, request.auto_orient, &preview_anchor, control)
            .map_err(|error| format!("The scan preview could not decode the image: {error}"))?;
    control.checkpoint(35, "Applying scan clean-up")?;
    let (prepared, report) = prepare_image(
        image,
        request.colour_mode,
        SCAN_PREVIEW_MAX_DIMENSION * 72.0 / 150.0,
        SCAN_PREVIEW_MAX_DIMENSION * 72.0 / 150.0,
        150,
        ScanCleanupOptions {
            auto_crop: request.auto_crop,
            correct_perspective: request.correct_perspective,
            remove_shadows: request.remove_shadows,
        },
        control,
    )?;
    let (width, height) = prepared.dimensions();
    control.checkpoint(75, "Encoding the private preview image")?;
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 86)
        .encode_image(&prepared)
        .map_err(|error| format!("The scan preview could not be encoded: {error}"))?;
    control.checkpoint(96, "Checking the source image")?;
    verify_image_source_fingerprints(&[source_fingerprint])?;
    control.checkpoint(99, "Finalising the scan clean-up preview")?;
    Ok(PreviewScanImageResult {
        bytes,
        mime_type: "image/jpeg",
        width,
        height,
        page_boundary_detected: report.page_boundary_detected,
        cropped: report.cropped,
        perspective_corrected: report.perspective_corrected,
        shadow_removed: report.shadow_removed,
        used_image_magick,
    })
}

fn safe_scan_preview_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source image changed during scan clean-up preview. Create the preview again."
            .to_string();
    }
    if normalised.contains("imagemagick") {
        return "Scan clean-up preview could not decode this image locally. Install ImageMagick for HEIC, AVIF or other unsupported formats, then try again."
            .to_string();
    }
    if normalised.contains("512 mb") || normalised.contains("safety limit") {
        return "The source image exceeds a scan clean-up preview safety limit.".to_string();
    }
    "Scan clean-up preview could not complete bounded local image preparation. Review the image and try again."
        .to_string()
}

pub(crate) fn validate_review_scan_ocr_request(
    request: &ReviewScanOcrRequest,
) -> Result<(), String> {
    validate_ocr_language(&request.language)?;
    validate_image_input(&request.input_path)?;
    Ok(())
}

pub(crate) fn run_scan_ocr_review_job_with_control(
    request: ReviewScanOcrRequest,
    control: &ScanExecutionControl,
) -> Result<OcrConfidenceResult, String> {
    review_scan_ocr_with_control(request, &std::env::temp_dir(), control).map_err(|error| {
        if error == SCAN_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_ocr_review_job_error(&error)
        }
    })
}

fn review_scan_ocr_with_control(
    request: ReviewScanOcrRequest,
    workspace: &Path,
    control: &ScanExecutionControl,
) -> Result<OcrConfidenceResult, String> {
    review_scan_ocr_with_control_and_analyser(
        request,
        workspace,
        control,
        |raster, language, width, height, dpi, execution_control| {
            analyse_raster_with_tesseract_with_cancellation(
                raster,
                language,
                width,
                height,
                dpi,
                &|| execution_control.is_cancelled(),
            )
            .map_err(|error| {
                if error == OCR_REVIEW_CANCELLED_ERROR {
                    SCAN_JOB_CANCELLED_ERROR.to_string()
                } else {
                    error
                }
            })
        },
    )
}

fn review_scan_ocr_with_control_and_analyser<F>(
    request: ReviewScanOcrRequest,
    workspace: &Path,
    control: &ScanExecutionControl,
    analyser: F,
) -> Result<OcrConfidenceResult, String>
where
    F: FnOnce(
        &Path,
        &str,
        u32,
        u32,
        u32,
        &ScanExecutionControl,
    ) -> Result<OcrConfidenceResult, String>,
{
    control.checkpoint(2, "Checking OCR confidence-review settings")?;
    validate_review_scan_ocr_request(&request)?;
    let input = validate_image_input(&request.input_path)?;
    let source_fingerprint = image_source_fingerprint(&input)?;
    control.checkpoint(10, "Decoding the selected scan page")?;
    let review_anchor = workspace.join("ocr-review.pdf");
    let (image, _) = decode_scan_image(&input, request.auto_orient, &review_anchor, control)
        .map_err(|error| format!("The OCR review could not decode the image: {error}"))?;
    control.checkpoint(28, "Applying reviewed scan clean-up")?;
    let (prepared, _) = prepare_image(
        image,
        request.colour_mode,
        SCAN_PREVIEW_MAX_DIMENSION * 72.0 / 150.0,
        SCAN_PREVIEW_MAX_DIMENSION * 72.0 / 150.0,
        150,
        ScanCleanupOptions {
            auto_crop: request.auto_crop,
            correct_perspective: request.correct_perspective,
            remove_shadows: request.remove_shadows,
        },
        control,
    )?;
    let (width, height) = prepared.dimensions();
    control.checkpoint(52, "Preparing the private OCR review raster")?;
    let raster = TemporaryRaster::new(&review_anchor)?;
    prepared
        .save(raster.path())
        .map_err(|error| format!("The OCR review raster could not be prepared: {error}"))?;
    control.checkpoint(60, "Running local Tesseract confidence review")?;
    let result = analyser(
        raster.path(),
        &request.language,
        width,
        height,
        150,
        control,
    )?;
    control.checkpoint(94, "Checking the OCR confidence report")?;
    verify_image_source_fingerprints(&[source_fingerprint])?;
    control.checkpoint(99, "Finalising the OCR confidence report")?;
    Ok(result)
}

fn safe_ocr_review_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source image changed during OCR confidence review. Review it again."
            .to_string();
    }
    if normalised.contains("tesseract")
        || normalised.contains("language pack")
        || normalised.contains("ocr language")
    {
        return "Local OCR confidence review is unavailable. Check Tesseract and the selected language pack, then try again."
            .to_string();
    }
    if normalised.contains("512 mb") || normalised.contains("safety limit") {
        return "The source image exceeds an OCR confidence-review safety limit.".to_string();
    }
    "OCR confidence review could not complete bounded local image preparation and recognition. Review the image and try again."
        .to_string()
}

pub(crate) fn create_scan_pdf_with_control(
    request: CreateScanPdfRequest,
    control: &ScanExecutionControl,
) -> Result<CreateScanPdfResult, String> {
    control.checkpoint(1, "Validating scan settings")?;
    validate_request(&request)?;
    if request.recognise_text {
        control.checkpoint(2, "Checking OCR engines and language packs")?;
        ensure_ocr_ready(&request.ocr_language)?;
    }
    let output = validated_new_pdf_output(&request.output_path)?;
    let mut inputs = Vec::with_capacity(request.input_paths.len());
    let mut source_fingerprints = Vec::with_capacity(request.input_paths.len());
    for (index, path) in request.input_paths.iter().enumerate() {
        control.checkpoint(
            stage_progress(2, 8, index, request.input_paths.len()),
            format!(
                "Checking image {} of {}",
                index + 1,
                request.input_paths.len()
            ),
        )?;
        let input = validate_image_input(path)?;
        source_fingerprints.push(image_source_fingerprint(&input)?);
        inputs.push(input);
    }
    control.checkpoint(9, "Preparing scan pages")?;
    let base_output = TemporaryOutput::new(&output)?;
    let final_output = if request.recognise_text {
        Some(TemporaryOutput::new(&output)?)
    } else {
        None
    };

    let (mut document, used_image_magick, cleanup_summary) =
        build_scan_document(&inputs, &request, &output, control)?;
    control.checkpoint(66, "Writing the image PDF")?;
    document.change_producer("Tüfekci Paperworks");
    let base_file = document
        .save(base_output.path())
        .map_err(|error| format!("The scan PDF could not be written: {error}"))?;
    base_file
        .sync_all()
        .map_err(|error| format!("The scan PDF could not be flushed to storage: {error}"))?;
    control.checkpoint(72, "Verifying the image PDF")?;
    verify_scan_pdf(base_output.path(), inputs.len())?;

    let mut searchable_text_pages = 0_usize;
    let mut pages_without_searchable_text = Vec::new();
    let prepared_output = if let Some(final_output) = final_output.as_ref() {
        control.checkpoint(76, "Starting local OCR")?;
        let ocr_control = control.pdf_subrange(76, 90, "");
        run_ocrmypdf(
            base_output.path(),
            final_output.path(),
            &request.ocr_language,
            request.straighten,
            &request.ocr_user_words,
            &ocr_control,
        )?;
        control.checkpoint(91, "Verifying searchable text layers")?;
        verify_scan_pdf(final_output.path(), inputs.len())?;
        pages_without_searchable_text = inspect_searchable_text_pages(final_output.path())?;
        searchable_text_pages = inputs
            .len()
            .saturating_sub(pages_without_searchable_text.len());
        final_output
    } else {
        &base_output
    };

    let mut warnings = Vec::new();
    if used_image_magick {
        warnings.push(
            "ImageMagick normalised one or more formats that the embedded codecs could not read."
                .to_string(),
        );
    }
    if request.recognise_text && !pages_without_searchable_text.is_empty() {
        let page_summary = summarise_page_numbers(&pages_without_searchable_text);
        warnings.push(format!(
            "Searchable text could not be verified on {page_summary}. Blank or image-only pages may need manual review."
        ));
    }
    if (request.auto_crop || request.correct_perspective)
        && cleanup_summary.page_boundaries_detected < inputs.len()
    {
        let missed = inputs.len() - cleanup_summary.page_boundaries_detected;
        warnings.push(format!(
            "Automatic page boundaries were not confidently detected on {missed} of {} pages; those pages were fitted without geometric correction.",
            inputs.len()
        ));
    }

    let protected_output = if let Some(protection) = request.output_protection.as_ref() {
        let protected_output = TemporaryOutput::new(&output)?;
        let protection_control = control.pdf_subrange(92, 95, "Applying AES-256 output protection");
        lock_pdf_changes_with_control(
            prepared_output.path(),
            protected_output.path(),
            &protection.open_password,
            &protection.owner_password,
            &protection_control,
        )
        .map_err(map_pdf_job_cancellation)?;
        control.checkpoint(96, "Opening protected scan PDF for verification")?;
        let mut protected_document = load_scan_pdf(
            protected_output.path(),
            Some(&protection.open_password),
            true,
        )?;
        verify_scan_document(&protected_document, inputs.len())?;
        if request.recognise_text {
            let protected_pages_without_text =
                inspect_searchable_text_pages_in_document(&mut protected_document)?;
            if protected_pages_without_text != pages_without_searchable_text {
                return Err(
                    "The protected OCR PDF changed its verified searchable-text coverage."
                        .to_string(),
                );
            }
        }
        warnings.push(
            "The scan copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
        Some(protected_output)
    } else {
        None
    };
    let published_output = protected_output.as_ref().unwrap_or(prepared_output);

    control.checkpoint(98, "Rechecking source images before publication")?;
    verify_image_source_fingerprints(&source_fingerprints)?;
    control.checkpoint(99, "Publishing the verified PDF")?;
    let bytes_written = published_output.persist(&output)?;
    let result = CreateScanPdfResult {
        output_path: output.to_string_lossy().into_owned(),
        bytes_written,
        page_count: inputs.len(),
        ocr_applied: request.recognise_text,
        used_image_magick,
        pages_cropped: cleanup_summary.pages_cropped,
        pages_perspective_corrected: cleanup_summary.pages_perspective_corrected,
        pages_shadow_cleaned: cleanup_summary.pages_shadow_cleaned,
        searchable_text_pages,
        pages_without_searchable_text,
        ocr_hints_applied: unique_ocr_user_word_count(&request.ocr_user_words),
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    };
    control.report(100, "Scan PDF completed");
    Ok(result)
}

pub(crate) fn run_scan_pdf_job_with_control(
    request: CreateScanPdfRequest,
    control: &ScanExecutionControl,
) -> Result<CreateScanPdfResult, String> {
    create_scan_pdf_with_control(request, control).map_err(|error| {
        if error == SCAN_JOB_CANCELLED_ERROR || error == PDF_JOB_CANCELLED_ERROR {
            SCAN_JOB_CANCELLED_ERROR.to_string()
        } else {
            safe_scan_job_error(&error)
        }
    })
}

pub(crate) fn validate_scan_pdf_request(request: &CreateScanPdfRequest) -> Result<(), String> {
    validate_request(request)
}

fn validate_request(request: &CreateScanPdfRequest) -> Result<(), String> {
    if request.input_paths.is_empty() {
        return Err("Choose at least one image for the scan PDF.".to_string());
    }
    if request.input_paths.len() > MAX_SCAN_PAGES {
        return Err(format!(
            "A scan job may contain no more than {MAX_SCAN_PAGES} images."
        ));
    }
    for (label, value) in [
        ("Paper width", request.paper_width_pt),
        ("Paper height", request.paper_height_pt),
    ] {
        if !value.is_finite() || !(18.0..=14_400.0).contains(&value) {
            return Err(format!("{label} is outside the supported PDF page range."));
        }
    }
    if !request.margin_pt.is_finite()
        || request.margin_pt < 0.0
        || request.margin_pt * 2.0 >= request.paper_width_pt
        || request.margin_pt * 2.0 >= request.paper_height_pt
    {
        return Err("The page margin leaves no usable image area.".to_string());
    }
    if !matches!(request.dpi, 150 | 300 | 600) {
        return Err("Scan resolution must be 150, 300 or 600 DPI.".to_string());
    }
    if !(40..=100).contains(&request.jpeg_quality) {
        return Err("JPEG quality must be between 40 and 100.".to_string());
    }
    if request.recognise_text {
        validate_ocr_language(&request.ocr_language)?;
    }
    validate_ocr_user_words(&request.ocr_user_words)?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    Ok(())
}

fn validate_ocr_user_words(words: &[String]) -> Result<(), String> {
    if words.len() > MAX_OCR_USER_WORDS {
        return Err(format!(
            "OCR review may supply no more than {MAX_OCR_USER_WORDS} recognition hints."
        ));
    }
    let mut total_bytes = 0_usize;
    for word in words {
        reject_control_characters("OCR recognition hint", word)?;
        let trimmed = word.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_OCR_USER_WORD_BYTES {
            return Err(format!(
                "Each OCR recognition hint must contain 1 to {MAX_OCR_USER_WORD_BYTES} UTF-8 bytes."
            ));
        }
        total_bytes = total_bytes.saturating_add(trimmed.len() + 1);
        if total_bytes > MAX_OCR_USER_WORD_TOTAL_BYTES {
            return Err("OCR recognition hints exceed the 16 KB safety limit.".to_string());
        }
    }
    Ok(())
}

fn unique_ocr_user_word_count(words: &[String]) -> usize {
    words
        .iter()
        .map(|word| word.trim())
        .filter(|word| !word.is_empty())
        .collect::<BTreeSet<_>>()
        .len()
}

fn validate_image_input(path: &str) -> Result<PathBuf, String> {
    reject_control_characters("Image path", path)?;
    let canonical =
        fs::canonicalize(path).map_err(|error| format!("An image could not be opened: {error}"))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("An image could not be inspected: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose existing image files for the scan PDF.".to_string());
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "{} is larger than the 512 MB per-image safety limit.",
            display_name(&canonical)
        ));
    }
    Ok(canonical)
}

fn image_source_fingerprint(path: &Path) -> Result<ImageSourceFingerprint, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("An image could not be inspected: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose existing image files for the scan PDF.".to_string());
    }
    Ok(ImageSourceFingerprint {
        path: path.to_path_buf(),
        bytes: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn verify_image_source_fingerprints(fingerprints: &[ImageSourceFingerprint]) -> Result<(), String> {
    for expected in fingerprints {
        if image_source_fingerprint(&expected.path)? != *expected {
            return Err(
                "A source image changed on disk during scan processing. Choose the images again."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn map_pdf_job_cancellation(error: String) -> String {
    if error == PDF_JOB_CANCELLED_ERROR {
        SCAN_JOB_CANCELLED_ERROR.to_string()
    } else {
        error
    }
}

fn safe_scan_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "A source image changed during scan processing. Choose the images again."
            .to_string();
    }
    if normalised.contains("qpdf") || normalised.contains("aes-256") {
        return "AES-256 scan protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The protected scan PDF could not be opened with the supplied passwords."
            .to_string();
    }
    if normalised.contains("ocrmypdf")
        || normalised.contains("tesseract")
        || normalised.contains("ocr ")
        || normalised.contains("language pack")
    {
        return "Searchable OCR could not complete with the selected local engine and language pack."
            .to_string();
    }
    "Scan PDF creation failed a local image or PDF safety check. Review the scan settings and try again."
        .to_string()
}

fn build_scan_document(
    inputs: &[PathBuf],
    request: &CreateScanPdfRequest,
    destination: &Path,
    control: &ScanExecutionControl,
) -> Result<(Document, bool, ScanCleanupSummary), String> {
    let mut document = Document::with_version("1.4");
    let pages_id = document.new_object_id();
    let mut kids = Vec::with_capacity(inputs.len());
    let mut used_image_magick = false;
    let mut cleanup_summary = ScanCleanupSummary::default();

    for (index, path) in inputs.iter().enumerate() {
        control.checkpoint(
            stage_progress(10, 62, index, inputs.len()),
            format!("Preparing image {} of {}", index + 1, inputs.len()),
        )?;
        let (image, used_fallback) =
            decode_scan_image(path, request.auto_orient, destination, control).map_err(
                |error| {
                    if error == SCAN_JOB_CANCELLED_ERROR {
                        error
                    } else {
                        format!(
                            "Image {} ({}) could not be prepared: {error}",
                            index + 1,
                            display_name(path)
                        )
                    }
                },
            )?;
        used_image_magick |= used_fallback;
        let (prepared, cleanup_report) = prepare_image(
            image,
            request.colour_mode,
            request.paper_width_pt - request.margin_pt * 2.0,
            request.paper_height_pt - request.margin_pt * 2.0,
            request.dpi,
            ScanCleanupOptions {
                auto_crop: request.auto_crop,
                correct_perspective: request.correct_perspective,
                remove_shadows: request.remove_shadows,
            },
            control,
        )?;
        cleanup_summary.record(cleanup_report);
        let (image_stream, image_width, image_height) =
            encode_pdf_image(&prepared, request.colour_mode, request.jpeg_quality)?;
        control.ensure_not_cancelled()?;
        let image_id = document.add_object(image_stream);

        let available_width = request.paper_width_pt - request.margin_pt * 2.0;
        let available_height = request.paper_height_pt - request.margin_pt * 2.0;
        let placement_scale = (available_width / f64::from(image_width))
            .min(available_height / f64::from(image_height));
        let placed_width = f64::from(image_width) * placement_scale;
        let placed_height = f64::from(image_height) * placement_scale;
        let x = (request.paper_width_pt - placed_width) / 2.0;
        let y = (request.paper_height_pt - placed_height) / 2.0;
        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        real(placed_width),
                        0.into(),
                        0.into(),
                        real(placed_height),
                        real(x),
                        real(y),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(b"ScanImage".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        }
        .encode()
        .map_err(|error| format!("A scan page could not be encoded: {error}"))?;
        let content_id = document.add_object(Stream::new(dictionary! {}, content));
        let mut xobjects = Dictionary::new();
        xobjects.set("ScanImage", image_id);
        let resources = dictionary! { "XObject" => xobjects };
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![
                Object::Integer(0),
                Object::Integer(0),
                real(request.paper_width_pt),
                real(request.paper_height_pt),
            ],
            "Resources" => resources,
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => inputs.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    Ok((document, used_image_magick, cleanup_summary))
}

fn decode_scan_image(
    path: &Path,
    auto_orient: bool,
    destination: &Path,
    control: &ScanExecutionControl,
) -> Result<(DynamicImage, bool), String> {
    match decode_embedded(path, auto_orient) {
        Ok(image) => Ok((image, false)),
        Err(embedded_error) => {
            let normalised = TemporaryRaster::new(destination)?;
            let mut command = Command::new("magick");
            command.arg(path);
            if auto_orient {
                command.arg("-auto-orient");
            }
            command.arg("-strip").arg(normalised.path());
            let output = run_cancellable_command(
                &mut command,
                control,
                |_| {},
                |_| {
                    format!(
                        "{embedded_error}. Install ImageMagick to read AVIF, HEIC, HEIF or other platform formats."
                    )
                },
            )?;
            if !output.status.success() {
                let detail = first_output_line(&output.stderr)
                    .or_else(|| first_output_line(&output.stdout))
                    .unwrap_or_else(|| "ImageMagick returned an unknown error.".to_string());
                return Err(format!("{embedded_error}. ImageMagick: {detail}"));
            }

            decode_embedded(normalised.path(), false).map(|image| (image, true))
        }
    }
}

fn decode_embedded(path: &Path, auto_orient: bool) -> Result<DynamicImage, String> {
    match catch_unwind(AssertUnwindSafe(|| -> Result<DynamicImage, String> {
        let mut reader = ImageReader::open(path)
            .map_err(|error| error.to_string())?
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_DECODE_ALLOCATION);
        reader.limits(limits);
        let mut decoder = reader.into_decoder().map_err(|error| error.to_string())?;
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let mut image = DynamicImage::from_decoder(decoder).map_err(|error| error.to_string())?;
        if auto_orient {
            image.apply_orientation(orientation);
        }
        if image.width() == 0 || image.height() == 0 {
            return Err("The decoded image has no pixels".to_string());
        }
        Ok(image)
    })) {
        Ok(result) => result,
        Err(_) => Err("The embedded image decoder rejected the file safely".to_string()),
    }
}

fn prepare_image(
    image: DynamicImage,
    colour_mode: ScanColourMode,
    available_width_pt: f64,
    available_height_pt: f64,
    dpi: u32,
    cleanup_options: ScanCleanupOptions,
    control: &ScanExecutionControl,
) -> Result<(DynamicImage, ScanCleanupReport), String> {
    let max_width = ((available_width_pt / 72.0) * f64::from(dpi))
        .round()
        .clamp(1.0, f64::from(u32::MAX)) as u32;
    let max_height = ((available_height_pt / 72.0) * f64::from(dpi))
        .round()
        .clamp(1.0, f64::from(u32::MAX)) as u32;
    let flattened = flatten_transparency(image);
    let (cleaned, cleanup_report) =
        clean_scan_image(flattened, max_width, max_height, cleanup_options, || {
            control.ensure_not_cancelled()
        })?;

    let prepared = match colour_mode {
        ScanColourMode::Colour => DynamicImage::ImageRgb8(cleaned),
        ScanColourMode::Greyscale => {
            DynamicImage::ImageLuma8(DynamicImage::ImageRgb8(cleaned).to_luma8())
        }
        ScanColourMode::Monochrome => {
            let mut greyscale = DynamicImage::ImageRgb8(cleaned).to_luma8();
            let threshold = otsu_threshold(&greyscale);
            for (index, pixel) in greyscale.pixels_mut().enumerate() {
                if index % 65_536 == 0 {
                    control.ensure_not_cancelled()?;
                }
                pixel.0[0] = if pixel.0[0] < threshold { 0 } else { 255 };
            }
            DynamicImage::ImageLuma8(greyscale)
        }
    };
    Ok((prepared, cleanup_report))
}

fn flatten_transparency(image: DynamicImage) -> RgbImage {
    if !image.has_alpha() {
        return image.to_rgb8();
    }

    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba.get_pixel(x, y).0;
        let alpha = u16::from(pixel[3]);
        Rgb([
            blend_white(pixel[0], alpha),
            blend_white(pixel[1], alpha),
            blend_white(pixel[2], alpha),
        ])
    })
}

fn blend_white(channel: u8, alpha: u16) -> u8 {
    (((u16::from(channel) * alpha) + (255 * (255 - alpha)) + 127) / 255) as u8
}

fn otsu_threshold(image: &image::GrayImage) -> u8 {
    let mut histogram = [0_u64; 256];
    for pixel in image.pixels() {
        histogram[usize::from(pixel.0[0])] += 1;
    }
    let total = u64::from(image.width()) * u64::from(image.height());
    let weighted_total = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as u64 * count)
        .sum::<u64>();
    let mut background_weight = 0_u64;
    let mut background_sum = 0_u64;
    let mut best_variance = -1.0_f64;
    let mut best_threshold = 127_u8;

    for (value, count) in histogram.iter().enumerate() {
        background_weight += count;
        if background_weight == 0 {
            continue;
        }
        let foreground_weight = total.saturating_sub(background_weight);
        if foreground_weight == 0 {
            break;
        }
        background_sum += value as u64 * count;
        let background_mean = background_sum as f64 / background_weight as f64;
        let foreground_mean = (weighted_total - background_sum) as f64 / foreground_weight as f64;
        let difference = background_mean - foreground_mean;
        let variance =
            background_weight as f64 * foreground_weight as f64 * difference * difference;
        if variance > best_variance {
            best_variance = variance;
            best_threshold = value as u8;
        }
    }
    best_threshold
}

fn encode_pdf_image(
    image: &DynamicImage,
    colour_mode: ScanColourMode,
    quality: u8,
) -> Result<(Stream, u32, u32), String> {
    let (width, height) = image.dimensions();
    let colour_space = match colour_mode {
        ScanColourMode::Colour => "DeviceRGB",
        ScanColourMode::Greyscale | ScanColourMode::Monochrome => "DeviceGray",
    };

    if matches!(colour_mode, ScanColourMode::Monochrome) {
        let bytes = image.to_luma8().into_raw();
        let mut stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(width),
                "Height" => i64::from(height),
                "ColorSpace" => colour_space,
                "BitsPerComponent" => 8,
            },
            bytes,
        );
        stream
            .compress()
            .map_err(|error| format!("The monochrome scan could not be compressed: {error}"))?;
        return Ok((stream, width, height));
    }

    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, quality)
        .encode_image(image)
        .map_err(|error| format!("The scan image could not be compressed: {error}"))?;
    Ok((
        Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(width),
                "Height" => i64::from(height),
                "ColorSpace" => colour_space,
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg,
        ),
        width,
        height,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OcrPdfOutputKind {
    Pdf,
    PdfA1,
    PdfA2,
    PdfA3,
}

impl OcrPdfOutputKind {
    fn argument(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::PdfA1 => "pdfa-1",
            Self::PdfA2 => "pdfa-2",
            Self::PdfA3 => "pdfa-3",
        }
    }

    fn is_pdfa(self) -> bool {
        self != Self::Pdf
    }
}

pub(crate) fn run_ocrmypdf(
    input: &Path,
    output: &Path,
    language: &str,
    straighten: bool,
    user_words: &[String],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    run_ocrmypdf_configured(
        input,
        output,
        OcrPdfOutputKind::Pdf,
        Some(language),
        straighten,
        user_words,
        control,
    )
}

pub(crate) fn run_ocrmypdf_pdfa(
    input: &Path,
    output: &Path,
    output_kind: OcrPdfOutputKind,
    language: Option<&str>,
    straighten: bool,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if !output_kind.is_pdfa() {
        return Err("Choose a PDF/A output profile for archival conversion.".to_string());
    }
    run_ocrmypdf_configured(
        input,
        output,
        output_kind,
        language,
        straighten,
        &[],
        control,
    )
}

fn run_ocrmypdf_configured(
    input: &Path,
    output: &Path,
    output_kind: OcrPdfOutputKind,
    language: Option<&str>,
    straighten: bool,
    user_words: &[String],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let user_words_file = if language.is_some() {
        TemporaryUserWords::new(output, user_words)?
    } else {
        None
    };
    let progress_plugin = TemporaryOcrProgressPlugin::new(output)?;
    let mut command = Command::new("ocrmypdf");
    command
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg("--plugin")
        .arg(progress_plugin.path())
        .arg(format!("--output-type={}", output_kind.argument()))
        .arg(if output_kind.is_pdfa() {
            "--optimize=0"
        } else {
            "--optimize=1"
        });
    if let Some(language) = language {
        command
            .arg("--rotate-pages")
            .arg("--skip-text")
            .arg(format!("--language={language}"));
        if straighten {
            command.arg("--deskew");
        }
        if let Some(user_words_file) = user_words_file.as_ref() {
            command.arg("--user-words").arg(user_words_file.path());
        }
    } else {
        command.arg("--ocr-engine=none").arg("--skip-text");
    }
    command.arg(input).arg(output);
    let progress_control = control.clone();
    let pdfa_progress = output_kind.is_pdfa();
    let mut progress_parser = OcrProgressParser::default();
    let result = run_cancellable_command(
        &mut command,
        control,
        move |chunk| {
            progress_parser.push(chunk, |update| {
                let stage = if pdfa_progress {
                    format!("Local PDF/A conversion: {}%", update.percent)
                } else {
                    stage_for_ocr(update)
                };
                let _ = progress_control.checkpoint(update.percent, stage);
            });
        },
        |error| {
            format!(
                "OCRmyPDF could not be started. Install OCRmyPDF and the selected Tesseract language pack: {error}"
            )
        },
    )?;
    if result.status.success() {
        return Ok(());
    }

    let detail = first_ocr_diagnostic(&result.stderr)
        .or_else(|| first_output_line(&result.stdout))
        .unwrap_or_else(|| "OCRmyPDF returned an unknown error.".to_string());
    Err(format!(
        "{} did not complete: {detail}",
        if output_kind.is_pdfa() {
            "PDF/A conversion"
        } else {
            "OCR"
        }
    ))
}

fn run_cancellable_command<C, F, P>(
    command: &mut Command,
    control: &C,
    stderr_observer: P,
    start_error: F,
) -> Result<Output, String>
where
    C: CancellableExecutionControl,
    F: FnOnce(std::io::Error) -> String,
    P: FnMut(&[u8]) + Send + 'static,
{
    control.ensure_not_cancelled()?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(command).map_err(start_error)?;
    let stdout_reader = child
        .take_stdout()
        .map(|pipe| read_child_output(pipe, |_| {}));
    let stderr_reader = child
        .take_stderr()
        .map(|pipe| read_child_output(pipe, stderr_observer));

    loop {
        if control.is_cancelled() {
            let _ = child.terminate_tree();
            let _ = child.wait();
            drop(stdout_reader);
            drop(stderr_reader);
            return Err(control.cancellation_error().to_string());
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Output {
                    status,
                    stdout: finish_child_output(stdout_reader),
                    stderr: finish_child_output(stderr_reader),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(75)),
            Err(error) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                drop(stdout_reader);
                drop(stderr_reader);
                return Err(format!(
                    "The external document tool could not be monitored safely: {error}"
                ));
            }
        }
    }
}

fn read_child_output<R, F>(mut pipe: R, mut observer: F) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
    F: FnMut(&[u8]) + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 8 * 1024];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            observer(&chunk[..read]);
            let remaining = MAX_TOOL_OUTPUT_BYTES.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..read.min(remaining)]);
        }
        bytes
    })
}

fn finish_child_output(reader: Option<thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

fn stage_progress(start: u8, end: u8, index: usize, total: usize) -> u8 {
    if total == 0 || start >= end {
        return start;
    }
    start.saturating_add((((end - start) as usize * index) / total) as u8)
}

fn verify_scan_pdf(path: &Path, expected_pages: usize) -> Result<(), String> {
    let document = load_scan_pdf(path, None, false)?;
    verify_scan_document(&document, expected_pages)
}

fn load_scan_pdf(
    path: &Path,
    opening_password: Option<&str>,
    require_encryption: bool,
) -> Result<Document, String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The scan PDF failed verification: {error}"))?;
    if require_encryption && !document.is_encrypted() {
        return Err("The protected scan PDF did not contain AES-256 encryption.".to_string());
    }
    if !require_encryption && document.is_encrypted() {
        return Err("The prepared scan PDF unexpectedly remained encrypted.".to_string());
    }
    if document.is_encrypted() {
        document
            .decrypt(opening_password.unwrap_or_default())
            .map_err(|_| {
                "The protected scan PDF could not be decrypted for verification.".to_string()
            })?;
    }
    Ok(document)
}

fn verify_scan_document(document: &Document, expected_pages: usize) -> Result<(), String> {
    let pages = document.get_pages();
    if pages.len() != expected_pages {
        return Err(format!(
            "The scan PDF failed verification: expected {expected_pages} pages but found {}.",
            pages.len()
        ));
    }
    for (page_number, page_id) in pages {
        let images = document.get_page_images(page_id).map_err(|error| {
            format!("The scan PDF page {page_number} could not be inspected: {error}")
        })?;
        if images.is_empty() {
            return Err(format!(
                "The scan PDF page {page_number} does not contain its source image."
            ));
        }
    }
    Ok(())
}

pub(crate) fn inspect_searchable_text_pages(path: &Path) -> Result<Vec<u32>, String> {
    let mut document = load_scan_pdf(path, None, false)
        .map_err(|error| format!("The OCR text layer could not be reopened: {error}"))?;
    inspect_searchable_text_pages_in_document(&mut document)
}

pub(crate) fn inspect_searchable_text_pages_in_document(
    document: &mut Document,
) -> Result<Vec<u32>, String> {
    let pages = document.get_pages();
    let mut pages_without_text = Vec::new();
    for (page_number, page_id) in pages {
        let text = extract_searchable_text_from_page(document, page_number, page_id)?;
        if text.trim().is_empty() {
            pages_without_text.push(page_number);
        }
    }
    Ok(pages_without_text)
}

fn extract_searchable_text_from_page(
    document: &mut Document,
    page_number: u32,
    page_id: ObjectId,
) -> Result<String, String> {
    let direct_text = document
        .extract_text_with_limit(&[page_number], MAX_OCR_TEXT_LAYER_CONTENT_BYTES)
        .map_err(|error| {
            format!("The OCR text layer on page {page_number} could not be decoded safely: {error}")
        })?;
    if !direct_text.trim().is_empty() {
        return Ok(direct_text);
    }

    let page_content = document
        .get_page_content_with_limit(page_id, MAX_OCR_TEXT_LAYER_CONTENT_BYTES)
        .map_err(|error| {
            format!("The OCR page {page_number} content could not be decoded safely: {error}")
        })?;
    let page_resources = effective_page_resources(document, page_id)?;
    let mut state = OcrFormTextState::default();
    let mut text = String::new();
    extract_invoked_form_text(
        document,
        page_number,
        page_id,
        &page_content,
        &page_resources,
        0,
        &mut state,
        &mut text,
    )?;
    Ok(text)
}

#[derive(Default)]
struct OcrFormTextState {
    active: BTreeSet<ObjectId>,
    visited: BTreeSet<ObjectId>,
    visits: usize,
}

#[allow(clippy::too_many_arguments)]
fn extract_invoked_form_text(
    document: &mut Document,
    page_number: u32,
    page_id: ObjectId,
    content_bytes: &[u8],
    resources: &Object,
    depth: usize,
    state: &mut OcrFormTextState,
    output: &mut String,
) -> Result<(), String> {
    if depth > MAX_OCR_FORM_DEPTH {
        return Err(format!(
            "The OCR text layer on page {page_number} exceeds the Form nesting limit."
        ));
    }
    if content_bytes.len() > MAX_OCR_TEXT_LAYER_CONTENT_BYTES {
        return Err(format!(
            "The OCR text layer on page {page_number} exceeds the decoded-content limit."
        ));
    }
    let content = Content::decode_strict(content_bytes).map_err(|error| {
        format!("The OCR text layer on page {page_number} contains invalid PDF operations: {error}")
    })?;
    let resource_dictionary = resolved_dictionary_clone(document, resources)
        .map_err(|error| format!("The OCR resources on page {page_number} are invalid: {error}"))?;
    let xobjects = match resource_dictionary.get(b"XObject") {
        Ok(value) => resolved_dictionary_clone(document, value).map_err(|error| {
            format!("The OCR XObject resources on page {page_number} are invalid: {error}")
        })?,
        Err(_) => return Ok(()),
    };

    for operation in content.operations {
        if operation.operator != "Do" {
            continue;
        }
        let Some(name) = operation
            .operands
            .first()
            .and_then(|operand| operand.as_name().ok())
        else {
            continue;
        };
        let Ok(xobject) = xobjects.get(name).cloned() else {
            continue;
        };
        let stream = match resolved_stream_clone(document, &xobject) {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        if !stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|subtype| subtype == b"Form")
        {
            continue;
        }

        let identity = xobject.as_reference().ok();
        if identity.is_some_and(|id| state.active.contains(&id)) {
            return Err(format!(
                "The OCR text layer on page {page_number} contains a cyclic Form reference."
            ));
        }
        if identity.is_some_and(|id| state.visited.contains(&id)) {
            continue;
        }
        if state.visits >= MAX_OCR_FORM_VISITS {
            return Err(format!(
                "The OCR text layer on page {page_number} exceeds the Form visit limit."
            ));
        }
        state.visits += 1;
        if let Some(id) = identity {
            state.active.insert(id);
            state.visited.insert(id);
        }

        let form_resources = stream
            .dict
            .get(b"Resources")
            .cloned()
            .unwrap_or_else(|_| resources.clone());
        let form_text = extract_text_with_temporary_page(
            document,
            page_number,
            page_id,
            xobject,
            form_resources.clone(),
        )?;
        if !form_text.trim().is_empty() {
            if output.len().saturating_add(form_text.len()) > MAX_OCR_TEXT_LAYER_CONTENT_BYTES {
                return Err(format!(
                    "The OCR text layer on page {page_number} exceeds the extracted-text limit."
                ));
            }
            output.push_str(&form_text);
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }

        let form_content = stream
            .decompressed_content_with_limit(MAX_OCR_TEXT_LAYER_CONTENT_BYTES)
            .map_err(|error| {
                format!("The OCR Form on page {page_number} could not be decoded safely: {error}")
            })?;
        extract_invoked_form_text(
            document,
            page_number,
            page_id,
            &form_content,
            &form_resources,
            depth + 1,
            state,
            output,
        )?;
        if let Some(id) = identity {
            state.active.remove(&id);
        }
    }
    Ok(())
}

fn extract_text_with_temporary_page(
    document: &mut Document,
    page_number: u32,
    page_id: ObjectId,
    content: Object,
    resources: Object,
) -> Result<String, String> {
    let original = document
        .get_object(page_id)
        .map_err(|error| format!("The OCR page object is unavailable: {error}"))?
        .clone();
    let mut temporary_page = original
        .as_dict()
        .map_err(|_| "The OCR page object is not a dictionary.".to_string())?
        .clone();
    temporary_page.set("Contents", content);
    temporary_page.set("Resources", resources);
    *document
        .get_object_mut(page_id)
        .map_err(|error| format!("The OCR page object cannot be inspected: {error}"))? =
        Object::Dictionary(temporary_page);

    let extraction = document
        .extract_text_with_limit(&[page_number], MAX_OCR_TEXT_LAYER_CONTENT_BYTES)
        .map_err(|error| format!("The OCR Form text could not be decoded safely: {error}"));
    *document
        .get_object_mut(page_id)
        .map_err(|error| format!("The OCR page object could not be restored: {error}"))? = original;
    extraction
}

fn effective_page_resources(document: &Document, page_id: ObjectId) -> Result<Object, String> {
    let mut current = page_id;
    let mut visited = BTreeSet::new();
    for _ in 0..64 {
        if !visited.insert(current) {
            return Err("The OCR page resource tree contains a cycle.".to_string());
        }
        let dictionary = document
            .get_dictionary(current)
            .map_err(|error| format!("The OCR page resource tree is invalid: {error}"))?;
        if let Ok(resources) = dictionary.get(b"Resources") {
            return Ok(resources.clone());
        }
        let Ok(parent) = dictionary.get(b"Parent").and_then(Object::as_reference) else {
            return Ok(Object::Dictionary(Dictionary::new()));
        };
        current = parent;
    }
    Err("The OCR page resource tree exceeds its depth limit.".to_string())
}

fn resolved_dictionary_clone(document: &Document, object: &Object) -> Result<Dictionary, String> {
    match object {
        Object::Dictionary(dictionary) => Ok(dictionary.clone()),
        Object::Reference(id) => document
            .get_dictionary(*id)
            .cloned()
            .map_err(|error| format!("broken dictionary reference {} {} R: {error}", id.0, id.1)),
        _ => Err("the resource is not a dictionary".to_string()),
    }
}

fn resolved_stream_clone(document: &Document, object: &Object) -> Result<Stream, String> {
    match object {
        Object::Stream(stream) => Ok(stream.clone()),
        Object::Reference(id) => document
            .get_object(*id)
            .and_then(Object::as_stream)
            .cloned()
            .map_err(|error| format!("broken stream reference {} {} R: {error}", id.0, id.1)),
        _ => Err("the resource is not a stream".to_string()),
    }
}

fn summarise_page_numbers(page_numbers: &[u32]) -> String {
    if page_numbers.len() == 1 {
        return format!("page {}", page_numbers[0]);
    }
    if page_numbers.len() <= 8 {
        let mut labels = page_numbers
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let last = labels.pop().unwrap_or_default();
        return format!("pages {} and {last}", labels.join(", "));
    }
    format!(
        "{} pages (beginning with {}, {}, {} and {})",
        page_numbers.len(),
        page_numbers[0],
        page_numbers[1],
        page_numbers[2],
        page_numbers[3]
    )
}

fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image")
        .to_string()
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn first_ocr_diagnostic(bytes: &[u8]) -> Option<String> {
    let lines = String::from_utf8_lossy(bytes)
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_progress_line(line.as_bytes()))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    lines
        .iter()
        .rev()
        .find(|line| is_likely_ocr_error(line))
        .cloned()
        .or_else(|| lines.last().cloned())
}

fn is_likely_ocr_error(line: &str) -> bool {
    let normalised = line.to_ascii_lowercase();
    [
        "error",
        "exception",
        "failed",
        "failure",
        "cannot",
        "can't",
        "could not",
        "invalid",
        "missing",
        "not found",
        "unsupported",
    ]
    .iter()
    .any(|keyword| normalised.contains(keyword))
}

struct TemporaryOcrProgressPlugin {
    lease: TemporaryLease,
}

impl TemporaryOcrProgressPlugin {
    fn new(anchor: &Path) -> Result<Self, String> {
        let parent = anchor
            .parent()
            .ok_or_else(|| "The OCR progress plug-in folder is invalid.".to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("The system clock is invalid: {error}"))?
            .as_nanos();
        let path = parent.join(format!(".ocr-progress.{}.{nonce}.py", std::process::id()));
        let mut lease = register_temporary_path(&path, TemporaryKind::OcrProgressPlugin)?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(lease.path()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                lease.cancel_without_target_cleanup();
            }
            format!("The OCR progress plug-in could not be prepared: {error}")
        })?;
        file.write_all(OCR_PROGRESS_PLUGIN_SOURCE)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("The OCR progress plug-in could not be completed: {error}"))?;
        Ok(Self { lease })
    }

    fn path(&self) -> &Path {
        self.lease.path()
    }
}

struct TemporaryUserWords {
    lease: TemporaryLease,
}

impl TemporaryUserWords {
    fn new(anchor: &Path, words: &[String]) -> Result<Option<Self>, String> {
        let unique_words = words
            .iter()
            .map(|word| word.trim())
            .filter(|word| !word.is_empty())
            .collect::<BTreeSet<_>>();
        if unique_words.is_empty() {
            return Ok(None);
        }
        let parent = anchor
            .parent()
            .ok_or_else(|| "The OCR temporary folder is invalid.".to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("The system clock is invalid: {error}"))?
            .as_nanos();
        let path = parent.join(format!(
            ".ocr-user-words.{}.{nonce}.txt",
            std::process::id()
        ));
        let mut lease = register_temporary_path(&path, TemporaryKind::OcrUserWords)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(lease.path())
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    lease.cancel_without_target_cleanup();
                }
                format!("OCR recognition hints could not be prepared: {error}")
            })?;
        for word in unique_words {
            writeln!(file, "{word}")
                .map_err(|error| format!("OCR recognition hints could not be written: {error}"))?;
        }
        file.sync_all()
            .map_err(|error| format!("OCR recognition hints could not be flushed: {error}"))?;
        Ok(Some(Self { lease }))
    }

    fn path(&self) -> &Path {
        self.lease.path()
    }
}

struct TemporaryRaster {
    lease: TemporaryLease,
}

impl TemporaryRaster {
    fn new(destination: &Path) -> Result<Self, String> {
        let parent = destination
            .parent()
            .ok_or_else(|| "The destination folder is invalid.".to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("The system clock is invalid: {error}"))?
            .as_nanos();
        let path = parent.join(format!(
            ".scan-normalised.{}.{}.png",
            std::process::id(),
            nonce
        ));
        let lease = register_temporary_path(&path, TemporaryKind::ScanRaster)?;
        Ok(Self { lease })
    }

    fn path(&self) -> &Path {
        self.lease.path()
    }
}

#[cfg(test)]
pub(crate) fn test_scan_pdf_request(
    input_paths: Vec<String>,
    output_path: String,
) -> CreateScanPdfRequest {
    CreateScanPdfRequest {
        input_paths,
        output_path,
        paper_width_pt: 595.0,
        paper_height_pt: 842.0,
        margin_pt: 18.0,
        dpi: 150,
        jpeg_quality: 85,
        colour_mode: ScanColourMode::Colour,
        auto_orient: true,
        auto_crop: false,
        correct_perspective: false,
        remove_shadows: false,
        recognise_text: false,
        straighten: false,
        ocr_language: "eng".to_string(),
        ocr_user_words: Vec::new(),
        output_protection: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::sync::Mutex;
    use std::time::Instant;

    #[test]
    fn flattens_transparency_onto_white() {
        let source = ImageBuffer::from_pixel(1, 1, Rgba([20, 40, 60, 0]));
        let flattened = flatten_transparency(DynamicImage::ImageRgba8(source));
        assert_eq!(flattened.get_pixel(0, 0).0, [255, 255, 255]);
    }

    #[test]
    fn monochrome_processing_uses_only_black_and_white() {
        let source = RgbImage::from_fn(32, 8, |x, _| {
            if x < 16 {
                Rgb([25, 25, 25])
            } else {
                Rgb([230, 230, 230])
            }
        });
        let prepared = prepare_image(
            DynamicImage::ImageRgb8(source),
            ScanColourMode::Monochrome,
            595.0,
            842.0,
            150,
            ScanCleanupOptions::default(),
            &ScanExecutionControl::direct(),
        )
        .unwrap()
        .0;
        assert!(prepared
            .to_luma8()
            .pixels()
            .all(|pixel| matches!(pixel.0[0], 0 | 255)));
    }

    #[test]
    fn preview_uses_the_export_cleanup_pipeline() {
        let directory = TestDirectory::new();
        let input = directory.path.join("photographed-page.png");
        let mut source = RgbImage::from_pixel(420, 320, Rgb([28, 34, 40]));
        for y in 35..285 {
            for x in 55..365 {
                let shade = 180 + ((x - 55) * 60 / 310) as u8;
                source.put_pixel(x, y, Rgb([shade, shade, shade]));
            }
        }
        source.save(&input).unwrap();

        let result = preview_scan_image_blocking(
            PreviewScanImageRequest {
                input_path: input.to_string_lossy().into_owned(),
                colour_mode: ScanColourMode::Greyscale,
                auto_orient: true,
                auto_crop: true,
                correct_perspective: false,
                remove_shadows: true,
            },
            &directory.path,
        )
        .unwrap();

        assert!(result.bytes.starts_with(&[0xff, 0xd8]));
        assert!(result.page_boundary_detected);
        assert!(result.cropped);
        assert!(result.shadow_removed);
        assert!(!result.perspective_corrected);
        assert!(result.width < 420);
        assert!(result.height < 320);
    }

    #[test]
    fn controlled_scan_preview_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("controlled-preview.png");
        RgbImage::from_pixel(120, 80, Rgb([248, 248, 248]))
            .save(&input)
            .unwrap();
        let reports = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let captured_reports = Arc::clone(&reports);
        let control = ScanExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, stage| {
                captured_reports.lock().unwrap().push((progress, stage));
            }),
        );

        let result =
            preview_scan_image_with_control(preview_request(&input), &directory.path, &control)
                .unwrap();

        assert!(result.bytes.starts_with(&[0xff, 0xd8]));
        let reports = reports.lock().unwrap();
        assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(reports
            .iter()
            .any(|(_, stage)| stage == "Applying scan clean-up"));
        assert_eq!(reports.last().map(|entry| entry.0), Some(99));
        drop(reports);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let cancelling_control = ScanExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 35 {
                    cancellation_flag.store(true, Ordering::Release);
                }
            }),
        );
        let error = preview_scan_image_with_control(
            preview_request(&input),
            &directory.path,
            &cancelling_control,
        )
        .unwrap_err();
        assert_eq!(error, SCAN_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn scan_preview_rejects_a_source_changed_before_result_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("changed-preview-source.png");
        RgbImage::from_pixel(80, 120, Rgb([245, 245, 245]))
            .save(&input)
            .unwrap();
        let source_to_change = input.clone();
        let control = ScanExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress == 96 {
                    let mut source = OpenOptions::new()
                        .append(true)
                        .open(&source_to_change)
                        .unwrap();
                    source.write_all(b"changed after preview encoding").unwrap();
                    source.sync_all().unwrap();
                }
            }),
        );

        let error =
            preview_scan_image_with_control(preview_request(&input), &directory.path, &control)
                .unwrap_err();

        assert!(error.contains("changed on disk"));
    }

    #[test]
    fn scan_preview_job_errors_are_content_free() {
        let error = safe_scan_preview_job_error(
            "C:\\Private\\client-passport.heic made ImageMagick expose private pixels",
        );
        assert_eq!(
            error,
            "Scan clean-up preview could not decode this image locally. Install ImageMagick for HEIC, AVIF or other unsupported formats, then try again."
        );
        assert!(!error.contains("passport"));
        assert!(!error.contains("private pixels"));
    }

    #[test]
    fn controlled_ocr_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("ocr-review-source.png");
        RgbImage::from_pixel(120, 80, Rgb([248, 248, 248]))
            .save(&input)
            .unwrap();
        let reports = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let captured_reports = Arc::clone(&reports);
        let control = ScanExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, stage| {
                captured_reports.lock().unwrap().push((progress, stage));
            }),
        );

        let result = review_scan_ocr_with_control_and_analyser(
            ocr_review_request(&input),
            &directory.path,
            &control,
            |_, language, width, height, _, _| {
                Ok(OcrConfidenceResult {
                    language: language.to_string(),
                    image_width: width,
                    image_height: height,
                    word_count: 1,
                    average_confidence: Some(72.0),
                    minimum_confidence: Some(72.0),
                    low_confidence_threshold: 80.0,
                    low_confidence_count: 1,
                    low_confidence_words: Vec::new(),
                    malformed_rows: 0,
                    warnings: Vec::new(),
                })
            },
        )
        .unwrap();

        assert_eq!(result.language, "eng");
        assert_eq!(result.word_count, 1);
        let reports = reports.lock().unwrap();
        assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(reports
            .iter()
            .any(|(_, stage)| stage.contains("Tesseract confidence review")));
        assert_eq!(reports.last().map(|entry| entry.0), Some(99));
        assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 1);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let cancelling_control = ScanExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 28 {
                    cancellation_flag.store(true, Ordering::Release);
                }
            }),
        );
        let error = review_scan_ocr_with_control_and_analyser(
            ocr_review_request(&input),
            &directory.path,
            &cancelling_control,
            |_, _, _, _, _, _| unreachable!("cancelled before recognition"),
        )
        .unwrap_err();
        assert_eq!(error, SCAN_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn ocr_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("changed-ocr-review-source.png");
        RgbImage::from_pixel(80, 120, Rgb([245, 245, 245]))
            .save(&input)
            .unwrap();
        let source_to_change = input.clone();

        let error = review_scan_ocr_with_control_and_analyser(
            ocr_review_request(&input),
            &directory.path,
            &ScanExecutionControl::direct(),
            move |_, language, width, height, _, _| {
                let mut source = OpenOptions::new()
                    .append(true)
                    .open(&source_to_change)
                    .unwrap();
                source.write_all(b"changed after OCR analysis").unwrap();
                source.sync_all().unwrap();
                Ok(OcrConfidenceResult {
                    language: language.to_string(),
                    image_width: width,
                    image_height: height,
                    word_count: 0,
                    average_confidence: None,
                    minimum_confidence: None,
                    low_confidence_threshold: 80.0,
                    low_confidence_count: 0,
                    low_confidence_words: Vec::new(),
                    malformed_rows: 0,
                    warnings: Vec::new(),
                })
            },
        )
        .unwrap_err();

        assert!(error.contains("changed on disk"));
    }

    #[test]
    fn ocr_review_job_errors_are_content_free() {
        let error = safe_ocr_review_job_error(
            "C:\\Private\\client-passport.png made Tesseract expose private words",
        );
        assert_eq!(
            error,
            "Local OCR confidence review is unavailable. Check Tesseract and the selected language pack, then try again."
        );
        assert!(!error.contains("passport"));
        assert!(!error.contains("private words"));
    }

    #[test]
    fn creates_and_verifies_a_paper_sized_scan_pdf() {
        let directory = TestDirectory::new();
        let first = directory.path.join("first.png");
        let second = directory.path.join("second.png");
        let output = directory.path.join("scan.pdf");
        RgbImage::from_fn(80, 120, |x, y| Rgb([(x * 3) as u8, (y * 2) as u8, 90]))
            .save(&first)
            .unwrap();
        RgbImage::from_pixel(140, 60, Rgb([245, 245, 245]))
            .save(&second)
            .unwrap();

        let result = create_scan_pdf(test_scan_pdf_request(
            vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
            output.to_string_lossy().into_owned(),
        ))
        .unwrap();

        assert_eq!(result.page_count, 2);
        assert!(result.bytes_written > 0);
        assert_eq!(Document::load(&output).unwrap().get_pages().len(), 2);
    }

    #[test]
    fn controlled_scan_reports_monotonic_progress() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.png");
        let output = directory.path.join("controlled.pdf");
        RgbImage::from_pixel(80, 120, Rgb([40, 90, 160]))
            .save(&input)
            .unwrap();
        let reports = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let captured_reports = Arc::clone(&reports);
        let control = ScanExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, stage| {
                captured_reports.lock().unwrap().push((progress, stage));
            }),
        );

        create_scan_pdf_with_control(
            test_scan_pdf_request(
                vec![input.to_string_lossy().into_owned()],
                output.to_string_lossy().into_owned(),
            ),
            &control,
        )
        .unwrap();

        let reports = reports.lock().unwrap();
        assert_eq!(reports.first().map(|report| report.0), Some(1));
        assert_eq!(reports.last().map(|report| report.0), Some(100));
        assert!(reports.windows(2).all(|window| window[0].0 <= window[1].0));
        assert!(reports
            .iter()
            .any(|(_, stage)| stage == "Publishing the verified PDF"));
        assert!(output.exists());
    }

    #[test]
    fn cancellation_before_work_never_publishes_output() {
        let directory = TestDirectory::new();
        let output = directory.path.join("cancelled.pdf");
        let control =
            ScanExecutionControl::new(Arc::new(AtomicBool::new(true)), Arc::new(|_, _| {}));
        let error = create_scan_pdf_with_control(
            test_scan_pdf_request(
                vec![directory
                    .path
                    .join("unused.png")
                    .to_string_lossy()
                    .into_owned()],
                output.to_string_lossy().into_owned(),
            ),
            &control,
        )
        .unwrap_err();

        assert_eq!(error, SCAN_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_during_page_preparation_never_publishes_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.png");
        let output = directory.path.join("cancelled-during-work.pdf");
        RgbImage::from_pixel(80, 120, Rgb([80, 120, 160]))
            .save(&input)
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress_cancelled = Arc::clone(&cancelled);
        let control = ScanExecutionControl::new(
            Arc::clone(&cancelled),
            Arc::new(move |progress, _| {
                if progress >= 10 {
                    progress_cancelled.store(true, Ordering::Release);
                }
            }),
        );

        let error = create_scan_pdf_with_control(
            test_scan_pdf_request(
                vec![input.to_string_lossy().into_owned()],
                output.to_string_lossy().into_owned(),
            ),
            &control,
        )
        .unwrap_err();

        assert_eq!(error, SCAN_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
    }

    #[test]
    fn streamed_ocr_progress_can_cancel_an_external_tool_promptly() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress_cancelled = Arc::clone(&cancelled);
        let reports = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let captured_reports = Arc::clone(&reports);
        let control = ScanExecutionControl::new(
            Arc::clone(&cancelled),
            Arc::new(move |progress, stage| {
                captured_reports
                    .lock()
                    .unwrap()
                    .push((progress, stage.clone()));
                if stage.contains("50%") {
                    progress_cancelled.store(true, Ordering::Release);
                }
            }),
        );
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("scan_export::tests::cancellable_command_helper")
            .arg("--nocapture")
            .env("PAPERWORKS_OCR_PROGRESS_TEST_CHILD", "1");
        let progress_control = control.pdf_subrange(76, 90, "");
        let mut parser = OcrProgressParser::default();
        let started = Instant::now();

        let error = run_cancellable_command(
            &mut command,
            &control,
            move |chunk| {
                parser.push(chunk, |update| {
                    let _ = progress_control.checkpoint(update.percent, stage_for_ocr(update));
                });
            },
            |error| format!("The progress test helper could not start: {error}"),
        )
        .unwrap_err();

        assert_eq!(error, SCAN_JOB_CANCELLED_ERROR);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(reports
            .lock()
            .unwrap()
            .iter()
            .any(|(progress, stage)| *progress == 83 && stage.contains("50%")));
    }

    #[test]
    fn cancellable_command_helper() {
        if std::env::var_os("PAPERWORKS_OCR_PROGRESS_TEST_CHILD").is_none() {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        writeln!(stderr, "PAPERWORKS_OCR_PROGRESS_V1\t50\t1\t2").unwrap();
        stderr.flush().unwrap();
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn ocr_diagnostics_skip_progress_records() {
        let bytes = b"PAPERWORKS_OCR_PROGRESS_V1\t50\t1\t2\r\
            OCR: 75% 3/4\n\
            The selected language pack stopped unexpectedly.\n";
        assert_eq!(
            first_ocr_diagnostic(bytes).as_deref(),
            Some("The selected language pack stopped unexpectedly.")
        );
    }

    #[test]
    fn ocr_diagnostics_prefer_an_error_over_orientation_information() {
        let bytes = "1 page is facing \u{21e7}, confidence 15.81 - rotation appears correct\n\
            TesseractConfigError: Error occurred while parsing a configuration file\n";
        assert_eq!(
            first_ocr_diagnostic(bytes.as_bytes()).as_deref(),
            Some("TesseractConfigError: Error occurred while parsing a configuration file")
        );
    }

    #[test]
    fn searchable_text_verification_decodes_only_invoked_form_xobjects() {
        let (mut invoked, invoked_page) = nested_ocr_form_document(true);
        let original_page = invoked.get_object(invoked_page).unwrap().clone();
        assert!(invoked.extract_text(&[1]).unwrap().trim().is_empty());

        let extracted = extract_searchable_text_from_page(&mut invoked, 1, invoked_page).unwrap();
        assert!(extracted.contains("Nested OCR text"));
        assert_eq!(invoked.get_object(invoked_page).unwrap(), &original_page);
        assert!(inspect_searchable_text_pages_in_document(&mut invoked)
            .unwrap()
            .is_empty());

        let (mut unused, unused_page) = nested_ocr_form_document(false);
        assert!(
            extract_searchable_text_from_page(&mut unused, 1, unused_page)
                .unwrap()
                .trim()
                .is_empty()
        );
        assert_eq!(
            inspect_searchable_text_pages_in_document(&mut unused).unwrap(),
            vec![1]
        );
    }

    #[test]
    fn temporary_ocr_progress_plugin_is_removed_on_drop() {
        let directory = TestDirectory::new();
        let anchor = directory.path.join("scan.pdf");
        let plugin = TemporaryOcrProgressPlugin::new(&anchor).unwrap();
        let path = plugin.path().to_path_buf();
        let source = fs::read_to_string(&path).unwrap();

        assert!(source.contains("PAPERWORKS_OCR_PROGRESS_V1"));
        assert!(source.contains("def check_options"));
        assert!(source.contains("options.progress_bar = True"));
        assert!(source.contains("def get_progressbar_class"));
        assert!(path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".ocr-progress.")));
        drop(plugin);
        assert!(!path.exists());
    }

    #[test]
    fn source_change_before_publication_never_publishes_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("changing-source.png");
        let output = directory.path.join("source-changed.pdf");
        RgbImage::from_pixel(80, 120, Rgb([80, 120, 160]))
            .save(&input)
            .unwrap();
        let source_to_change = input.clone();
        let changed = Arc::new(AtomicBool::new(false));
        let progress_changed = Arc::clone(&changed);
        let control = ScanExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress >= 98 && !progress_changed.swap(true, Ordering::AcqRel) {
                    OpenOptions::new()
                        .append(true)
                        .open(&source_to_change)
                        .unwrap()
                        .write_all(b"changed")
                        .unwrap();
                }
            }),
        );

        let error = create_scan_pdf_with_control(
            test_scan_pdf_request(
                vec![input.to_string_lossy().into_owned()],
                output.to_string_lossy().into_owned(),
            ),
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    #[ignore = "requires OCRmyPDF, Tesseract eng/tur packs, and PAPERWORKS_OCR_CORPUS"]
    fn live_ocr_corpus_verifies_searchable_text_and_recognition_recall() {
        let corpus = std::env::var_os("PAPERWORKS_OCR_CORPUS")
            .map(PathBuf::from)
            .expect("set PAPERWORKS_OCR_CORPUS to the OCR corpus directory");
        let cases = [
            ("english", "eng", 0.85_f64),
            ("turkish", "tur", 0.75_f64),
            ("rotated", "eng", 0.80_f64),
            ("noisy", "eng", 0.65_f64),
        ];
        let directory = TestDirectory::new();

        for (name, language, minimum_recall) in cases {
            let image = corpus.join(format!("{name}.png"));
            let expected_path = corpus.join(format!("{name}.txt"));
            let expected_metadata = fs::metadata(&expected_path)
                .unwrap_or_else(|error| panic!("{name}.txt is unavailable: {error}"));
            assert!(
                expected_metadata.len() <= 1024 * 1024,
                "{name}.txt exceeds the 1 MB corpus limit"
            );
            let expected = fs::read_to_string(&expected_path)
                .unwrap_or_else(|error| panic!("{name}.txt is not valid UTF-8: {error}"));
            let output = directory.path.join(format!("{name}-ocr.pdf"));
            let mut request = test_scan_pdf_request(
                vec![image.to_string_lossy().into_owned()],
                output.to_string_lossy().into_owned(),
            );
            request.dpi = 300;
            request.recognise_text = true;
            request.straighten = true;
            request.ocr_language = language.to_string();
            let reports = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
            let captured_reports = Arc::clone(&reports);
            let control = ScanExecutionControl::new(
                Arc::new(AtomicBool::new(false)),
                Arc::new(move |progress, stage| {
                    captured_reports.lock().unwrap().push((progress, stage));
                }),
            );

            let result = create_scan_pdf_with_control(request, &control)
                .unwrap_or_else(|error| panic!("{name} OCR export failed: {error}"));
            assert_eq!(result.searchable_text_pages, 1, "{name} has no text layer");
            assert!(
                result.pages_without_searchable_text.is_empty(),
                "{name} failed page-level text verification"
            );
            let reports = reports.lock().unwrap();
            let ocr_reports = reports
                .iter()
                .filter(|(_, stage)| stage.starts_with("Local OCR:"))
                .collect::<Vec<_>>();
            assert!(
                !ocr_reports.is_empty(),
                "{name} did not report engine-level OCR progress"
            );
            assert!(
                ocr_reports
                    .iter()
                    .all(|(progress, _)| (76..=90).contains(progress)),
                "{name} reported OCR outside the reserved scan interval"
            );
            assert!(
                ocr_reports
                    .iter()
                    .any(|(progress, stage)| *progress == 90 && stage.contains("100%")),
                "{name} did not report OCR completion"
            );
            drop(reports);
            let mut document = Document::load(&output)
                .unwrap_or_else(|error| panic!("{name} output could not be reopened: {error}"));
            let page_id = *document
                .get_pages()
                .get(&1)
                .unwrap_or_else(|| panic!("{name} output is missing page 1"));
            let recognised = extract_searchable_text_from_page(&mut document, 1, page_id)
                .unwrap_or_else(|error| panic!("{name} text layer could not be read: {error}"));
            let recall = token_recall(&expected, &recognised);
            assert!(
                recall >= minimum_recall,
                "{name} token recall {recall:.3} is below {minimum_recall:.3}"
            );
            println!(
                "PAPERWORKS_OCR_CASE_V1\t{name}\t{language}\t{recall:.6}\t{minimum_recall:.6}\t1\tprogress-verified"
            );
        }
    }

    fn preview_request(input: &Path) -> PreviewScanImageRequest {
        PreviewScanImageRequest {
            input_path: input.to_string_lossy().into_owned(),
            colour_mode: ScanColourMode::Colour,
            auto_orient: true,
            auto_crop: false,
            correct_perspective: false,
            remove_shadows: false,
        }
    }

    fn nested_ocr_form_document(invoke_form: bool) -> (Document, ObjectId) {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let form_content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), Object::Integer(16)],
                ),
                Operation::new("Td", vec![Object::Integer(40), Object::Integer(80)]),
                Operation::new("Tj", vec![Object::string_literal("Nested OCR text")]),
                Operation::new("ET", vec![]),
            ],
        }
        .encode()
        .unwrap();
        let form_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "FormType" => 1,
                "BBox" => vec![0.into(), 0.into(), 200.into(), 120.into()],
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => font_id },
                },
            },
            form_content,
        ));
        let page_operations = if invoke_form {
            vec![Operation::new(
                "Do",
                vec![Object::Name(b"OCRLayer".to_vec())],
            )]
        } else {
            vec![Operation::new("q", vec![]), Operation::new("Q", vec![])]
        };
        let page_content = Content {
            operations: page_operations,
        }
        .encode()
        .unwrap();
        let content_id = document.add_object(Stream::new(dictionary! {}, page_content));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "OCRLayer" => form_id },
            },
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        (document, page_id)
    }

    fn ocr_review_request(input: &Path) -> ReviewScanOcrRequest {
        ReviewScanOcrRequest {
            input_path: input.to_string_lossy().into_owned(),
            colour_mode: ScanColourMode::Colour,
            auto_orient: true,
            auto_crop: false,
            correct_perspective: false,
            remove_shadows: false,
            language: "eng".to_string(),
        }
    }

    fn token_recall(expected: &str, recognised: &str) -> f64 {
        let expected = normalised_tokens(expected);
        let recognised = normalised_tokens(recognised);
        if expected.is_empty() {
            return 1.0;
        }
        expected
            .iter()
            .filter(|token| recognised.contains(*token))
            .count() as f64
            / expected.len() as f64
    }

    fn normalised_tokens(text: &str) -> BTreeSet<String> {
        text.split(|character: char| !character.is_alphanumeric())
            .map(|token| token.trim().to_lowercase())
            .filter(|token| !token.is_empty())
            .collect()
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
                "tufekci-paperworks-scan-test-{}-{nonce}",
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
