use crate::file_safety::{TemporaryOutput, ValidatedPdfPaths};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::{
    DynamicImage, GenericImageView, GrayImage, ImageFormat, ImageReader, Limits, RgbImage,
};
use lopdf::{Document, LoadOptions, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Cursor, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::time::SystemTime;

const MIN_JPEG_QUALITY: u8 = 40;
const MAX_JPEG_QUALITY: u8 = 95;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_IMAGE_DIMENSION: u32 = 20_000;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
const MAX_DECODED_IMAGE_BYTES: usize = 160 * 1024 * 1024;
const MAX_IMAGES_TO_PROCESS: usize = 500;
const MAX_TOTAL_PIXELS_TO_PROCESS: u64 = 1_000_000_000;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MIN_IMAGE_SAVING_BYTES: usize = 128;
const PREVIEW_MAX_DIMENSION: u32 = 720;
const PREVIEW_MAX_BYTES: usize = 3 * 1024 * 1024;
pub(crate) const COMPRESSION_NOT_SMALLER_ERROR: &str =
    "This PDF is already efficient at the selected quality. No smaller copy was published.";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPdfCompressionRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
    pub(crate) jpeg_quality: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCompressedPdfRequest {
    pub(crate) input_path: String,
    pub(crate) output_path: String,
    pub(crate) input_password: Option<String>,
    pub(crate) jpeg_quality: u8,
    pub(crate) acknowledge_certificate_signatures: bool,
    #[serde(default)]
    pub(crate) output_protection: Option<PdfOutputProtection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfCompressionPreview {
    file_name: String,
    page_count: usize,
    jpeg_quality: u8,
    original_bytes: u64,
    estimated_bytes: u64,
    saving_bytes: u64,
    saving_percent: f64,
    can_reduce: bool,
    image_count: usize,
    compatible_image_count: usize,
    images_recompressed: usize,
    skipped_image_count: usize,
    unchanged_compatible_image_count: usize,
    objects_pruned: usize,
    processing_limit_reached: bool,
    sample_width: Option<u32>,
    sample_height: Option<u32>,
    sample_original_bytes: Option<u64>,
    sample_compressed_bytes: Option<u64>,
    sample_would_be_recompressed: bool,
    source_preview_data_url: Option<String>,
    compressed_preview_data_url: Option<String>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCompressedPdfResult {
    pub(crate) output_path: String,
    pub(crate) page_count: usize,
    pub(crate) bytes_written: u64,
    pub(crate) original_bytes: u64,
    pub(crate) saved_bytes: u64,
    pub(crate) saved_percent: f64,
    pub(crate) images_recompressed: usize,
    pub(crate) skipped_image_count: usize,
    pub(crate) encryption: &'static str,
    pub(crate) warnings: Vec<String>,
}

struct LoadedPdf {
    document: Document,
    source_bytes: u64,
    page_count: usize,
    was_encrypted: bool,
    had_certificate_signature: bool,
    had_form_fields: bool,
    had_bookmarks: bool,
}

struct PreparedCompression {
    document: Document,
    source_bytes: u64,
    estimated_bytes: u64,
    page_count: usize,
    stats: CompressionStats,
    sample: Option<CompressionSample>,
    warnings: Vec<String>,
    had_form_fields: bool,
    had_bookmarks: bool,
}

#[derive(Default)]
struct CompressionStats {
    image_count: usize,
    compatible_image_count: usize,
    images_recompressed: usize,
    objects_pruned: usize,
    processing_limit_reached: bool,
}

struct CompressionSample {
    source: DynamicImage,
    candidate_jpeg: Vec<u8>,
    source_stream_bytes: usize,
    width: u32,
    height: u32,
    would_be_recompressed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceFingerprint {
    bytes: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Copy)]
enum PdfImageColour {
    Grey,
    Rgb,
}

struct EncodedCandidate {
    image: DynamicImage,
    jpeg: Vec<u8>,
    width: u32,
    height: u32,
}

#[cfg(test)]
pub fn preview_pdf_compression(
    request: PreviewPdfCompressionRequest,
) -> Result<PdfCompressionPreview, String> {
    preview_pdf_compression_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_preview_pdf_compression_request(
    request: &PreviewPdfCompressionRequest,
) -> Result<(), String> {
    validate_quality(request.jpeg_quality)?;
    validate_password(request.input_password.as_deref())?;
    crate::file_safety::canonical_pdf_input(&request.input_path)?;
    Ok(())
}

pub(crate) fn preview_pdf_compression_with_control(
    request: PreviewPdfCompressionRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfCompressionPreview, String> {
    control.checkpoint(2, "Validating compression preview request")?;
    validate_preview_pdf_compression_request(&request)?;
    let PreviewPdfCompressionRequest {
        input_path,
        input_password,
        jpeg_quality,
    } = request;
    let input = crate::file_safety::canonical_pdf_input(&input_path)?;
    let opening_source_fingerprint = source_fingerprint(&input)?;
    control.checkpoint(8, "Opening source PDF for compression preview")?;
    let loaded = load_pdf(&input, input_password.as_deref())?;
    control.checkpoint(18, "Preparing bounded compression analysis")?;
    let prepared = prepare_compression(loaded, jpeg_quality, true, false, control)?;
    let saving_bytes = prepared
        .source_bytes
        .saturating_sub(prepared.estimated_bytes);
    let can_reduce = prepared.estimated_bytes < prepared.source_bytes;
    let saving_percent = percentage(saving_bytes, prepared.source_bytes);
    let skipped_image_count = prepared
        .stats
        .image_count
        .saturating_sub(prepared.stats.compatible_image_count);
    let unchanged_compatible_image_count = prepared
        .stats
        .compatible_image_count
        .saturating_sub(prepared.stats.images_recompressed);
    let (
        source_preview_data_url,
        compressed_preview_data_url,
        sample_width,
        sample_height,
        sample_original_bytes,
        sample_compressed_bytes,
        sample_would_be_recompressed,
    ) = match prepared.sample {
        Some(sample) => {
            control.checkpoint(78, "Encoding source image sample")?;
            let source_preview = encode_preview_png(&sample.source)?;
            control.checkpoint(84, "Encoding compressed image sample")?;
            let candidate = decode_jpeg(
                &sample.candidate_jpeg,
                sample.width,
                sample.height,
                PdfImageColour::Rgb,
            )?;
            let candidate_preview = encode_preview_png(&candidate)?;
            control.ensure_not_cancelled()?;
            (
                Some(data_url("image/png", &source_preview)),
                Some(data_url("image/png", &candidate_preview)),
                Some(sample.width),
                Some(sample.height),
                Some(sample.source_stream_bytes as u64),
                Some(sample.candidate_jpeg.len() as u64),
                sample.would_be_recompressed,
            )
        }
        None => (None, None, None, None, None, None, false),
    };

    let preview = PdfCompressionPreview {
        file_name: input
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.pdf")
            .to_string(),
        page_count: prepared.page_count,
        jpeg_quality,
        original_bytes: prepared.source_bytes,
        estimated_bytes: prepared.estimated_bytes,
        saving_bytes,
        saving_percent,
        can_reduce,
        image_count: prepared.stats.image_count,
        compatible_image_count: prepared.stats.compatible_image_count,
        images_recompressed: prepared.stats.images_recompressed,
        skipped_image_count,
        unchanged_compatible_image_count,
        objects_pruned: prepared.stats.objects_pruned,
        processing_limit_reached: prepared.stats.processing_limit_reached,
        sample_width,
        sample_height,
        sample_original_bytes,
        sample_compressed_bytes,
        sample_would_be_recompressed,
        source_preview_data_url,
        compressed_preview_data_url,
        warnings: prepared.warnings,
    };
    control.checkpoint(98, "Rechecking source PDF before returning preview")?;
    verify_source_fingerprint(&input, opening_source_fingerprint)?;
    control.ensure_not_cancelled()?;
    Ok(preview)
}

pub(crate) fn run_pdf_compression_preview_job_with_control(
    request: PreviewPdfCompressionRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfCompressionPreview, String> {
    preview_pdf_compression_with_control(request, control)
        .map(job_safe_compression_preview)
        .map_err(|error| {
            if error == PDF_JOB_CANCELLED_ERROR {
                error
            } else {
                safe_compression_preview_job_error(&error)
            }
        })
}

fn job_safe_compression_preview(mut preview: PdfCompressionPreview) -> PdfCompressionPreview {
    preview.file_name = "PDF".to_string();
    preview
}

fn safe_compression_preview_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during compression preview. Choose it again and recalculate."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The source PDF could not be opened with the supplied password.".to_string();
    }
    "Compression preview could not complete a bounded image and structure analysis. Review the PDF and try again."
        .to_string()
}

#[cfg(test)]
pub fn export_compressed_pdf(
    request: ExportCompressedPdfRequest,
) -> Result<ExportCompressedPdfResult, String> {
    export_compressed_pdf_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_export_compressed_pdf_request(
    request: &ExportCompressedPdfRequest,
) -> Result<(), String> {
    validate_quality(request.jpeg_quality)?;
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    Ok(())
}

pub(crate) fn export_compressed_pdf_with_control(
    request: ExportCompressedPdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportCompressedPdfResult, String> {
    control.checkpoint(2, "Validating compression request")?;
    validate_export_compressed_pdf_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    let opening_source_fingerprint = source_fingerprint(&paths.input)?;
    control.checkpoint(8, "Opening source PDF")?;
    let loaded = load_pdf(&paths.input, request.input_password.as_deref())?;
    control.checkpoint(18, "Checking document rewrite safety")?;
    ensure_document_rewrite_acknowledged(
        &loaded.document,
        &paths.input,
        request.acknowledge_certificate_signatures,
    )?;
    let output_will_be_protected = request.output_protection.is_some();
    let mut prepared = prepare_compression(
        loaded,
        request.jpeg_quality,
        false,
        output_will_be_protected,
        control,
    )?;
    if prepared.estimated_bytes >= prepared.source_bytes {
        return Err(COMPRESSION_NOT_SMALLER_ERROR.to_string());
    }
    if output_will_be_protected {
        prepared.warnings.push(
            "The compressed copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    }

    let prepared_output = TemporaryOutput::new(&paths.output)?;
    control.checkpoint(78, "Writing compressed temporary PDF")?;
    save_modern_create_new(prepared.document, prepared_output.path())?;
    control.checkpoint(84, "Reopening compressed PDF")?;
    let verification = Document::load_with_options(
        prepared_output.path(),
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The compressed PDF failed its reopening check: {error}"))?;
    control.checkpoint(88, "Verifying compressed PDF structure")?;
    verify_compressed_pdf(
        &verification,
        prepared.page_count,
        prepared.had_form_fields,
        prepared.had_bookmarks,
    )?;

    let protected_output = if let Some(protection) = request.output_protection.as_ref() {
        let protected_output = TemporaryOutput::new(&paths.output)?;
        let protection_control =
            control.subrange(89, 94, "Applying AES-256 output protection".to_string());
        lock_pdf_changes_with_control(
            prepared_output.path(),
            protected_output.path(),
            &protection.open_password,
            &protection.owner_password,
            &protection_control,
        )?;
        control.checkpoint(95, "Opening protected compressed PDF for verification")?;
        let mut protected_verification = Document::load_with_options(
            protected_output.path(),
            LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
        )
        .map_err(|error| format!("The protected compressed PDF could not be reopened: {error}"))?;
        if !protected_verification.is_encrypted() {
            return Err(
                "The protected compressed PDF did not contain AES-256 encryption and was not saved."
                    .to_string(),
            );
        }
        protected_verification
            .decrypt(&protection.open_password)
            .map_err(|_| {
                "The protected compressed PDF could not be decrypted for verification.".to_string()
            })?;
        control.checkpoint(96, "Repeating compression verification after decryption")?;
        verify_compressed_pdf(
            &protected_verification,
            prepared.page_count,
            prepared.had_form_fields,
            prepared.had_bookmarks,
        )?;
        Some(protected_output)
    } else {
        None
    };
    let final_output = protected_output.as_ref().unwrap_or(&prepared_output);

    control.checkpoint(97, "Checking final compressed size")?;
    let written_before_publication = fs::metadata(final_output.path())
        .map_err(|error| format!("The compressed PDF could not be inspected: {error}"))?
        .len();
    if written_before_publication >= prepared.source_bytes {
        return Err(COMPRESSION_NOT_SMALLER_ERROR.to_string());
    }
    control.checkpoint(98, "Rechecking source PDF before publication")?;
    verify_source_fingerprint(&paths.input, opening_source_fingerprint)?;
    control.checkpoint(99, "Publishing verified compressed PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let saved_bytes = prepared.source_bytes.saturating_sub(bytes_written);

    Ok(ExportCompressedPdfResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: prepared.page_count,
        bytes_written,
        original_bytes: prepared.source_bytes,
        saved_bytes,
        saved_percent: percentage(saved_bytes, prepared.source_bytes),
        images_recompressed: prepared.stats.images_recompressed,
        skipped_image_count: prepared
            .stats
            .image_count
            .saturating_sub(prepared.stats.compatible_image_count),
        encryption: if output_will_be_protected {
            "AES-256"
        } else {
            "None"
        },
        warnings: prepared.warnings,
    })
}

pub(crate) fn run_pdf_compression_job_with_control(
    request: ExportCompressedPdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportCompressedPdfResult, String> {
    export_compressed_pdf_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_compression_job_error(&error)
        }
    })
}

fn verify_compressed_pdf(
    document: &Document,
    page_count: usize,
    had_form_fields: bool,
    had_bookmarks: bool,
) -> Result<(), String> {
    if document.is_encrypted() {
        return Err(
            "The compressed PDF unexpectedly remained encrypted during verification.".to_string(),
        );
    }
    if document.get_pages().len() != page_count {
        return Err("The compressed PDF changed the page count and was not saved.".to_string());
    }
    if had_form_fields
        && !document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        return Err("The compressed PDF lost its form structure and was not saved.".to_string());
    }
    if had_bookmarks
        && !document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"Outlines"))
    {
        return Err("The compressed PDF lost its bookmarks and was not saved.".to_string());
    }
    Ok(())
}

fn safe_compression_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if error == COMPRESSION_NOT_SMALLER_ERROR {
        return error.to_string();
    }
    if normalised.contains("changed on disk") {
        return "The source PDF changed during compression. Choose it again and recalculate the preview."
            .to_string();
    }
    if normalised.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before compression."
            .to_string();
    }
    if normalised.contains("qpdf") || normalised.contains("aes-256") {
        return "AES-256 compression protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The PDF could not be opened or protected with the supplied passwords.".to_string();
    }
    "Compression failed a structural safety check. Recalculate the preview and try again."
        .to_string()
}

fn validate_quality(quality: u8) -> Result<(), String> {
    if !(MIN_JPEG_QUALITY..=MAX_JPEG_QUALITY).contains(&quality) {
        return Err(format!(
            "Image quality must be between {MIN_JPEG_QUALITY} and {MAX_JPEG_QUALITY}."
        ));
    }
    Ok(())
}

fn validate_password(password: Option<&str>) -> Result<(), String> {
    if password.is_some_and(|value| value.len() > MAX_PASSWORD_BYTES) {
        return Err("The source password is too long to process safely.".to_string());
    }
    Ok(())
}

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedPdf, String> {
    let source_bytes = fs::metadata(path)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?
        .len();
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
                "The PDF could not be decrypted for compression. Check its password.".to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    let had_certificate_signature = document_has_certificate_signature(&document);
    let had_form_fields = document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"AcroForm"));
    let had_bookmarks = document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"Outlines"));
    Ok(LoadedPdf {
        document,
        source_bytes,
        page_count,
        was_encrypted,
        had_certificate_signature,
        had_form_fields,
        had_bookmarks,
    })
}

fn prepare_compression(
    loaded: LoadedPdf,
    jpeg_quality: u8,
    include_preview: bool,
    output_will_be_protected: bool,
    control: &PdfJobExecutionControl,
) -> Result<PreparedCompression, String> {
    let LoadedPdf {
        mut document,
        source_bytes,
        page_count,
        was_encrypted,
        had_certificate_signature,
        had_form_fields,
        had_bookmarks,
    } = loaded;
    control.checkpoint(24, "Inspecting embedded images")?;
    let (mut stats, sample) =
        optimise_images(&mut document, jpeg_quality, include_preview, control)?;
    control.checkpoint(70, "Optimising PDF streams")?;
    document.compress();
    stats.objects_pruned = document.prune_objects().len();
    control.checkpoint(74, "Calculating rewritten size")?;
    let mut estimate_document = document.clone();
    let estimated_bytes = estimate_modern_size(&mut estimate_document)?;
    let skipped = stats
        .image_count
        .saturating_sub(stats.compatible_image_count);
    let mut warnings = Vec::new();
    if stats.images_recompressed > 0 {
        warnings.push(format!(
            "{} compatible raster image{} will be recompressed with JPEG quality {}. Text, vectors, links, forms, and OCR text layers remain PDF content.",
            stats.images_recompressed,
            if stats.images_recompressed == 1 { "" } else { "s" },
            jpeg_quality
        ));
    }
    if skipped > 0 {
        warnings.push(format!(
            "{skipped} image{} use colour spaces, masks, filters, dimensions, or data that this preservation-first pass does not JPEG-recompress. Lossless stream optimisation may still change their encoded representation.",
            if skipped == 1 { "" } else { "s" }
        ));
    }
    if stats.processing_limit_reached {
        warnings.push(
            "The bounded image-work limit was reached. Remaining images stay unchanged; split very large PDFs before compressing them more deeply."
                .to_string(),
        );
    }
    if was_encrypted && !output_will_be_protected {
        warnings.push(
            "The compressed copy is not password-protected. Use Protect to apply new AES-256 encryption."
                .to_string(),
        );
    }
    if had_certificate_signature {
        warnings.push(
            "Compression rewrites the PDF and invalidates any existing certificate signature."
                .to_string(),
        );
    }
    if had_form_fields {
        warnings.push(
            "Interactive form structures are preserved and checked, but their appearance should be reviewed in the compressed copy."
                .to_string(),
        );
    }
    if estimated_bytes >= source_bytes {
        warnings.push(
            "The selected quality does not produce a smaller verified rewrite. Try a lower quality or keep the source."
                .to_string(),
        );
    }

    Ok(PreparedCompression {
        document,
        source_bytes,
        estimated_bytes,
        page_count,
        stats,
        sample,
        warnings,
        had_form_fields,
        had_bookmarks,
    })
}

fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose an existing PDF file as the source.".to_string());
    }
    Ok(SourceFingerprint {
        bytes: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn verify_source_fingerprint(path: &Path, expected: SourceFingerprint) -> Result<(), String> {
    if source_fingerprint(path)? != expected {
        return Err(
            "The source PDF changed on disk during compression. Choose it again and recalculate the preview."
                .to_string(),
        );
    }
    Ok(())
}

fn optimise_images(
    document: &mut Document,
    quality: u8,
    include_preview: bool,
    control: &PdfJobExecutionControl,
) -> Result<(CompressionStats, Option<CompressionSample>), String> {
    let image_ids: Vec<ObjectId> = document
        .objects
        .iter()
        .filter_map(|(id, object)| is_image_object(object).then_some(*id))
        .collect();
    let mut stats = CompressionStats {
        image_count: image_ids.len(),
        ..CompressionStats::default()
    };
    let mut sample: Option<CompressionSample> = None;
    let mut processed_images = 0_usize;
    let mut processed_pixels = 0_u64;

    let image_total = image_ids.len();
    for (index, id) in image_ids.into_iter().enumerate() {
        control.checkpoint(
            stage_progress(26, 68, index, image_total),
            format!("Optimising embedded image {} of {image_total}", index + 1),
        )?;
        let Some(Object::Stream(stream)) = document.objects.get(&id) else {
            continue;
        };
        let Some((width, height)) = image_dimensions(stream) else {
            continue;
        };
        let pixels = u64::from(width) * u64::from(height);
        if processed_images >= MAX_IMAGES_TO_PROCESS
            || processed_pixels.saturating_add(pixels) > MAX_TOTAL_PIXELS_TO_PROCESS
        {
            stats.processing_limit_reached = true;
            continue;
        }
        processed_images += 1;
        processed_pixels = processed_pixels.saturating_add(pixels);

        let source_stream_bytes = stream.content.len();
        let Ok(candidate) = create_candidate(stream, quality) else {
            continue;
        };
        stats.compatible_image_count += 1;
        let would_be_recompressed =
            candidate.jpeg.len().saturating_add(MIN_IMAGE_SAVING_BYTES) < source_stream_bytes;
        let candidate_area = u64::from(candidate.width) * u64::from(candidate.height);
        let sample_area = sample
            .as_ref()
            .map(|current| u64::from(current.width) * u64::from(current.height))
            .unwrap_or(0);
        if include_preview && candidate_area > sample_area {
            sample = Some(CompressionSample {
                source: candidate.image.clone(),
                candidate_jpeg: candidate.jpeg.clone(),
                source_stream_bytes,
                width: candidate.width,
                height: candidate.height,
                would_be_recompressed,
            });
        }
        if !would_be_recompressed {
            continue;
        }

        if let Some(Object::Stream(stream)) = document.objects.get_mut(&id) {
            stream.dict.remove(b"DecodeParms");
            stream.dict.set("Filter", "DCTDecode");
            stream.set_content(candidate.jpeg);
            stats.images_recompressed += 1;
        }
    }

    control.ensure_not_cancelled()?;
    Ok((stats, sample))
}

fn stage_progress(start: u8, end: u8, index: usize, total: usize) -> u8 {
    if total == 0 || end <= start {
        return end;
    }
    let completed = index.min(total) as f64 / total as f64;
    start.saturating_add(((end - start) as f64 * completed).round() as u8)
}

fn is_image_object(object: &Object) -> bool {
    matches!(
        object,
        Object::Stream(stream)
            if stream
                .dict
                .get(b"Subtype")
                .and_then(Object::as_name)
                .is_ok_and(|name| name == b"Image")
    )
}

fn image_dimensions(stream: &Stream) -> Option<(u32, u32)> {
    let width = stream
        .dict
        .get(b"Width")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|value| u32::try_from(value).ok())?;
    let height = stream
        .dict
        .get(b"Height")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|value| u32::try_from(value).ok())?;
    let pixels = u64::from(width) * u64::from(height);
    (width > 0
        && height > 0
        && width <= MAX_IMAGE_DIMENSION
        && height <= MAX_IMAGE_DIMENSION
        && pixels <= MAX_IMAGE_PIXELS)
        .then_some((width, height))
}

fn create_candidate(stream: &Stream, quality: u8) -> Result<EncodedCandidate, ()> {
    let (width, height) = image_dimensions(stream).ok_or(())?;
    let bits = stream
        .dict
        .get(b"BitsPerComponent")
        .and_then(Object::as_i64)
        .unwrap_or(8);
    if bits != 8
        || stream.dict.has(b"SMask")
        || stream.dict.has(b"Mask")
        || stream.dict.has(b"Decode")
        || stream.dict.has(b"ImageMask")
        || stream.dict.has(b"Matte")
        || stream.dict.has(b"TufekciSignature")
    {
        return Err(());
    }
    let colour = match stream
        .dict
        .get(b"ColorSpace")
        .and_then(Object::as_name)
        .map_err(|_| ())?
    {
        b"DeviceGray" => PdfImageColour::Grey,
        b"DeviceRGB" => PdfImageColour::Rgb,
        _ => return Err(()),
    };
    let filters = match stream.dict.get(b"Filter") {
        Err(_) => Vec::new(),
        Ok(_) => stream.filters().map_err(|_| ())?,
    };
    let image = if filters.len() == 1 && filters[0] == b"DCTDecode" {
        decode_jpeg(&stream.content, width, height, colour).map_err(|_| ())?
    } else {
        if filters.len() > 1
            || filters
                .iter()
                .any(|filter| !matches!(*filter, b"FlateDecode" | b"LZWDecode" | b"ASCII85Decode"))
            || !supported_decode_parameters(stream)
        {
            return Err(());
        }
        decode_raw_image(stream, width, height, colour).map_err(|_| ())?
    };
    let jpeg = encode_jpeg(&image, quality).map_err(|_| ())?;
    Ok(EncodedCandidate {
        image,
        jpeg,
        width,
        height,
    })
}

fn supported_decode_parameters(stream: &Stream) -> bool {
    let Ok(parameters) = stream.dict.get(b"DecodeParms") else {
        return true;
    };
    let Ok(dictionary) = parameters.as_dict() else {
        return false;
    };
    let predictor = dictionary
        .get(b"Predictor")
        .and_then(Object::as_i64)
        .unwrap_or(1);
    predictor == 1 || (10..=15).contains(&predictor)
}

fn decode_raw_image(
    stream: &Stream,
    width: u32,
    height: u32,
    colour: PdfImageColour,
) -> Result<DynamicImage, String> {
    let channels = match colour {
        PdfImageColour::Grey => 1_u64,
        PdfImageColour::Rgb => 3_u64,
    };
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(channels))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes <= MAX_DECODED_IMAGE_BYTES)
        .ok_or_else(|| "The embedded image is too large to decode safely.".to_string())?;
    let bytes = stream
        .get_plain_content_with_limit(expected)
        .map_err(|error| format!("The embedded image could not be decoded safely: {error}"))?;
    if bytes.len() != expected {
        return Err("The embedded image data does not match its declared dimensions.".to_string());
    }
    match colour {
        PdfImageColour::Grey => GrayImage::from_raw(width, height, bytes)
            .map(DynamicImage::ImageLuma8)
            .ok_or_else(|| "The greyscale image data is invalid.".to_string()),
        PdfImageColour::Rgb => RgbImage::from_raw(width, height, bytes)
            .map(DynamicImage::ImageRgb8)
            .ok_or_else(|| "The colour image data is invalid.".to_string()),
    }
}

fn decode_jpeg(
    bytes: &[u8],
    width: u32,
    height: u32,
    colour: PdfImageColour,
) -> Result<DynamicImage, String> {
    if bytes.is_empty() || bytes.len() > MAX_DECODED_IMAGE_BYTES {
        return Err("The embedded JPEG is empty or too large to decode safely.".to_string());
    }
    match catch_unwind(AssertUnwindSafe(|| -> Result<DynamicImage, String> {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES as u64);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|error| format!("The embedded JPEG could not be decoded: {error}"))?;
        if decoded.dimensions() != (width, height) {
            return Err("The embedded JPEG dimensions do not match the PDF image.".to_string());
        }
        Ok(match colour {
            PdfImageColour::Grey => DynamicImage::ImageLuma8(decoded.to_luma8()),
            PdfImageColour::Rgb => DynamicImage::ImageRgb8(decoded.to_rgb8()),
        })
    })) {
        Ok(result) => result,
        Err(_) => Err("The embedded JPEG was rejected safely.".to_string()),
    }
}

fn encode_jpeg(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    match catch_unwind(AssertUnwindSafe(|| {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, quality)
            .encode_image(image)
            .map_err(|error| format!("The image compression preview failed: {error}"))?;
        if bytes.len() > MAX_DECODED_IMAGE_BYTES {
            return Err("The compressed image exceeded the safe size limit.".to_string());
        }
        Ok(bytes)
    })) {
        Ok(result) => result,
        Err(_) => Err("The embedded image could not be encoded safely.".to_string()),
    }
}

fn encode_preview_png(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut dimension = PREVIEW_MAX_DIMENSION;
    loop {
        let preview = image.thumbnail(dimension, dimension);
        let mut cursor = Cursor::new(Vec::new());
        preview
            .write_to(&mut cursor, ImageFormat::Png)
            .map_err(|error| format!("The image preview could not be encoded: {error}"))?;
        let bytes = cursor.into_inner();
        if bytes.len() <= PREVIEW_MAX_BYTES || dimension <= 320 {
            return Ok(bytes);
        }
        dimension = (dimension * 2 / 3).max(320);
    }
}

fn data_url(mime_type: &str, bytes: &[u8]) -> String {
    format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes))
}

fn estimate_modern_size(document: &mut Document) -> Result<u64, String> {
    let mut writer = CountingWriter::default();
    document
        .save_modern(&mut writer)
        .map_err(|error| format!("The compressed-size estimate could not be written: {error}"))?;
    Ok(writer.bytes)
}

fn save_modern_create_new(mut document: Document, path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("The temporary compressed PDF could not be created: {error}"))?;
    let mut writer = BufWriter::new(file);
    document
        .save_modern(&mut writer)
        .map_err(|error| format!("The compressed PDF could not be written: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("The compressed PDF could not be flushed: {error}"))?;
    let file = writer
        .into_inner()
        .map_err(|error| format!("The compressed PDF could not be completed: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("The compressed PDF could not be secured on disk: {error}"))
}

fn percentage(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 * 100.0 / whole as f64 * 10.0).round() / 10.0
    }
}

#[derive(Default)]
struct CountingWriter {
    bytes: u64,
}

impl Write for CountingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| std::io::Error::other("The output size overflowed."))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_control::PDF_JOB_CANCELLED_ERROR;
    use lopdf::{dictionary, Dictionary};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn previews_a_smaller_candidate_without_publishing_a_file() {
        let directory = TestDirectory::new();
        let input = directory.path.join("large-image.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();

        let result = preview_pdf_compression(PreviewPdfCompressionRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
            jpeg_quality: 75,
        })
        .unwrap();

        assert_eq!(result.page_count, 1);
        assert_eq!(result.image_count, 1);
        assert_eq!(result.compatible_image_count, 1);
        assert_eq!(result.images_recompressed, 1);
        assert!(result.can_reduce);
        assert!(result.estimated_bytes < result.original_bytes);
        assert!(result
            .source_preview_data_url
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(result
            .compressed_preview_data_url
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 1);

        let output = directory.path.join("compressed.pdf");
        let exported = export_compressed_pdf(ExportCompressedPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            jpeg_quality: 75,
            output_protection: None,
        })
        .unwrap();
        assert_eq!(result.estimated_bytes, exported.bytes_written);
    }

    #[test]
    fn controlled_preview_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("controlled-preview.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let progress = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let observed = Arc::clone(&progress);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |value, stage| observed.lock().unwrap().push((value, stage))),
        );

        let preview = run_pdf_compression_preview_job_with_control(
            PreviewPdfCompressionRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                jpeg_quality: 75,
            },
            &control,
        )
        .unwrap();

        assert_eq!(preview.file_name, "PDF");
        assert!(preview.can_reduce);
        let progress = progress.lock().unwrap();
        assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(progress
            .iter()
            .any(|(value, stage)| *value >= 26 && stage.contains("embedded image")));
        assert_eq!(progress.last().map(|entry| entry.0), Some(98));

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let cancelling_control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |value, _| {
                if value >= 26 {
                    cancellation_flag.store(true, Ordering::Release);
                }
            }),
        );
        let error = preview_pdf_compression_with_control(
            PreviewPdfCompressionRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                jpeg_quality: 75,
            },
            &cancelling_control,
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn preview_rejects_a_source_changed_at_the_final_gate() {
        let directory = TestDirectory::new();
        let input = directory.path.join("mutated-preview.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let source_to_mutate = input.clone();
        let mutated = Arc::new(AtomicBool::new(false));
        let progress_mutated = Arc::clone(&mutated);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress >= 98 && !progress_mutated.swap(true, Ordering::AcqRel) {
                    let mut source = OpenOptions::new()
                        .append(true)
                        .open(&source_to_mutate)
                        .unwrap();
                    source.write_all(b"\n% changed during preview").unwrap();
                    source.sync_all().unwrap();
                }
            }),
        );

        let error = preview_pdf_compression_with_control(
            PreviewPdfCompressionRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                jpeg_quality: 75,
            },
            &control,
        )
        .unwrap_err();

        assert!(mutated.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
    }

    #[test]
    fn compression_preview_job_errors_are_content_free() {
        let error = safe_compression_preview_job_error(
            "C:\\Private\\Client.pdf could not decrypt with private-password",
        );
        assert_eq!(
            error,
            "The source PDF could not be opened with the supplied password."
        );
        assert!(!error.contains("Client"));
        assert!(!error.contains("private-password"));
    }

    #[test]
    fn exports_a_smaller_reopenable_copy_and_preserves_page_structure() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("compressed.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();

        let result = export_compressed_pdf(ExportCompressedPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            jpeg_quality: 70,
            output_protection: None,
        })
        .unwrap();

        assert!(result.bytes_written < result.original_bytes);
        assert!(result.saved_bytes > 0);
        assert_eq!(result.images_recompressed, 1);
        assert_eq!(result.encryption, "None");
        let output_document = Document::load(&output).unwrap();
        assert_eq!(output_document.get_pages().len(), 1);
        assert!(output_document.objects.values().any(|object| {
            matches!(object, Object::Stream(stream) if is_image_object(object)
                && stream.filters().is_ok_and(|filters| filters == vec![b"DCTDecode".as_slice()]))
        }));
    }

    #[test]
    fn refuses_to_overwrite_the_source_or_an_existing_destination() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let request = |output: &Path| ExportCompressedPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            jpeg_quality: 70,
            output_protection: None,
        };

        let same_path_error = export_compressed_pdf(request(&input)).unwrap_err();
        assert!(
            same_path_error.contains("cannot be overwritten")
                || same_path_error.contains("already exists")
        );
        let existing = directory.path.join("existing.pdf");
        fs::write(&existing, b"already here").unwrap();
        assert!(export_compressed_pdf(request(&existing))
            .unwrap_err()
            .contains("already exists"));
    }

    #[test]
    fn requires_acknowledgement_before_rewriting_a_signed_pdf() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed.pdf");
        let output = directory.path.join("compressed.pdf");
        compression_fixture(true)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();

        let error = export_compressed_pdf(ExportCompressedPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            jpeg_quality: 70,
            output_protection: None,
        })
        .unwrap_err();

        assert!(error.contains("certificate signature"));
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_during_image_work_never_publishes_the_destination() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("compressed.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress_cancelled = Arc::clone(&cancelled);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 26 {
                    progress_cancelled.store(true, Ordering::Release);
                }
            }),
        );

        let error = export_compressed_pdf_with_control(
            ExportCompressedPdfRequest {
                acknowledge_certificate_signatures: false,
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                jpeg_quality: 70,
                output_protection: None,
            },
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
    }

    #[test]
    fn source_change_before_publication_discards_the_compressed_candidate() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("compressed.pdf");
        compression_fixture(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let source_to_mutate = input.clone();
        let mutated = Arc::new(AtomicBool::new(false));
        let progress_mutated = Arc::clone(&mutated);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress >= 98 && !progress_mutated.swap(true, Ordering::AcqRel) {
                    let mut source = OpenOptions::new()
                        .append(true)
                        .open(&source_to_mutate)
                        .unwrap();
                    source.write_all(b"\n").unwrap();
                    source.sync_all().unwrap();
                }
            }),
        );

        let error = export_compressed_pdf_with_control(
            ExportCompressedPdfRequest {
                acknowledge_certificate_signatures: false,
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                jpeg_quality: 70,
                output_protection: None,
            },
            &control,
        )
        .unwrap_err();

        assert!(mutated.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn does_not_jpeg_recompress_masked_or_non_device_colour_images() {
        let mut document = compression_fixture(false);
        let image = document
            .objects
            .values_mut()
            .find_map(|object| match object {
                Object::Stream(stream)
                    if stream
                        .dict
                        .get(b"Subtype")
                        .and_then(Object::as_name)
                        .is_ok_and(|name| name == b"Image") =>
                {
                    Some(stream)
                }
                _ => None,
            })
            .unwrap();
        image.dict.set("ColorSpace", "DeviceCMYK");
        image.dict.set("Mask", vec![0.into(), 1.into()]);

        let (stats, sample) =
            optimise_images(&mut document, 75, true, &PdfJobExecutionControl::direct()).unwrap();

        assert_eq!(stats.image_count, 1);
        assert_eq!(stats.compatible_image_count, 0);
        assert_eq!(stats.images_recompressed, 0);
        assert!(sample.is_none());
    }

    #[test]
    fn rejects_quality_outside_the_reviewed_range() {
        assert!(validate_quality(39)
            .unwrap_err()
            .contains("between 40 and 95"));
        assert!(validate_quality(96)
            .unwrap_err()
            .contains("between 40 and 95"));
    }

    fn compression_fixture(signed: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let width = 320_u32;
        let height = 240_u32;
        let mut pixels = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[
                    ((x / 8) % 32) as u8 * 8,
                    ((y / 8) % 30) as u8 * 8,
                    (((x + y) / 16) % 32) as u8 * 8,
                ]);
            }
        }
        let image_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(width),
                "Height" => i64::from(height),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            pixels,
        ));
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"q 320 0 0 240 0 0 cm /Im1 Do Q".to_vec(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 320.into(), 240.into()],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im1" => image_id },
            },
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let mut catalog = Dictionary::from_iter(vec![(b"Type".to_vec(), "Catalog".into())]);
        catalog.set("Pages", pages_id);
        if signed {
            let signature_id = document.add_object(dictionary! {
                "FT" => "Sig",
                "V" => dictionary! {
                    "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                    "Contents" => Object::String(vec![1, 2, 3], lopdf::StringFormat::Hexadecimal),
                },
            });
            catalog.set(
                "AcroForm",
                dictionary! { "Fields" => vec![signature_id.into()] },
            );
        }
        let catalog_id = document.add_object(catalog);
        document.trailer.set("Root", catalog_id);
        document
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-compression-test",
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
