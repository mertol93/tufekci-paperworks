use crate::archive::{convert_pdfa_candidate_with_control, ensure_pdf_archive_ready, PdfAProfile};
use crate::compression::{
    export_compressed_pdf_with_control, ExportCompressedPdfRequest, COMPRESSION_NOT_SMALLER_ERROR,
};
use crate::file_safety::{
    canonical_pdf_input, paths_are_equal, publish_prepared_file, reject_control_characters,
    validated_new_pdf_output, ValidatedPdfPaths,
};
use crate::health::ensure_pdf_rewrite_acknowledged;
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::ocr::{ensure_ocr_ready, validate_ocr_language};
use crate::privacy::{clean_pdf_privacy_with_control, CleanPdfPrivacyRequest, PrivacyCleanOptions};
use crate::privacy_inspection::{
    run_pdf_privacy_inspection_job_with_control, validate_inspect_pdf_privacy_request,
    InspectPdfPrivacyRequest, PdfPrivacyInspectionResult,
};
use crate::protection::{
    decrypt_pdf_to_path_with_control, lock_pdf_changes_from_source_with_control, validate_password,
    validate_pdf_output_protection, PdfOutputProtection,
};
use crate::scan_export::{
    inspect_searchable_text_pages, inspect_searchable_text_pages_in_document, run_ocrmypdf,
};
use crate::temporary_cleanup::{register_temporary_path, TemporaryKind, TemporaryLease};
use lopdf::{Document, LoadOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BATCH_INPUTS: usize = 50;
const MAX_BATCH_TOTAL_SOURCE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const MAX_OUTPUT_FILENAME_BYTES: usize = 240;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_BATCH_PAGE_COUNT: usize = 100_000;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MIN_JPEG_QUALITY: u8 = 40;
const MAX_JPEG_QUALITY: u8 = 95;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRecipeInput {
    input_path: String,
    input_password: Option<String>,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    expected_page_count: usize,
    output_file_name: String,
    acknowledge_certificate_signatures: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRecipeOptions {
    clean_privacy: bool,
    privacy_options: PrivacyCleanOptions,
    compress: bool,
    jpeg_quality: u8,
    recognise_text: bool,
    straighten: bool,
    ocr_language: String,
    archive_profile: Option<PdfAProfile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBatchRecipeRequest {
    inputs: Vec<BatchRecipeInput>,
    output_directory: String,
    options: BatchRecipeOptions,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRecipeItemResult {
    source_file_name: String,
    output_path: Option<String>,
    page_count: usize,
    bytes_written: u64,
    privacy_structures_removed: usize,
    images_recompressed: usize,
    searchable_text_pages: usize,
    steps_applied: Vec<String>,
    skipped_reason: Option<String>,
    note: Option<String>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBatchRecipeResult {
    output_directory: String,
    input_count: usize,
    output_count: usize,
    skipped_count: usize,
    bytes_written: u64,
    encryption: &'static str,
    items: Vec<BatchRecipeItemResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SearchableOcrRequest {
    pub(crate) input_path: String,
    pub(crate) output_path: String,
    pub(crate) input_password: Option<String>,
    pub(crate) language: String,
    pub(crate) straighten: bool,
    pub(crate) acknowledge_certificate_signatures: bool,
    #[serde(default)]
    pub(crate) output_protection: Option<PdfOutputProtection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchableOcrResult {
    output_path: String,
    page_count: usize,
    searchable_text_pages: usize,
    pages_without_searchable_text: usize,
    bytes_written: u64,
    language: String,
    deskew_requested: bool,
    encryption: &'static str,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectBatchSourcesRequest {
    pub(crate) sources: Vec<InspectPdfPrivacyRequest>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSourceInspectionItem {
    source_index: usize,
    inspection: Option<PdfPrivacyInspectionResult>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectBatchSourcesResult {
    source_count: usize,
    inspected_count: usize,
    failed_count: usize,
    items: Vec<BatchSourceInspectionItem>,
}

struct ValidatedBatchInput {
    input: PathBuf,
    input_password: Option<String>,
    output: PathBuf,
    source_file_name: String,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    expected_page_count: usize,
    source_encrypted: bool,
    acknowledge_certificate_signatures: bool,
}

struct ValidatedBatch {
    inputs: Vec<ValidatedBatchInput>,
    output_directory: PathBuf,
    options: BatchRecipeOptions,
    output_protection: Option<PdfOutputProtection>,
}

struct PreparedOutput {
    item_index: usize,
    staged_path: PathBuf,
    destination: PathBuf,
}

pub(crate) fn validate_inspect_batch_sources_request(
    request: &InspectBatchSourcesRequest,
) -> Result<(), String> {
    if request.sources.is_empty() {
        return Err("Choose at least one PDF to inspect for the batch recipe.".to_string());
    }
    if request.sources.len() > MAX_BATCH_INPUTS {
        return Err(format!(
            "A batch inspection may contain no more than {MAX_BATCH_INPUTS} source PDFs."
        ));
    }
    let mut canonical_sources = HashSet::with_capacity(request.sources.len());
    for source in &request.sources {
        validate_inspect_pdf_privacy_request(source)?;
        let canonical = canonical_pdf_input(&source.input_path)?;
        if !canonical_sources.insert(canonical) {
            return Err("Choose each batch inspection source only once.".to_string());
        }
    }
    Ok(())
}

pub(crate) fn run_batch_source_inspection_job_with_control(
    request: InspectBatchSourcesRequest,
    control: &PdfJobExecutionControl,
) -> Result<InspectBatchSourcesResult, String> {
    control.checkpoint(2, "Validating batch source review")?;
    validate_inspect_batch_sources_request(&request)?;
    let source_count = request.sources.len();
    let mut items = Vec::with_capacity(source_count);
    let mut inspected_count = 0_usize;
    let mut failed_count = 0_usize;

    for (source_index, source) in request.sources.into_iter().enumerate() {
        control.ensure_not_cancelled()?;
        let item_start = batch_progress(5, 96, source_index, source_count);
        let item_end = batch_progress(5, 96, source_index + 1, source_count);
        let item_control = control.subrange(
            item_start,
            item_end,
            format!("PDF {} of {source_count}", source_index + 1),
        );
        match run_pdf_privacy_inspection_job_with_control(source, &item_control) {
            Ok(inspection) => {
                inspected_count += 1;
                items.push(BatchSourceInspectionItem {
                    source_index,
                    inspection: Some(inspection),
                    error: None,
                });
            }
            Err(error) if error == PDF_JOB_CANCELLED_ERROR => return Err(error),
            Err(error) => {
                failed_count += 1;
                items.push(BatchSourceInspectionItem {
                    source_index,
                    inspection: None,
                    error: Some(error),
                });
            }
        }
    }

    control.checkpoint(99, "Finalising batch source review")?;
    Ok(InspectBatchSourcesResult {
        source_count,
        inspected_count,
        failed_count,
        items,
    })
}

pub(crate) fn run_batch_recipe_with_control(
    request: RunBatchRecipeRequest,
    control: &PdfJobExecutionControl,
) -> Result<RunBatchRecipeResult, String> {
    run_batch_recipe_with_control_and_engines(
        request,
        control,
        &run_ocrmypdf,
        &ensure_ocr_ready,
        &run_batch_pdfa_candidate,
        &|control| ensure_pdf_archive_ready(None, control),
    )
}

fn run_batch_recipe_with_control_and_engines<F, R, A, AR>(
    request: RunBatchRecipeRequest,
    control: &PdfJobExecutionControl,
    ocr_runner: &F,
    ocr_readiness: &R,
    archive_runner: &A,
    archive_readiness: &AR,
) -> Result<RunBatchRecipeResult, String>
where
    F: Fn(&Path, &Path, &str, bool, &[String], &PdfJobExecutionControl) -> Result<(), String>,
    R: Fn(&str) -> Result<(), String>,
    A: Fn(&Path, &Path, PdfAProfile, usize, &PdfJobExecutionControl) -> Result<(), String>,
    AR: Fn(&PdfJobExecutionControl) -> Result<(), String>,
{
    control.checkpoint(2, "Validating batch recipe")?;
    let validated = validate_request(request)?;
    if validated.options.recognise_text {
        ocr_readiness(&validated.options.ocr_language)?;
    }
    if validated.options.archive_profile.is_some() {
        archive_readiness(control)?;
    }
    let workspace = BatchWorkspace::new(&validated.output_directory)?;
    let input_count = validated.inputs.len();
    let mut items = Vec::with_capacity(input_count);
    let mut prepared_outputs = Vec::with_capacity(input_count);

    for (index, input) in validated.inputs.iter().enumerate() {
        control.ensure_not_cancelled()?;
        verify_source_fingerprint(input)?;
        let item_start = batch_progress(5, 94, index, input_count);
        let item_end = batch_progress(5, 94, index + 1, input_count);
        let item_label = format!("File {} of {input_count}", index + 1);
        let mut item = BatchRecipeItemResult {
            source_file_name: input.source_file_name.clone(),
            output_path: None,
            page_count: input.expected_page_count,
            bytes_written: 0,
            privacy_structures_removed: 0,
            images_recompressed: 0,
            searchable_text_pages: 0,
            steps_applied: Vec::new(),
            skipped_reason: None,
            note: None,
            warnings: Vec::new(),
        };
        let mut current_input = input.input.clone();
        let mut current_password = input
            .source_encrypted
            .then(|| input.input_password.clone().unwrap_or_default());
        let mut final_staged_path = None;
        let mut expected_pages_without_text = None;
        let step_count = usize::from(validated.options.recognise_text)
            + usize::from(validated.options.clean_privacy)
            + usize::from(validated.options.compress)
            + usize::from(validated.options.archive_profile.is_some())
            + usize::from(validated.output_protection.is_some());
        let mut step_index = 0_usize;

        if validated.options.recognise_text {
            ensure_pdf_rewrite_acknowledged(
                &input.input.to_string_lossy(),
                input.input_password.as_deref(),
                input.acknowledge_certificate_signatures,
            )?;
            let source_needs_unlock = current_password.is_some();
            let ocr_step_control = batch_step_control(
                control,
                item_start,
                item_end,
                step_index,
                step_count,
                &item_label,
                "searchable OCR",
            );
            if let Some(password) = current_password.as_deref() {
                let unlocked_path = workspace.path.join(format!("item-{index}-unlocked.pdf"));
                let unlock_control =
                    ocr_step_control.subrange(0, 15, "Preparing protected source".to_string());
                decrypt_pdf_to_path_with_control(
                    &current_input,
                    &unlocked_path,
                    password,
                    &unlock_control,
                )
                .map_err(|error| {
                    if error == PDF_JOB_CANCELLED_ERROR {
                        error
                    } else {
                        format!(
                            "{} could not be prepared for OCR: {error}",
                            item.source_file_name
                        )
                    }
                })?;
                current_input = unlocked_path;
            }

            let stage_path = workspace.path.join(format!("item-{index}-ocr.pdf"));
            let stage_control = ocr_step_control.subrange(
                if source_needs_unlock { 15 } else { 0 },
                100,
                "Recognising text".to_string(),
            );
            ocr_runner(
                &current_input,
                &stage_path,
                &validated.options.ocr_language,
                validated.options.straighten,
                &[],
                &stage_control,
            )
            .map_err(|error| {
                if error == PDF_JOB_CANCELLED_ERROR {
                    error
                } else {
                    format!(
                        "{} could not be OCR-processed: {error}",
                        item.source_file_name
                    )
                }
            })?;
            let pages_without_text =
                verify_batch_ocr_output(&stage_path, input.expected_page_count)?;
            item.searchable_text_pages = input
                .expected_page_count
                .saturating_sub(pages_without_text.len());
            if !pages_without_text.is_empty() {
                item.warnings.push(format!(
                    "OCR completed, but {} contain no searchable text. Review blank or low-confidence pages.",
                    summarise_pages(&pages_without_text)
                ));
            }
            expected_pages_without_text = Some(pages_without_text);
            item.steps_applied.push("Searchable OCR".to_string());
            if validated.options.straighten {
                item.steps_applied.push("Deskew".to_string());
            }
            item.bytes_written = file_size(&stage_path)?;
            current_input = stage_path.clone();
            current_password = None;
            final_staged_path = Some(stage_path);
            step_index += 1;
        }

        if validated.options.clean_privacy {
            let stage_path = workspace.path.join(format!("item-{index}-privacy.pdf"));
            let stage_control = batch_step_control(
                control,
                item_start,
                item_end,
                step_index,
                step_count,
                &item_label,
                "privacy cleaning",
            );
            let (expected_source_size, expected_source_modified_at_ms) =
                file_fingerprint(&current_input)?;
            let result = clean_pdf_privacy_with_control(
                CleanPdfPrivacyRequest {
                    expected_source_modified_at_ms,
                    expected_source_size,
                    input_path: current_input.to_string_lossy().into_owned(),
                    output_path: stage_path.to_string_lossy().into_owned(),
                    input_password: current_password.clone(),
                    options: validated.options.privacy_options,
                    acknowledge_certificate_signatures: input.acknowledge_certificate_signatures,
                    output_protection: None,
                },
                &stage_control,
            )
            .map_err(|error| {
                if error == PDF_JOB_CANCELLED_ERROR {
                    error
                } else {
                    format!(
                        "{} could not be privacy-cleaned: {error}",
                        item.source_file_name
                    )
                }
            })?;
            item.page_count = result.page_count;
            item.bytes_written = result.bytes_written;
            item.privacy_structures_removed = result.metadata_structures_removed
                + result.active_content_structures_removed
                + result.attachment_structures_removed
                + result.annotation_structures_removed
                + result.thumbnail_structures_removed
                + result.web_capture_structures_removed;
            item.steps_applied.push("Privacy cleaning".to_string());
            item.warnings.extend(result.warnings);
            current_input = stage_path.clone();
            current_password = None;
            final_staged_path = Some(stage_path);
            step_index += 1;
        }

        if validated.options.compress {
            let stage_path = workspace.path.join(format!("item-{index}-compressed.pdf"));
            let stage_control = batch_step_control(
                control,
                item_start,
                item_end,
                step_index,
                step_count,
                &item_label,
                "compression",
            );
            match export_compressed_pdf_with_control(
                ExportCompressedPdfRequest {
                    input_path: current_input.to_string_lossy().into_owned(),
                    output_path: stage_path.to_string_lossy().into_owned(),
                    input_password: current_password.clone(),
                    jpeg_quality: validated.options.jpeg_quality,
                    acknowledge_certificate_signatures: input.acknowledge_certificate_signatures,
                    output_protection: None,
                },
                &stage_control,
            ) {
                Ok(result) => {
                    item.page_count = result.page_count;
                    item.bytes_written = result.bytes_written;
                    item.images_recompressed = result.images_recompressed;
                    item.steps_applied.push("Compression".to_string());
                    item.warnings.extend(result.warnings);
                    current_input = stage_path.clone();
                    current_password = None;
                    final_staged_path = Some(stage_path);
                }
                Err(error) if error == COMPRESSION_NOT_SMALLER_ERROR => {
                    if final_staged_path.is_some() {
                        item.note = Some(
                            "The prepared copy was already efficient at this quality, so the preceding recipe steps were published without an additional compression rewrite."
                                .to_string(),
                        );
                    } else if validated.options.archive_profile.is_some() {
                        item.note = Some(
                            "The source was already efficient at this quality, so PDF/A conversion continued without an additional compression rewrite."
                                .to_string(),
                        );
                    } else if validated.output_protection.is_some() {
                        item.note = Some(
                            "The source was already efficient at this quality, so output protection was applied without an additional compression rewrite."
                                .to_string(),
                        );
                    } else {
                        item.skipped_reason = Some(
                            "The source was already efficient at this quality; no duplicate copy was created."
                                .to_string(),
                        );
                        final_staged_path = None;
                    }
                }
                Err(error) if error == PDF_JOB_CANCELLED_ERROR => return Err(error),
                Err(error) => {
                    return Err(format!(
                        "{} could not be compressed: {error}",
                        item.source_file_name
                    ));
                }
            }
            step_index += 1;
        }

        if let Some(profile) = validated.options.archive_profile {
            ensure_pdf_rewrite_acknowledged(
                &input.input.to_string_lossy(),
                input.input_password.as_deref(),
                input.acknowledge_certificate_signatures,
            )?;
            let stage_path = workspace.path.join(format!("item-{index}-archive.pdf"));
            let archive_step_control = batch_step_control(
                control,
                item_start,
                item_end,
                step_index,
                step_count,
                &item_label,
                profile.label(),
            );
            if let Some(password) = current_password.as_deref() {
                let unlocked_path = workspace
                    .path
                    .join(format!("item-{index}-archive-unlocked.pdf"));
                decrypt_pdf_to_path_with_control(
                    &current_input,
                    &unlocked_path,
                    password,
                    &archive_step_control.subrange(
                        0,
                        15,
                        "Preparing protected archive source".to_string(),
                    ),
                )?;
                current_input = unlocked_path;
                current_password = None;
            }
            archive_runner(
                &current_input,
                &stage_path,
                profile,
                input.expected_page_count,
                &archive_step_control.subrange(15, 100, "Converting and validating".to_string()),
            )
            .map_err(|error| {
                if error == PDF_JOB_CANCELLED_ERROR {
                    error
                } else {
                    format!(
                        "{} could not be archived as {}: {error}",
                        item.source_file_name,
                        profile.label()
                    )
                }
            })?;
            item.steps_applied.push(profile.label().to_string());
            item.bytes_written = file_size(&stage_path)?;
            item.skipped_reason = None;
            current_input = stage_path.clone();
            final_staged_path = Some(stage_path);
            step_index += 1;
        }

        if let Some(expected) = expected_pages_without_text.as_ref() {
            let observed = verify_batch_ocr_output(&current_input, input.expected_page_count)?;
            if &observed != expected {
                return Err(format!(
                    "{} changed its verified searchable-text coverage during later recipe steps.",
                    item.source_file_name
                ));
            }
        }

        if let Some(protection) = validated.output_protection.as_ref() {
            let stage_path = workspace.path.join(format!("item-{index}-protected.pdf"));
            let stage_control = batch_step_control(
                control,
                item_start,
                item_end,
                step_index,
                step_count,
                &item_label,
                "AES-256 protection",
            );
            lock_pdf_changes_from_source_with_control(
                &current_input,
                current_password.as_deref(),
                &stage_path,
                &protection.open_password,
                &protection.owner_password,
                &stage_control,
            )
            .map_err(|error| {
                if error == PDF_JOB_CANCELLED_ERROR {
                    error
                } else {
                    format!("{} could not be protected: {error}", item.source_file_name)
                }
            })?;
            verify_protected_batch_output(
                &stage_path,
                &protection.open_password,
                input.expected_page_count,
                expected_pages_without_text.as_deref(),
            )?;
            item.steps_applied.push("AES-256 protection".to_string());
            item.bytes_written = file_size(&stage_path)?;
            final_staged_path = Some(stage_path);
        }

        if let Some(staged_path) = final_staged_path {
            item.output_path = Some(input.output.to_string_lossy().into_owned());
            prepared_outputs.push(PreparedOutput {
                item_index: items.len(),
                staged_path,
                destination: input.output.clone(),
            });
        }
        items.push(item);
    }

    control.checkpoint(95, "Rechecking batch sources")?;
    for input in &validated.inputs {
        verify_source_fingerprint(input)?;
    }
    control.checkpoint(97, "Publishing the verified batch")?;
    let mut published_paths = Vec::with_capacity(prepared_outputs.len());
    for prepared in prepared_outputs {
        match publish_prepared_file(&prepared.staged_path, &prepared.destination) {
            Ok(bytes_written) => {
                items[prepared.item_index].bytes_written = bytes_written;
                published_paths.push(prepared.destination);
            }
            Err(error) => {
                for path in published_paths {
                    let _ = fs::remove_file(path);
                }
                return Err(format!(
                    "The batch was prepared but could not be published completely: {error}"
                ));
            }
        }
    }

    let output_count = items
        .iter()
        .filter(|item| item.output_path.is_some())
        .count();
    let skipped_count = items.len().saturating_sub(output_count);
    let bytes_written = items.iter().map(|item| item.bytes_written).sum();
    let encryption = if validated.output_protection.is_some() && output_count > 0 {
        "AES-256"
    } else {
        "None"
    };
    Ok(RunBatchRecipeResult {
        output_directory: validated.output_directory.to_string_lossy().into_owned(),
        input_count,
        output_count,
        skipped_count,
        bytes_written,
        encryption,
        items,
    })
}

fn run_batch_pdfa_candidate(
    input: &Path,
    output: &Path,
    profile: PdfAProfile,
    expected_page_count: usize,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    convert_pdfa_candidate_with_control(input, output, profile, expected_page_count, control)
        .map(|_| ())
}

pub(crate) fn run_batch_recipe_job_with_control(
    request: RunBatchRecipeRequest,
    control: &PdfJobExecutionControl,
) -> Result<RunBatchRecipeResult, String> {
    run_batch_recipe_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_batch_job_error(&error)
        }
    })
}

pub(crate) fn validate_searchable_ocr_request(
    request: &SearchableOcrRequest,
) -> Result<(), String> {
    validate_ocr_language(&request.language)?;
    if let Some(password) = request.input_password.as_deref() {
        validate_password("Source PDF password", password, true)?;
    }
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    Ok(())
}

pub(crate) fn run_searchable_ocr_job_with_control(
    request: SearchableOcrRequest,
    control: &PdfJobExecutionControl,
) -> Result<SearchableOcrResult, String> {
    run_searchable_ocr_with_control_and_runner(request, control, run_batch_recipe_with_control)
        .map_err(|error| {
            if error == PDF_JOB_CANCELLED_ERROR {
                error
            } else {
                safe_searchable_ocr_job_error(&error)
            }
        })
}

fn run_searchable_ocr_with_control_and_runner<B>(
    request: SearchableOcrRequest,
    control: &PdfJobExecutionControl,
    batch_runner: B,
) -> Result<SearchableOcrResult, String>
where
    B: FnOnce(
        RunBatchRecipeRequest,
        &PdfJobExecutionControl,
    ) -> Result<RunBatchRecipeResult, String>,
{
    control.checkpoint(2, "Validating searchable OCR request")?;
    validate_searchable_ocr_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    let source_metadata = fs::metadata(&paths.input)
        .map_err(|error| format!("The OCR source PDF could not be inspected: {error}"))?;
    control.checkpoint(5, "Inspecting source PDF structure")?;
    let source_inspection = inspect_batch_pdf(&paths.input, request.input_password.as_deref())?;
    let output_directory = paths
        .output
        .parent()
        .ok_or_else(|| "The OCR destination folder is invalid.".to_string())?;
    let output_file_name = paths
        .output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Choose a valid OCR destination filename.".to_string())?;
    let source_was_encrypted = source_inspection.encrypted;
    let certificate_acknowledged = request.acknowledge_certificate_signatures;
    let language = request.language.clone();
    let deskew_requested = request.straighten;

    let batch_request = RunBatchRecipeRequest {
        inputs: vec![BatchRecipeInput {
            input_path: paths.input.to_string_lossy().into_owned(),
            input_password: request.input_password,
            expected_source_size: source_metadata.len(),
            expected_source_modified_at_ms: modified_at_ms(&source_metadata),
            expected_page_count: source_inspection.page_count,
            output_file_name: output_file_name.to_string(),
            acknowledge_certificate_signatures: certificate_acknowledged,
        }],
        output_directory: output_directory.to_string_lossy().into_owned(),
        options: BatchRecipeOptions {
            clean_privacy: false,
            privacy_options: PrivacyCleanOptions {
                remove_metadata: false,
                remove_active_content: false,
                remove_attachments: false,
                remove_annotations_and_forms: false,
                remove_thumbnails: false,
            },
            compress: false,
            jpeg_quality: 78,
            recognise_text: true,
            straighten: request.straighten,
            ocr_language: request.language,
            archive_profile: None,
        },
        output_protection: request.output_protection,
    };

    let batch_control = control.subrange(6, 98, "Searchable OCR".to_string());
    let batch_result = batch_runner(batch_request, &batch_control)?;
    if batch_result.input_count != 1
        || batch_result.output_count != 1
        || batch_result.skipped_count != 0
        || batch_result.items.len() != 1
    {
        return Err(
            "The searchable OCR job did not publish exactly one verified output.".to_string(),
        );
    }
    let encryption = batch_result.encryption;
    let mut item = batch_result
        .items
        .into_iter()
        .next()
        .ok_or_else(|| "The searchable OCR result was missing.".to_string())?;
    let output_path = item
        .output_path
        .take()
        .ok_or_else(|| "The searchable OCR output was not published.".to_string())?;
    if source_was_encrypted && encryption == "None" {
        item.warnings.push(
            "The searchable copy is not password-protected. Enable output protection to keep an opening password."
                .to_string(),
        );
    }
    if certificate_acknowledged {
        item.warnings.push(
            "Searchable OCR rewrote this PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }
    if encryption == "AES-256" {
        item.warnings.push(
            "The searchable copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    }
    Ok(SearchableOcrResult {
        output_path,
        page_count: item.page_count,
        searchable_text_pages: item.searchable_text_pages,
        pages_without_searchable_text: item.page_count.saturating_sub(item.searchable_text_pages),
        bytes_written: item.bytes_written,
        language,
        deskew_requested,
        encryption,
        warnings: item.warnings,
    })
}

fn safe_searchable_ocr_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed after")
        || normalised.contains("changed before")
        || normalised.contains("changed on disk")
    {
        return "The source PDF changed during OCR. Choose it again before creating a searchable copy."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The source PDF could not be opened with the supplied password.".to_string();
    }
    if normalised.contains("certificate") || normalised.contains("signed") {
        return "Review and acknowledge the existing certificate signatures before running OCR."
            .to_string();
    }
    if normalised.contains("qpdf")
        || normalised.contains("aes-256")
        || normalised.contains("protected")
    {
        return "AES-256 OCR output could not be completed. Check the local QPDF installation and try again."
            .to_string();
    }
    if normalised.contains("ocr")
        || normalised.contains("tesseract")
        || normalised.contains("language pack")
    {
        return "Searchable OCR could not complete. Check OCRmyPDF, Tesseract, the selected language pack, and the source PDF."
            .to_string();
    }
    "Searchable OCR failed a bounded PDF structure or publication check. Review the source and try again."
        .to_string()
}

pub(crate) fn validate_batch_recipe_request(request: &RunBatchRecipeRequest) -> Result<(), String> {
    if request.inputs.is_empty() {
        return Err("Choose at least one PDF for the batch recipe.".to_string());
    }
    if request.inputs.len() > MAX_BATCH_INPUTS {
        return Err(format!(
            "A batch recipe may contain at most {MAX_BATCH_INPUTS} PDFs."
        ));
    }
    if request.options.straighten && !request.options.recognise_text {
        return Err("Enable searchable OCR before deskewing scanned pages.".to_string());
    }
    if !request.options.recognise_text
        && !request.options.clean_privacy
        && !request.options.compress
        && request.options.archive_profile.is_none()
    {
        return Err(
            "Enable searchable OCR, privacy cleaning, compression, PDF/A conversion, or a combination."
                .to_string(),
        );
    }
    if request.options.recognise_text {
        validate_ocr_language(&request.options.ocr_language)?;
    }
    if request.options.clean_privacy
        && !(request.options.privacy_options.remove_metadata
            || request.options.privacy_options.remove_active_content
            || request.options.privacy_options.remove_attachments
            || request.options.privacy_options.remove_annotations_and_forms
            || request.options.privacy_options.remove_thumbnails)
    {
        return Err("Select at least one privacy category for the recipe.".to_string());
    }
    if request.options.compress
        && !(MIN_JPEG_QUALITY..=MAX_JPEG_QUALITY).contains(&request.options.jpeg_quality)
    {
        return Err(format!(
            "Batch image quality must be between {MIN_JPEG_QUALITY} and {MAX_JPEG_QUALITY}."
        ));
    }
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    if request.options.archive_profile.is_some() && request.output_protection.is_some() {
        return Err(
            "PDF/A output cannot be encrypted. Disable batch output protection before archiving."
                .to_string(),
        );
    }
    reject_control_characters("Output directory", &request.output_directory)?;
    for input in &request.inputs {
        reject_control_characters("Batch source path", &input.input_path)?;
        validate_output_file_name(&input.output_file_name)?;
        if input.expected_page_count == 0 || input.expected_page_count > MAX_BATCH_PAGE_COUNT {
            return Err(format!(
                "A batch source must contain between 1 and {MAX_BATCH_PAGE_COUNT} pages."
            ));
        }
        if input
            .input_password
            .as_deref()
            .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
        {
            return Err(format!(
                "A batch source password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
            ));
        }
    }
    Ok(())
}

fn validate_request(request: RunBatchRecipeRequest) -> Result<ValidatedBatch, String> {
    validate_batch_recipe_request(&request)?;
    let output_directory = fs::canonicalize(&request.output_directory)
        .map_err(|error| format!("The batch destination folder could not be opened: {error}"))?;
    if !output_directory.is_dir() {
        return Err("Choose an existing folder for batch output.".to_string());
    }

    let mut inputs = Vec::with_capacity(request.inputs.len());
    let mut source_keys = HashSet::new();
    let mut output_keys = HashSet::new();
    let mut total_source_bytes = 0_u64;
    for input in request.inputs {
        let canonical_input = canonical_pdf_input(&input.input_path)?;
        let metadata = fs::metadata(&canonical_input)
            .map_err(|error| format!("A batch source could not be inspected: {error}"))?;
        if metadata.len() != input.expected_source_size
            || modified_at_ms(&metadata) != input.expected_source_modified_at_ms
        {
            return Err(format!(
                "{} changed after batch inspection. Inspect the batch again.",
                source_file_name(&canonical_input)
            ));
        }
        let source_inspection =
            inspect_batch_pdf(&canonical_input, input.input_password.as_deref())?;
        if source_inspection.page_count != input.expected_page_count {
            return Err(format!(
                "{} no longer matches its inspected page count. Inspect the batch again.",
                source_file_name(&canonical_input)
            ));
        }
        total_source_bytes = total_source_bytes.saturating_add(metadata.len());
        if total_source_bytes > MAX_BATCH_TOTAL_SOURCE_BYTES {
            return Err(format!(
                "The selected PDFs exceed the {} GiB batch source limit.",
                MAX_BATCH_TOTAL_SOURCE_BYTES / (1024 * 1024 * 1024)
            ));
        }

        validate_output_file_name(&input.output_file_name)?;
        let requested_output = output_directory.join(&input.output_file_name);
        let output = validated_new_pdf_output(&requested_output.to_string_lossy())?;
        if paths_are_equal(&canonical_input, &output) {
            return Err("A batch output cannot overwrite its source PDF.".to_string());
        }
        let source_key = path_key(&canonical_input);
        if !source_keys.insert(source_key) {
            return Err("Each source PDF may appear only once in a batch recipe.".to_string());
        }
        let output_key = path_key(&output);
        if !output_keys.insert(output_key) {
            return Err("Every batch output filename must be unique.".to_string());
        }

        inputs.push(ValidatedBatchInput {
            source_file_name: source_file_name(&canonical_input),
            input: canonical_input,
            input_password: input.input_password,
            output,
            expected_source_size: input.expected_source_size,
            expected_source_modified_at_ms: input.expected_source_modified_at_ms,
            expected_page_count: input.expected_page_count,
            source_encrypted: source_inspection.encrypted,
            acknowledge_certificate_signatures: input.acknowledge_certificate_signatures,
        });
    }

    Ok(ValidatedBatch {
        inputs,
        output_directory,
        options: request.options,
        output_protection: request.output_protection,
    })
}

fn safe_batch_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed before processing") || normalised.contains("changed on disk") {
        return "A batch source changed after review. Inspect all batch sources again before running the recipe."
            .to_string();
    }
    if normalised.contains("verapdf")
        || normalised.contains("pdf/a")
        || normalised.contains("archived")
    {
        return "PDF/A batch conversion or validation could not complete. Check OCRmyPDF, Ghostscript, veraPDF, and the selected source PDFs."
            .to_string();
    }
    if normalised.contains("ocrmypdf")
        || normalised.contains("tesseract")
        || normalised.contains("searchable ocr")
        || normalised.contains("ocr-processed")
    {
        return "Searchable batch OCR could not complete. Check OCRmyPDF, Tesseract, the selected language pack, and the source PDFs."
            .to_string();
    }
    if normalised.contains("qpdf")
        || normalised.contains("aes-256")
        || normalised.contains("protected cleaned")
        || normalised.contains("protected compressed")
    {
        return "AES-256 batch protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "A batch source could not be opened with its supplied password.".to_string();
    }
    "The batch recipe failed a structural safety check. Inspect the sources and try again."
        .to_string()
}

fn verify_source_fingerprint(input: &ValidatedBatchInput) -> Result<(), String> {
    let metadata = fs::metadata(&input.input)
        .map_err(|error| format!("A batch source could not be inspected: {error}"))?;
    if metadata.len() != input.expected_source_size
        || modified_at_ms(&metadata) != input.expected_source_modified_at_ms
    {
        return Err(format!(
            "{} changed before processing. No batch outputs were published.",
            input.source_file_name
        ));
    }
    Ok(())
}

fn validate_output_file_name(file_name: &str) -> Result<(), String> {
    reject_control_characters("Batch output filename", file_name)?;
    if file_name.is_empty() || file_name.len() > MAX_OUTPUT_FILENAME_BYTES {
        return Err(format!(
            "Each batch output filename must contain 1 to {MAX_OUTPUT_FILENAME_BYTES} UTF-8 bytes."
        ));
    }
    let path = Path::new(file_name);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err(
            "Each batch output must be a plain filename ending in .pdf, without folders."
                .to_string(),
        );
    }
    Ok(())
}

fn batch_progress(start: u8, end: u8, completed: usize, total: usize) -> u8 {
    if total == 0 || end <= start {
        return end;
    }
    start.saturating_add(
        (((end - start) as u128 * completed.min(total) as u128) / total as u128) as u8,
    )
}

fn batch_step_control(
    control: &PdfJobExecutionControl,
    item_start: u8,
    item_end: u8,
    step_index: usize,
    step_count: usize,
    item_label: &str,
    step_label: &str,
) -> PdfJobExecutionControl {
    let start = batch_progress(item_start, item_end, step_index, step_count);
    let end = batch_progress(item_start, item_end, step_index + 1, step_count);
    control.subrange(start, end, format!("{item_label}, {step_label}"))
}

struct BatchPdfInspection {
    encrypted: bool,
    page_count: usize,
}

fn inspect_batch_pdf(path: &Path, password: Option<&str>) -> Result<BatchPdfInspection, String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|_| "A batch source is not a readable PDF.".to_string())?;
    let encrypted = document.is_encrypted();
    if encrypted {
        document
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "A batch source could not be opened with its supplied password.".to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 || page_count > MAX_BATCH_PAGE_COUNT {
        return Err(format!(
            "A batch source must contain between 1 and {MAX_BATCH_PAGE_COUNT} pages."
        ));
    }
    Ok(BatchPdfInspection {
        encrypted,
        page_count,
    })
}

fn verify_batch_ocr_output(path: &Path, expected_page_count: usize) -> Result<Vec<u32>, String> {
    let inspection = inspect_batch_pdf(path, None)
        .map_err(|_| "The OCR output could not be reopened safely.".to_string())?;
    if inspection.encrypted || inspection.page_count != expected_page_count {
        return Err("The OCR output did not preserve the reviewed PDF page structure.".to_string());
    }
    inspect_searchable_text_pages(path)
}

fn verify_protected_batch_output(
    path: &Path,
    opening_password: &str,
    expected_page_count: usize,
    expected_pages_without_text: Option<&[u32]>,
) -> Result<(), String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|_| "The protected batch output could not be reopened safely.".to_string())?;
    if !document.is_encrypted() {
        return Err("The protected batch output does not contain PDF encryption.".to_string());
    }
    document.decrypt(opening_password).map_err(|_| {
        "The protected batch output could not be decrypted for verification.".to_string()
    })?;
    if document.get_pages().len() != expected_page_count {
        return Err(
            "The protected batch output did not preserve the reviewed page count.".to_string(),
        );
    }
    if let Some(expected) = expected_pages_without_text {
        let observed = inspect_searchable_text_pages_in_document(&mut document)?;
        if observed != expected {
            return Err(
                "The protected batch output changed its verified searchable-text coverage."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn file_fingerprint(path: &Path) -> Result<(u64, Option<u64>), String> {
    let metadata = fs::metadata(path)
        .map_err(|_| "A prepared batch stage could not be inspected.".to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("A prepared batch stage is missing or empty.".to_string());
    }
    Ok((metadata.len(), modified_at_ms(&metadata)))
}

fn file_size(path: &Path) -> Result<u64, String> {
    file_fingerprint(path).map(|(size, _)| size)
}

fn summarise_pages(pages: &[u32]) -> String {
    let shown = pages
        .iter()
        .take(12)
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if pages.len() > 12 {
        format!(" and {} more", pages.len() - 12)
    } else {
        String::new()
    };
    format!(
        "{} {shown}{suffix}",
        if pages.len() == 1 { "page" } else { "pages" }
    )
}

fn source_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.pdf")
        .to_string()
}

fn modified_at_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

fn path_key(path: &Path) -> String {
    if cfg!(windows) {
        path.to_string_lossy().to_lowercase()
    } else {
        path.to_string_lossy().into_owned()
    }
}

struct BatchWorkspace {
    path: PathBuf,
    _lease: TemporaryLease,
}

impl BatchWorkspace {
    fn new(parent: &Path) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("The system clock is invalid: {error}"))?
            .as_nanos();
        for attempt in 0..16_u8 {
            let path = parent.join(format!(
                ".paperworks-batch-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            if fs::symlink_metadata(&path).is_ok() {
                continue;
            }
            let mut lease = register_temporary_path(&path, TemporaryKind::BatchDirectory)?;
            match fs::create_dir(lease.path()) {
                Ok(()) => {
                    lease.write_directory_ownership_token()?;
                    return Ok(Self {
                        path: lease.path().to_path_buf(),
                        _lease: lease,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    lease.cancel_without_target_cleanup();
                    continue;
                }
                Err(error) => {
                    lease.cancel_without_target_cleanup();
                    return Err(format!(
                        "The isolated batch workspace could not be created: {error}"
                    ));
                }
            }
        }
        Err("A unique isolated batch workspace could not be created.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan_export::{create_scan_pdf, test_scan_pdf_request};
    use crate::test_support::create_unique_test_directory;
    use lopdf::{dictionary, Document, Object, Stream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn batch_source_review_retains_ordered_successes_and_content_free_failures() {
        let directory = TestDirectory::new();
        let first = directory.path.join("first.pdf");
        let private_failure = directory.path.join("private-client-failure.pdf");
        save_fixture(&first, "First Author");
        fs::write(&private_failure, b"private malformed content").unwrap();

        let result = run_batch_source_inspection_job_with_control(
            InspectBatchSourcesRequest {
                sources: vec![
                    InspectPdfPrivacyRequest {
                        input_path: first.to_string_lossy().into_owned(),
                        input_password: None,
                    },
                    InspectPdfPrivacyRequest {
                        input_path: private_failure.to_string_lossy().into_owned(),
                        input_password: None,
                    },
                ],
            },
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();

        assert_eq!(result.source_count, 2);
        assert_eq!(result.inspected_count, 1);
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.items[0].source_index, 0);
        assert!(result.items[0].inspection.is_some());
        assert!(result.items[0].error.is_none());
        assert_eq!(result.items[1].source_index, 1);
        assert!(result.items[1].inspection.is_none());
        assert_eq!(
            result.items[1].error.as_deref(),
            Some(
                "Privacy Inspection could not complete its bounded structure and page analysis. Review the PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&result).unwrap();
        assert!(!serialised.contains("private-client-failure.pdf"));
        assert!(!serialised.contains("private malformed content"));
    }

    #[test]
    fn batch_source_review_reports_per_pdf_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let first = directory.path.join("first.pdf");
        let second = directory.path.join("second.pdf");
        save_fixture(&first, "First Author");
        save_fixture(&second, "Second Author");
        let request = || InspectBatchSourcesRequest {
            sources: vec![
                InspectPdfPrivacyRequest {
                    input_path: first.to_string_lossy().into_owned(),
                    input_password: None,
                },
                InspectPdfPrivacyRequest {
                    input_path: second.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ],
        };
        let stages = Arc::new(Mutex::new(Vec::new()));
        let stages_for_progress = Arc::clone(&stages);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            stages_for_progress.lock().unwrap().push((value, stage));
        });

        let result = run_batch_source_inspection_job_with_control(
            request(),
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap();

        assert_eq!(result.inspected_count, 2);
        let stages = stages.lock().unwrap();
        assert!(stages
            .iter()
            .any(|(_, stage)| stage.starts_with("PDF 1 of 2:")));
        assert!(stages
            .iter()
            .any(|(_, stage)| stage.starts_with("PDF 2 of 2:")));
        drop(stages);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "PDF 1 of 2: Checking the privacy-inspection request" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = run_batch_source_inspection_job_with_control(
            request(),
            &PdfJobExecutionControl::new(cancelled, progress),
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn batch_source_review_rejects_duplicate_and_over_limit_requests() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Author");
        let source = InspectPdfPrivacyRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        };

        let duplicate_error = validate_inspect_batch_sources_request(&InspectBatchSourcesRequest {
            sources: vec![source.clone(), source.clone()],
        })
        .unwrap_err();
        let over_limit_error =
            validate_inspect_batch_sources_request(&InspectBatchSourcesRequest {
                sources: vec![source; MAX_BATCH_INPUTS + 1],
            })
            .unwrap_err();

        assert!(duplicate_error.contains("only once"));
        assert!(over_limit_error.contains("50"));
    }

    #[test]
    fn privacy_batch_prepares_every_file_before_publishing() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let first = directory.path.join("first.pdf");
        let second = directory.path.join("second.pdf");
        save_fixture(&first, "First Author");
        save_fixture(&second, "Second Author");

        let result = run_batch_recipe_with_control(
            batch_request(
                &[(&first, "first-clean.pdf"), (&second, "second-clean.pdf")],
                &output_directory,
            ),
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();

        assert_eq!(result.input_count, 2);
        assert_eq!(result.output_count, 2);
        assert_eq!(result.skipped_count, 0);
        assert!(result.bytes_written > 0);
        for file_name in ["first-clean.pdf", "second-clean.pdf"] {
            let output = output_directory.join(file_name);
            assert!(output.exists());
            let cleaned = Document::load(output).unwrap();
            assert!(!cleaned.trailer.has(b"Info"));
        }
        assert!(Document::load(first).unwrap().trailer.has(b"Info"));
        assert!(Document::load(second).unwrap().trailer.has(b"Info"));
        assert!(!fs::read_dir(&output_directory).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".paperworks-batch")));
    }

    #[test]
    fn searchable_ocr_batch_publishes_verified_text_and_deskew_steps() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Archive Author");
        let mut request = batch_request(&[(&input, "searchable.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.recognise_text = true;
        request.options.straighten = true;

        let result = run_batch_recipe_with_control_and_engines(
            request,
            &PdfJobExecutionControl::direct(),
            &|source, output, language, straighten, words, control| {
                assert_eq!(language, "eng");
                assert!(straighten);
                assert!(words.is_empty());
                control.checkpoint(50, "Test OCR")?;
                fs::copy(source, output)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            },
            &|_| Ok(()),
            &|_, _, _, _, _| Ok(()),
            &|_| Ok(()),
        )
        .unwrap();

        assert_eq!(result.output_count, 1);
        assert_eq!(result.items[0].searchable_text_pages, 1);
        assert_eq!(result.items[0].steps_applied, ["Searchable OCR", "Deskew"]);
        assert!(output_directory.join("searchable.pdf").exists());
    }

    #[test]
    fn searchable_ocr_batch_reports_textless_pages_without_claiming_coverage() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Archive Author");
        let mut request = batch_request(&[(&input, "searchable.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.recognise_text = true;

        let result = run_batch_recipe_with_control_and_engines(
            request,
            &PdfJobExecutionControl::direct(),
            &|_, output, _, _, _, _| {
                save_fixture_pages(output, "OCR Author", 1, false);
                Ok(())
            },
            &|_| Ok(()),
            &|_, _, _, _, _| Ok(()),
            &|_| Ok(()),
        )
        .unwrap();

        assert_eq!(result.items[0].searchable_text_pages, 0);
        assert!(result.items[0].warnings[0].contains("page 1"));
        assert!(output_directory.join("searchable.pdf").exists());
    }

    #[test]
    fn searchable_ocr_batch_rejects_page_count_changes_before_publication() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Archive Author");
        let mut request = batch_request(&[(&input, "searchable.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.recognise_text = true;

        let error = run_batch_recipe_with_control_and_engines(
            request,
            &PdfJobExecutionControl::direct(),
            &|_, output, _, _, _, _| {
                save_fixture_pages(output, "OCR Author", 2, true);
                Ok(())
            },
            &|_| Ok(()),
            &|_, _, _, _, _| Ok(()),
            &|_| Ok(()),
        )
        .unwrap_err();

        assert!(error.contains("preserve the reviewed PDF page structure"));
        assert!(!output_directory.join("searchable.pdf").exists());
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    #[test]
    fn standalone_searchable_ocr_publishes_one_verified_copy() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("source-searchable.pdf");
        save_fixture(&input, "OCR Author");

        let result = run_searchable_ocr_with_control_and_runner(
            searchable_ocr_request(&input, &output),
            &PdfJobExecutionControl::direct(),
            |request, control| {
                run_batch_recipe_with_control_and_engines(
                    request,
                    control,
                    &|source, destination, language, straighten, words, child_control| {
                        assert_eq!(language, "eng");
                        assert!(straighten);
                        assert!(words.is_empty());
                        child_control.checkpoint(50, "Test standalone OCR")?;
                        fs::copy(source, destination)
                            .map(|_| ())
                            .map_err(|error| error.to_string())
                    },
                    &|language| {
                        assert_eq!(language, "eng");
                        Ok(())
                    },
                    &|_, _, _, _, _| Ok(()),
                    &|_| Ok(()),
                )
            },
        )
        .unwrap();

        assert_eq!(
            PathBuf::from(&result.output_path),
            fs::canonicalize(&output).unwrap()
        );
        assert_eq!(result.page_count, 1);
        assert_eq!(result.searchable_text_pages, 1);
        assert_eq!(result.pages_without_searchable_text, 0);
        assert_eq!(result.language, "eng");
        assert!(result.deskew_requested);
        assert_eq!(result.encryption, "None");
        assert!(result.bytes_written > 0);
        assert!(result.warnings.is_empty());
        assert!(output.is_file());
    }

    #[test]
    fn standalone_searchable_ocr_rejects_overwrite_and_invalid_language() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("source-searchable.pdf");
        save_fixture(&input, "OCR Author");

        let overwrite = searchable_ocr_request(&input, &input);
        let overwrite_error = validate_searchable_ocr_request(&overwrite).unwrap_err();
        assert!(
            overwrite_error.contains("cannot be overwritten")
                || overwrite_error.contains("destination already exists")
        );

        let mut invalid_language = searchable_ocr_request(&input, &output);
        invalid_language.language = "eng;private".to_string();
        assert!(validate_searchable_ocr_request(&invalid_language)
            .unwrap_err()
            .contains("valid installed OCR language"));
        assert!(!output.exists());
    }

    #[test]
    fn standalone_searchable_ocr_job_errors_are_content_free() {
        let error = safe_searchable_ocr_job_error(
            "OCRmyPDF failed for C:\\Clients\\private-record.pdf using secret-language-data",
        );

        assert_eq!(
            error,
            "Searchable OCR could not complete. Check OCRmyPDF, Tesseract, the selected language pack, and the source PDF."
        );
        assert!(!error.contains("Clients"));
        assert!(!error.contains("private-record"));
        assert!(!error.contains("secret-language-data"));
    }

    #[test]
    fn pdfa_batch_publishes_only_the_validated_archive_stage() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Archive Author");
        let mut request = batch_request(&[(&input, "archive.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.archive_profile = Some(PdfAProfile::PdfA2b);

        let result = run_batch_recipe_with_control_and_engines(
            request,
            &PdfJobExecutionControl::direct(),
            &|_, _, _, _, _, _| Ok(()),
            &|_| Ok(()),
            &|source, output, profile, pages, control| {
                assert_eq!(profile, PdfAProfile::PdfA2b);
                assert_eq!(pages, 1);
                control.checkpoint(80, "Test PDF/A validation")?;
                fs::copy(source, output)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            },
            &|_| Ok(()),
        )
        .unwrap();

        assert_eq!(result.output_count, 1);
        assert_eq!(result.items[0].steps_applied, ["PDF/A-2b"]);
        assert!(output_directory.join("archive.pdf").is_file());
    }

    #[test]
    fn pdfa_batch_rejects_incompatible_output_encryption() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Archive Author");
        let mut request = batch_request(&[(&input, "archive.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.archive_profile = Some(PdfAProfile::PdfA2b);
        request.output_protection = Some(PdfOutputProtection {
            open_password: "opening-secret".to_string(),
            owner_password: "administrator-secret".to_string(),
        });

        assert!(validate_batch_recipe_request(&request)
            .unwrap_err()
            .contains("cannot be encrypted"));
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    #[test]
    #[ignore = "requires OCRmyPDF, Tesseract eng data, and PAPERWORKS_OCR_CORPUS"]
    fn live_batch_and_standalone_ocr_corpus_verify_searchable_publication() {
        let corpus = std::env::var_os("PAPERWORKS_OCR_CORPUS")
            .map(PathBuf::from)
            .expect("PAPERWORKS_OCR_CORPUS is required");
        let source_image = corpus.join("english.png");
        assert!(
            source_image.is_file(),
            "the public English OCR fixture is missing"
        );
        let directory = TestDirectory::new();
        let image_pdf = directory.path.join("image-only.pdf");
        create_scan_pdf(test_scan_pdf_request(
            vec![source_image.to_string_lossy().into_owned()],
            image_pdf.to_string_lossy().into_owned(),
        ))
        .unwrap();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let mut request = batch_request(&[(&image_pdf, "searchable.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.recognise_text = true;
        request.options.straighten = true;

        let result =
            run_batch_recipe_with_control(request, &PdfJobExecutionControl::direct()).unwrap();

        assert_eq!(result.output_count, 1);
        assert_eq!(result.items[0].searchable_text_pages, 1);
        assert!(result.items[0].warnings.is_empty());
        assert!(output_directory.join("searchable.pdf").is_file());

        let standalone_output = directory.path.join("standalone-searchable.pdf");
        let standalone = run_searchable_ocr_job_with_control(
            searchable_ocr_request(&image_pdf, &standalone_output),
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();
        assert_eq!(standalone.page_count, 1);
        assert_eq!(standalone.searchable_text_pages, 1);
        assert_eq!(standalone.pages_without_searchable_text, 0);
        assert!(standalone_output.is_file());
    }

    #[test]
    #[ignore = "requires OCRmyPDF, Ghostscript, veraPDF, Tesseract eng data, and PAPERWORKS_OCR_CORPUS"]
    fn live_pdfa_batch_recipe_verifies_compliant_publication() {
        let corpus = std::env::var_os("PAPERWORKS_OCR_CORPUS")
            .map(PathBuf::from)
            .expect("PAPERWORKS_OCR_CORPUS is required");
        let source_image = corpus.join("english.png");
        assert!(
            source_image.is_file(),
            "the public English OCR fixture is missing"
        );
        let directory = TestDirectory::new();
        let image_pdf = directory.path.join("image-only.pdf");
        create_scan_pdf(test_scan_pdf_request(
            vec![source_image.to_string_lossy().into_owned()],
            image_pdf.to_string_lossy().into_owned(),
        ))
        .unwrap();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let mut request = batch_request(&[(&image_pdf, "archive.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.recognise_text = true;
        request.options.straighten = true;
        request.options.archive_profile = Some(PdfAProfile::PdfA2b);

        let result =
            run_batch_recipe_with_control(request, &PdfJobExecutionControl::direct()).unwrap();

        assert_eq!(result.output_count, 1);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.items[0].searchable_text_pages, 1);
        assert_eq!(
            result.items[0].steps_applied,
            ["Searchable OCR", "Deskew", "PDF/A-2b"]
        );
        assert!(output_directory.join("archive.pdf").is_file());
        println!("PAPERWORKS_PDFA_BATCH_V1\tpdfa-2b\t1\t1\tvalidated");
    }

    #[test]
    fn validates_ocr_recipe_dependencies_before_creating_a_workspace() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Archive Author");
        let mut request = batch_request(&[(&input, "searchable.pdf")], &output_directory);
        request.options.clean_privacy = false;
        request.options.straighten = true;
        assert!(validate_batch_recipe_request(&request)
            .unwrap_err()
            .contains("before deskewing"));

        request.options.recognise_text = true;
        request.options.ocr_language = "eng;tur".to_string();
        assert!(validate_batch_recipe_request(&request)
            .unwrap_err()
            .contains("valid installed OCR language"));
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    #[test]
    fn protected_batch_verification_rejects_an_unencrypted_candidate() {
        let directory = TestDirectory::new();
        let candidate = directory.path.join("candidate.pdf");
        save_fixture(&candidate, "Archive Author");
        assert!(
            verify_protected_batch_output(&candidate, "opening-secret", 1, Some(&[]))
                .unwrap_err()
                .contains("does not contain PDF encryption")
        );
    }

    #[test]
    fn cancellation_inside_a_recipe_publishes_no_batch_outputs() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Private Author");
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress_cancelled = Arc::clone(&cancelled);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 5 {
                    progress_cancelled.store(true, Ordering::Release);
                }
            }),
        );

        let error = run_batch_recipe_with_control(
            batch_request(&[(&input, "clean.pdf")], &output_directory),
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output_directory.join("clean.pdf").exists());
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    #[test]
    fn rejects_duplicate_outputs_before_creating_a_workspace() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let first = directory.path.join("first.pdf");
        let second = directory.path.join("second.pdf");
        save_fixture(&first, "First Author");
        save_fixture(&second, "Second Author");

        let error = run_batch_recipe_with_control(
            batch_request(
                &[(&first, "same.pdf"), (&second, "same.pdf")],
                &output_directory,
            ),
            &PdfJobExecutionControl::direct(),
        )
        .unwrap_err();

        assert!(error.contains("output filename must be unique"));
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    #[test]
    fn rejects_invalid_output_passwords_before_creating_a_workspace() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Private Author");
        let mut request = batch_request(&[(&input, "clean.pdf")], &output_directory);
        request.output_protection = Some(PdfOutputProtection {
            open_password: "same-password".to_string(),
            owner_password: "same-password".to_string(),
        });

        let error =
            run_batch_recipe_with_control(request, &PdfJobExecutionControl::direct()).unwrap_err();

        assert!(error.contains("different administrator password"));
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    #[test]
    fn protection_failures_map_to_a_content_free_batch_error() {
        let error = safe_batch_job_error(
            "QPDF could not protect C:\\private\\source.pdf with never-return-this-password",
        );

        assert_eq!(
            error,
            "AES-256 batch protection could not complete. Install QPDF or add it to PATH, then try again."
        );
        assert!(!error.contains("private"));
        assert!(!error.contains("never-return"));
    }

    #[test]
    fn rejects_a_source_changed_after_preparation_before_publishing() {
        let directory = TestDirectory::new();
        let output_directory = directory.path.join("outputs");
        fs::create_dir(&output_directory).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input, "Original Author");
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress >= 90 && !changed_for_progress.swap(true, Ordering::AcqRel) {
                    save_fixture(
                        &input_for_progress,
                        "A deliberately longer author value written after preparation",
                    );
                }
            }),
        );

        let error = run_batch_recipe_with_control(
            batch_request(&[(&input, "clean.pdf")], &output_directory),
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk") || error.contains("changed before processing"));
        assert_eq!(fs::read_dir(output_directory).unwrap().count(), 0);
    }

    fn batch_request(inputs: &[(&Path, &str)], output_directory: &Path) -> RunBatchRecipeRequest {
        RunBatchRecipeRequest {
            inputs: inputs
                .iter()
                .map(|(path, output_file_name)| {
                    let metadata = fs::metadata(path).unwrap();
                    BatchRecipeInput {
                        input_path: path.to_string_lossy().into_owned(),
                        input_password: None,
                        expected_source_size: metadata.len(),
                        expected_source_modified_at_ms: modified_at_ms(&metadata),
                        expected_page_count: 1,
                        output_file_name: (*output_file_name).to_string(),
                        acknowledge_certificate_signatures: false,
                    }
                })
                .collect(),
            output_directory: output_directory.to_string_lossy().into_owned(),
            options: BatchRecipeOptions {
                clean_privacy: true,
                privacy_options: PrivacyCleanOptions {
                    remove_metadata: true,
                    remove_active_content: false,
                    remove_attachments: false,
                    remove_annotations_and_forms: false,
                    remove_thumbnails: false,
                },
                compress: false,
                jpeg_quality: 78,
                recognise_text: false,
                straighten: false,
                ocr_language: "eng".to_string(),
                archive_profile: None,
            },
            output_protection: None,
        }
    }

    fn searchable_ocr_request(input: &Path, output: &Path) -> SearchableOcrRequest {
        SearchableOcrRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            language: "eng".to_string(),
            straighten: true,
            acknowledge_certificate_signatures: false,
            output_protection: None,
        }
    }

    fn save_fixture(path: &Path, author: &str) {
        save_fixture_pages(path, author, 1, true);
    }

    fn save_fixture_pages(path: &Path, author: &str, page_count: usize, with_text: bool) {
        let mut document = fixture_document(author, page_count, with_text);
        document.save(path).unwrap().sync_all().unwrap();
    }

    fn fixture_document(author: &str, page_count: usize, with_text: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let mut page_ids = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            let content = if with_text {
                b"BT /F1 12 Tf 20 30 Td (Visible page) Tj ET".to_vec()
            } else {
                Vec::new()
            };
            let content_id = document.add_object(Stream::new(dictionary! {}, content));
            page_ids.push(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => font_id },
                },
                "Contents" => content_id,
            }));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => page_count as i64,
            }),
        );
        let catalogue_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        let info_id = document.add_object(dictionary! {
            "Author" => Object::string_literal(author),
        });
        document.trailer.set("Root", catalogue_id);
        document.trailer.set("Info", info_id);
        document
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = create_unique_test_directory("tufekci-paperworks-batch-test");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
