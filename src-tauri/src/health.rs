use crate::file_safety::{canonical_pdf_input, reject_control_characters};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use lopdf::content::Content;
use lopdf::{decode_text_string, Dictionary, Document, LoadOptions, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

const MAX_PAGE_CONTENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_HEALTH_PAGES: usize = 20_000;
const MAX_HEALTH_OBJECTS: usize = 1_000_000;
const MAX_OBJECT_REFERENCES: usize = 2_000_000;
const MAX_UNIQUE_FONTS: usize = 20_000;
const MAX_FINDINGS: usize = 2_000;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_ICC_PROFILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ICC_TAGS: usize = 4_096;
const MAX_ICC_ISSUE_EXAMPLES: usize = 8;
const MAX_FORM_XOBJECT_DEPTH: usize = 32;
const MAX_FORM_XOBJECT_VISITS: usize = 100_000;
const MAX_RESOURCE_ISSUES_PER_PAGE: usize = 10_000;
const OVERSIZED_IMAGE_PIXELS: i64 = 25_000_000;
const OVERSIZED_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_EDIT_SAFETY_SOURCES: usize = 250;
const MAX_EDIT_SAFETY_PAGES: usize = 100_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfHealthRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfEditSafetyRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfEditSafetySourcesRequest {
    pub(crate) sources: Vec<InspectPdfEditSafetyRequest>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditSafetyResult {
    certificate_signature: bool,
    encrypted: bool,
    form_fields: bool,
    page_count: usize,
    source_modified_at_ms: Option<u64>,
    source_size: u64,
    xfa: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditSafetyInspectionItem {
    source_index: usize,
    result: Option<PdfEditSafetyResult>,
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditSafetyInspectionResult {
    source_count: usize,
    inspected_count: usize,
    failed_count: usize,
    items: Vec<PdfEditSafetyInspectionItem>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Warning,
    Danger,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Attention,
    Risk,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingCategory {
    Accessibility,
    Colour,
    Document,
    Fonts,
    Pages,
    Privacy,
    Security,
    Structure,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFinding {
    category: FindingCategory,
    code: String,
    severity: FindingSeverity,
    title: String,
    detail: String,
    page_number: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfTechnicalSummary {
    indirect_object_count: usize,
    broken_reference_count: usize,
    form_content_error_count: usize,
    form_resource_error_count: usize,
    form_xobject_count: usize,
    page_content_error_count: usize,
    missing_resource_count: usize,
    font_count: usize,
    embedded_font_count: usize,
    unembedded_font_count: usize,
    fonts_missing_unicode_map: usize,
    output_intent_count: usize,
    icc_profile_count: usize,
    invalid_icc_profile_count: usize,
    colour_issue_count: usize,
    pages_using_device_cmyk: Vec<u32>,
}

pub(crate) struct PdfPrintResourceAudit {
    pub(crate) broken_reference_count: usize,
    pub(crate) colour_issue_count: usize,
    pub(crate) examples: Vec<String>,
    pub(crate) font_count: usize,
    pub(crate) incomplete: bool,
    pub(crate) invalid_icc_profile_count: usize,
    pub(crate) output_intent_count: usize,
    pub(crate) resource_issue_count: usize,
    pub(crate) unembedded_font_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfAccessibilitySummary {
    title: Option<String>,
    displays_document_title: bool,
    default_language: Option<String>,
    marked_as_tagged: bool,
    structure_tree_present: bool,
    structure_element_count: usize,
    pages_with_structure_parents: usize,
    figure_count: usize,
    figures_missing_alt_text: usize,
    interactive_pages_without_structured_tab_order: Vec<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfHealthResult {
    accessibility: PdfAccessibilitySummary,
    file_name: String,
    file_size: u64,
    page_count: usize,
    pdf_version: String,
    status: HealthStatus,
    danger_count: usize,
    warning_count: usize,
    info_count: usize,
    blank_pages: Vec<u32>,
    duplicate_groups: Vec<Vec<u32>>,
    technical: PdfTechnicalSummary,
    findings: Vec<HealthFinding>,
}

fn inspect_pdf_edit_safety_path(
    input_path: &str,
    input_password: Option<&str>,
) -> Result<PdfEditSafetyResult, String> {
    inspect_pdf_edit_safety_source_with_control(
        InspectPdfEditSafetyRequest {
            input_path: input_path.to_string(),
            input_password: input_password.map(str::to_string),
        },
        &PdfJobExecutionControl::direct(),
        false,
    )
}

pub(crate) fn validate_inspect_pdf_edit_safety_sources_request(
    request: &InspectPdfEditSafetySourcesRequest,
) -> Result<(), String> {
    if request.sources.is_empty() {
        return Err("Choose at least one PDF for its edit-safety check.".to_string());
    }
    if request.sources.len() > MAX_EDIT_SAFETY_SOURCES {
        return Err(format!(
            "An edit-safety check may contain no more than {MAX_EDIT_SAFETY_SOURCES} source PDFs."
        ));
    }
    for source in &request.sources {
        validate_inspect_pdf_edit_safety_source_request(source)?;
    }
    Ok(())
}

pub(crate) fn run_pdf_edit_safety_inspection_job_with_control(
    request: InspectPdfEditSafetySourcesRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfEditSafetyInspectionResult, String> {
    control.checkpoint(2, "Validating edit-safety inspection")?;
    validate_inspect_pdf_edit_safety_sources_request(&request)?;
    let source_count = request.sources.len();
    let mut items = Vec::with_capacity(source_count);
    let mut inspected_count = 0_usize;
    let mut failed_count = 0_usize;

    for (source_index, source) in request.sources.into_iter().enumerate() {
        control.ensure_not_cancelled()?;
        let item_start = edit_safety_progress(5, 96, source_index, source_count);
        let item_end = edit_safety_progress(5, 96, source_index + 1, source_count);
        let item_control = control.subrange(
            item_start,
            item_end,
            format!("PDF {} of {source_count}", source_index + 1),
        );
        match inspect_pdf_edit_safety_source_with_control(source, &item_control, true) {
            Ok(result) => {
                inspected_count += 1;
                items.push(PdfEditSafetyInspectionItem {
                    source_index,
                    result: Some(result),
                    error: None,
                });
            }
            Err(error) if error == PDF_JOB_CANCELLED_ERROR => return Err(error),
            Err(error) => {
                failed_count += 1;
                items.push(PdfEditSafetyInspectionItem {
                    source_index,
                    result: None,
                    error: Some(safe_edit_safety_job_error(&error)),
                });
            }
        }
    }

    control.checkpoint(99, "Finalising edit-safety inspection")?;
    Ok(PdfEditSafetyInspectionResult {
        source_count,
        inspected_count,
        failed_count,
        items,
    })
}

fn validate_inspect_pdf_edit_safety_source_request(
    request: &InspectPdfEditSafetyRequest,
) -> Result<(), String> {
    if request.input_path.trim().is_empty() {
        return Err("Choose a source PDF for its edit-safety check.".to_string());
    }
    reject_control_characters("Edit-safety source path", &request.input_path)?;
    if request
        .input_password
        .as_deref()
        .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
    {
        return Err(format!(
            "The PDF password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }
    Ok(())
}

fn inspect_pdf_edit_safety_source_with_control(
    request: InspectPdfEditSafetyRequest,
    control: &PdfJobExecutionControl,
    enforce_scheduler_bounds: bool,
) -> Result<PdfEditSafetyResult, String> {
    control.checkpoint(3, "Checking the source PDF")?;
    validate_inspect_pdf_edit_safety_source_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let opening_metadata = fs::metadata(&input)
        .map_err(|error| format!("The PDF edit-safety fingerprint could not be read: {error}"))?;
    let opening_modified_at_ms = modified_at_ms(&opening_metadata);
    control.checkpoint(12, "Opening the PDF structure")?;
    let mut document = Document::load_with_options(
        &input,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The PDF could not be parsed for its edit-safety check: {error}"))?;
    control.ensure_not_cancelled()?;
    if enforce_scheduler_bounds && document.objects.len() > MAX_HEALTH_OBJECTS {
        return Err(format!(
            "The edit-safety check supports at most {MAX_HEALTH_OBJECTS} PDF objects."
        ));
    }
    let was_encrypted = document.is_encrypted() || document.was_encrypted();
    if document.is_encrypted() {
        control.checkpoint(38, "Opening the protected PDF")?;
        document
            .decrypt(request.input_password.as_deref().unwrap_or_default())
            .map_err(|_| {
                "The PDF could not be decrypted for its edit-safety check. Check its password."
                    .to_string()
            })?;
    }
    control.checkpoint(48, "Checking readable pages")?;
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    if enforce_scheduler_bounds && page_count > MAX_EDIT_SAFETY_PAGES {
        return Err(format!(
            "The edit-safety check supports at most {MAX_EDIT_SAFETY_PAGES} pages per PDF."
        ));
    }
    control.checkpoint(58, "Checking certificate signatures")?;
    let certificate_signature =
        document_has_certificate_signature_with_control(&document, control)?;
    control.checkpoint(72, "Checking interactive forms")?;
    let form_fields = document
        .catalog()
        .is_ok_and(|catalog| catalog.has(b"AcroForm"));
    control.checkpoint(78, "Checking XFA structures")?;
    let xfa = document_contains_key_with_control(&document, b"XFA", control)?;
    control.checkpoint(92, "Verifying the source fingerprint")?;
    let closing_metadata = fs::metadata(&input).map_err(|error| {
        format!("The PDF edit-safety fingerprint could not be checked: {error}")
    })?;
    let source_modified_at_ms = modified_at_ms(&closing_metadata);
    if opening_metadata.len() != closing_metadata.len()
        || opening_modified_at_ms != source_modified_at_ms
    {
        return Err(
            "The PDF changed while its edit-safety check was running. Review it again.".to_string(),
        );
    }

    Ok(PdfEditSafetyResult {
        certificate_signature,
        encrypted: was_encrypted,
        form_fields,
        page_count,
        source_modified_at_ms,
        source_size: closing_metadata.len(),
        xfa,
    })
}

fn edit_safety_progress(start: u8, end: u8, completed: usize, total: usize) -> u8 {
    if total == 0 || end <= start {
        return end;
    }
    start.saturating_add(
        (((end - start) as u128 * completed.min(total) as u128) / total as u128) as u8,
    )
}

fn safe_edit_safety_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed while") {
        return "The source PDF changed during its edit-safety check. Run the check again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The source PDF could not be decrypted for its edit-safety check. Check its password."
            .to_string();
    }
    if normalised.contains("readable pages") {
        return "The source PDF does not contain any readable pages.".to_string();
    }
    if normalised.contains("supports at most") {
        return error.to_string();
    }
    if normalised.contains("could not be opened")
        || normalised.contains("existing pdf file")
        || normalised.contains("source path")
    {
        return "The source PDF could not be opened for its edit-safety check.".to_string();
    }
    "The edit-safety check could not complete its bounded structural inspection. Review the PDF and try again."
        .to_string()
}

pub(crate) fn ensure_pdf_rewrite_acknowledged(
    input_path: &str,
    input_password: Option<&str>,
    acknowledged: bool,
) -> Result<(), String> {
    if acknowledged {
        return Ok(());
    }
    let result = inspect_pdf_edit_safety_path(input_path, input_password)?;
    if result.certificate_signature {
        return Err(certificate_signature_rewrite_error(Path::new(input_path)));
    }
    Ok(())
}

pub(crate) fn ensure_document_rewrite_acknowledged(
    document: &Document,
    path: &Path,
    acknowledged: bool,
) -> Result<(), String> {
    if !acknowledged && document_has_certificate_signature(document) {
        return Err(certificate_signature_rewrite_error(path));
    }
    Ok(())
}

fn certificate_signature_rewrite_error(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("The source PDF");
    format!(
        "{name} contains a certificate signature. This workflow rewrites the PDF and will invalidate that signature. Confirm the certificate-signature warning before continuing."
    )
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
pub fn inspect_pdf_health(request: InspectPdfHealthRequest) -> Result<PdfHealthResult, String> {
    inspect_pdf_health_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_health_request(
    request: &InspectPdfHealthRequest,
) -> Result<(), String> {
    if request
        .input_password
        .as_deref()
        .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
    {
        return Err(format!(
            "The PDF password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }
    canonical_pdf_input(&request.input_path)?;
    Ok(())
}

pub(crate) fn run_pdf_health_job_with_control(
    request: InspectPdfHealthRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfHealthResult, String> {
    inspect_pdf_health_with_control(request, control)
        .map(job_safe_health_result)
        .map_err(|error| {
            if error == PDF_JOB_CANCELLED_ERROR {
                error
            } else {
                safe_health_job_error(&error)
            }
        })
}

fn inspect_pdf_health_with_control(
    request: InspectPdfHealthRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfHealthResult, String> {
    control.checkpoint(2, "Checking the source PDF")?;
    validate_inspect_pdf_health_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_modified = metadata.modified().ok();
    control.checkpoint(7, "Opening the PDF structure")?;
    let mut document = Document::load_with_options(
        &input,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The PDF could not be parsed for its health check: {error}"))?;
    let was_encrypted = document.is_encrypted() || document.was_encrypted();
    if document.is_encrypted() {
        document
            .decrypt(request.input_password.as_deref().unwrap_or_default())
            .map_err(|_| {
                "The PDF could not be decrypted for its health check. Check its password."
                    .to_string()
            })?;
    }

    control.checkpoint(15, "Checking document bounds")?;
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    if pages.len() > MAX_HEALTH_PAGES {
        return Err(format!(
            "Document Health supports at most {MAX_HEALTH_PAGES} pages in one PDF."
        ));
    }
    if document.objects.len() > MAX_HEALTH_OBJECTS {
        return Err(format!(
            "Document Health supports at most {MAX_HEALTH_OBJECTS} indirect objects in one PDF."
        ));
    }
    let mut findings = Vec::new();
    control.checkpoint(20, "Inspecting document features")?;
    inspect_document_features(
        &document,
        was_encrypted,
        metadata.len(),
        &mut findings,
        control,
    )?;
    let mut technical = inspect_technical_resources(&document, &pages, &mut findings, control)?;
    let accessibility = inspect_accessibility(&document, &pages, &mut findings, control)?;
    let (blank_pages, duplicate_groups) =
        inspect_pages(&document, &pages, &mut technical, &mut findings, control)?;
    finalise_technical_findings(&mut technical, &mut findings);
    let danger_count = findings
        .iter()
        .filter(|finding| matches!(finding.severity, FindingSeverity::Danger))
        .count();
    let warning_count = findings
        .iter()
        .filter(|finding| matches!(finding.severity, FindingSeverity::Warning))
        .count();
    let info_count = findings.len() - danger_count - warning_count;
    let status = if danger_count > 0 {
        HealthStatus::Risk
    } else if warning_count > 0 {
        HealthStatus::Attention
    } else {
        HealthStatus::Healthy
    };

    control.checkpoint(98, "Rechecking the source PDF")?;
    let closing_metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be rechecked: {error}"))?;
    if metadata.len() != closing_metadata.len()
        || source_modified != closing_metadata.modified().ok()
    {
        return Err(
            "The source PDF changed while Document Health was inspecting it. Run the health check again."
                .to_string(),
        );
    }
    control.checkpoint(99, "Finalising the health report")?;
    Ok(PdfHealthResult {
        accessibility,
        file_name: input
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("PDF")
            .to_string(),
        file_size: metadata.len(),
        page_count: pages.len(),
        pdf_version: document.version,
        status,
        danger_count,
        warning_count,
        info_count,
        blank_pages,
        duplicate_groups,
        technical,
        findings,
    })
}

fn job_safe_health_result(mut result: PdfHealthResult) -> PdfHealthResult {
    result.file_name = "PDF".to_string();
    result
}

fn safe_health_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed while") {
        return "The source PDF changed during its health check. Run the check again.".to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The source PDF could not be decrypted for its health check. Check its password."
            .to_string();
    }
    if normalised.contains("supports at most") || normalised.contains("readable pages") {
        return error.to_string();
    }
    "Document Health could not complete a bounded structural check. Review the PDF and try again."
        .to_string()
}

fn health_loop_checkpoint(
    control: &PdfJobExecutionControl,
    start: u8,
    end: u8,
    index: usize,
    total: usize,
    interval: usize,
    stage: &str,
) -> Result<(), String> {
    control.ensure_not_cancelled()?;
    let completed = index.saturating_add(1);
    if index == 0 || completed == total || completed.is_multiple_of(interval.max(1)) {
        let span = u16::from(end.saturating_sub(start));
        let denominator = total.max(1) as u128;
        let offset = (u128::from(span) * completed as u128 / denominator) as u8;
        control.checkpoint(start.saturating_add(offset).min(end), stage)?;
    }
    Ok(())
}

#[derive(Default)]
struct StructureStats {
    element_count: usize,
    figure_count: usize,
    figures_missing_alt_text: usize,
    node_count: usize,
}

fn inspect_accessibility(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<PdfAccessibilitySummary, String> {
    control.checkpoint(69, "Inspecting accessibility structure")?;
    let catalog = document.catalog().ok();
    let title = document_title(document);
    let displays_document_title = catalog
        .and_then(|value| value.get(b"ViewerPreferences").ok())
        .and_then(|value| resolved_dictionary(document, value))
        .and_then(|value| value.get(b"DisplayDocTitle").ok())
        .and_then(|value| document.dereference(value).ok().map(|(_, value)| value))
        .and_then(|value| value.as_bool().ok())
        .unwrap_or(false);
    let default_language = catalog
        .and_then(|value| value.get(b"Lang").ok())
        .and_then(|value| resolved_text(document, value));
    let marked_as_tagged = catalog
        .and_then(|value| value.get(b"MarkInfo").ok())
        .and_then(|value| resolved_dictionary(document, value))
        .and_then(|value| value.get(b"Marked").ok())
        .and_then(|value| document.dereference(value).ok().map(|(_, value)| value))
        .and_then(|value| value.as_bool().ok())
        .unwrap_or(false);
    let structure_root = catalog
        .and_then(|value| value.get(b"StructTreeRoot").ok())
        .and_then(|value| resolved_dictionary(document, value));
    let structure_tree_present = structure_root.is_some();
    let mut structure_stats = StructureStats::default();
    if let Some(root) = structure_root {
        let role_map = structure_role_map(document, root);
        let mut visited = HashSet::new();
        if let Ok(kids) = root.get(b"K") {
            inspect_structure_object(
                document,
                kids,
                &role_map,
                &mut structure_stats,
                &mut visited,
                0,
                control,
            )?;
        }
    }

    let mut pages_with_structure_parents = 0;
    let mut interactive_pages_without_structured_tab_order = Vec::new();
    for (index, (page_number, page_id)) in pages.iter().enumerate() {
        health_loop_checkpoint(
            control,
            72,
            77,
            index,
            pages.len(),
            64,
            "Inspecting accessibility page signals",
        )?;
        let Ok(page) = document.get_dictionary(*page_id) else {
            continue;
        };
        if page.get(b"StructParents").and_then(Object::as_i64).is_ok() {
            pages_with_structure_parents += 1;
        }
        let has_interactive_content = document
            .get_page_annotations(*page_id)
            .unwrap_or_default()
            .iter()
            .any(|annotation| {
                annotation
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|name| name == b"Widget" || name == b"Link")
            });
        let uses_structure_order = page
            .get(b"Tabs")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"S");
        if has_interactive_content && !uses_structure_order {
            interactive_pages_without_structured_tab_order.push(*page_number);
        }
    }

    if title.is_none() {
        push_finding(
            findings,
            "accessibility-title",
            FindingSeverity::Warning,
            "Document title is missing",
            "Add a descriptive Title entry to the document information dictionary so assistive technology can identify the PDF.",
            None,
        );
    } else if !displays_document_title {
        push_finding(
            findings,
            "accessibility-display-title",
            FindingSeverity::Warning,
            "Document title is not selected for display",
            "The PDF has a title, but ViewerPreferences does not set DisplayDocTitle to true. Readers may show the filename instead.",
            None,
        );
    }
    if default_language.is_none() {
        push_finding(
            findings,
            "accessibility-language",
            FindingSeverity::Warning,
            "Default document language is missing",
            "Add a valid Lang entry to the document catalogue so screen readers can choose appropriate pronunciation rules.",
            None,
        );
    }

    match (marked_as_tagged, structure_tree_present) {
        (false, false) => push_finding(
            findings,
            "accessibility-untagged",
            FindingSeverity::Warning,
            "Document is not tagged",
            "No tagged-PDF declaration or structure tree was found. Assistive technology cannot depend on semantic headings, lists, tables, figures, or reading order.",
            None,
        ),
        (true, false) => push_finding(
            findings,
            "accessibility-structure-tree",
            FindingSeverity::Warning,
            "Tagged declaration has no structure tree",
            "MarkInfo declares a tagged PDF, but the document catalogue has no usable StructTreeRoot.",
            None,
        ),
        (false, true) => push_finding(
            findings,
            "accessibility-mark-info",
            FindingSeverity::Warning,
            "Structure tree is not declared as tagged",
            "A StructTreeRoot exists, but MarkInfo does not declare Marked as true. Reader support may be inconsistent.",
            None,
        ),
        (true, true) => {
            if structure_stats.element_count == 0 {
                push_finding(
                    findings,
                    "accessibility-empty-structure",
                    FindingSeverity::Warning,
                    "Structure tree has no semantic elements",
                    "The PDF is marked as tagged, but no structure elements were found beneath StructTreeRoot.",
                    None,
                );
            } else {
                push_finding(
                    findings,
                    "accessibility-reading-order-review",
                    FindingSeverity::Info,
                    "Manual reading-order review required",
                    &format!(
                        "The structure tree contains {} semantic element{}. Static inspection cannot prove that its sequence matches the visual meaning; verify it with a screen reader or accessibility API.",
                        structure_stats.element_count,
                        if structure_stats.element_count == 1 { "" } else { "s" }
                    ),
                    None,
                );
            }
        }
    }

    if structure_stats.figures_missing_alt_text > 0 {
        push_finding(
            findings,
            "accessibility-figure-alt",
            FindingSeverity::Warning,
            "Figure alternative text is missing",
            &format!(
                "{} of {} Figure structure element{} lack a non-empty Alt entry. Add equivalent text or mark purely decorative images as artifacts.",
                structure_stats.figures_missing_alt_text,
                structure_stats.figure_count,
                if structure_stats.figure_count == 1 { "" } else { "s" }
            ),
            None,
        );
    }
    if !interactive_pages_without_structured_tab_order.is_empty() {
        push_finding(
            findings,
            "accessibility-tab-order",
            FindingSeverity::Warning,
            "Interactive page tab order needs review",
            &format!(
                "Page{} {} contain form widgets or links without a Tabs value of S. Confirm that keyboard focus follows the document structure.",
                if interactive_pages_without_structured_tab_order.len() == 1 { "" } else { "s" },
                page_list(&interactive_pages_without_structured_tab_order)
            ),
            interactive_pages_without_structured_tab_order.first().copied(),
        );
    }

    Ok(PdfAccessibilitySummary {
        title,
        displays_document_title,
        default_language,
        marked_as_tagged,
        structure_tree_present,
        structure_element_count: structure_stats.element_count,
        pages_with_structure_parents,
        figure_count: structure_stats.figure_count,
        figures_missing_alt_text: structure_stats.figures_missing_alt_text,
        interactive_pages_without_structured_tab_order,
    })
}

fn document_title(document: &Document) -> Option<String> {
    document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|value| resolved_dictionary(document, value))
        .and_then(|value| value.get(b"Title").ok())
        .and_then(|value| resolved_text(document, value))
}

fn resolved_dictionary<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    let (_, object) = document.dereference(object).ok()?;
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn resolved_text(document: &Document, object: &Object) -> Option<String> {
    let (_, object) = document.dereference(object).ok()?;
    let text = decode_text_string(object).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn structure_role_map(document: &Document, root: &Dictionary) -> HashMap<Vec<u8>, Vec<u8>> {
    let Some(role_map) = root
        .get(b"RoleMap")
        .ok()
        .and_then(|value| resolved_dictionary(document, value))
    else {
        return HashMap::new();
    };
    role_map
        .iter()
        .filter_map(|(custom, standard)| {
            let (_, standard) = document.dereference(standard).ok()?;
            Some((custom.clone(), standard.as_name().ok()?.to_vec()))
        })
        .collect()
}

fn inspect_structure_object(
    document: &Document,
    object: &Object,
    role_map: &HashMap<Vec<u8>, Vec<u8>>,
    stats: &mut StructureStats,
    visited: &mut HashSet<ObjectId>,
    depth: usize,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    stats.node_count = stats.node_count.saturating_add(1);
    if stats.node_count.is_multiple_of(256) {
        control.ensure_not_cancelled()?;
    }
    if depth > 256 {
        return Ok(());
    }
    let object = match object {
        Object::Reference(id) => {
            if !visited.insert(*id) {
                return Ok(());
            }
            let Ok(object) = document.get_object(*id) else {
                return Ok(());
            };
            object
        }
        object => object,
    };
    match object {
        Object::Array(values) => {
            for value in values {
                inspect_structure_object(
                    document,
                    value,
                    role_map,
                    stats,
                    visited,
                    depth + 1,
                    control,
                )?;
            }
        }
        Object::Dictionary(dictionary) => {
            if let Ok(role) = dictionary.get(b"S").and_then(Object::as_name) {
                stats.element_count += 1;
                let standard_role = role_map.get(role).map(Vec::as_slice).unwrap_or(role);
                if standard_role == b"Figure" {
                    stats.figure_count += 1;
                    if dictionary
                        .get(b"Alt")
                        .ok()
                        .and_then(|value| resolved_text(document, value))
                        .is_none()
                    {
                        stats.figures_missing_alt_text += 1;
                    }
                }
            }
            if let Ok(kids) = dictionary.get(b"K") {
                inspect_structure_object(
                    document,
                    kids,
                    role_map,
                    stats,
                    visited,
                    depth + 1,
                    control,
                )?;
            }
        }
        Object::Stream(stream) => {
            if let Ok(kids) = stream.dict.get(b"K") {
                inspect_structure_object(
                    document,
                    kids,
                    role_map,
                    stats,
                    visited,
                    depth + 1,
                    control,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn inspect_document_features(
    document: &Document,
    was_encrypted: bool,
    file_size: u64,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if was_encrypted {
        push_finding(
            findings,
            "encrypted",
            FindingSeverity::Info,
            "Password protection detected",
            "The file is encrypted. Structural exports need the current password and may require new protection settings.",
            None,
        );
    }
    if file_size > 500 * 1024 * 1024 {
        push_finding(
            findings,
            "large-file",
            FindingSeverity::Warning,
            "Very large PDF",
            "The file is larger than 500 MB. Rendering, OCR, and export may need substantial memory and time.",
            None,
        );
    }
    if document.objects.len() > 250_000 {
        push_finding(
            findings,
            "large-object-table",
            FindingSeverity::Warning,
            "Unusually complex object table",
            &format!(
                "The PDF contains {} indirect objects, which may slow inspection and export.",
                document.objects.len()
            ),
            None,
        );
    }

    let catalog = document.catalog().ok();
    if document.trailer.has(b"Info") || catalog.is_some_and(|value| value.has(b"Metadata")) {
        push_finding(
            findings,
            "metadata",
            FindingSeverity::Info,
            "Document metadata present",
            "The file contains document information or XMP metadata that may identify its author, software, or history.",
            None,
        );
    }
    if catalog.is_some_and(|value| value.has(b"Outlines")) {
        push_finding(
            findings,
            "bookmarks",
            FindingSeverity::Info,
            "Bookmarks detected",
            "Check bookmark destinations after page reordering, merging, splitting, or extraction.",
            None,
        );
    }
    if catalog.is_some_and(|value| value.has(b"AcroForm")) {
        push_finding(
            findings,
            "forms",
            FindingSeverity::Info,
            "Interactive form fields detected",
            "Review field values and appearances after structural editing or flattening.",
            None,
        );
    }
    if document_contains_key_with_control(document, b"XFA", control)? {
        push_finding(
            findings,
            "xfa",
            FindingSeverity::Warning,
            "XFA form content detected",
            "XFA forms have limited support outside Adobe software and may not survive structural editing.",
            None,
        );
    }
    if document_has_certificate_signature_with_control(document, control)? {
        push_finding(
            findings,
            "certificate-signature",
            FindingSeverity::Warning,
            "Certificate signature detected",
            "Any content or page-tree edit is expected to invalidate the existing certificate signature.",
            None,
        );
    }

    let has_javascript = document_dictionaries_any_with_control(
        document,
        &|dictionary| {
            dictionary.has(b"JS")
                || dictionary
                    .get(b"S")
                    .and_then(Object::as_name)
                    .is_ok_and(|name| name == b"JavaScript")
        },
        control,
    )?;
    if has_javascript {
        push_finding(
            findings,
            "javascript",
            FindingSeverity::Danger,
            "Embedded JavaScript detected",
            "JavaScript can run actions in compatible PDF readers. Remove it before sharing unless it is explicitly required and trusted.",
            None,
        );
    }
    if catalog.is_some_and(|value| value.has(b"OpenAction") || value.has(b"AA")) {
        push_finding(
            findings,
            "automatic-actions",
            FindingSeverity::Warning,
            "Automatic document actions detected",
            "The PDF requests an action when it opens or when a document event occurs. Review it before sharing.",
            None,
        );
    }
    if document_dictionaries_any_with_control(
        document,
        &|dictionary| {
            dictionary
                .get(b"S")
                .and_then(Object::as_name)
                .is_ok_and(|name| name == b"Launch")
        },
        control,
    )? {
        push_finding(
            findings,
            "launch-action",
            FindingSeverity::Danger,
            "External launch action detected",
            "The PDF contains an action that may ask a reader to launch an external file or application.",
            None,
        );
    }
    if document_dictionaries_any_with_control(
        document,
        &|dictionary| dictionary.has(b"EF") || dictionary.has(b"EmbeddedFiles"),
        control,
    )? {
        push_finding(
            findings,
            "attachments",
            FindingSeverity::Warning,
            "Embedded files or attachments detected",
            "Attachments can contain private or executable content and should be reviewed before sharing.",
            None,
        );
    }
    if catalog.is_some_and(|value| value.has(b"Names") || value.has(b"Dests")) {
        push_finding(
            findings,
            "named-destinations",
            FindingSeverity::Info,
            "Named destinations or document names detected",
            "Check internal links and named destinations after merging, splitting, or page reordering.",
            None,
        );
    }
    Ok(())
}

#[derive(Default)]
struct ReferenceAudit {
    broken: usize,
    checked: usize,
    depth_limited: bool,
    examples: Vec<String>,
    reference_limited: bool,
}

#[derive(Default)]
struct FontAudit {
    invalid: Vec<String>,
    standard_unembedded: Vec<String>,
    unembedded: Vec<String>,
    unicode_missing: Vec<String>,
    unicode_missing_simple: Vec<String>,
}

#[derive(Default)]
struct FontResourceAudit {
    findings: FontAudit,
    limited: bool,
    seen: HashSet<String>,
}

#[derive(Default)]
struct FontEmbeddingStatus {
    descriptor_present: bool,
    embedded: bool,
    malformed: bool,
}

#[derive(Default)]
struct ColourAudit {
    examples: Vec<String>,
    invalid_profile_ids: HashSet<String>,
    issue_count: usize,
    profile_ids: HashSet<String>,
}

impl ColourAudit {
    fn issue(&mut self, value: String) {
        self.issue_count += 1;
        push_example(&mut self.examples, value);
    }

    fn profile_issue(&mut self, identity: &str, value: String) {
        self.invalid_profile_ids.insert(identity.to_string());
        self.issue(value);
    }
}

#[derive(Default)]
struct IccProfileValidation {
    examples: Vec<String>,
    issue_count: usize,
}

impl IccProfileValidation {
    fn issue(&mut self, value: impl Into<String>) {
        self.issue_count += 1;
        if self.examples.len() < MAX_ICC_ISSUE_EXAMPLES {
            self.examples.push(value.into());
        }
    }
}

#[derive(Default)]
struct PageResourceInventory {
    colour_spaces: HashSet<Vec<u8>>,
    ext_gstates: HashSet<Vec<u8>>,
    fonts: HashSet<Vec<u8>>,
    properties: HashSet<Vec<u8>>,
    shadings: HashSet<Vec<u8>>,
    xobjects: HashSet<Vec<u8>>,
}

#[derive(Default)]
struct ResourceTreeWalkState {
    active_forms: HashSet<String>,
    cycle_count: usize,
    cycle_examples: Vec<String>,
    depth_limited: bool,
    resource_error_count: usize,
    resource_error_examples: Vec<String>,
    unique_forms: HashSet<String>,
    visit_count: usize,
    visit_limited: bool,
    visited_contexts: HashSet<String>,
}

struct ResourceTreeWalkFrame<'a> {
    allowed_xobjects: Option<&'a HashSet<Vec<u8>>>,
    context: &'a str,
    depth: usize,
    page_number: u32,
}

#[derive(Clone, Copy)]
struct PageContentInspectionFrame {
    page_id: ObjectId,
    page_number: u32,
}

#[derive(Default)]
struct PageContentResourceAudit {
    form_content_error_count: usize,
    form_content_examples: Vec<String>,
    form_resource_error_count: usize,
    form_resource_examples: Vec<String>,
    malformed_operator_count: usize,
    malformed_operator_examples: Vec<String>,
    missing: HashSet<String>,
    missing_limited: bool,
}

impl PageContentResourceAudit {
    fn missing(&mut self, kind: &str, name: &[u8], context: &str) {
        if self.missing.len() >= MAX_RESOURCE_ISSUES_PER_PAGE {
            self.missing_limited = true;
            return;
        }
        self.missing.insert(format!(
            "{kind} /{} in {context}",
            display_resource_name(name)
        ));
    }

    fn malformed_operator(&mut self, context: &str) {
        self.malformed_operator_count += 1;
        push_example(&mut self.malformed_operator_examples, context.to_string());
    }

    fn form_content_error(&mut self, value: String) {
        self.form_content_error_count += 1;
        push_example(&mut self.form_content_examples, value);
    }

    fn form_resource_error(&mut self, value: String) {
        self.form_resource_error_count += 1;
        push_example(&mut self.form_resource_examples, value);
    }
}

impl ResourceTreeWalkState {
    fn resource_error(&mut self, value: String) {
        self.resource_error_count += 1;
        push_example(&mut self.resource_error_examples, value);
    }

    fn cycle(&mut self, value: String) {
        self.cycle_count += 1;
        push_example(&mut self.cycle_examples, value);
    }
}

fn inspect_technical_resources(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<PdfTechnicalSummary, String> {
    let mut summary = PdfTechnicalSummary {
        indirect_object_count: document.objects.len(),
        ..PdfTechnicalSummary::default()
    };
    control.checkpoint(23, "Inspecting object references")?;
    inspect_object_references(document, &mut summary, findings, control)?;
    control.checkpoint(40, "Inspecting page and Form fonts")?;
    inspect_fonts(document, pages, &mut summary, findings, control)?;
    control.checkpoint(54, "Inspecting colour profiles and resources")?;
    inspect_colour_profiles(document, pages, &mut summary, findings, control)?;
    Ok(summary)
}

pub(crate) fn inspect_pdf_print_resources(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    control: &PdfJobExecutionControl,
) -> Result<PdfPrintResourceAudit, String> {
    let mut findings = Vec::new();
    let summary = inspect_technical_resources(document, pages, &mut findings, control)?;
    let incomplete = findings
        .iter()
        .any(|finding| finding.code.contains("limit") || finding.code == "font-invalid-resources");
    let examples = findings
        .iter()
        .filter(|finding| {
            finding.code.starts_with("font-")
                || finding.code.starts_with("colour-")
                || finding.code == "broken-object-references"
        })
        .take(MAX_ICC_ISSUE_EXAMPLES)
        .map(|finding| format!("{}: {}", finding.title, finding.detail))
        .collect();
    Ok(PdfPrintResourceAudit {
        broken_reference_count: summary.broken_reference_count,
        colour_issue_count: summary.colour_issue_count,
        examples,
        font_count: summary.font_count,
        incomplete,
        invalid_icc_profile_count: summary.invalid_icc_profile_count,
        output_intent_count: summary.output_intent_count,
        resource_issue_count: summary
            .form_content_error_count
            .saturating_add(summary.form_resource_error_count)
            .saturating_add(summary.page_content_error_count)
            .saturating_add(summary.missing_resource_count),
        unembedded_font_count: summary.unembedded_font_count,
    })
}

fn inspect_object_references(
    document: &Document,
    summary: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let mut audit = ReferenceAudit::default();
    audit_references_in_object(
        document,
        &Object::Dictionary(document.trailer.clone()),
        "trailer",
        0,
        &mut audit,
        control,
    )?;
    for (index, (id, object)) in document.objects.iter().enumerate() {
        health_loop_checkpoint(
            control,
            24,
            39,
            index,
            document.objects.len(),
            256,
            "Inspecting object references",
        )?;
        if audit.reference_limited {
            break;
        }
        audit_references_in_object(
            document,
            object,
            &format!("object {} {}", id.0, id.1),
            0,
            &mut audit,
            control,
        )?;
    }
    summary.broken_reference_count = audit.broken;
    if audit.broken > 0 {
        let examples = if audit.examples.is_empty() {
            String::new()
        } else {
            format!(" Examples: {}.", audit.examples.join("; "))
        };
        push_finding(
            findings,
            "broken-object-references",
            FindingSeverity::Danger,
            "Broken object references detected",
            &format!(
                "The PDF contains {} reference{} to missing indirect objects. Pages, fonts, links, or forms may render incompletely.{}",
                audit.broken,
                if audit.broken == 1 { "" } else { "s" },
                examples
            ),
            None,
        );
    }
    if audit.reference_limited {
        push_finding(
            findings,
            "resource-reference-limit",
            FindingSeverity::Warning,
            "Reference inspection reached its limit",
            &format!(
                "The PDF contains more than {MAX_OBJECT_REFERENCES} nested object references. Remaining references were not traversed by this preflight."
            ),
            None,
        );
    }
    if audit.depth_limited {
        push_finding(
            findings,
            "resource-nesting-limit",
            FindingSeverity::Warning,
            "Object nesting inspection reached its limit",
            "One or more direct PDF arrays or dictionaries exceed 64 nesting levels. Those branches were not traversed beyond the limit, but the rest of the object graph was inspected.",
            None,
        );
    }
    Ok(())
}

fn audit_references_in_object(
    document: &Document,
    object: &Object,
    path: &str,
    depth: usize,
    audit: &mut ReferenceAudit,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if audit.reference_limited {
        return Ok(());
    }
    if depth > 64 {
        audit.depth_limited = true;
        return Ok(());
    }
    match object {
        Object::Reference(id) => {
            audit.checked += 1;
            if audit.checked.is_multiple_of(1_024) {
                control.ensure_not_cancelled()?;
            }
            if audit.checked > MAX_OBJECT_REFERENCES {
                audit.reference_limited = true;
                return Ok(());
            }
            if !document.objects.contains_key(id) {
                audit.broken += 1;
                if audit.examples.len() < 8 {
                    audit
                        .examples
                        .push(format!("{path} -> {} {} R", id.0, id.1));
                }
            }
        }
        Object::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                audit_references_in_object(
                    document,
                    value,
                    &format!("{path}[{index}]"),
                    depth + 1,
                    audit,
                    control,
                )?;
                if audit.reference_limited {
                    break;
                }
            }
        }
        Object::Dictionary(dictionary) => {
            for (key, value) in dictionary.iter() {
                audit_references_in_object(
                    document,
                    value,
                    &format!("{path}/{}", String::from_utf8_lossy(key)),
                    depth + 1,
                    audit,
                    control,
                )?;
                if audit.reference_limited {
                    break;
                }
            }
        }
        Object::Stream(stream) => {
            audit_references_in_object(
                document,
                &Object::Dictionary(stream.dict.clone()),
                path,
                depth + 1,
                audit,
                control,
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn inspect_fonts(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    summary: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let mut resource_audit = FontResourceAudit::default();
    let mut tree_state = ResourceTreeWalkState::default();
    for (index, (page_number, page_id)) in pages.iter().enumerate() {
        health_loop_checkpoint(
            control,
            41,
            53,
            index,
            pages.len(),
            64,
            "Inspecting page and Form fonts",
        )?;
        let resources = match page_resources(document, *page_id) {
            Ok(resources) => resources,
            Err(error) => {
                push_example(
                    &mut resource_audit.findings.invalid,
                    format!("page {page_number} resources: {error}"),
                );
                continue;
            }
        };
        walk_resource_tree(
            document,
            *page_number,
            &resources,
            &mut tree_state,
            &mut |resources, _, context, _| {
                inspect_fonts_in_resources(
                    document,
                    resources,
                    context,
                    &mut resource_audit,
                    summary,
                    control,
                )
            },
            control,
        )?;
        if resource_audit.limited {
            break;
        }
    }

    if !resource_audit.findings.invalid.is_empty() {
        push_finding(
            findings,
            "font-invalid-resources",
            FindingSeverity::Danger,
            "Invalid font resources detected",
            &format!(
                "One or more page or Form XObject font resources cannot be resolved safely: {}.",
                resource_audit.findings.invalid.join("; ")
            ),
            first_page_from_examples(&resource_audit.findings.invalid),
        );
    }
    if !resource_audit.findings.unembedded.is_empty() {
        push_finding(
            findings,
            "font-unembedded",
            FindingSeverity::Warning,
            "Fonts are not embedded",
            &format!(
                "One or more non-standard fonts may be substituted or missing on another device. Examples: {}.",
                resource_audit.findings.unembedded.join("; ")
            ),
            first_page_from_examples(&resource_audit.findings.unembedded),
        );
    }
    if !resource_audit.findings.standard_unembedded.is_empty() {
        push_finding(
            findings,
            "font-standard-unembedded",
            FindingSeverity::Info,
            "Standard PDF fonts are not embedded",
            &format!(
                "Reader substitution is expected for the standard fourteen fonts, but appearance can still vary. Examples: {}.",
                resource_audit.findings.standard_unembedded.join("; ")
            ),
            first_page_from_examples(&resource_audit.findings.standard_unembedded),
        );
    }
    if !resource_audit.findings.unicode_missing.is_empty() {
        push_finding(
            findings,
            "font-unicode-map",
            FindingSeverity::Warning,
            "Font Unicode maps are missing",
            &format!(
                "Composite fonts lack a ToUnicode map. Copying, search, OCR verification, and assistive technology may return incorrect text. Examples: {}.",
                resource_audit.findings.unicode_missing.join("; ")
            ),
            first_page_from_examples(&resource_audit.findings.unicode_missing),
        );
    }
    if !resource_audit.findings.unicode_missing_simple.is_empty() {
        push_finding(
            findings,
            "font-simple-unicode-map",
            FindingSeverity::Info,
            "Simple fonts rely on encoding tables",
            &format!(
                "These fonts have no explicit ToUnicode map. Their standard or custom encoding may still extract correctly, but copying and assistive technology should be checked: {}.",
                resource_audit.findings.unicode_missing_simple.join("; ")
            ),
            first_page_from_examples(&resource_audit.findings.unicode_missing_simple),
        );
    }
    if resource_audit.limited {
        push_finding(
            findings,
            "font-inspection-limit",
            FindingSeverity::Warning,
            "Font inspection reached its limit",
            &format!(
                "The PDF contains more than {MAX_UNIQUE_FONTS} unique font resources. Remaining fonts were not inspected."
            ),
            None,
        );
    }
    if tree_state.visit_limited || tree_state.depth_limited {
        push_finding(
            findings,
            "font-form-inspection-limit",
            FindingSeverity::Warning,
            "Nested font inspection reached its limit",
            &format!(
                "Font resources inside Form XObjects were inspected through at most {MAX_FORM_XOBJECT_DEPTH} levels and {MAX_FORM_XOBJECT_VISITS} contexts. One or more remaining branches were not inspected."
            ),
            None,
        );
    }
    Ok(())
}

fn inspect_fonts_in_resources(
    document: &Document,
    resources: &Dictionary,
    context: &str,
    audit: &mut FontResourceAudit,
    summary: &mut PdfTechnicalSummary,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if audit.limited {
        return Ok(());
    }
    let entries = match resource_entries(document, resources, b"Font") {
        Ok(entries) => entries,
        Err(error) => {
            push_example(
                &mut audit.findings.invalid,
                format!("{context} font dictionary: {error}"),
            );
            return Ok(());
        }
    };
    for (index, (resource_name, object)) in entries.into_iter().enumerate() {
        if index.is_multiple_of(128) {
            control.ensure_not_cancelled()?;
        }
        let identity = object_identity(&object);
        if audit.seen.contains(&identity) {
            continue;
        }
        if audit.seen.len() >= MAX_UNIQUE_FONTS {
            audit.limited = true;
            return Ok(());
        }
        audit.seen.insert(identity);
        summary.font_count += 1;
        let resource_name = display_resource_name(&resource_name);
        let label = format!("/{resource_name} in {context}");
        let font = match resolve_object(document, &object) {
            Ok(Object::Dictionary(dictionary)) => dictionary,
            Ok(Object::Stream(stream)) => &stream.dict,
            Ok(_) => {
                push_example(
                    &mut audit.findings.invalid,
                    format!("{label} is not a font dictionary"),
                );
                summary.unembedded_font_count += 1;
                continue;
            }
            Err(error) => {
                push_example(&mut audit.findings.invalid, format!("{label}: {error}"));
                summary.unembedded_font_count += 1;
                continue;
            }
        };
        let subtype = font
            .get(b"Subtype")
            .and_then(Object::as_name)
            .unwrap_or_default();
        let base_name = font
            .get(b"BaseFont")
            .and_then(Object::as_name)
            .ok()
            .map(display_resource_name)
            .unwrap_or(resource_name);
        let display = format!("{base_name} in {context}");
        if subtype.is_empty() {
            push_example(
                &mut audit.findings.invalid,
                format!("{display} has no valid font subtype"),
            );
        }
        let embedding = inspect_font_embedding(document, font, subtype, 0);
        if embedding.embedded {
            summary.embedded_font_count += 1;
        } else {
            summary.unembedded_font_count += 1;
            if is_standard_fourteen_font(&base_name) {
                push_example(&mut audit.findings.standard_unembedded, display.clone());
            } else {
                let reason = if embedding.malformed {
                    "has malformed descendant or descriptor data"
                } else if embedding.descriptor_present {
                    "has a descriptor but no embedded font programme"
                } else {
                    "has no embedded font programme"
                };
                push_example(
                    &mut audit.findings.unembedded,
                    format!("{display} {reason}"),
                );
            }
        }
        if !font.has(b"ToUnicode") && subtype != b"Type3" {
            summary.fonts_missing_unicode_map += 1;
            if subtype == b"Type0" {
                push_example(&mut audit.findings.unicode_missing, display);
            } else {
                push_example(&mut audit.findings.unicode_missing_simple, display);
            }
        }
    }
    Ok(())
}

fn inspect_font_embedding(
    document: &Document,
    font: &Dictionary,
    subtype: &[u8],
    depth: usize,
) -> FontEmbeddingStatus {
    if depth > 8 {
        return FontEmbeddingStatus {
            malformed: true,
            ..FontEmbeddingStatus::default()
        };
    }
    if subtype == b"Type3" {
        return FontEmbeddingStatus {
            descriptor_present: true,
            embedded: font.has(b"CharProcs"),
            malformed: !font.has(b"CharProcs"),
        };
    }
    if subtype == b"Type0" {
        let descendants = match font
            .get(b"DescendantFonts")
            .ok()
            .and_then(|value| resolve_object(document, value).ok())
            .and_then(|value| value.as_array().ok())
        {
            Some(values) if !values.is_empty() => values,
            _ => {
                return FontEmbeddingStatus {
                    malformed: true,
                    ..FontEmbeddingStatus::default()
                }
            }
        };
        let mut status = FontEmbeddingStatus {
            embedded: true,
            ..FontEmbeddingStatus::default()
        };
        for descendant in descendants {
            let Some(dictionary) = resolve_object(document, descendant)
                .ok()
                .and_then(object_dictionary)
            else {
                status.embedded = false;
                status.malformed = true;
                continue;
            };
            let descendant_subtype = dictionary
                .get(b"Subtype")
                .and_then(Object::as_name)
                .unwrap_or_default();
            let descendant_status =
                inspect_font_embedding(document, dictionary, descendant_subtype, depth + 1);
            status.descriptor_present |= descendant_status.descriptor_present;
            status.embedded &= descendant_status.embedded;
            status.malformed |= descendant_status.malformed;
        }
        return status;
    }

    let Some(descriptor) = font
        .get(b"FontDescriptor")
        .ok()
        .and_then(|value| resolve_object(document, value).ok())
        .and_then(object_dictionary)
    else {
        return FontEmbeddingStatus::default();
    };
    let mut status = FontEmbeddingStatus {
        descriptor_present: true,
        ..FontEmbeddingStatus::default()
    };
    for key in [
        b"FontFile".as_slice(),
        b"FontFile2".as_slice(),
        b"FontFile3".as_slice(),
    ] {
        let Some(file) = descriptor
            .get(key)
            .ok()
            .and_then(|value| resolve_object(document, value).ok())
        else {
            continue;
        };
        match file {
            Object::Stream(stream) if !stream.content.is_empty() => status.embedded = true,
            Object::Stream(_) => status.malformed = true,
            _ => status.malformed = true,
        }
    }
    status
}

fn inspect_colour_profiles(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    summary: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let mut audit = ColourAudit::default();
    let output_intents = document
        .catalog()
        .ok()
        .and_then(|catalogue| catalogue.get(b"OutputIntents").ok());
    match output_intents {
        None => push_finding(
            findings,
            "colour-output-intent-missing",
            FindingSeverity::Info,
            "No output colour intent",
            "The PDF does not declare a calibrated output condition. Screen appearance can still be acceptable, but print and archive workflows should review colour management.",
            None,
        ),
        Some(value) => match resolve_object(document, value).and_then(|value| {
            value
                .as_array()
                .map_err(|_| "OutputIntents is not an array".to_string())
        }) {
            Ok(intents) => {
                summary.output_intent_count = intents.len();
                if intents.is_empty() {
                    audit.issue("the OutputIntents array is empty".to_string());
                }
                for (index, intent) in intents.iter().enumerate() {
                    if index.is_multiple_of(64) {
                        control.ensure_not_cancelled()?;
                    }
                    let Some(dictionary) = resolve_object(document, intent)
                        .ok()
                        .and_then(object_dictionary)
                    else {
                        audit.issue(format!("output intent {} is not a dictionary", index + 1));
                        continue;
                    };
                    if dictionary
                        .get(b"S")
                        .and_then(Object::as_name)
                        .is_err()
                    {
                        audit.issue(format!("output intent {} has no valid subtype", index + 1));
                    }
                    if let Ok(profile) = dictionary.get(b"DestOutputProfile") {
                        inspect_icc_profile(
                            document,
                            profile,
                            &format!("output intent {}", index + 1),
                            &mut audit,
                        );
                    } else {
                        audit.issue(format!(
                            "output intent {} has no destination ICC profile",
                            index + 1
                        ));
                    }
                }
            }
            Err(error) => audit.issue(error),
        },
    }

    let mut tree_state = ResourceTreeWalkState::default();
    for (index, (page_number, page_id)) in pages.iter().enumerate() {
        health_loop_checkpoint(
            control,
            55,
            68,
            index,
            pages.len(),
            64,
            "Inspecting colour profiles and resources",
        )?;
        let resources = match page_resources(document, *page_id) {
            Ok(resources) => resources,
            Err(error) => {
                audit.issue(format!("page {page_number} resources: {error}"));
                continue;
            }
        };
        walk_resource_tree(
            document,
            *page_number,
            &resources,
            &mut tree_state,
            &mut |resources, _, context, page_number| {
                inspect_colour_resources(
                    document,
                    resources,
                    context,
                    page_number,
                    summary,
                    &mut audit,
                    control,
                )
            },
            control,
        )?;
    }
    if tree_state.resource_error_count > 0 {
        audit.issue(format!(
            "{} nested Form XObject resource branch{} could not be traversed safely. Examples: {}",
            tree_state.resource_error_count,
            if tree_state.resource_error_count == 1 {
                ""
            } else {
                "es"
            },
            tree_state.resource_error_examples.join("; ")
        ));
    }
    if tree_state.cycle_count > 0 {
        audit.issue(format!(
            "{} cyclic Form XObject resource branch{} was stopped. Examples: {}",
            tree_state.cycle_count,
            if tree_state.cycle_count == 1 {
                ""
            } else {
                "es"
            },
            tree_state.cycle_examples.join("; ")
        ));
    }
    if tree_state.visit_limited || tree_state.depth_limited {
        audit.issue(format!(
            "nested colour-resource inspection reached its {MAX_FORM_XOBJECT_DEPTH}-level or {MAX_FORM_XOBJECT_VISITS}-context limit"
        ));
    }
    summary.icc_profile_count = audit.profile_ids.len();
    summary.invalid_icc_profile_count = audit.invalid_profile_ids.len();
    summary.colour_issue_count += audit.issue_count;
    if !audit.examples.is_empty() {
        push_finding(
            findings,
            "colour-profile-invalid",
            FindingSeverity::Warning,
            "Colour-profile structures need review",
            &format!(
                "The PDF contains invalid or incomplete colour-management data: {}.",
                audit.examples.join("; ")
            ),
            first_page_from_examples(&audit.examples),
        );
    }
    Ok(())
}

fn inspect_colour_resources(
    document: &Document,
    resources: &Dictionary,
    context: &str,
    page_number: u32,
    summary: &mut PdfTechnicalSummary,
    audit: &mut ColourAudit,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    match resource_entries(document, resources, b"ColorSpace") {
        Ok(colour_spaces) => {
            for (index, (name, value)) in colour_spaces.into_iter().enumerate() {
                if index.is_multiple_of(128) {
                    control.ensure_not_cancelled()?;
                }
                inspect_colour_space(
                    document,
                    &value,
                    &format!(
                        "colour space /{} in {context}",
                        display_resource_name(&name)
                    ),
                    page_number,
                    summary,
                    audit,
                );
            }
        }
        Err(error) => audit.issue(format!("{context} colour-space dictionary: {error}")),
    }
    match resource_entries(document, resources, b"XObject") {
        Ok(xobjects) => {
            for (index, (name, value)) in xobjects.into_iter().enumerate() {
                if index.is_multiple_of(128) {
                    control.ensure_not_cancelled()?;
                }
                let Some(stream) =
                    resolve_object(document, &value)
                        .ok()
                        .and_then(|value| match value {
                            Object::Stream(stream) => Some(stream),
                            _ => None,
                        })
                else {
                    continue;
                };
                if stream
                    .dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|subtype| subtype == b"Image")
                {
                    if let Ok(colour_space) = stream.dict.get(b"ColorSpace") {
                        inspect_colour_space(
                            document,
                            colour_space,
                            &format!("image /{} in {context}", display_resource_name(&name)),
                            page_number,
                            summary,
                            audit,
                        );
                    }
                }
            }
        }
        Err(error) => audit.issue(format!("{context} XObject dictionary: {error}")),
    }
    Ok(())
}

fn inspect_colour_space(
    document: &Document,
    value: &Object,
    label: &str,
    page_number: u32,
    summary: &mut PdfTechnicalSummary,
    audit: &mut ColourAudit,
) {
    let Ok(value) = resolve_object(document, value) else {
        audit.issue(format!("{label} cannot be resolved"));
        return;
    };
    match value {
        Object::Name(name) if name == b"DeviceCMYK" => {
            summary.pages_using_device_cmyk.push(page_number);
        }
        Object::Array(values) if !values.is_empty() => {
            let family = values[0].as_name().unwrap_or_default();
            if family == b"ICCBased" {
                if let Some(profile) = values.get(1) {
                    inspect_icc_profile(document, profile, label, audit);
                } else {
                    audit.issue(format!("{label} has no ICC profile stream"));
                }
            } else if family == b"DeviceN" || family == b"Separation" {
                // Specialist colourants are valid but depend on the target output workflow.
            }
        }
        _ => {}
    }
}

fn inspect_icc_profile(document: &Document, value: &Object, label: &str, audit: &mut ColourAudit) {
    let identity = object_identity(value);
    if !audit.profile_ids.insert(identity.clone()) {
        return;
    }
    let Ok(Object::Stream(stream)) = resolve_object(document, value) else {
        audit.profile_issue(&identity, format!("{label} ICC profile is not a stream"));
        return;
    };
    let channels = stream.dict.get(b"N").and_then(Object::as_i64).ok();
    if !matches!(channels, Some(1 | 3 | 4)) {
        audit.profile_issue(
            &identity,
            format!("{label} ICC profile has an invalid component count"),
        );
    }
    let decoded = match stream.decompressed_content_with_limit(MAX_ICC_PROFILE_BYTES) {
        Ok(decoded) => decoded,
        Err(error) => {
            audit.profile_issue(
                &identity,
                format!(
                    "{label} ICC profile could not be decoded within the 16 MiB limit: {error}"
                ),
            );
            return;
        }
    };
    let validation = validate_icc_profile_bytes(
        &decoded,
        channels.and_then(|channels| u8::try_from(channels).ok()),
    );
    if validation.issue_count > 0 {
        audit.invalid_profile_ids.insert(identity);
        audit.issue_count += validation.issue_count;
        for issue in validation.examples {
            push_example(&mut audit.examples, format!("{label} ICC profile {issue}"));
        }
    }
}

fn validate_icc_profile_bytes(bytes: &[u8], declared_channels: Option<u8>) -> IccProfileValidation {
    let mut validation = IccProfileValidation::default();
    if bytes.len() < 132 {
        validation.issue("is shorter than the 132-byte header and tag-count minimum");
        return validation;
    }

    let declared_size = read_be_u32(bytes, 0).unwrap_or_default() as usize;
    if declared_size < 132 {
        validation.issue(format!(
            "declares an invalid {declared_size}-byte profile size"
        ));
    }
    if declared_size != bytes.len() {
        validation.issue(format!(
            "declares {declared_size} bytes but decodes to {} bytes",
            bytes.len()
        ));
    }
    if bytes[36..40] != *b"acsp" {
        validation.issue("does not contain the required 'acsp' file signature");
    }

    let major_version = bytes[8];
    if !matches!(major_version, 2 | 4) {
        validation.issue(format!(
            "uses unsupported ICC major version {major_version}"
        ));
    }
    let device_class = signature(bytes, 12);
    if !matches!(
        device_class.as_slice(),
        b"scnr" | b"mntr" | b"prtr" | b"link" | b"spac" | b"abst" | b"nmcl"
    ) {
        validation.issue(format!(
            "uses unknown device class '{}'",
            display_signature(&device_class)
        ));
    }
    let data_colour_space = signature(bytes, 16);
    let profile_channels = icc_colour_space_channels(&data_colour_space);
    if profile_channels.is_none() {
        validation.issue(format!(
            "uses unknown data colour space '{}'",
            display_signature(&data_colour_space)
        ));
    } else if declared_channels.is_some() && declared_channels != profile_channels {
        validation.issue(format!(
            "declares /N {} but its '{}' data colour space has {} components",
            declared_channels.unwrap_or_default(),
            display_signature(&data_colour_space),
            profile_channels.unwrap_or_default()
        ));
    }
    let connection_space = signature(bytes, 20);
    if !matches!(connection_space.as_slice(), b"XYZ " | b"Lab ") {
        validation.issue(format!(
            "uses invalid profile connection space '{}'",
            display_signature(&connection_space)
        ));
    }
    if !valid_icc_datetime(bytes) {
        validation.issue("contains an invalid creation date and time");
    }
    let rendering_intent = read_be_u32(bytes, 64).unwrap_or(u32::MAX);
    if rendering_intent > 3 {
        validation.issue(format!("uses invalid rendering intent {rendering_intent}"));
    }
    if bytes[100..128].iter().any(|byte| *byte != 0) {
        validation.issue("uses non-zero reserved header bytes");
    }

    let tag_count = read_be_u32(bytes, 128).unwrap_or_default() as usize;
    if tag_count == 0 {
        validation.issue("contains no tag records");
        return validation;
    }
    if tag_count > MAX_ICC_TAGS {
        validation.issue(format!(
            "declares {tag_count} tags, above the {MAX_ICC_TAGS}-tag inspection limit"
        ));
        return validation;
    }
    let Some(table_bytes) = tag_count.checked_mul(12) else {
        validation.issue("has an overflowing tag-table length");
        return validation;
    };
    let Some(table_end) = 132_usize.checked_add(table_bytes) else {
        validation.issue("has an overflowing tag-table boundary");
        return validation;
    };
    let bounded_profile_size = declared_size.min(bytes.len());
    if table_end > bounded_profile_size {
        validation.issue("has a tag table outside the declared profile");
        return validation;
    }

    let mut tag_signatures = HashSet::new();
    for index in 0..tag_count {
        let entry = 132 + index * 12;
        let tag_signature = signature(bytes, entry);
        if !tag_signatures.insert(tag_signature) {
            validation.issue(format!(
                "contains duplicate tag signature '{}'",
                display_signature(&tag_signature)
            ));
        }
        let offset = read_be_u32(bytes, entry + 4).unwrap_or_default() as usize;
        let size = read_be_u32(bytes, entry + 8).unwrap_or_default() as usize;
        let Some(end) = offset.checked_add(size) else {
            validation.issue(format!("tag {} has an overflowing data range", index + 1));
            continue;
        };
        if offset < table_end || !offset.is_multiple_of(4) {
            validation.issue(format!(
                "tag {} has an invalid or unaligned data offset",
                index + 1
            ));
        }
        if size < 8 {
            validation.issue(format!("tag {} is shorter than 8 bytes", index + 1));
        }
        if end > bounded_profile_size {
            validation.issue(format!(
                "tag {} extends outside the declared profile",
                index + 1
            ));
            continue;
        }
        if size >= 8 {
            let type_signature = signature(bytes, offset);
            if !type_signature
                .iter()
                .all(|byte| matches!(byte, 0x20..=0x7e))
            {
                validation.issue(format!(
                    "tag {} has an invalid data-type signature",
                    index + 1
                ));
            }
            if bytes[offset + 4..offset + 8].iter().any(|byte| *byte != 0) {
                validation.issue(format!(
                    "tag {} has non-zero reserved type bytes",
                    index + 1
                ));
            }
        }
    }
    validation
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn signature(bytes: &[u8], offset: usize) -> [u8; 4] {
    bytes
        .get(offset..offset.saturating_add(4))
        .and_then(|value| value.try_into().ok())
        .unwrap_or_default()
}

fn display_signature(value: &[u8; 4]) -> String {
    value
        .iter()
        .map(|byte| {
            if matches!(byte, 0x20..=0x7e) {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect()
}

fn icc_colour_space_channels(value: &[u8; 4]) -> Option<u8> {
    match value.as_slice() {
        b"GRAY" => Some(1),
        b"2CLR" => Some(2),
        b"XYZ " | b"Lab " | b"Luv " | b"YCbr" | b"Yxy " | b"RGB " | b"HSV " | b"HLS " | b"CMY " => {
            Some(3)
        }
        b"CMYK" | b"4CLR" => Some(4),
        b"5CLR" => Some(5),
        b"6CLR" => Some(6),
        b"7CLR" => Some(7),
        b"8CLR" => Some(8),
        b"9CLR" => Some(9),
        b"ACLR" => Some(10),
        b"BCLR" => Some(11),
        b"CCLR" => Some(12),
        b"DCLR" => Some(13),
        b"ECLR" => Some(14),
        b"FCLR" => Some(15),
        _ => None,
    }
}

fn valid_icc_datetime(bytes: &[u8]) -> bool {
    let values =
        [24, 26, 28, 30, 32, 34].map(|offset| read_be_u16(bytes, offset).unwrap_or_default());
    let [year, month, day, hour, minute, second] = values;
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => 0,
    };
    (1900..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=days_in_month).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 59
}

fn page_resources(document: &Document, page_id: ObjectId) -> Result<Dictionary, String> {
    let Some(value) = inherited_page_value(document, page_id, b"Resources")? else {
        return Ok(Dictionary::new());
    };
    resolve_object(document, &value)?
        .as_dict()
        .cloned()
        .map_err(|_| "the page Resources entry is not a dictionary".to_string())
}

fn resource_entries(
    document: &Document,
    resources: &Dictionary,
    key: &[u8],
) -> Result<Vec<(Vec<u8>, Object)>, String> {
    let Some(value) = resources.get(key).ok() else {
        return Ok(Vec::new());
    };
    let dictionary = resolve_object(document, value)?.as_dict().map_err(|_| {
        format!(
            "the {} resource entry is not a dictionary",
            String::from_utf8_lossy(key)
        )
    })?;
    Ok(dictionary
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect())
}

fn walk_resource_tree<F>(
    document: &Document,
    page_number: u32,
    resources: &Dictionary,
    state: &mut ResourceTreeWalkState,
    callback: &mut F,
    control: &PdfJobExecutionControl,
) -> Result<(), String>
where
    F: FnMut(&Dictionary, Option<&Stream>, &str, u32) -> Result<(), String>,
{
    control.ensure_not_cancelled()?;
    let context = format!("page {page_number}");
    callback(resources, None, &context, page_number)?;
    if state.visit_limited {
        return Ok(());
    }
    walk_nested_form_resources(
        document,
        resources,
        ResourceTreeWalkFrame {
            allowed_xobjects: None,
            context: &context,
            depth: 0,
            page_number,
        },
        state,
        callback,
        control,
    )
}

fn walk_nested_form_resources<F>(
    document: &Document,
    resources: &Dictionary,
    frame: ResourceTreeWalkFrame<'_>,
    state: &mut ResourceTreeWalkState,
    callback: &mut F,
    control: &PdfJobExecutionControl,
) -> Result<(), String>
where
    F: FnMut(&Dictionary, Option<&Stream>, &str, u32) -> Result<(), String>,
{
    control.ensure_not_cancelled()?;
    if state.visit_limited {
        return Ok(());
    }
    if frame.depth >= MAX_FORM_XOBJECT_DEPTH {
        state.depth_limited = true;
        return Ok(());
    }
    let xobjects = match resource_entries(document, resources, b"XObject") {
        Ok(xobjects) => xobjects,
        Err(error) => {
            state.resource_error(format!("{} XObject dictionary: {error}", frame.context));
            return Ok(());
        }
    };
    for (name, value) in xobjects {
        control.ensure_not_cancelled()?;
        if frame
            .allowed_xobjects
            .is_some_and(|allowed| !allowed.contains(&name))
        {
            continue;
        }
        let display_name = display_resource_name(&name);
        let stream = match resolve_object(document, &value) {
            Ok(Object::Stream(stream)) => stream,
            Ok(_) => continue,
            Err(error) => {
                state.resource_error(format!(
                    "{} XObject /{display_name}: {error}",
                    frame.context
                ));
                continue;
            }
        };
        if !stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|subtype| subtype == b"Form")
        {
            continue;
        }

        let form_context = if frame.context == format!("page {}", frame.page_number) {
            format!("form /{display_name} on page {}", frame.page_number)
        } else {
            format!("form /{display_name} inside {}", frame.context)
        };
        let identity = match value {
            Object::Reference(id) => format!("{}:{}", id.0, id.1),
            _ => format!("direct:{form_context}"),
        };
        if state.active_forms.contains(&identity) {
            state.cycle(format!("{form_context} refers back to an active form"));
            continue;
        }
        if !state.unique_forms.contains(&identity)
            && state.unique_forms.len() >= MAX_FORM_XOBJECT_VISITS
        {
            state.visit_limited = true;
            return Ok(());
        }
        state.unique_forms.insert(identity.clone());
        let (form_resources, owns_resources) =
            match resolved_form_resources(document, stream, resources) {
                Ok(resources) => resources,
                Err(error) => {
                    state.resource_error(format!("{form_context} resources: {error}"));
                    continue;
                }
            };
        let visit_key = if owns_resources {
            format!("{}:{identity}", frame.page_number)
        } else {
            format!(
                "{}:{identity}:inherited:{}",
                frame.page_number, frame.context
            )
        };
        if state.visited_contexts.contains(&visit_key) {
            continue;
        }
        if state.visit_count >= MAX_FORM_XOBJECT_VISITS {
            state.visit_limited = true;
            return Ok(());
        }
        state.visited_contexts.insert(visit_key);
        state.visit_count += 1;
        state.active_forms.insert(identity.clone());
        callback(
            form_resources,
            Some(stream),
            &form_context,
            frame.page_number,
        )?;
        let inherited_invocations = if owns_resources {
            None
        } else {
            Some(invoked_form_xobject_names(stream, state, control)?)
        };
        walk_nested_form_resources(
            document,
            form_resources,
            ResourceTreeWalkFrame {
                allowed_xobjects: inherited_invocations.as_ref(),
                context: &form_context,
                depth: frame.depth + 1,
                page_number: frame.page_number,
            },
            state,
            callback,
            control,
        )?;
        state.active_forms.remove(&identity);
        if state.visit_limited {
            return Ok(());
        }
    }
    Ok(())
}

fn invoked_form_xobject_names(
    stream: &Stream,
    state: &mut ResourceTreeWalkState,
    control: &PdfJobExecutionControl,
) -> Result<HashSet<Vec<u8>>, String> {
    let Ok(bytes) = stream.decompressed_content_with_limit(MAX_PAGE_CONTENT_BYTES) else {
        return Ok(HashSet::new());
    };
    let Ok(content) = Content::decode_strict(&bytes) else {
        return Ok(HashSet::new());
    };
    let mut names = HashSet::new();
    for (index, operation) in content.operations.into_iter().enumerate() {
        if index.is_multiple_of(2_048) {
            control.ensure_not_cancelled()?;
        }
        if operation.operator == "Do" {
            if let Some(name) = operation
                .operands
                .first()
                .and_then(|operand| operand.as_name().ok())
            {
                if names.len() >= MAX_FORM_XOBJECT_VISITS {
                    state.visit_limited = true;
                    break;
                }
                names.insert(name.to_vec());
            }
        }
    }
    Ok(names)
}

fn resolved_form_resources<'a>(
    document: &'a Document,
    stream: &'a Stream,
    inherited: &'a Dictionary,
) -> Result<(&'a Dictionary, bool), String> {
    let Ok(value) = stream.dict.get(b"Resources") else {
        return Ok((inherited, false));
    };
    let resources = resolve_object(document, value)?
        .as_dict()
        .map_err(|_| "the Resources entry is not a dictionary".to_string())?;
    Ok((resources, true))
}

fn display_resource_name(name: &[u8]) -> String {
    let display = String::from_utf8_lossy(name)
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect::<String>();
    if display.is_empty() {
        "unnamed".to_string()
    } else {
        display
    }
}

fn resource_inventory(
    document: &Document,
    resources: &Dictionary,
) -> Result<PageResourceInventory, String> {
    Ok(PageResourceInventory {
        colour_spaces: resource_names(document, resources, b"ColorSpace")?,
        ext_gstates: resource_names(document, resources, b"ExtGState")?,
        fonts: resource_names(document, resources, b"Font")?,
        properties: resource_names(document, resources, b"Properties")?,
        shadings: resource_names(document, resources, b"Shading")?,
        xobjects: resource_names(document, resources, b"XObject")?,
    })
}

fn resource_names(
    document: &Document,
    resources: &Dictionary,
    key: &[u8],
) -> Result<HashSet<Vec<u8>>, String> {
    Ok(resource_entries(document, resources, key)?
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

fn resolve_object<'a>(document: &'a Document, object: &'a Object) -> Result<&'a Object, String> {
    match object {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| format!("broken reference {} {} R: {error}", id.0, id.1)),
        _ => Ok(object),
    }
}

fn object_dictionary(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn object_identity(object: &Object) -> String {
    match object {
        Object::Reference(id) => format!("{}:{}", id.0, id.1),
        _ => {
            let mut hasher = DefaultHasher::new();
            format!("{object:?}").hash(&mut hasher);
            format!("direct:{:016x}", hasher.finish())
        }
    }
}

fn is_standard_fourteen_font(name: &str) -> bool {
    let name = name
        .split_once('+')
        .map(|(_, suffix)| suffix)
        .unwrap_or(name);
    matches!(
        name,
        "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Symbol"
            | "ZapfDingbats"
    )
}

fn push_example(examples: &mut Vec<String>, value: String) {
    if examples.len() < 8 {
        examples.push(value);
    }
}

fn first_page_from_examples(examples: &[String]) -> Option<u32> {
    examples.iter().find_map(|example| {
        let (_, suffix) = example.split_once("page ")?;
        suffix
            .split(|character: char| !character.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    })
}

fn inspect_pages(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    technical: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
    control: &PdfJobExecutionControl,
) -> Result<(Vec<u32>, Vec<Vec<u32>>), String> {
    let mut blank_pages = Vec::new();
    let mut page_fingerprints: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut seen_images = HashSet::new();
    let mut page_sizes = HashSet::new();
    let mut form_tree_state = ResourceTreeWalkState::default();

    for (index, (page_number, page_id)) in pages.iter().enumerate() {
        health_loop_checkpoint(
            control,
            78,
            96,
            index,
            pages.len(),
            16,
            "Inspecting page content and images",
        )?;
        let geometry = page_geometry(document, *page_id);
        if let Ok((width, height)) = geometry.as_ref() {
            page_sizes.insert((width.round() as i64, height.round() as i64));
            let aspect = (width / height).max(height / width);
            if *width < 36.0
                || *height < 36.0
                || *width > 14_400.0
                || *height > 14_400.0
                || aspect > 5.0
            {
                push_finding(
                    findings,
                    &format!("unusual-page-{page_number}"),
                    FindingSeverity::Warning,
                    "Unusual page geometry",
                    &format!(
                        "Page {page_number} is {:.1} x {:.1} points and may need review before printing or conversion.",
                        width, height
                    ),
                    Some(*page_number),
                );
            }
        } else if let Err(error) = geometry.as_ref() {
            push_finding(
                findings,
                &format!("invalid-page-box-{page_number}"),
                FindingSeverity::Warning,
                "Invalid page dimensions",
                &format!(
                    "Page {page_number} does not have a dependable media or crop box: {error}"
                ),
                Some(*page_number),
            );
        }

        let content = match document.get_page_content_with_limit(*page_id, MAX_PAGE_CONTENT_BYTES) {
            Ok(content) => {
                match Content::decode_strict(&content) {
                    Ok(decoded) => inspect_page_content_resources(
                        document,
                        PageContentInspectionFrame {
                            page_id: *page_id,
                            page_number: *page_number,
                        },
                        Some(&decoded),
                        technical,
                        findings,
                        &mut form_tree_state,
                        control,
                    )?,
                    Err(error) => {
                        technical.page_content_error_count += 1;
                        push_finding(
                            findings,
                            &format!("malformed-page-content-{page_number}"),
                            FindingSeverity::Danger,
                            "Malformed page content stream",
                            &format!(
                                "Page {page_number} content cannot be decoded as valid PDF drawing operations: {error}"
                            ),
                            Some(*page_number),
                        );
                        inspect_page_content_resources(
                            document,
                            PageContentInspectionFrame {
                                page_id: *page_id,
                                page_number: *page_number,
                            },
                            None,
                            technical,
                            findings,
                            &mut form_tree_state,
                            control,
                        )?;
                    }
                }
                Some(content)
            }
            Err(error) => {
                technical.page_content_error_count += 1;
                push_finding(
                    findings,
                    &format!("large-page-stream-{page_number}"),
                    FindingSeverity::Danger,
                    "Page content could not be inspected safely",
                    &format!(
                        "Page {page_number} could not be decoded within the 32 MB inspection limit: {error}"
                    ),
                    Some(*page_number),
                );
                None
            }
        };
        let images = document.get_page_images(*page_id).unwrap_or_default();
        for (image_index, image) in images.iter().enumerate() {
            if image_index.is_multiple_of(128) {
                control.ensure_not_cancelled()?;
            }
            if !seen_images.insert(image.id) {
                continue;
            }
            let pixels = image.width.saturating_mul(image.height);
            if pixels > OVERSIZED_IMAGE_PIXELS || image.content.len() > OVERSIZED_IMAGE_BYTES {
                push_finding(
                    findings,
                    &format!("oversized-image-{}-{}", image.id.0, image.id.1),
                    FindingSeverity::Warning,
                    "Oversized image resource",
                    &format!(
                        "Page {page_number} uses an image of {} x {} pixels ({:.1} MB compressed).",
                        image.width,
                        image.height,
                        image.content.len() as f64 / (1024.0 * 1024.0)
                    ),
                    Some(*page_number),
                );
            }
        }
        let annotations = document.get_page_annotations(*page_id).unwrap_or_default();
        if let Some(content) = content.as_ref() {
            if !has_visible_page_content(content) && images.is_empty() && annotations.is_empty() {
                blank_pages.push(*page_number);
            }
            if !blank_pages.contains(page_number) {
                let fingerprint = fingerprint_page(
                    document,
                    *page_id,
                    content,
                    &images,
                    &annotations,
                    geometry.ok(),
                    control,
                )?;
                page_fingerprints
                    .entry(fingerprint)
                    .or_default()
                    .push(*page_number);
            }
        }
    }

    finalise_form_resource_findings(&form_tree_state, technical, findings);
    if page_sizes.len() > 5 {
        push_finding(
            findings,
            "mixed-page-sizes",
            FindingSeverity::Info,
            "Many page sizes detected",
            &format!(
                "The PDF contains at least {} distinct page sizes. Check layout before printing or combining it with other files.",
                page_sizes.len()
            ),
            None,
        );
    }
    if !blank_pages.is_empty() {
        push_finding(
            findings,
            "likely-blank-pages",
            FindingSeverity::Info,
            "Likely blank pages detected",
            &format!(
                "Review page{} {} before removal.",
                if blank_pages.len() == 1 { "" } else { "s" },
                page_list(&blank_pages)
            ),
            blank_pages.first().copied(),
        );
    }
    let mut duplicate_groups = page_fingerprints
        .into_values()
        .filter(|group| group.len() > 1)
        .collect::<Vec<_>>();
    duplicate_groups.sort_by_key(|group| group[0]);
    for (index, group) in duplicate_groups.iter().enumerate() {
        push_finding(
            findings,
            &format!("likely-duplicate-group-{}", index + 1),
            FindingSeverity::Info,
            "Likely duplicate pages detected",
            &format!("Pages {} have matching page content and image resources. Review them before removal.", page_list(group)),
            group.first().copied(),
        );
    }
    Ok((blank_pages, duplicate_groups))
}

fn inspect_page_content_resources(
    document: &Document,
    frame: PageContentInspectionFrame,
    content: Option<&Content>,
    technical: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
    tree_state: &mut ResourceTreeWalkState,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let page_number = frame.page_number;
    let resources = match page_resources(document, frame.page_id) {
        Ok(resources) => resources,
        Err(error) => {
            technical.page_content_error_count += 1;
            push_finding(
                findings,
                &format!("resource-page-dictionary-{page_number}"),
                FindingSeverity::Danger,
                "Invalid page resource dictionary",
                &format!("Page {page_number} resources cannot be inspected: {error}"),
                Some(page_number),
            );
            return Ok(());
        }
    };
    let mut audit = PageContentResourceAudit::default();
    let mut uses_device_cmyk = false;
    if let Some(content) = content {
        if let Err(error) = inspect_content_resource_operations(
            document,
            content,
            &resources,
            &format!("page {page_number}"),
            &mut audit,
            &mut uses_device_cmyk,
            control,
        ) {
            if error == PDF_JOB_CANCELLED_ERROR {
                return Err(error);
            }
            technical.page_content_error_count += 1;
            push_finding(
                findings,
                &format!("resource-page-dictionary-{page_number}"),
                FindingSeverity::Danger,
                "Invalid page resource dictionary",
                &format!("Page {page_number} resources cannot be inspected: {error}"),
                Some(page_number),
            );
        }
    }
    walk_resource_tree(
        document,
        page_number,
        &resources,
        tree_state,
        &mut |form_resources, form, context, _| {
            let Some(form) = form else {
                return Ok(());
            };
            let bytes = match form.decompressed_content_with_limit(MAX_PAGE_CONTENT_BYTES) {
                Ok(bytes) => bytes,
                Err(error) => {
                    audit.form_content_error(format!(
                        "{context} could not be decoded within the 32 MiB limit: {error}"
                    ));
                    return Ok(());
                }
            };
            let content = match Content::decode_strict(&bytes) {
                Ok(content) => content,
                Err(error) => {
                    audit.form_content_error(format!(
                        "{context} contains malformed drawing operations: {error}"
                    ));
                    return Ok(());
                }
            };
            if let Err(error) = inspect_content_resource_operations(
                document,
                &content,
                form_resources,
                context,
                &mut audit,
                &mut uses_device_cmyk,
                control,
            ) {
                if error == PDF_JOB_CANCELLED_ERROR {
                    return Err(error);
                }
                audit.form_resource_error(format!("{context}: {error}"));
            }
            Ok(())
        },
        control,
    )?;

    if uses_device_cmyk {
        technical.pages_using_device_cmyk.push(page_number);
    }
    if !audit.missing.is_empty() {
        let missing_count = audit.missing.len();
        technical.missing_resource_count += missing_count;
        let mut labels = audit.missing.into_iter().collect::<Vec<_>>();
        labels.sort();
        labels.truncate(8);
        let bound = if audit.missing_limited {
            format!("at least {MAX_RESOURCE_ISSUES_PER_PAGE}")
        } else {
            missing_count.to_string()
        };
        push_finding(
            findings,
            &format!("missing-resource-page-{page_number}"),
            FindingSeverity::Danger,
            "Page or Form XObject content uses missing resources",
            &format!(
                "Page {page_number} and its nested Form XObjects request {bound} resource reference{} that are absent from the applicable Resources dictionary. Examples: {}. Rendering may be incomplete.",
                if bound == "1" { "" } else { "s" },
                labels.join(", ")
            ),
            Some(page_number),
        );
    }
    if audit.malformed_operator_count > 0 {
        technical.page_content_error_count += 1;
        push_finding(
            findings,
            &format!("malformed-page-operands-{page_number}"),
            FindingSeverity::Warning,
            "Drawing operators have invalid resource operands",
            &format!(
                "Page {page_number} or its nested forms contain {} drawing operator{} without the expected named resource operand. Contexts: {}.",
                audit.malformed_operator_count,
                if audit.malformed_operator_count == 1 { "" } else { "s" },
                audit.malformed_operator_examples.join(", ")
            ),
            Some(page_number),
        );
    }
    if audit.form_content_error_count > 0 {
        technical.form_content_error_count += audit.form_content_error_count;
        push_finding(
            findings,
            &format!("malformed-form-content-page-{page_number}"),
            FindingSeverity::Danger,
            "Form XObject content could not be inspected",
            &format!(
                "{} nested Form XObject content stream{} could not be decoded safely. Examples: {}.",
                audit.form_content_error_count,
                if audit.form_content_error_count == 1 { "" } else { "s" },
                audit.form_content_examples.join("; ")
            ),
            Some(page_number),
        );
    }
    if audit.form_resource_error_count > 0 {
        technical.form_resource_error_count += audit.form_resource_error_count;
        push_finding(
            findings,
            &format!("invalid-form-resources-page-{page_number}"),
            FindingSeverity::Danger,
            "Form XObject resources could not be inspected",
            &format!(
                "{} nested Form XObject resource dictionar{} could not be resolved safely. Examples: {}.",
                audit.form_resource_error_count,
                if audit.form_resource_error_count == 1 { "y" } else { "ies" },
                audit.form_resource_examples.join("; ")
            ),
            Some(page_number),
        );
    }
    Ok(())
}

fn inspect_content_resource_operations(
    document: &Document,
    content: &Content,
    resources: &Dictionary,
    context: &str,
    audit: &mut PageContentResourceAudit,
    uses_device_cmyk: &mut bool,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let inventory = resource_inventory(document, resources)?;
    for (index, operation) in content.operations.iter().enumerate() {
        if index.is_multiple_of(2_048) {
            control.ensure_not_cancelled()?;
        }
        let requirement = match operation.operator.as_str() {
            "Tf" => Some(("font", operation.operands.first(), &inventory.fonts)),
            "Do" => Some(("XObject", operation.operands.first(), &inventory.xobjects)),
            "gs" => Some((
                "graphics state",
                operation.operands.first(),
                &inventory.ext_gstates,
            )),
            "sh" => Some(("shading", operation.operands.first(), &inventory.shadings)),
            "CS" | "cs" => {
                if let Some(operand) = operation.operands.first() {
                    if operand.as_name().is_ok_and(|name| name == b"DeviceCMYK") {
                        *uses_device_cmyk = true;
                        None
                    } else if operand.as_name().is_ok_and(is_builtin_colour_space) {
                        None
                    } else {
                        Some(("colour space", Some(operand), &inventory.colour_spaces))
                    }
                } else {
                    audit.malformed_operator(context);
                    None
                }
            }
            "BDC" | "DP" => operation
                .operands
                .get(1)
                .filter(|operand| matches!(operand, Object::Name(_)))
                .map(|operand| ("property list", Some(operand), &inventory.properties)),
            "K" | "k" => {
                *uses_device_cmyk = true;
                None
            }
            _ => None,
        };
        let Some((kind, operand, available)) = requirement else {
            continue;
        };
        let Some(name) = operand.and_then(|value| value.as_name().ok()) else {
            audit.malformed_operator(context);
            continue;
        };
        if !available.contains(name) {
            audit.missing(kind, name, context);
        }
    }
    Ok(())
}

fn finalise_form_resource_findings(
    state: &ResourceTreeWalkState,
    technical: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
) {
    technical.form_xobject_count = state.unique_forms.len();
    technical.form_resource_error_count += state.resource_error_count + state.cycle_count;
    if state.resource_error_count > 0 {
        push_finding(
            findings,
            "form-xobject-resources-invalid",
            FindingSeverity::Danger,
            "Invalid Form XObject resource branches",
            &format!(
                "{} nested Form XObject resource branch{} could not be resolved safely. Examples: {}.",
                state.resource_error_count,
                if state.resource_error_count == 1 { "" } else { "es" },
                state.resource_error_examples.join("; ")
            ),
            first_page_from_examples(&state.resource_error_examples),
        );
    }
    if state.cycle_count > 0 {
        push_finding(
            findings,
            "form-xobject-resource-cycle",
            FindingSeverity::Danger,
            "Cyclic Form XObject resources detected",
            &format!(
                "{} nested Form XObject branch{} refers back to an active form. Recursive rendering was not followed. Examples: {}.",
                state.cycle_count,
                if state.cycle_count == 1 { "" } else { "es" },
                state.cycle_examples.join("; ")
            ),
            first_page_from_examples(&state.cycle_examples),
        );
    }
    if state.visit_limited || state.depth_limited {
        push_finding(
            findings,
            "form-xobject-inspection-limit",
            FindingSeverity::Warning,
            "Form XObject inspection reached its limit",
            &format!(
                "Nested Form XObjects were inspected through at most {MAX_FORM_XOBJECT_DEPTH} levels and {MAX_FORM_XOBJECT_VISITS} page-specific contexts. One or more remaining branches were not inspected."
            ),
            None,
        );
    }
}

fn is_builtin_colour_space(name: &[u8]) -> bool {
    matches!(
        name,
        b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Pattern"
    )
}

fn finalise_technical_findings(
    technical: &mut PdfTechnicalSummary,
    findings: &mut Vec<HealthFinding>,
) {
    technical.pages_using_device_cmyk.sort_unstable();
    technical.pages_using_device_cmyk.dedup();
    if !technical.pages_using_device_cmyk.is_empty() && technical.output_intent_count == 0 {
        technical.colour_issue_count += 1;
        push_finding(
            findings,
            "colour-device-cmyk-unmanaged",
            FindingSeverity::Warning,
            "Device CMYK has no output profile",
            &format!(
                "Page{} {} use device-dependent CMYK without a document output intent. Printed colour can vary between devices and workflows.",
                if technical.pages_using_device_cmyk.len() == 1 { "" } else { "s" },
                page_list(&technical.pages_using_device_cmyk)
            ),
            technical.pages_using_device_cmyk.first().copied(),
        );
    }
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<(f64, f64), String> {
    let value = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "no page box was found".to_string())?;
    let value = match value {
        Object::Reference(id) => document.get_object(id).map_err(|error| error.to_string())?,
        ref value => value,
    };
    let values = value
        .as_array()
        .map_err(|_| "the page box is not an array".to_string())?;
    if values.len() != 4 {
        return Err("the page box does not contain four coordinates".to_string());
    }
    let numbers = values
        .iter()
        .map(pdf_number)
        .collect::<Result<Vec<_>, _>>()?;
    let width = numbers[2] - numbers[0];
    let height = numbers[3] - numbers[1];
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Err("the page box has invalid dimensions".to_string());
    }
    Ok((width, height))
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
            return Err("the page tree is cyclic".to_string());
        }
        let dictionary = document
            .get_dictionary(current_id)
            .map_err(|error| error.to_string())?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value.clone()));
        }
        match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent_id) => current_id = parent_id,
            Err(_) => return Ok(None),
        }
    }
    Err("the page tree is too deeply nested".to_string())
}

fn pdf_number(value: &Object) -> Result<f64, String> {
    match value {
        Object::Integer(value) => Ok(*value as f64),
        Object::Real(value) => Ok(f64::from(*value)),
        _ => Err("a page coordinate is not numeric".to_string()),
    }
}

fn has_visible_page_content(bytes: &[u8]) -> bool {
    let Ok(content) = Content::decode(bytes) else {
        return !bytes.iter().all(u8::is_ascii_whitespace);
    };
    content.operations.iter().any(|operation| {
        matches!(
            operation.operator.as_str(),
            "Tj" | "TJ"
                | "'"
                | "\""
                | "Do"
                | "BI"
                | "sh"
                | "S"
                | "s"
                | "f"
                | "F"
                | "f*"
                | "B"
                | "B*"
                | "b"
                | "b*"
        )
    })
}

fn fingerprint_page(
    document: &Document,
    page_id: ObjectId,
    content: &[u8],
    images: &[lopdf::xobject::PdfImage<'_>],
    annotations: &[&Dictionary],
    geometry: Option<(f64, f64)>,
    control: &PdfJobExecutionControl,
) -> Result<u64, String> {
    control.ensure_not_cancelled()?;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    geometry
        .map(|(width, height)| (width.to_bits(), height.to_bits()))
        .hash(&mut hasher);
    for (index, image) in images.iter().enumerate() {
        if index.is_multiple_of(64) {
            control.ensure_not_cancelled()?;
        }
        image.width.hash(&mut hasher);
        image.height.hash(&mut hasher);
        image.bits_per_component.hash(&mut hasher);
        image.filters.hash(&mut hasher);
        image.content.hash(&mut hasher);
    }
    for (index, annotation) in annotations.iter().enumerate() {
        if index.is_multiple_of(256) {
            control.ensure_not_cancelled()?;
        }
        format!("{annotation:?}").hash(&mut hasher);
    }
    if let Ok(fonts) = document.get_page_fonts(page_id) {
        for (index, (name, font)) in fonts.into_iter().enumerate() {
            if index.is_multiple_of(128) {
                control.ensure_not_cancelled()?;
            }
            name.hash(&mut hasher);
            format!("{font:?}").hash(&mut hasher);
        }
    }
    Ok(hasher.finish())
}

fn document_contains_key_with_control(
    document: &Document,
    key: &[u8],
    control: &PdfJobExecutionControl,
) -> Result<bool, String> {
    document_dictionaries_any_with_control(document, &|dictionary| dictionary.has(key), control)
}

pub(crate) fn document_has_certificate_signature(document: &Document) -> bool {
    document_dictionaries_any(document, &|dictionary| {
        dictionary.has(b"ByteRange")
            || dictionary
                .get(b"FT")
                .and_then(Object::as_name)
                .is_ok_and(|name| name == b"Sig")
    })
}

pub(crate) fn document_has_certificate_signature_with_control(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<bool, String> {
    document_dictionaries_any_with_control(
        document,
        &|dictionary| {
            dictionary.has(b"ByteRange")
                || dictionary
                    .get(b"FT")
                    .and_then(Object::as_name)
                    .is_ok_and(|name| name == b"Sig")
        },
        control,
    )
}

fn document_dictionaries_any(
    document: &Document,
    predicate: &impl Fn(&Dictionary) -> bool,
) -> bool {
    document_dictionaries_any_with_control(document, predicate, &PdfJobExecutionControl::direct())
        .unwrap_or(false)
}

fn document_dictionaries_any_with_control(
    document: &Document,
    predicate: &impl Fn(&Dictionary) -> bool,
    control: &PdfJobExecutionControl,
) -> Result<bool, String> {
    let mut nodes = 0_usize;
    for object in document.objects.values() {
        if object_dictionaries_any_with_control(object, predicate, 0, &mut nodes, control)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn object_dictionaries_any_with_control(
    object: &Object,
    predicate: &impl Fn(&Dictionary) -> bool,
    depth: usize,
    nodes: &mut usize,
    control: &PdfJobExecutionControl,
) -> Result<bool, String> {
    *nodes = nodes.saturating_add(1);
    if nodes.is_multiple_of(256) {
        control.ensure_not_cancelled()?;
    }
    if depth > 64 {
        return Ok(false);
    }
    match object {
        Object::Dictionary(dictionary) => {
            if predicate(dictionary) {
                return Ok(true);
            }
            for (_, value) in dictionary.iter() {
                if object_dictionaries_any_with_control(
                    value,
                    predicate,
                    depth + 1,
                    nodes,
                    control,
                )? {
                    return Ok(true);
                }
            }
        }
        Object::Stream(stream) => {
            if predicate(&stream.dict) {
                return Ok(true);
            }
            for (_, value) in stream.dict.iter() {
                if object_dictionaries_any_with_control(
                    value,
                    predicate,
                    depth + 1,
                    nodes,
                    control,
                )? {
                    return Ok(true);
                }
            }
        }
        Object::Array(values) => {
            for value in values {
                if object_dictionaries_any_with_control(
                    value,
                    predicate,
                    depth + 1,
                    nodes,
                    control,
                )? {
                    return Ok(true);
                }
            }
        }
        _ => {}
    }
    Ok(false)
}

fn push_finding(
    findings: &mut Vec<HealthFinding>,
    code: &str,
    severity: FindingSeverity,
    title: &str,
    detail: &str,
    page_number: Option<u32>,
) {
    if findings.len() >= MAX_FINDINGS {
        return;
    }
    if findings.len() == MAX_FINDINGS - 1 {
        findings.push(HealthFinding {
            category: FindingCategory::Document,
            code: "health-finding-limit".to_string(),
            severity: FindingSeverity::Warning,
            title: "Finding report reached its limit".to_string(),
            detail: format!(
                "Document Health retained the first {} findings. Additional findings were omitted; review the document with a specialist validator before relying on this report.",
                MAX_FINDINGS - 1
            ),
            page_number: None,
        });
        return;
    }
    findings.push(HealthFinding {
        category: finding_category(code),
        code: code.to_string(),
        severity,
        title: title.to_string(),
        detail: detail.to_string(),
        page_number,
    });
}

fn finding_category(code: &str) -> FindingCategory {
    if code.starts_with("accessibility-") {
        FindingCategory::Accessibility
    } else if code.starts_with("font-") {
        FindingCategory::Fonts
    } else if code.starts_with("colour-") || code.starts_with("color-") {
        FindingCategory::Colour
    } else if code.starts_with("broken-")
        || code.starts_with("malformed-")
        || code.starts_with("missing-resource-")
        || code.starts_with("resource-")
    {
        FindingCategory::Structure
    } else if code == "metadata" || code == "attachments" || code == "named-destinations" {
        FindingCategory::Privacy
    } else if matches!(
        code,
        "encrypted"
            | "certificate-signature"
            | "javascript"
            | "automatic-actions"
            | "launch-action"
            | "xfa"
    ) {
        FindingCategory::Security
    } else if code.starts_with("unusual-page-")
        || code.starts_with("invalid-page-")
        || code.starts_with("large-page-")
        || code.starts_with("oversized-image-")
        || code.starts_with("likely-blank-")
        || code.starts_with("likely-duplicate-")
        || code == "mixed-page-sizes"
    {
        FindingCategory::Pages
    } else {
        FindingCategory::Document
    }
}

fn page_list(pages: &[u32]) -> String {
    const DISPLAY_LIMIT: usize = 12;
    let mut text = pages
        .iter()
        .take(DISPLAY_LIMIT)
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    if pages.len() > DISPLAY_LIMIT {
        text.push_str(&format!(" and {} more", pages.len() - DISPLAY_LIMIT));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn detects_blank_and_duplicate_pages() {
        let directory = TestDirectory::new();
        let input = directory.path.join("content.pdf");
        sample_document(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let result = inspect_pdf_health(InspectPdfHealthRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(result.blank_pages, vec![1]);
        assert_eq!(result.duplicate_groups, vec![vec![2, 3]]);
        assert!(matches!(result.status, HealthStatus::Attention));
        assert!(result.accessibility.title.is_none());
        assert!(result.accessibility.default_language.is_none());
        assert!(!result.accessibility.marked_as_tagged);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.code == "accessibility-untagged"));
    }

    #[test]
    fn raises_security_and_oversized_image_findings() {
        let directory = TestDirectory::new();
        let input = directory.path.join("risk.pdf");
        sample_document(true)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let result = inspect_pdf_health(InspectPdfHealthRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert!(matches!(result.status, HealthStatus::Risk));
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.code == "javascript"));
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.code.starts_with("oversized-image")));
    }

    #[test]
    fn controlled_health_check_reports_progress_and_stops_on_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("cancel-health.pdf");
        sample_document(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress_values = Arc::new(Mutex::new(Vec::new()));
        let callback_cancelled = Arc::clone(&cancelled);
        let callback_values = Arc::clone(&progress_values);
        let control = PdfJobExecutionControl::new(
            Arc::clone(&cancelled),
            Arc::new(move |progress, _| {
                callback_values.lock().unwrap().push(progress);
                if progress >= 40 {
                    callback_cancelled.store(true, Ordering::Release);
                }
            }),
        );

        let error = run_pdf_health_job_with_control(
            InspectPdfHealthRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        let values = progress_values.lock().unwrap();
        assert!(values.len() >= 5);
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(values.iter().any(|progress| *progress >= 40));
    }

    #[test]
    fn health_check_rejects_a_source_changed_before_its_report_is_returned() {
        let directory = TestDirectory::new();
        let input = directory.path.join("changing-health.pdf");
        sample_document(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let changed_path = input.clone();
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress == 98 {
                    fs::OpenOptions::new()
                        .append(true)
                        .open(&changed_path)
                        .unwrap()
                        .write_all(b"changed")
                        .unwrap();
                }
            }),
        );

        let error = inspect_pdf_health_with_control(
            InspectPdfHealthRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &control,
        )
        .unwrap_err();

        assert!(error.contains("changed while Document Health"));
    }

    #[test]
    fn lightweight_edit_safety_detects_certificate_fields_and_forms() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed-form.pdf");
        let mut document = sample_document(false);
        let signature_id = document.add_object(dictionary! {
            "FT" => "Sig",
            "T" => Object::string_literal("Approval"),
            "V" => dictionary! {
                "Type" => "Sig",
                "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                "Contents" => Object::string_literal("certificate bytes"),
            },
        });
        document.catalog_mut().unwrap().set(
            "AcroForm",
            dictionary! { "Fields" => vec![signature_id.into()] },
        );
        document.save(&input).unwrap().sync_all().unwrap();

        let result = inspect_pdf_edit_safety_path(&input.to_string_lossy(), None).unwrap();

        assert!(result.certificate_signature);
        assert!(result.form_fields);
        assert!(!result.xfa);
        assert!(!result.encrypted);
        assert_eq!(result.page_count, 3);
        assert_eq!(result.source_size, fs::metadata(&input).unwrap().len());
        assert_eq!(
            result.source_modified_at_ms,
            modified_at_ms(&fs::metadata(&input).unwrap())
        );
    }

    #[test]
    fn edit_safety_inspection_retains_ordered_successes_and_content_free_failures() {
        let directory = TestDirectory::new();
        let valid = directory.path.join("valid-edit-safety.pdf");
        let private_failure = directory.path.join("private-edit-safety-failure.pdf");
        sample_document(false)
            .save(&valid)
            .unwrap()
            .sync_all()
            .unwrap();
        fs::write(&private_failure, b"private malformed edit-safety content").unwrap();

        let result = run_pdf_edit_safety_inspection_job_with_control(
            InspectPdfEditSafetySourcesRequest {
                sources: vec![
                    InspectPdfEditSafetyRequest {
                        input_path: valid.to_string_lossy().into_owned(),
                        input_password: None,
                    },
                    InspectPdfEditSafetyRequest {
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
        assert_eq!(result.items[0].result.as_ref().unwrap().page_count, 3);
        assert!(result.items[0].error.is_none());
        assert_eq!(result.items[1].source_index, 1);
        assert!(result.items[1].result.is_none());
        assert_eq!(
            result.items[1].error.as_deref(),
            Some(
                "The edit-safety check could not complete its bounded structural inspection. Review the PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&result).unwrap();
        assert!(!serialised.contains("private-edit-safety-failure.pdf"));
        assert!(!serialised.contains("private malformed edit-safety content"));
    }

    #[test]
    fn edit_safety_inspection_validates_source_and_password_limits() {
        let source = InspectPdfEditSafetyRequest {
            input_path: "not-opened-during-validation.pdf".to_string(),
            input_password: None,
        };
        let over_source_limit =
            validate_inspect_pdf_edit_safety_sources_request(&InspectPdfEditSafetySourcesRequest {
                sources: vec![source.clone(); MAX_EDIT_SAFETY_SOURCES + 1],
            })
            .unwrap_err();
        let over_password_limit =
            validate_inspect_pdf_edit_safety_sources_request(&InspectPdfEditSafetySourcesRequest {
                sources: vec![InspectPdfEditSafetyRequest {
                    input_path: source.input_path,
                    input_password: Some("p".repeat(MAX_PASSWORD_BYTES + 1)),
                }],
            })
            .unwrap_err();

        assert!(over_source_limit.contains("250"));
        assert!(over_password_limit.contains("1024 UTF-8 bytes"));
    }

    #[test]
    fn controlled_edit_safety_inspection_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let first = directory.path.join("first-edit-safety.pdf");
        let second = directory.path.join("second-edit-safety.pdf");
        sample_document(false)
            .save(&first)
            .unwrap()
            .sync_all()
            .unwrap();
        sample_document(false)
            .save(&second)
            .unwrap()
            .sync_all()
            .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stages = Arc::new(Mutex::new(Vec::new()));
        let callback_cancelled = Arc::clone(&cancelled);
        let callback_stages = Arc::clone(&stages);
        let control = PdfJobExecutionControl::new(
            Arc::clone(&cancelled),
            Arc::new(move |progress, stage| {
                callback_stages
                    .lock()
                    .unwrap()
                    .push((progress, stage.clone()));
                if stage == "PDF 1 of 2: Checking certificate signatures" {
                    callback_cancelled.store(true, Ordering::Release);
                }
            }),
        );

        let error = run_pdf_edit_safety_inspection_job_with_control(
            InspectPdfEditSafetySourcesRequest {
                sources: vec![
                    InspectPdfEditSafetyRequest {
                        input_path: first.to_string_lossy().into_owned(),
                        input_password: None,
                    },
                    InspectPdfEditSafetyRequest {
                        input_path: second.to_string_lossy().into_owned(),
                        input_password: None,
                    },
                ],
            },
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        let stages = stages.lock().unwrap();
        assert!(stages.len() >= 5);
        assert!(stages.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(stages
            .iter()
            .any(|(_, stage)| stage == "PDF 1 of 2: Checking certificate signatures"));
    }

    #[test]
    fn edit_safety_inspection_rejects_a_source_changed_at_the_final_gate() {
        let directory = TestDirectory::new();
        let input = directory.path.join("changing-edit-safety.pdf");
        sample_document(false)
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let changed_path = input.clone();
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |_, stage| {
                if stage == "Verifying the source fingerprint" {
                    fs::OpenOptions::new()
                        .append(true)
                        .open(&changed_path)
                        .unwrap()
                        .write_all(b"changed")
                        .unwrap();
                }
            }),
        );

        let error = inspect_pdf_edit_safety_source_with_control(
            InspectPdfEditSafetyRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &control,
            true,
        )
        .unwrap_err();

        assert!(error.contains("changed while its edit-safety check"));
    }

    #[test]
    fn inspects_tagged_structure_and_figure_alternative_text() {
        let directory = TestDirectory::new();
        let input = directory.path.join("tagged.pdf");
        let mut document = sample_document(false);
        let structure_root_id = document.new_object_id();
        let described_figure_id = document.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => "Figure",
            "P" => structure_root_id,
            "Alt" => Object::string_literal("A chart showing quarterly revenue"),
        });
        let undescribed_figure_id = document.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => "Illustration",
            "P" => structure_root_id,
        });
        document.objects.insert(
            structure_root_id,
            Object::Dictionary(dictionary! {
                "Type" => "StructTreeRoot",
                "K" => vec![described_figure_id.into(), undescribed_figure_id.into()],
                "RoleMap" => dictionary! { "Illustration" => "Figure" },
            }),
        );
        let info_id = document.add_object(dictionary! {
            "Title" => Object::string_literal("Accessible quarterly report"),
        });
        document.trailer.set("Info", info_id);
        {
            let catalog = document.catalog_mut().unwrap();
            catalog.set("Lang", Object::string_literal("en-GB"));
            catalog.set("MarkInfo", dictionary! { "Marked" => true });
            catalog.set(
                "ViewerPreferences",
                dictionary! { "DisplayDocTitle" => true },
            );
            catalog.set("StructTreeRoot", structure_root_id);
        }
        for (index, page_id) in document.get_pages().values().copied().enumerate() {
            document
                .get_dictionary_mut(page_id)
                .unwrap()
                .set("StructParents", index as i64);
        }
        document.save(&input).unwrap().sync_all().unwrap();

        let result = inspect_pdf_health(InspectPdfHealthRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(
            result.accessibility.title.as_deref(),
            Some("Accessible quarterly report")
        );
        assert_eq!(
            result.accessibility.default_language.as_deref(),
            Some("en-GB")
        );
        assert!(result.accessibility.displays_document_title);
        assert!(result.accessibility.marked_as_tagged);
        assert!(result.accessibility.structure_tree_present);
        assert_eq!(result.accessibility.structure_element_count, 2);
        assert_eq!(result.accessibility.pages_with_structure_parents, 3);
        assert_eq!(result.accessibility.figure_count, 2);
        assert_eq!(result.accessibility.figures_missing_alt_text, 1);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.code == "accessibility-reading-order-review"));
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.code == "accessibility-figure-alt"));
        assert!(!result
            .findings
            .iter()
            .any(|finding| finding.code == "accessibility-title"));
    }

    #[test]
    fn reports_broken_references_fonts_page_resources_and_unmanaged_cmyk() {
        let mut document = sample_document(false);
        let pages = document.get_pages();
        let custom_font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "PrivateSans",
            "FontDescriptor" => dictionary! {
                "Type" => "FontDescriptor",
                "FontName" => "PrivateSans",
            },
        });
        let first_content = document.add_object(lopdf::Stream::new(
            dictionary! {},
            b"BT /MissingFont 12 Tf 10 20 Td (Lost text) Tj ET /MissingImage Do 0 0 0 1 k".to_vec(),
        ));
        document.get_dictionary_mut(pages[&1]).unwrap().set(
            "Resources",
            dictionary! {
                "Font" => dictionary! { "FCustom" => custom_font_id },
                "ColorSpace" => dictionary! { "PrintCMYK" => "DeviceCMYK" },
            },
        );
        document
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("Contents", first_content);
        let malformed_content = document.add_object(lopdf::Stream::new(
            dictionary! {},
            b"q 1 0 0 1 10 10 cm (corrupted Q".to_vec(),
        ));
        document
            .get_dictionary_mut(pages[&2])
            .unwrap()
            .set("Contents", malformed_content);
        document
            .catalog_mut()
            .unwrap()
            .set("PieceInfo", Object::Reference((999_999, 0)));

        let mut findings = Vec::new();
        let technical = inspect_test_technical_and_pages(&document, &pages, &mut findings);

        assert_eq!(technical.broken_reference_count, 1);
        assert!(technical.missing_resource_count >= 2);
        assert!(technical.page_content_error_count >= 1);
        assert_eq!(technical.font_count, 2);
        assert!(technical.unembedded_font_count >= 1);
        assert_eq!(technical.pages_using_device_cmyk, vec![1]);
        assert!(technical.colour_issue_count >= 1);
        for code in [
            "broken-object-references",
            "font-unembedded",
            "missing-resource-page-1",
            "malformed-page-content-2",
            "colour-device-cmyk-unmanaged",
        ] {
            assert!(
                findings.iter().any(|finding| finding.code == code),
                "missing {code}"
            );
        }
    }

    #[test]
    fn recognises_embedded_font_programmes_and_valid_output_profiles() {
        let mut document = sample_document(false);
        let pages = document.get_pages();
        let font_program = document.add_object(lopdf::Stream::new(
            dictionary! { "Length1" => 4 },
            vec![1, 2, 3, 4],
        ));
        let unicode_map = document.add_object(lopdf::Stream::new(
            dictionary! {},
            b"/CIDInit /ProcSet findresource begin end".to_vec(),
        ));
        let embedded_font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "EmbeddedSans",
            "ToUnicode" => unicode_map,
            "FontDescriptor" => dictionary! {
                "Type" => "FontDescriptor",
                "FontName" => "EmbeddedSans",
                "FontFile2" => font_program,
            },
        });
        for page_id in pages.values() {
            document.get_dictionary_mut(*page_id).unwrap().set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => embedded_font } },
            );
        }
        let profile = document.add_object(lopdf::Stream::new(
            dictionary! { "N" => 3 },
            valid_rgb_icc_profile(),
        ));
        document.catalog_mut().unwrap().set(
            "OutputIntents",
            vec![Object::Dictionary(dictionary! {
                "Type" => "OutputIntent",
                "S" => "GTS_PDFA1",
                "OutputConditionIdentifier" => Object::string_literal("Test RGB"),
                "DestOutputProfile" => profile,
            })],
        );

        let mut findings = Vec::new();
        let technical = inspect_technical_resources(
            &document,
            &pages,
            &mut findings,
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();

        assert_eq!(technical.font_count, 1);
        assert_eq!(technical.embedded_font_count, 1);
        assert_eq!(technical.unembedded_font_count, 0);
        assert_eq!(technical.fonts_missing_unicode_map, 0);
        assert_eq!(technical.output_intent_count, 1);
        assert_eq!(technical.icc_profile_count, 1);
        assert_eq!(technical.invalid_icc_profile_count, 0);
        assert_eq!(technical.colour_issue_count, 0);
        assert!(!findings
            .iter()
            .any(|finding| finding.code == "font-unembedded"
                || finding.code == "colour-profile-invalid"));
    }

    #[test]
    fn rejects_malformed_binary_icc_headers_channels_and_tag_ranges() {
        let mut profile = valid_rgb_icc_profile();
        profile[0..4].copy_from_slice(&159_u32.to_be_bytes());
        profile[26..28].copy_from_slice(&2_u16.to_be_bytes());
        profile[28..30].copy_from_slice(&31_u16.to_be_bytes());
        profile[36..40].copy_from_slice(b"nope");
        profile[136..140].copy_from_slice(&10_u32.to_be_bytes());

        let validation = validate_icc_profile_bytes(&profile, Some(4));

        assert!(validation.issue_count >= 5);
        let detail = validation.examples.join("; ");
        assert!(detail.contains("declares 159 bytes"));
        assert!(detail.contains("'acsp'"));
        assert!(detail.contains("declares /N 4"));
        assert!(detail.contains("invalid creation date"));
        assert!(detail.contains("invalid or unaligned data offset"));
    }

    #[test]
    fn audits_fonts_colour_and_missing_resources_inside_nested_forms() {
        let mut document = sample_document(false);
        let pages = document.get_pages();
        let nested_font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "NestedPrivateSans",
            "FontDescriptor" => dictionary! {
                "Type" => "FontDescriptor",
                "FontName" => "NestedPrivateSans",
            },
        });
        let malformed_profile =
            document.add_object(Stream::new(dictionary! { "N" => 3 }, vec![7_u8; 128]));
        let inner_form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => dictionary! {
                    "Font" => dictionary! { "NestedFont" => nested_font },
                    "ColorSpace" => dictionary! {
                        "NestedRGB" => vec![
                            Object::Name(b"ICCBased".to_vec()),
                            Object::Reference(malformed_profile),
                        ],
                    },
                },
            },
            b"BT /MissingFont 12 Tf 10 20 Td (Nested) Tj ET /NestedRGB cs 0 0 0 1 k".to_vec(),
        ));
        let outer_form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => dictionary! {
                    "XObject" => dictionary! { "Inner" => inner_form },
                },
            },
            b"/Inner Do".to_vec(),
        ));
        let page = document.get_dictionary_mut(pages[&1]).unwrap();
        page.set(
            "Resources",
            dictionary! { "XObject" => dictionary! { "Outer" => outer_form } },
        );
        let page_content = document.add_object(Stream::new(dictionary! {}, b"/Outer Do".to_vec()));
        document
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("Contents", page_content);

        let mut findings = Vec::new();
        let technical = inspect_test_technical_and_pages(&document, &pages, &mut findings);

        assert_eq!(technical.form_xobject_count, 2);
        assert_eq!(technical.invalid_icc_profile_count, 1);
        assert!(technical.font_count >= 2);
        assert!(technical.unembedded_font_count >= 1);
        assert!(technical.missing_resource_count >= 1);
        assert_eq!(technical.form_content_error_count, 0);
        assert!(technical.pages_using_device_cmyk.contains(&1));
        assert!(findings
            .iter()
            .any(|finding| finding.code == "colour-profile-invalid"
                && finding.detail.contains("form /Inner")));
        assert!(findings
            .iter()
            .any(|finding| finding.code == "font-unembedded"
                && finding.detail.contains("form /Inner")));
        assert!(findings
            .iter()
            .any(|finding| finding.code == "missing-resource-page-1"
                && finding.detail.contains("form /Inner")));
    }

    #[test]
    fn omitted_form_resources_use_the_page_dictionary_without_a_false_cycle() {
        let mut document = sample_document(false);
        let pages = document.get_pages();
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            },
            b"BT /F1 12 Tf 10 20 Td (Inherited font) Tj ET".to_vec(),
        ));
        let inherited_font = document
            .get_dictionary(pages[&2])
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Font")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"F1")
            .unwrap()
            .clone();
        let page = document.get_dictionary_mut(pages[&1]).unwrap();
        page.set(
            "Resources",
            dictionary! {
                "Font" => dictionary! { "F1" => inherited_font },
                "XObject" => dictionary! { "Inherited" => form },
            },
        );
        let page_content =
            document.add_object(Stream::new(dictionary! {}, b"/Inherited Do".to_vec()));
        document
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("Contents", page_content);

        let mut findings = Vec::new();
        let technical = inspect_test_technical_and_pages(&document, &pages, &mut findings);

        assert_eq!(technical.form_xobject_count, 1);
        assert_eq!(technical.missing_resource_count, 0);
        assert_eq!(technical.form_resource_error_count, 0);
        assert!(!findings
            .iter()
            .any(|finding| finding.code == "form-xobject-resource-cycle"));
    }

    #[test]
    fn bounds_form_cycles_and_reports_malformed_nested_content_and_resources() {
        let mut document = sample_document(false);
        let pages = document.get_pages();
        let form_a = document.add_object(Object::Null);
        let form_b = document.add_object(Object::Null);
        let invalid_resources = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
                "Resources" => 42,
            },
            Vec::new(),
        ));
        document.objects.insert(
            form_a,
            Object::Stream(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
                    "Resources" => dictionary! {
                        "XObject" => dictionary! { "B" => form_b },
                    },
                },
                b"/B Do".to_vec(),
            )),
        );
        document.objects.insert(
            form_b,
            Object::Stream(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
                    "Resources" => dictionary! {
                        "XObject" => dictionary! { "A" => form_a },
                    },
                },
                b"q 1 0 0 1 10 10 cm (corrupted Q".to_vec(),
            )),
        );
        document.get_dictionary_mut(pages[&1]).unwrap().set(
            "Resources",
            dictionary! {
                "XObject" => dictionary! {
                    "A" => form_a,
                    "BadResources" => invalid_resources,
                },
            },
        );
        let page_content = document.add_object(Stream::new(dictionary! {}, b"/A Do".to_vec()));
        document
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("Contents", page_content);

        let mut findings = Vec::new();
        let technical = inspect_test_technical_and_pages(&document, &pages, &mut findings);

        assert_eq!(technical.form_xobject_count, 3);
        assert!(technical.form_resource_error_count >= 2);
        assert!(technical.form_content_error_count >= 1);
        for code in [
            "form-xobject-resource-cycle",
            "form-xobject-resources-invalid",
            "malformed-form-content-page-1",
        ] {
            assert!(
                findings.iter().any(|finding| finding.code == code),
                "missing {code}"
            );
        }
    }

    #[test]
    fn discloses_the_nested_form_depth_limit_without_following_the_tail() {
        let mut document = sample_document(false);
        let pages = document.get_pages();
        let mut nested_form = None;
        for _ in 0..(MAX_FORM_XOBJECT_DEPTH + 2) {
            let mut resources = Dictionary::new();
            let content = if let Some(child) = nested_form {
                resources.set("XObject", dictionary! { "Next" => child });
                b"/Next Do".to_vec()
            } else {
                Vec::new()
            };
            nested_form = Some(document.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
                    "Resources" => resources,
                },
                content,
            )));
        }
        document.get_dictionary_mut(pages[&1]).unwrap().set(
            "Resources",
            dictionary! {
                "XObject" => dictionary! { "Root" => nested_form.unwrap() },
            },
        );
        let page_content = document.add_object(Stream::new(dictionary! {}, b"/Root Do".to_vec()));
        document
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("Contents", page_content);

        let mut findings = Vec::new();
        let technical = inspect_test_technical_and_pages(&document, &pages, &mut findings);

        assert_eq!(technical.form_xobject_count, MAX_FORM_XOBJECT_DEPTH);
        assert!(findings
            .iter()
            .any(|finding| finding.code == "form-xobject-inspection-limit"));
    }

    #[test]
    fn keeps_inspecting_after_a_deep_object_branch_and_reports_finding_truncation() {
        let mut document = sample_document(false);
        let mut nested = Object::Reference((900_001, 0));
        for _ in 0..70 {
            nested = Object::Array(vec![nested]);
        }
        document.trailer.set("DeepBranch", nested);
        document
            .trailer
            .set("MissingDirect", Object::Reference((900_002, 0)));

        let mut findings = Vec::new();
        let mut technical = PdfTechnicalSummary::default();
        inspect_object_references(
            &document,
            &mut technical,
            &mut findings,
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();

        assert_eq!(technical.broken_reference_count, 1);
        assert!(findings
            .iter()
            .any(|finding| finding.code == "resource-nesting-limit"));
        assert!(findings
            .iter()
            .any(|finding| finding.code == "broken-object-references"));

        findings.clear();
        for index in 0..=MAX_FINDINGS {
            push_finding(
                &mut findings,
                &format!("example-{index}"),
                FindingSeverity::Info,
                "Example",
                "Example finding",
                None,
            );
        }
        assert_eq!(findings.len(), MAX_FINDINGS);
        assert_eq!(
            findings.last().map(|finding| finding.code.as_str()),
            Some("health-finding-limit")
        );
    }

    fn inspect_test_technical_and_pages(
        document: &Document,
        pages: &std::collections::BTreeMap<u32, ObjectId>,
        findings: &mut Vec<HealthFinding>,
    ) -> PdfTechnicalSummary {
        let control = PdfJobExecutionControl::direct();
        let mut technical =
            inspect_technical_resources(document, pages, findings, &control).unwrap();
        inspect_pages(document, pages, &mut technical, findings, &control).unwrap();
        finalise_technical_findings(&mut technical, findings);
        technical
    }

    fn sample_document(with_risks: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let image_id = with_risks.then(|| {
            document.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => 10_000,
                    "Height" => 10_000,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                },
                vec![0],
            ))
        });
        let mut kids = Vec::new();
        for page_number in 1..=3 {
            let content = if page_number == 1 {
                Vec::new()
            } else {
                b"BT /F1 12 Tf 10 20 Td (Repeated page) Tj ET".to_vec()
            };
            let content_id = document.add_object(Stream::new(dictionary! {}, content));
            let mut resources = dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            };
            if let Some(image_id) = image_id.filter(|_| page_number >= 2) {
                resources.set("XObject", dictionary! { "LargeImage" => image_id });
            }
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
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
                "Count" => 3,
            }),
        );
        let mut catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        };
        if with_risks {
            catalog.set(
                "OpenAction",
                Object::Dictionary(dictionary! {
                    "S" => "JavaScript",
                    "JS" => Object::string_literal("app.alert('test')"),
                }),
            );
        }
        let catalog_id = document.add_object(catalog);
        document.trailer.set("Root", catalog_id);
        document
    }

    fn valid_rgb_icc_profile() -> Vec<u8> {
        let mut profile = vec![0_u8; 160];
        profile[0..4].copy_from_slice(&160_u32.to_be_bytes());
        profile[8] = 4;
        profile[9] = 0x30;
        profile[12..16].copy_from_slice(b"mntr");
        profile[16..20].copy_from_slice(b"RGB ");
        profile[20..24].copy_from_slice(b"XYZ ");
        for (offset, value) in [
            (24, 2026_u16),
            (26, 7),
            (28, 25),
            (30, 12),
            (32, 30),
            (34, 0),
        ] {
            profile[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
        profile[36..40].copy_from_slice(b"acsp");
        profile[40..44].copy_from_slice(b"MSFT");
        profile[128..132].copy_from_slice(&1_u32.to_be_bytes());
        profile[132..136].copy_from_slice(b"desc");
        profile[136..140].copy_from_slice(&144_u32.to_be_bytes());
        profile[140..144].copy_from_slice(&16_u32.to_be_bytes());
        profile[144..148].copy_from_slice(b"text");
        profile[152..160].copy_from_slice(b"Profile\0");
        profile
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path =
                crate::test_support::create_unique_test_directory("tufekci-paperworks-health-test");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
