use crate::bookmarks::{
    inspect_document_bookmarks_with_control, replace_bookmarks, PdfBookmarkEntry, MAX_BOOKMARKS,
};
use crate::file_safety::{
    canonical_pdf_input, paths_are_equal, reject_control_characters, validated_new_pdf_output,
    TemporaryOutput,
};
use crate::health::{
    document_has_certificate_signature, document_has_certificate_signature_with_control,
    ensure_document_rewrite_acknowledged,
};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use lopdf::{dictionary, Dictionary, Document, LoadOptions, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_COMBINE_SOURCES: usize = 250;
const MAX_OUTPUT_PAGES: usize = 50_000;
const MAX_PAGE_RANGE_LENGTH: usize = 4_096;
const MAX_SPLIT_GROUPS: usize = 250;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CombinePdfSource {
    input_path: String,
    input_password: Option<String>,
    page_range: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CombinePdfRequest {
    sources: Vec<CombinePdfSource>,
    output_path: String,
    acknowledge_certificate_signatures: bool,
    #[serde(default)]
    preserve_bookmarks: bool,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombinePdfResult {
    output_path: String,
    bytes_written: u64,
    page_count: usize,
    bookmark_count: usize,
    omitted_bookmark_count: usize,
    encryption: &'static str,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPdfRequest {
    input_path: String,
    input_password: Option<String>,
    output_directory: String,
    page_groups: Vec<String>,
    acknowledge_certificate_signatures: bool,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPdfOutput {
    output_path: String,
    bytes_written: u64,
    page_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPdfResult {
    outputs: Vec<SplitPdfOutput>,
    total_pages: usize,
    encryption: &'static str,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPageImportRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
    pub(crate) page_range: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageImportInspection {
    certificate_signature: bool,
    encrypted: bool,
    page_count: usize,
    selected_pages: Vec<u32>,
}

struct BuiltPdf {
    page_count: usize,
    bookmarks: Vec<PdfBookmarkEntry>,
    omitted_bookmark_count: usize,
    temporary: TemporaryOutput,
    source_fingerprints: Vec<SourceFingerprint>,
    warnings: Vec<String>,
}

struct SelectedPageDestinations {
    first_output_page: HashMap<u32, u32>,
    repeated_source_pages: HashSet<u32>,
}

struct RemappedBookmarks {
    bookmarks: Vec<PdfBookmarkEntry>,
    omitted: usize,
    repeated_destination: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceFingerprint {
    path: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
}

#[cfg(test)]
pub fn combine_pdf_pages(request: CombinePdfRequest) -> Result<CombinePdfResult, String> {
    combine_pdf_pages_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn combine_pdf_pages_with_control(
    request: CombinePdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<CombinePdfResult, String> {
    control.checkpoint(2, "Validating merge request")?;
    validate_combine_pdf_request(&request)?;
    let output = validated_new_pdf_output(&request.output_path)?;
    let output_will_be_protected = request.output_protection.is_some();
    let build_control = control.subrange(7, 84, "Merge".to_string());
    let mut built = build_combined_pdf(
        &request.sources,
        &output,
        request.acknowledge_certificate_signatures,
        request.preserve_bookmarks,
        output_will_be_protected,
        &build_control,
    )?;
    let protected_output = if let Some(protection) = request.output_protection.as_ref() {
        let protected_output = TemporaryOutput::new(&output)?;
        let protection_control =
            control.subrange(86, 92, "Applying AES-256 output protection".to_string());
        lock_pdf_changes_with_control(
            built.temporary.path(),
            protected_output.path(),
            &protection.open_password,
            &protection.owner_password,
            &protection_control,
        )?;
        let verification_control =
            control.subrange(93, 97, "Verifying protected merge".to_string());
        verify_combined_pdf(
            protected_output.path(),
            built.page_count,
            &built.bookmarks,
            Some(&protection.open_password),
            true,
            &verification_control,
        )?;
        built.warnings.push(
            "The combined copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
        Some(protected_output)
    } else {
        None
    };
    let final_output = protected_output.as_ref().unwrap_or(&built.temporary);

    control.checkpoint(98, "Rechecking merge sources before publication")?;
    verify_source_fingerprints(&built.source_fingerprints)?;
    control.checkpoint(99, "Publishing verified combined PDF")?;
    let bytes_written = final_output.persist(&output)?;

    Ok(CombinePdfResult {
        output_path: output.to_string_lossy().into_owned(),
        bytes_written,
        page_count: built.page_count,
        bookmark_count: built.bookmarks.len(),
        omitted_bookmark_count: built.omitted_bookmark_count,
        encryption: if output_will_be_protected {
            "AES-256"
        } else {
            "None"
        },
        warnings: built.warnings,
    })
}

pub(crate) fn run_pdf_merge_job_with_control(
    request: CombinePdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<CombinePdfResult, String> {
    combine_pdf_pages_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_merge_job_error(&error)
        }
    })
}

#[cfg(test)]
pub fn split_pdf(request: SplitPdfRequest) -> Result<SplitPdfResult, String> {
    split_pdf_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn split_pdf_with_control(
    request: SplitPdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<SplitPdfResult, String> {
    control.checkpoint(2, "Validating split request")?;
    validate_split_pdf_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let output_directory = canonical_output_directory(&request.output_directory)?;
    let source_name = safe_output_stem(&input);
    let output_will_be_protected = request.output_protection.is_some();
    control.checkpoint(6, "Opening split source PDF")?;
    let source_page_count = readable_page_count(&input, request.input_password.as_deref())?;

    for (index, expression) in request.page_groups.iter().enumerate() {
        control.checkpoint(
            progress_between(7, 11, index, request.page_groups.len()),
            "Validating split page groups",
        )?;
        parse_page_range(expression, source_page_count)?;
    }

    let outputs = request
        .page_groups
        .iter()
        .enumerate()
        .map(|(index, _)| {
            validated_new_pdf_output(
                &output_directory
                    .join(format!("{source_name}-part-{:02}.pdf", index + 1))
                    .to_string_lossy(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_outputs(&outputs)?;

    let mut built_outputs = Vec::with_capacity(outputs.len());
    let build_end = if output_will_be_protected { 72 } else { 91 };
    for (index, (expression, output)) in request.page_groups.iter().zip(outputs.iter()).enumerate()
    {
        control.ensure_not_cancelled()?;
        let source = CombinePdfSource {
            input_path: input.to_string_lossy().into_owned(),
            input_password: request.input_password.clone(),
            page_range: Some(expression.clone()),
        };
        let part_control = control.subrange(
            progress_between(12, build_end, index, outputs.len()),
            progress_between(12, build_end, index + 1, outputs.len()),
            format!("Part {} of {}", index + 1, outputs.len()),
        );
        built_outputs.push((
            output.clone(),
            build_combined_pdf(
                &[source],
                output,
                request.acknowledge_certificate_signatures,
                false,
                output_will_be_protected,
                &part_control,
            )?,
        ));
    }

    let protected_outputs = if let Some(protection) = request.output_protection.as_ref() {
        let mut protected_outputs = Vec::with_capacity(built_outputs.len());
        for (index, (output, built)) in built_outputs.iter().enumerate() {
            let part_control = control.subrange(
                progress_between(74, 91, index, built_outputs.len()),
                progress_between(74, 91, index + 1, built_outputs.len()),
                format!("Protecting part {} of {}", index + 1, built_outputs.len()),
            );
            let protected_output = TemporaryOutput::new(output)?;
            let encryption_control = part_control.subrange(0, 64, "Applying AES-256".to_string());
            lock_pdf_changes_with_control(
                built.temporary.path(),
                protected_output.path(),
                &protection.open_password,
                &protection.owner_password,
                &encryption_control,
            )?;
            let verification_control =
                part_control.subrange(65, 99, "Repeating structural verification".to_string());
            verify_combined_pdf(
                protected_output.path(),
                built.page_count,
                &built.bookmarks,
                Some(&protection.open_password),
                true,
                &verification_control,
            )?;
            protected_outputs.push(protected_output);
        }
        control.checkpoint(92, "Every protected split PDF verified")?;
        Some(protected_outputs)
    } else {
        None
    };

    control.checkpoint(93, "Rechecking split source before publication")?;
    for (_, built) in &built_outputs {
        verify_source_fingerprints(&built.source_fingerprints)?;
    }
    control.checkpoint(95, "Publishing verified split PDFs")?;
    let mut published_paths = Vec::new();
    let mut results = Vec::with_capacity(built_outputs.len());
    let mut warnings = Vec::new();
    for (index, (output, built)) in built_outputs.into_iter().enumerate() {
        let final_output = protected_outputs
            .as_ref()
            .and_then(|protected| protected.get(index))
            .unwrap_or(&built.temporary);
        match final_output.persist(&output) {
            Ok(bytes_written) => {
                warnings.extend(built.warnings);
                published_paths.push(output.clone());
                results.push(SplitPdfOutput {
                    output_path: output.to_string_lossy().into_owned(),
                    bytes_written,
                    page_count: built.page_count,
                });
            }
            Err(error) => {
                for published in published_paths {
                    let _ = fs::remove_file(published);
                }
                return Err(format!(
                    "The split PDFs were prepared but could not all be published: {error}"
                ));
            }
        }
    }
    if output_will_be_protected {
        warnings.push(
            "Every split copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    }
    warnings.sort();
    warnings.dedup();

    Ok(SplitPdfResult {
        total_pages: results.iter().map(|output| output.page_count).sum(),
        outputs: results,
        encryption: if output_will_be_protected {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_split_job_with_control(
    request: SplitPdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<SplitPdfResult, String> {
    split_pdf_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_split_job_error(&error)
        }
    })
}

#[cfg(test)]
pub fn inspect_page_import(
    request: InspectPageImportRequest,
) -> Result<PageImportInspection, String> {
    inspect_page_import_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_page_import_request(
    request: &InspectPageImportRequest,
) -> Result<(), String> {
    reject_control_characters("Input path", &request.input_path)?;
    if request
        .input_password
        .as_deref()
        .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
    {
        return Err(format!(
            "The page import password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }
    validate_range_text(&request.page_range)
}

fn inspect_page_import_with_control(
    request: InspectPageImportRequest,
    control: &PdfJobExecutionControl,
) -> Result<PageImportInspection, String> {
    control.checkpoint(2, "Validating page import review")?;
    validate_inspect_page_import_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let source_fingerprint = source_fingerprint(&input)?;
    control.checkpoint(18, "Opening page import source")?;
    let document = load_source_document(&input, request.input_password.as_deref())?;
    let encrypted = document.was_encrypted();
    control.checkpoint(46, "Reading source page tree")?;
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The source PDF does not contain any readable pages.".to_string());
    }
    control.checkpoint(58, "Resolving requested page range")?;
    let selected_pages = parse_page_range(&request.page_range, page_count)?;
    control.checkpoint(70, "Inspecting source signatures")?;
    let certificate_signature =
        document_has_certificate_signature_with_control(&document, control)?;
    control.checkpoint(94, "Rechecking page import source")?;
    verify_page_import_source_fingerprint(&source_fingerprint)?;
    control.checkpoint(99, "Finalising page import review")?;

    Ok(PageImportInspection {
        certificate_signature,
        encrypted,
        page_count,
        selected_pages,
    })
}

pub(crate) fn run_page_import_inspection_job_with_control(
    request: InspectPageImportRequest,
    control: &PdfJobExecutionControl,
) -> Result<PageImportInspection, String> {
    inspect_page_import_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_page_import_inspection_job_error(&error)
        }
    })
}

pub(crate) fn validate_combine_pdf_request(request: &CombinePdfRequest) -> Result<(), String> {
    reject_control_characters("Output path", &request.output_path)?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    validate_sources(&request.sources)
}

pub(crate) fn validate_split_pdf_request(request: &SplitPdfRequest) -> Result<(), String> {
    validate_split_request(request)
}

fn validate_sources(sources: &[CombinePdfSource]) -> Result<(), String> {
    if sources.is_empty() {
        return Err("Choose at least one source PDF.".to_string());
    }
    if sources.len() > MAX_COMBINE_SOURCES {
        return Err(format!(
            "A merge may contain no more than {MAX_COMBINE_SOURCES} source PDFs."
        ));
    }
    for source in sources {
        reject_control_characters("Input path", &source.input_path)?;
        if source
            .input_password
            .as_deref()
            .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
        {
            return Err(format!(
                "A merge source password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
            ));
        }
        if let Some(range) = source.page_range.as_deref() {
            validate_range_text(range)?;
        }
    }
    Ok(())
}

fn validate_split_request(request: &SplitPdfRequest) -> Result<(), String> {
    reject_control_characters("Input path", &request.input_path)?;
    reject_control_characters("Output directory", &request.output_directory)?;
    if request
        .input_password
        .as_deref()
        .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
    {
        return Err(format!(
            "The split source password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    if request.page_groups.is_empty() {
        return Err("Enter at least one page group to split.".to_string());
    }
    if request.page_groups.len() > MAX_SPLIT_GROUPS {
        return Err(format!(
            "A split job may contain no more than {MAX_SPLIT_GROUPS} page groups."
        ));
    }
    for range in &request.page_groups {
        validate_range_text(range)?;
    }
    Ok(())
}

fn validate_range_text(range: &str) -> Result<(), String> {
    reject_control_characters("Page range", range)?;
    if range.len() > MAX_PAGE_RANGE_LENGTH {
        return Err(format!(
            "A page range may contain no more than {MAX_PAGE_RANGE_LENGTH} characters."
        ));
    }
    Ok(())
}

fn canonical_output_directory(path: &str) -> Result<PathBuf, String> {
    let directory = fs::canonicalize(path)
        .map_err(|error| format!("The output folder could not be opened: {error}"))?;
    if !directory.is_dir() {
        return Err("Choose an existing folder for the split PDFs.".to_string());
    }
    Ok(directory)
}

fn ensure_unique_outputs(outputs: &[PathBuf]) -> Result<(), String> {
    for (index, output) in outputs.iter().enumerate() {
        if outputs[..index]
            .iter()
            .any(|previous| paths_are_equal(previous, output))
        {
            return Err("The split job produced duplicate destination filenames.".to_string());
        }
    }
    Ok(())
}

fn readable_page_count(path: &Path, password: Option<&str>) -> Result<usize, String> {
    let mut document = load_source_document(path, password)?;
    let count = document.get_pages().len();
    document.prune_objects();
    if count == 0 {
        return Err("The source PDF does not contain any readable pages.".to_string());
    }
    Ok(count)
}

fn build_combined_pdf(
    sources: &[CombinePdfSource],
    output: &Path,
    acknowledge_certificate_signatures: bool,
    preserve_bookmarks: bool,
    output_will_be_protected: bool,
    control: &PdfJobExecutionControl,
) -> Result<BuiltPdf, String> {
    validate_sources(sources)?;
    control.checkpoint(2, "Preparing combined PDF")?;
    let temporary = TemporaryOutput::new(output)?;
    let mut document = Document::with_version("1.7");
    let pages_root_id = document.new_object_id();
    let mut output_page_ids = Vec::new();
    let mut used_page_ids = HashSet::new();
    let mut source_fingerprints = Vec::with_capacity(sources.len());
    let mut warnings = Vec::new();
    let mut combined_bookmarks = Vec::new();
    let mut omitted_bookmark_count = 0usize;

    for (source_index, source) in sources.iter().enumerate() {
        let source_start = progress_between(5, 72, source_index, sources.len());
        let source_end = progress_between(5, 72, source_index + 1, sources.len());
        control.checkpoint(
            source_start,
            format!("Opening source {} of {}", source_index + 1, sources.len()),
        )?;
        let input = canonical_pdf_input(&source.input_path)?;
        if paths_are_equal(&input, output) {
            return Err("A source PDF cannot be overwritten. Choose a new filename.".to_string());
        }
        source_fingerprints.push(source_fingerprint(&input)?);
        let mut source_document = load_source_document(&input, source.input_password.as_deref())?;
        ensure_document_rewrite_acknowledged(
            &source_document,
            &input,
            acknowledge_certificate_signatures,
        )?;
        let source_was_encrypted = source_document.was_encrypted();
        let source_pages = source_document.get_pages();
        if source_pages.is_empty() {
            return Err(format!(
                "Source {} ({}) does not contain any readable pages.",
                source_index + 1,
                display_name(&input)
            ));
        }
        let selected_pages = parse_page_range(
            source.page_range.as_deref().unwrap_or("all"),
            source_pages.len(),
        )?;
        if output_page_ids.len() + selected_pages.len() > MAX_OUTPUT_PAGES {
            return Err(format!(
                "The combined PDF would contain more than {MAX_OUTPUT_PAGES} pages."
            ));
        }

        let source_has_bookmarks = source_document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"Outlines"));
        collect_source_warnings(
            &source_document,
            &input,
            source_was_encrypted,
            preserve_bookmarks,
            output_will_be_protected,
            &mut warnings,
        );
        let page_progress_start = if preserve_bookmarks && source_has_bookmarks {
            let bookmark_progress_end = source_start + (source_end - source_start) / 2;
            let bookmark_control = control.subrange(
                source_start,
                bookmark_progress_end,
                format!(
                    "Reading navigation from source {} of {}",
                    source_index + 1,
                    sources.len()
                ),
            );
            let inspected =
                inspect_document_bookmarks_with_control(&source_document, &bookmark_control)?;
            let selected_destinations =
                first_selected_page_destinations(&selected_pages, output_page_ids.len());
            let remapped = remap_selected_bookmarks(&inspected.entries, &selected_destinations);
            if combined_bookmarks.len() + remapped.bookmarks.len() > MAX_BOOKMARKS {
                return Err(format!(
                    "The selected source bookmarks would exceed the combined limit of {MAX_BOOKMARKS}. Choose fewer pages or turn off bookmark preservation."
                ));
            }
            let name = display_name(&input);
            for warning in inspected.warnings {
                warnings.push(format!("{name}: {warning}"));
            }
            if remapped.omitted > 0 {
                warnings.push(format!(
                    "{name}: {} bookmark{} could not be preserved because the destination was unresolved or outside the selected pages.",
                    remapped.omitted,
                    if remapped.omitted == 1 { "" } else { "s" }
                ));
            }
            if remapped.repeated_destination {
                warnings.push(format!(
                    "{name}: bookmarks for repeated source pages point to the first copied occurrence."
                ));
            }
            omitted_bookmark_count += remapped.omitted;
            combined_bookmarks.extend(remapped.bookmarks);
            bookmark_progress_end
        } else {
            source_start
        };
        source_document.renumber_objects_with(document.max_id + 1);
        document.max_id = source_document.max_id;
        let renumbered_pages = source_document.get_pages();
        let mut snapshots = Vec::with_capacity(selected_pages.len());
        for (page_index, page_number) in selected_pages.iter().enumerate() {
            control.ensure_not_cancelled()?;
            if page_index == 0 || page_index % 128 == 0 {
                control.checkpoint(
                    progress_between(
                        page_progress_start,
                        source_end,
                        page_index,
                        selected_pages.len(),
                    ),
                    format!(
                        "Copying page {} of {} from source {} of {}",
                        page_index + 1,
                        selected_pages.len(),
                        source_index + 1,
                        sources.len()
                    ),
                )?;
            }
            let page_id = *renumbered_pages.get(page_number).ok_or_else(|| {
                format!("Page {page_number} disappeared while preparing the merge.")
            })?;
            snapshots.push((page_id, snapshot_page(&source_document, page_id)?));
        }

        control.ensure_not_cancelled()?;
        document.objects.extend(source_document.objects);
        for (page_id, mut page) in snapshots {
            page.set("Parent", pages_root_id);
            let output_id = if used_page_ids.insert(page_id) {
                document.objects.insert(page_id, Object::Dictionary(page));
                page_id
            } else {
                page.remove(b"StructParents");
                document.add_object(Object::Dictionary(page))
            };
            output_page_ids.push(output_id);
        }
        control.checkpoint(
            source_end,
            format!("Prepared source {} of {}", source_index + 1, sources.len()),
        )?;
    }

    control.checkpoint(76, "Building combined page tree")?;
    document.objects.insert(
        pages_root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => output_page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => output_page_ids.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_root_id,
    });
    document.trailer.set("Root", catalog_id);
    control.checkpoint(80, "Building combined bookmark tree")?;
    normalise_leaf_bookmarks(&mut combined_bookmarks);
    replace_bookmarks(&mut document, &combined_bookmarks)?;
    document.prune_objects();
    document.change_producer("Tüfekci Paperworks");

    control.checkpoint(84, "Writing combined temporary PDF")?;
    let file = document
        .save(temporary.path())
        .map_err(|error| format!("The combined PDF could not be written: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("The combined PDF could not be flushed to storage: {error}"))?;
    control.checkpoint(91, "Reopening combined PDF")?;
    verify_combined_pdf(
        temporary.path(),
        output_page_ids.len(),
        &combined_bookmarks,
        None,
        false,
        control,
    )?;
    warnings.sort();
    warnings.dedup();

    Ok(BuiltPdf {
        page_count: output_page_ids.len(),
        bookmarks: combined_bookmarks,
        omitted_bookmark_count,
        temporary,
        source_fingerprints,
        warnings,
    })
}

fn load_source_document(path: &Path, password: Option<&str>) -> Result<Document, String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| {
        format!(
            "{} could not be parsed as a PDF: {error}",
            display_name(path)
        )
    })?;
    if document.is_encrypted() {
        document
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                format!(
                    "{} is password-protected. Enter its opening or administrator password.",
                    display_name(path)
                )
            })?;
    }
    Ok(document)
}

fn snapshot_page(document: &Document, page_id: ObjectId) -> Result<Dictionary, String> {
    let mut page = document
        .get_dictionary(page_id)
        .map_err(|error| format!("A selected source page is invalid: {error}"))?
        .clone();
    for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
        if let Some(value) = inherited_page_value(document, page_id, key)? {
            page.set(key, value);
        }
    }
    page.remove(b"Parent");
    page.set("Type", Object::Name(b"Page".to_vec()));
    Ok(page)
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
            return Err("A source PDF contains a cyclic page tree.".to_string());
        }
        let dictionary = document
            .get_dictionary(current_id)
            .map_err(|error| format!("A source PDF page tree is invalid: {error}"))?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value.clone()));
        }
        match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent_id) => current_id = parent_id,
            Err(_) => return Ok(None),
        }
    }
    Err("A source PDF page tree is too deeply nested.".to_string())
}

fn first_selected_page_destinations(
    selected_pages: &[u32],
    output_page_offset: usize,
) -> SelectedPageDestinations {
    let mut first_output_page = HashMap::new();
    let mut repeated_source_pages = HashSet::new();
    for (index, source_page) in selected_pages.iter().copied().enumerate() {
        if let Entry::Vacant(entry) = first_output_page.entry(source_page) {
            entry.insert((output_page_offset + index + 1) as u32);
        } else {
            repeated_source_pages.insert(source_page);
        }
    }
    SelectedPageDestinations {
        first_output_page,
        repeated_source_pages,
    }
}

fn remap_selected_bookmarks(
    entries: &[PdfBookmarkEntry],
    destinations: &SelectedPageDestinations,
) -> RemappedBookmarks {
    let mut bookmarks = Vec::with_capacity(entries.len());
    let mut retained_ancestors = Vec::new();
    let mut repeated_destination = false;

    for entry in entries {
        let original_level = entry.level as usize;
        retained_ancestors.truncate(original_level);
        retained_ancestors.resize(original_level, false);
        let destination = entry
            .page_number
            .and_then(|page| destinations.first_output_page.get(&page).copied());
        let retained = destination.is_some();
        if let Some(output_page) = destination {
            repeated_destination |= entry
                .page_number
                .is_some_and(|page| destinations.repeated_source_pages.contains(&page));
            let mut remapped = entry.clone();
            remapped.page_number = Some(output_page);
            remapped.level = retained_ancestors
                .iter()
                .filter(|retained| **retained)
                .count() as u8;
            bookmarks.push(remapped);
        }
        retained_ancestors.push(retained);
    }

    RemappedBookmarks {
        omitted: entries.len() - bookmarks.len(),
        bookmarks,
        repeated_destination,
    }
}

fn normalise_leaf_bookmarks(bookmarks: &mut [PdfBookmarkEntry]) {
    for index in 0..bookmarks.len() {
        let has_children = bookmarks
            .get(index + 1)
            .is_some_and(|next| next.level > bookmarks[index].level);
        if !has_children {
            bookmarks[index].open = true;
        }
    }
}

fn collect_source_warnings(
    document: &Document,
    path: &Path,
    was_encrypted: bool,
    preserve_bookmarks: bool,
    output_will_be_protected: bool,
    warnings: &mut Vec<String>,
) {
    let name = display_name(path);
    if was_encrypted {
        warnings.push(if output_will_be_protected {
            format!(
                "{name} was encrypted. Its source security settings are replaced by the new AES-256 output passwords."
            )
        } else {
            format!("{name} was encrypted. The combined output is not password-protected.")
        });
    }
    if document_has_certificate_signature(document) {
        warnings.push(format!(
            "{name} contains a certificate signature that is invalidated by merging or extraction."
        ));
    }
    if document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        warnings.push(format!(
            "{name} contains form fields. Check their appearances in the combined output."
        ));
    }
    if !preserve_bookmarks
        && document
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"Outlines"))
    {
        warnings.push(format!(
            "{name} contains bookmarks. Source bookmarks are not copied into the combined output."
        ));
    }
}

fn verify_combined_pdf(
    path: &Path,
    expected_pages: usize,
    expected_bookmarks: &[PdfBookmarkEntry],
    opening_password: Option<&str>,
    require_encryption: bool,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The combined output failed verification: {error}"))?;
    if require_encryption && !document.is_encrypted() {
        return Err(
            "The protected combined output did not contain AES-256 encryption.".to_string(),
        );
    }
    if !require_encryption && document.is_encrypted() {
        return Err("The prepared combined output unexpectedly remained encrypted.".to_string());
    }
    if document.is_encrypted() {
        document
            .decrypt(opening_password.unwrap_or_default())
            .map_err(|_| {
                "The protected combined output could not be decrypted for verification.".to_string()
            })?;
    }
    verify_combined_document(&document, expected_pages, expected_bookmarks, control)
}

fn verify_combined_document(
    document: &Document,
    expected_pages: usize,
    expected_bookmarks: &[PdfBookmarkEntry],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let catalog = document
        .catalog()
        .map_err(|error| format!("The combined output catalogue is invalid: {error}"))?;
    if catalog.has(b"AcroForm") {
        return Err(
            "The combined output unexpectedly exposed a source form catalogue.".to_string(),
        );
    }
    let pages = document.get_pages();
    if pages.len() != expected_pages {
        return Err(format!(
            "The combined output failed verification: expected {expected_pages} pages but found {}.",
            pages.len()
        ));
    }
    for (index, (page_number, page_id)) in pages.into_iter().enumerate() {
        control.ensure_not_cancelled()?;
        if index == 0 || index % 256 == 0 {
            control.checkpoint(
                progress_between(92, 96, index, expected_pages),
                "Verifying combined PDF pages",
            )?;
        }
        let page = document.get_dictionary(page_id).map_err(|error| {
            format!("Combined output page {page_number} failed verification: {error}")
        })?;
        if !page.has_type(b"Page") {
            return Err(format!(
                "Combined output page {page_number} has an invalid page object."
            ));
        }
    }
    control.checkpoint(96, "Verifying combined bookmark tree")?;
    let bookmark_control = control.subrange(96, 98, "Verifying combined navigation".to_string());
    let actual_bookmarks =
        inspect_document_bookmarks_with_control(document, &bookmark_control)?.entries;
    if actual_bookmarks != expected_bookmarks {
        return Err(format!(
            "The combined output failed bookmark verification: expected {} entries but found {}.",
            expected_bookmarks.len(),
            actual_bookmarks.len()
        ));
    }
    control.checkpoint(98, "Combined PDF verified")?;
    Ok(())
}

fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    if !metadata.is_file() {
        return Err("Choose an existing PDF file as a merge source.".to_string());
    }
    Ok(SourceFingerprint {
        path: path.to_path_buf(),
        bytes: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn verify_source_fingerprints(fingerprints: &[SourceFingerprint]) -> Result<(), String> {
    for expected in fingerprints {
        if source_fingerprint(&expected.path)? != *expected {
            return Err(
                "A source PDF changed on disk during page combination. Choose the sources again."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn verify_page_import_source_fingerprint(expected: &SourceFingerprint) -> Result<(), String> {
    if source_fingerprint(&expected.path)? != *expected {
        return Err(
            "The page import source changed on disk during review. Choose it again.".to_string(),
        );
    }
    Ok(())
}

fn safe_page_import_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during page import review. Choose it again.".to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The page import PDF could not be opened with the supplied password.".to_string();
    }
    if normalised.contains("page range")
        || normalised.contains("outside this document")
        || normalised.contains("empty comma-separated")
    {
        return "The requested page range is not valid for this source PDF.".to_string();
    }
    if normalised.contains("does not contain any readable pages") {
        return "The source PDF does not contain any readable pages.".to_string();
    }
    "The page import review failed a structural safety check. Choose the source PDF again and try again."
        .to_string()
}

fn safe_merge_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "A source PDF changed during the merge. Choose the sources again.".to_string();
    }
    if normalised.contains("certificate signature") {
        return "A source contains a certificate signature. Confirm the rewrite warning before merging."
            .to_string();
    }
    if normalised.contains("qpdf") || normalised.contains("aes-256") {
        return "AES-256 merge protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "A source or protected output could not be opened with the supplied passwords."
            .to_string();
    }
    if normalised.contains("page range") || normalised.contains("outside this document") {
        return "One or more merge page ranges are invalid for their source PDFs.".to_string();
    }
    "The merge failed a structural safety check. Review the sources and try again.".to_string()
}

fn safe_split_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during splitting. Choose it again.".to_string();
    }
    if normalised.contains("certificate signature") {
        return "The source contains a certificate signature. Confirm the rewrite warning before splitting."
            .to_string();
    }
    if normalised.contains("qpdf") || normalised.contains("aes-256") {
        return "AES-256 split protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The split source could not be opened with the supplied password.".to_string();
    }
    if normalised.contains("page range") || normalised.contains("outside this document") {
        return "One or more split page groups are invalid for the source PDF.".to_string();
    }
    "The split failed a structural safety check. Review the page groups and try again.".to_string()
}

fn progress_between(start: u8, end: u8, completed: usize, total: usize) -> u8 {
    if total == 0 || end <= start {
        return end;
    }
    start.saturating_add(
        (((end - start) as u128 * completed.min(total) as u128) / total as u128) as u8,
    )
}

fn parse_page_range(expression: &str, page_count: usize) -> Result<Vec<u32>, String> {
    validate_range_text(expression)?;
    let expression = expression.trim();
    if expression.is_empty() || expression.eq_ignore_ascii_case("all") {
        if page_count > MAX_OUTPUT_PAGES {
            return Err(format!(
                "The expanded page range contains more than {MAX_OUTPUT_PAGES} pages."
            ));
        }
        return Ok((1..=page_count as u32).collect());
    }

    let mut pages = Vec::new();
    for token in expression.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err("Page ranges cannot contain empty comma-separated items.".to_string());
        }
        if token.eq_ignore_ascii_case("odd") || token.eq_ignore_ascii_case("even") {
            let wanted_remainder = usize::from(token.eq_ignore_ascii_case("odd"));
            pages.extend(
                (1..=page_count)
                    .filter(|page| page % 2 == wanted_remainder)
                    .map(|page| page as u32),
            );
        } else if let Some((start, end)) = token.split_once('-') {
            if end.contains('-') {
                return Err(format!("Page range '{token}' contains too many dashes."));
            }
            let start = parse_page_number(start, page_count)?;
            let end = parse_page_number(end, page_count)?;
            if start <= end {
                pages.extend(start..=end);
            } else {
                pages.extend((end..=start).rev());
            }
        } else {
            pages.push(parse_page_number(token, page_count)?);
        }
        if pages.len() > MAX_OUTPUT_PAGES {
            return Err(format!(
                "The expanded page range contains more than {MAX_OUTPUT_PAGES} pages."
            ));
        }
    }
    if pages.is_empty() {
        return Err("The page range did not select any pages.".to_string());
    }
    Ok(pages)
}

fn parse_page_number(value: &str, page_count: usize) -> Result<u32, String> {
    let trimmed = value.trim();
    let page = trimmed
        .parse::<u32>()
        .map_err(|_| format!("'{trimmed}' is not a valid page number."))?;
    if page == 0 || page as usize > page_count {
        return Err(format!(
            "Page {page} is outside this document's 1-{page_count} page range."
        ));
    }
    Ok(page)
}

fn safe_output_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let mut safe = stem
        .chars()
        .take(80)
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while safe.contains("--") {
        safe = safe.replace("--", "-");
    }
    let safe = safe.trim_matches([' ', '-', '_']);
    if safe.is_empty() {
        "document".to_string()
    } else {
        safe.to_string()
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("PDF")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_control::PDF_JOB_CANCELLED_ERROR;
    use lopdf::Stream;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn parses_ordered_ranges_odd_even_and_reverse_ranges() {
        assert_eq!(
            parse_page_range("1, 3-5, 2", 5).unwrap(),
            vec![1, 3, 4, 5, 2]
        );
        assert_eq!(parse_page_range("odd", 6).unwrap(), vec![1, 3, 5]);
        assert_eq!(parse_page_range("even", 6).unwrap(), vec![2, 4, 6]);
        assert_eq!(parse_page_range("4-2", 5).unwrap(), vec![4, 3, 2]);
    }

    #[test]
    fn rejects_out_of_range_and_malformed_page_ranges() {
        assert!(parse_page_range("0", 4).unwrap_err().contains("outside"));
        assert!(parse_page_range("5", 4).unwrap_err().contains("outside"));
        assert!(parse_page_range("1,,2", 4)
            .unwrap_err()
            .contains("empty comma-separated"));
        assert!(parse_page_range("all", MAX_OUTPUT_PAGES + 1)
            .unwrap_err()
            .contains("50000"));
    }

    #[test]
    fn page_import_inspection_resolves_ordered_ranges_without_writing_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        sample_document(5, 0)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();

        let inspection = inspect_page_import(InspectPageImportRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
            page_range: "5, 2-3, 2".to_string(),
        })
        .unwrap();

        assert_eq!(inspection.page_count, 5);
        assert_eq!(inspection.selected_pages, vec![5, 2, 3, 2]);
        assert!(!inspection.encrypted);
        assert!(!inspection.certificate_signature);
        assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 1);
    }

    #[test]
    fn controlled_page_import_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        sample_document(5, 0)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let stages = Arc::new(Mutex::new(Vec::new()));
        let stages_for_progress = Arc::clone(&stages);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            stages_for_progress.lock().unwrap().push((value, stage));
        });

        let inspection = run_page_import_inspection_job_with_control(
            InspectPageImportRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                page_range: "5, 2-3".to_string(),
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap();

        assert_eq!(inspection.selected_pages, vec![5, 2, 3]);
        let stages = stages.lock().unwrap();
        assert!(stages
            .iter()
            .any(|(_, stage)| stage == "Inspecting source signatures"));
        assert!(stages
            .iter()
            .any(|(_, stage)| stage == "Rechecking page import source"));
        drop(stages);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let cancel_progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Inspecting source signatures" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = run_page_import_inspection_job_with_control(
            InspectPageImportRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                page_range: "all".to_string(),
            },
            &PdfJobExecutionControl::new(cancelled, cancel_progress),
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn page_import_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-page-import-review.pdf");
        sample_document(2, 0)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking page import source"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during page import review\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });

        let error = run_page_import_inspection_job_with_control(
            InspectPageImportRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                page_range: "all".to_string(),
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert_eq!(
            error,
            "The source PDF changed during page import review. Choose it again."
        );
        assert!(!error.contains("private-page-import-review.pdf"));
    }

    #[test]
    fn merges_selected_pages_in_explicit_source_order() {
        let directory = TestDirectory::new();
        let first = directory.path.join("first.pdf");
        let second = directory.path.join("second.pdf");
        let output = directory.path.join("merged.pdf");
        sample_document(3, 100)
            .save(&first)
            .unwrap()
            .sync_all()
            .unwrap();
        sample_document(2, 200)
            .save(&second)
            .unwrap()
            .sync_all()
            .unwrap();

        let result = combine_pdf_pages(CombinePdfRequest {
            acknowledge_certificate_signatures: false,
            output_path: output.to_string_lossy().into_owned(),
            output_protection: None,
            preserve_bookmarks: false,
            sources: vec![
                CombinePdfSource {
                    input_path: second.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: Some("2,1".to_string()),
                },
                CombinePdfSource {
                    input_path: first.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: Some("3".to_string()),
                },
            ],
        })
        .unwrap();

        assert_eq!(result.page_count, 3);
        assert_eq!(result.bookmark_count, 0);
        assert_eq!(result.omitted_bookmark_count, 0);
        assert_eq!(result.encryption, "None");
        let document = Document::load(&output).unwrap();
        let pages = document.get_pages();
        let markers = pages
            .values()
            .map(|page_id| {
                document
                    .get_dictionary(*page_id)
                    .unwrap()
                    .get(b"TestMarker")
                    .unwrap()
                    .as_i64()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(markers, vec![202, 201, 103]);
    }

    #[test]
    fn remaps_selected_bookmarks_and_promotes_retained_descendants() {
        let entries = vec![
            bookmark("Omitted root", Some(1), 0),
            bookmark("Retained child", Some(2), 1),
            bookmark("Repeated grandchild", Some(4), 2),
            bookmark("Omitted sibling", Some(3), 0),
            bookmark("Unresolved", None, 0),
        ];
        let destinations = first_selected_page_destinations(&[4, 2, 4], 7);

        let mut remapped = remap_selected_bookmarks(&entries, &destinations);

        assert_eq!(remapped.omitted, 3);
        assert!(remapped.repeated_destination);
        assert_eq!(
            remapped
                .bookmarks
                .iter()
                .map(|entry| (entry.title.as_str(), entry.page_number, entry.level))
                .collect::<Vec<_>>(),
            vec![
                ("Retained child", Some(9), 0),
                ("Repeated grandchild", Some(8), 1),
            ]
        );
        remapped.bookmarks[0].open = false;
        remapped.bookmarks[1].open = false;
        normalise_leaf_bookmarks(&mut remapped.bookmarks);
        assert!(!remapped.bookmarks[0].open);
        assert!(remapped.bookmarks[1].open);
    }

    #[test]
    fn preserves_exact_selected_bookmarks_across_sources() {
        let directory = TestDirectory::new();
        let first = directory.path.join("first-navigation.pdf");
        let second = directory.path.join("second-navigation.pdf");
        let output = directory.path.join("combined-navigation.pdf");
        let mut first_document = sample_document(4, 100);
        replace_bookmarks(
            &mut first_document,
            &[
                bookmark("Omitted root", Some(1), 0),
                bookmark("Retained child", Some(2), 1),
                bookmark("Repeated grandchild", Some(4), 2),
                bookmark("Omitted sibling", Some(3), 0),
            ],
        )
        .unwrap();
        first_document.save(&first).unwrap().sync_all().unwrap();
        let mut second_document = sample_document(2, 200);
        replace_bookmarks(
            &mut second_document,
            &[bookmark("Second source", Some(1), 0)],
        )
        .unwrap();
        second_document.save(&second).unwrap().sync_all().unwrap();

        let result = combine_pdf_pages(CombinePdfRequest {
            acknowledge_certificate_signatures: false,
            output_path: output.to_string_lossy().into_owned(),
            output_protection: None,
            preserve_bookmarks: true,
            sources: vec![
                CombinePdfSource {
                    input_path: first.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: Some("4,2,4".to_string()),
                },
                CombinePdfSource {
                    input_path: second.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: Some("2,1".to_string()),
                },
            ],
        })
        .unwrap();

        assert_eq!(result.page_count, 5);
        assert_eq!(result.bookmark_count, 3);
        assert_eq!(result.omitted_bookmark_count, 2);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("first copied occurrence")));
        let combined = Document::load(&output).unwrap();
        let inspected = crate::bookmarks::inspect_document_bookmarks(&combined).unwrap();
        assert_eq!(
            inspected
                .entries
                .iter()
                .map(|entry| (entry.title.as_str(), entry.page_number, entry.level))
                .collect::<Vec<_>>(),
            vec![
                ("Retained child", Some(2), 0),
                ("Repeated grandchild", Some(1), 1),
                ("Second source", Some(5), 0),
            ]
        );
    }

    #[test]
    fn reports_but_does_not_expose_source_forms_or_disabled_bookmarks() {
        let directory = TestDirectory::new();
        let input = directory.path.join("structured.pdf");
        let output = directory.path.join("combined.pdf");
        let mut source = sample_document(1, 0);
        let field_id = source.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("Reference"),
        });
        let outlines_id = source.add_object(dictionary! {
            "Type" => "Outlines",
            "Count" => 0,
        });
        let catalog_id = source.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = source.get_dictionary_mut(catalog_id).unwrap();
        catalog.set(
            "AcroForm",
            dictionary! { "Fields" => vec![field_id.into()] },
        );
        catalog.set("Outlines", outlines_id);
        source.save(&input).unwrap().sync_all().unwrap();

        let result = combine_pdf_pages(CombinePdfRequest {
            acknowledge_certificate_signatures: false,
            output_path: output.to_string_lossy().into_owned(),
            output_protection: None,
            preserve_bookmarks: false,
            sources: vec![CombinePdfSource {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                page_range: Some("all".to_string()),
            }],
        })
        .unwrap();

        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("form fields")));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("bookmarks")));
        let combined = Document::load(&output).unwrap();
        let catalog = combined.catalog().unwrap();
        assert!(!catalog.has(b"AcroForm"));
        assert!(!catalog.has(b"Outlines"));
    }

    #[test]
    fn split_prepares_every_group_before_publishing_outputs() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        sample_document(5, 0)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let result = split_pdf(SplitPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
            output_directory: directory.path.to_string_lossy().into_owned(),
            page_groups: vec!["1-2".to_string(), "5,3".to_string()],
            output_protection: None,
        })
        .unwrap();

        assert_eq!(result.outputs.len(), 2);
        assert_eq!(result.total_pages, 4);
        assert_eq!(result.encryption, "None");
        assert_eq!(
            Document::load(&result.outputs[0].output_path)
                .unwrap()
                .get_pages()
                .len(),
            2
        );
        assert_eq!(
            Document::load(&result.outputs[1].output_path)
                .unwrap()
                .get_pages()
                .len(),
            2
        );
    }

    #[test]
    fn requires_acknowledgement_before_rewriting_a_signed_source() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed.pdf");
        let output = directory.path.join("combined.pdf");
        let mut document = sample_document(1, 0);
        document.add_object(dictionary! {
            "FT" => "Sig",
            "V" => dictionary! {
                "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
            },
        });
        document.save(&input).unwrap().sync_all().unwrap();

        let source = CombinePdfSource {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
            page_range: Some("all".to_string()),
        };
        let error = combine_pdf_pages(CombinePdfRequest {
            acknowledge_certificate_signatures: false,
            output_path: output.to_string_lossy().into_owned(),
            output_protection: None,
            preserve_bookmarks: false,
            sources: vec![source.clone()],
        })
        .unwrap_err();
        assert!(error.contains("certificate signature"));
        assert!(!output.exists());

        let result = combine_pdf_pages(CombinePdfRequest {
            acknowledge_certificate_signatures: true,
            output_path: output.to_string_lossy().into_owned(),
            output_protection: None,
            preserve_bookmarks: false,
            sources: vec![source],
        })
        .unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("certificate signature")));
    }

    #[test]
    fn cancellation_during_merge_page_copying_never_publishes_output() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("combined.pdf");
        sample_document(400, 0)
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
                if progress >= 20 {
                    cancelled_for_progress.store(true, Ordering::Release);
                }
            }),
        );

        let error = combine_pdf_pages_with_control(
            CombinePdfRequest {
                acknowledge_certificate_signatures: false,
                output_path: output.to_string_lossy().into_owned(),
                output_protection: None,
                preserve_bookmarks: false,
                sources: vec![CombinePdfSource {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: Some("all".to_string()),
                }],
            },
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
        let observed = observed.lock().unwrap();
        assert!(observed.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn source_change_before_merge_publication_discards_the_candidate() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("combined.pdf");
        sample_document(3, 0)
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
                    let mut source = fs::OpenOptions::new()
                        .append(true)
                        .open(&source_to_mutate)
                        .unwrap();
                    source.write_all(b"\n").unwrap();
                    source.sync_all().unwrap();
                }
            }),
        );

        let error = combine_pdf_pages_with_control(
            CombinePdfRequest {
                acknowledge_certificate_signatures: false,
                output_path: output.to_string_lossy().into_owned(),
                output_protection: None,
                preserve_bookmarks: false,
                sources: vec![CombinePdfSource {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: Some("all".to_string()),
                }],
            },
            &control,
        )
        .unwrap_err();

        assert!(mutated.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_during_split_preparation_never_publishes_parts() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        sample_document(400, 0)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 45 {
                    cancelled_for_progress.store(true, Ordering::Release);
                }
            }),
        );

        let error = split_pdf_with_control(
            SplitPdfRequest {
                acknowledge_certificate_signatures: false,
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                output_directory: directory.path.to_string_lossy().into_owned(),
                page_groups: vec!["1-200".to_string(), "201-400".to_string()],
                output_protection: None,
            },
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!fs::read_dir(&directory.path).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("-part-")));
    }

    #[test]
    fn source_change_before_split_publication_discards_every_part() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        sample_document(4, 0)
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
                if progress >= 93 && !progress_mutated.swap(true, Ordering::AcqRel) {
                    let mut source = fs::OpenOptions::new()
                        .append(true)
                        .open(&source_to_mutate)
                        .unwrap();
                    source.write_all(b"\n").unwrap();
                    source.sync_all().unwrap();
                }
            }),
        );

        let error = split_pdf_with_control(
            SplitPdfRequest {
                acknowledge_certificate_signatures: false,
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
                output_directory: directory.path.to_string_lossy().into_owned(),
                page_groups: vec!["1-2".to_string(), "3-4".to_string()],
                output_protection: None,
            },
            &control,
        )
        .unwrap_err();

        assert!(mutated.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!fs::read_dir(&directory.path).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("-part-")));
    }

    #[test]
    fn rejects_overlong_merge_split_and_page_import_passwords_before_file_work() {
        let password = "p".repeat(MAX_PASSWORD_BYTES + 1);
        let merge_error = validate_combine_pdf_request(&CombinePdfRequest {
            acknowledge_certificate_signatures: false,
            output_path: "combined.pdf".to_string(),
            output_protection: None,
            preserve_bookmarks: false,
            sources: vec![CombinePdfSource {
                input_path: "source.pdf".to_string(),
                input_password: Some(password.clone()),
                page_range: Some("all".to_string()),
            }],
        })
        .unwrap_err();
        let split_error = validate_split_pdf_request(&SplitPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: "source.pdf".to_string(),
            input_password: Some(password.clone()),
            output_directory: ".".to_string(),
            page_groups: vec!["all".to_string()],
            output_protection: None,
        })
        .unwrap_err();
        let import_error = validate_inspect_page_import_request(&InspectPageImportRequest {
            input_path: "source.pdf".to_string(),
            input_password: Some(password),
            page_range: "all".to_string(),
        })
        .unwrap_err();

        assert!(merge_error.contains("1024"));
        assert!(split_error.contains("1024"));
        assert!(import_error.contains("1024"));
    }

    #[test]
    fn merge_requests_reject_unknown_navigation_and_source_fields() {
        let unknown_request = serde_json::from_value::<CombinePdfRequest>(serde_json::json!({
            "acknowledgeCertificateSignatures": false,
            "outputPath": "combined.pdf",
            "outputProtection": null,
            "preserveBookmarks": true,
            "privateBookmarkTitle": "must not be accepted",
            "sources": [{
                "inputPath": "source.pdf",
                "inputPassword": null,
                "pageRange": "all"
            }]
        }))
        .unwrap_err()
        .to_string();
        assert!(unknown_request.contains("privateBookmarkTitle"));

        let unknown_source = serde_json::from_value::<CombinePdfRequest>(serde_json::json!({
            "acknowledgeCertificateSignatures": false,
            "outputPath": "combined.pdf",
            "preserveBookmarks": true,
            "sources": [{
                "inputPath": "source.pdf",
                "inputPassword": null,
                "pageRange": "all",
                "bookmarkText": "must not be accepted"
            }]
        }))
        .unwrap_err()
        .to_string();
        assert!(unknown_source.contains("bookmarkText"));
    }

    fn bookmark(title: &str, page_number: Option<u32>, level: u8) -> PdfBookmarkEntry {
        PdfBookmarkEntry {
            title: title.to_string(),
            page_number,
            level,
            bold: false,
            italic: false,
            open: true,
            colour: [0.0, 0.0, 0.0],
        }
    }

    fn sample_document(page_count: u32, marker_offset: i64) -> Document {
        let mut document = Document::with_version("1.4");
        let pages_id = document.new_object_id();
        let mut kids = Vec::new();
        for page_number in 1..=page_count {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => Dictionary::new(),
                "Contents" => content_id,
                "TestMarker" => marker_offset + i64::from(page_number),
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
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-combine-test",
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
