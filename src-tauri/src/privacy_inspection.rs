use crate::file_safety::canonical_pdf_input;
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use lopdf::content::Content;
use lopdf::{decode_text_string, Dictionary, Document, LoadOptions, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::UNIX_EPOCH;

const MAX_PRIVACY_PAGES: usize = 20_000;
const MAX_PRIVACY_OBJECTS: usize = 1_000_000;
const MAX_PRIVACY_NODES: usize = 2_000_000;
const MAX_PAGE_CONTENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_FINDINGS: usize = 256;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfPrivacyRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum PrivacyInspectionStatus {
    Clear,
    Review,
    Risk,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum PrivacyFindingSeverity {
    Danger,
    Info,
    Warning,
}

#[derive(Clone, Copy, Debug, Serialize)]
enum PrivacyCleanOption {
    #[serde(rename = "removeActiveContent")]
    ActiveContent,
    #[serde(rename = "removeAnnotationsAndForms")]
    AnnotationsAndForms,
    #[serde(rename = "removeAttachments")]
    Attachments,
    #[serde(rename = "removeMetadata")]
    Metadata,
    #[serde(rename = "removeThumbnails")]
    Thumbnails,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivacyFinding {
    clean_option: Option<PrivacyCleanOption>,
    code: String,
    detail: String,
    page_number: Option<u32>,
    severity: PrivacyFindingSeverity,
    title: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivacyInspectionSummary {
    active_content_structures: usize,
    annotation_and_form_structures: usize,
    attachment_structures: usize,
    cropped_content_risk_pages: Vec<u32>,
    default_hidden_optional_content_groups: usize,
    embedded_search_indexes: usize,
    hidden_annotation_count: usize,
    hidden_annotation_pages: Vec<u32>,
    hidden_optional_content_pages: Vec<u32>,
    incomplete_page_inspections: Vec<u32>,
    invisible_text_operations: usize,
    invisible_text_pages: Vec<u32>,
    metadata_structures: usize,
    node_inspection_truncated: bool,
    optional_content_groups: usize,
    optional_content_pages: Vec<u32>,
    private_extension_structures: usize,
    thumbnail_structures: usize,
    web_capture_structures: usize,
    zero_opacity_pages: Vec<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPrivacyInspectionResult {
    danger_count: usize,
    file_name: String,
    findings: Vec<PrivacyFinding>,
    info_count: usize,
    page_count: usize,
    pdf_version: String,
    source_modified_at_ms: Option<u64>,
    source_size: u64,
    status: PrivacyInspectionStatus,
    summary: PrivacyInspectionSummary,
    warning_count: usize,
}

#[derive(Default)]
struct WalkBudget {
    nodes: usize,
    truncated: bool,
}

#[derive(Default)]
struct OptionalContentModel {
    alternate_configurations: usize,
    groups: HashSet<String>,
    hidden_groups: HashSet<String>,
    malformed: bool,
}

#[derive(Default)]
struct OptionalPageResources {
    hidden_properties: HashSet<Vec<u8>>,
    hidden_xobjects: HashSet<Vec<u8>>,
    optional_properties: HashSet<Vec<u8>>,
    optional_xobjects: HashSet<Vec<u8>>,
    zero_opacity_states: HashSet<Vec<u8>>,
}

#[cfg(test)]
pub fn inspect_pdf_privacy(
    request: InspectPdfPrivacyRequest,
) -> Result<PdfPrivacyInspectionResult, String> {
    inspect_pdf_privacy_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_privacy_request(
    request: &InspectPdfPrivacyRequest,
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

pub(crate) fn inspect_pdf_privacy_with_control(
    request: InspectPdfPrivacyRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfPrivacyInspectionResult, String> {
    control.checkpoint(2, "Checking the privacy-inspection request")?;
    validate_inspect_pdf_privacy_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_modified = metadata.modified().ok();
    control.checkpoint(7, "Opening the PDF for privacy inspection")?;
    let mut document = Document::load_with_options(
        &input,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The PDF could not be parsed for privacy inspection: {error}"))?;
    if document.is_encrypted() {
        document
            .decrypt(request.input_password.as_deref().unwrap_or_default())
            .map_err(|_| {
                "The PDF could not be decrypted for privacy inspection. Check its password."
                    .to_string()
            })?;
    }

    control.checkpoint(15, "Checking privacy-inspection bounds")?;
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    if pages.len() > MAX_PRIVACY_PAGES {
        return Err(format!(
            "Privacy Inspection supports at most {MAX_PRIVACY_PAGES} pages in one PDF."
        ));
    }
    if document.objects.len() > MAX_PRIVACY_OBJECTS {
        return Err(format!(
            "Privacy Inspection supports at most {MAX_PRIVACY_OBJECTS} indirect objects in one PDF."
        ));
    }

    let mut summary = PrivacyInspectionSummary::default();
    inspect_document_structures(&document, &mut summary, control)?;
    let optional_content = inspect_optional_content_model(&document, control)?;
    summary.optional_content_groups = optional_content.groups.len();
    summary.default_hidden_optional_content_groups = optional_content.hidden_groups.len();
    inspect_page_privacy(&document, &pages, &optional_content, &mut summary, control)?;
    normalise_page_lists(&mut summary);

    control.checkpoint(95, "Preparing the privacy report")?;
    let mut findings = build_findings(&summary, &optional_content);
    let danger_count = findings
        .iter()
        .filter(|finding| matches!(finding.severity, PrivacyFindingSeverity::Danger))
        .count();
    let warning_count = findings
        .iter()
        .filter(|finding| matches!(finding.severity, PrivacyFindingSeverity::Warning))
        .count();
    let info_count = findings.len() - danger_count - warning_count;
    let status = if danger_count > 0 {
        PrivacyInspectionStatus::Risk
    } else if warning_count > 0 {
        PrivacyInspectionStatus::Review
    } else {
        PrivacyInspectionStatus::Clear
    };
    findings.truncate(MAX_FINDINGS);

    control.checkpoint(98, "Rechecking the source PDF")?;
    let closing_metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be rechecked: {error}"))?;
    if metadata.len() != closing_metadata.len()
        || source_modified != closing_metadata.modified().ok()
    {
        return Err(
            "The source PDF changed while Privacy Inspection was checking it. Run the inspection again."
                .to_string(),
        );
    }
    control.checkpoint(99, "Finalising the privacy report")?;
    Ok(PdfPrivacyInspectionResult {
        danger_count,
        file_name: input
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF")
            .to_string(),
        findings,
        info_count,
        page_count: pages.len(),
        pdf_version: document.version,
        source_modified_at_ms: modified_at_ms(&metadata),
        source_size: metadata.len(),
        status,
        summary,
        warning_count,
    })
}

pub(crate) fn run_pdf_privacy_inspection_job_with_control(
    request: InspectPdfPrivacyRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfPrivacyInspectionResult, String> {
    inspect_pdf_privacy_with_control(request, control)
        .map(job_safe_privacy_inspection_result)
        .map_err(|error| {
            if error == PDF_JOB_CANCELLED_ERROR {
                error
            } else {
                safe_privacy_inspection_job_error(&error)
            }
        })
}

fn job_safe_privacy_inspection_result(
    mut result: PdfPrivacyInspectionResult,
) -> PdfPrivacyInspectionResult {
    result.file_name = "PDF".to_string();
    result
}

fn safe_privacy_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed while") {
        return "The source PDF changed during Privacy Inspection. Run the inspection again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The source PDF could not be opened with the supplied password.".to_string();
    }
    if normalised.contains("supports at most") || normalised.contains("readable pages") {
        return error.to_string();
    }
    "Privacy Inspection could not complete its bounded structure and page analysis. Review the PDF and try again."
        .to_string()
}

fn inspect_document_structures(
    document: &Document,
    summary: &mut PrivacyInspectionSummary,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.checkpoint(20, "Inspecting private document structures")?;
    summary.metadata_structures += usize::from(document.trailer.has(b"Info"));
    summary.metadata_structures += usize::from(document.trailer.has(b"ID"));
    let mut budget = WalkBudget::default();
    inspect_object(
        &Object::Dictionary(document.trailer.clone()),
        summary,
        &mut budget,
        0,
        control,
    )?;
    let total = document.objects.len();
    for (index, object) in document.objects.values().enumerate() {
        if budget.truncated {
            break;
        }
        inspect_object(object, summary, &mut budget, 0, control)?;
        privacy_loop_checkpoint(
            control,
            20,
            45,
            index,
            total,
            512,
            "Inspecting private document structures",
        )?;
    }
    summary.node_inspection_truncated = budget.truncated;
    control.checkpoint(45, "Private document structures inspected")
}

fn inspect_object(
    object: &Object,
    summary: &mut PrivacyInspectionSummary,
    budget: &mut WalkBudget,
    depth: usize,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.ensure_not_cancelled()?;
    if budget.truncated {
        return Ok(());
    }
    budget.nodes += 1;
    if budget.nodes > MAX_PRIVACY_NODES || depth > 64 {
        budget.truncated = true;
        return Ok(());
    }
    match object {
        Object::Dictionary(dictionary) => {
            inspect_dictionary(dictionary, summary);
            for (_, value) in dictionary.iter() {
                inspect_object(value, summary, budget, depth + 1, control)?;
                if budget.truncated {
                    break;
                }
            }
        }
        Object::Stream(stream) => {
            inspect_dictionary(&stream.dict, summary);
            for (_, value) in stream.dict.iter() {
                inspect_object(value, summary, budget, depth + 1, control)?;
                if budget.truncated {
                    break;
                }
            }
        }
        Object::Array(values) => {
            for value in values {
                inspect_object(value, summary, budget, depth + 1, control)?;
                if budget.truncated {
                    break;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn inspect_dictionary(dictionary: &Dictionary, summary: &mut PrivacyInspectionSummary) {
    summary.metadata_structures += count_keys(
        dictionary,
        &[b"Metadata", b"PieceInfo", b"LastModified", b"SpiderInfo"],
    );
    summary.metadata_structures += usize::from(has_name(dictionary, b"Type", b"Metadata"));
    summary.active_content_structures +=
        count_keys(dictionary, &[b"AA", b"OpenAction", b"JS", b"JavaScript"]);
    summary.active_content_structures += usize::from(is_active_content_dictionary(dictionary));
    summary.attachment_structures += count_keys(
        dictionary,
        &[b"EmbeddedFiles", b"EF", b"AF", b"AFRelationship"],
    );
    summary.attachment_structures += usize::from(is_attachment_dictionary(dictionary));
    summary.annotation_and_form_structures += count_keys(dictionary, &[b"Annots", b"AcroForm"]);
    summary.annotation_and_form_structures += usize::from(is_annotation_dictionary(dictionary));
    summary.thumbnail_structures += usize::from(dictionary.has(b"Thumb"));
    summary.web_capture_structures += count_keys(dictionary, &[b"SpiderInfo", b"URLS", b"IDS"]);
    summary.private_extension_structures += count_keys(dictionary, &[b"Extensions", b"PieceInfo"]);
    summary.private_extension_structures += dictionary
        .iter()
        .filter(|(key, _)| key.starts_with(b"XX") && key.len() > 2)
        .count();
    if is_attachment_dictionary(dictionary) && filespec_is_search_index(dictionary) {
        summary.embedded_search_indexes += 1;
    }
}

fn count_keys(dictionary: &Dictionary, keys: &[&[u8]]) -> usize {
    keys.iter().filter(|key| dictionary.has(key)).count()
}

fn inspect_optional_content_model(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<OptionalContentModel, String> {
    control.checkpoint(46, "Inspecting optional-content configuration")?;
    let Some(value) = document
        .catalog()
        .ok()
        .and_then(|catalogue| catalogue.get(b"OCProperties").ok())
    else {
        control.checkpoint(49, "Optional-content configuration inspected")?;
        return Ok(OptionalContentModel::default());
    };
    let Some(properties) = resolve_object(document, value)
        .ok()
        .and_then(|object| object.as_dict().ok())
    else {
        control.checkpoint(49, "Optional-content configuration inspected")?;
        return Ok(OptionalContentModel {
            malformed: true,
            ..OptionalContentModel::default()
        });
    };

    let mut model = OptionalContentModel::default();
    let groups = properties
        .get(b"OCGs")
        .ok()
        .and_then(|object| resolve_object(document, object).ok())
        .and_then(|object| object.as_array().ok());
    let Some(groups) = groups else {
        model.malformed = true;
        control.checkpoint(49, "Optional-content configuration inspected")?;
        return Ok(model);
    };
    for group in groups {
        control.ensure_not_cancelled()?;
        model.groups.insert(object_identity(group));
    }

    model.alternate_configurations = properties
        .get(b"Configs")
        .ok()
        .and_then(|object| resolve_object(document, object).ok())
        .and_then(|object| object.as_array().ok())
        .map_or(0, Vec::len);
    let Some(default_configuration) = properties
        .get(b"D")
        .ok()
        .and_then(|object| resolve_object(document, object).ok())
        .and_then(|object| object.as_dict().ok())
    else {
        model.malformed = true;
        control.checkpoint(49, "Optional-content configuration inspected")?;
        return Ok(model);
    };

    let base_state = default_configuration
        .get(b"BaseState")
        .and_then(Object::as_name)
        .unwrap_or(b"ON");
    if base_state == b"OFF" {
        model.hidden_groups.clone_from(&model.groups);
    } else if base_state != b"ON" && base_state != b"Unchanged" {
        model.malformed = true;
    }
    apply_group_state_array(
        document,
        default_configuration,
        b"ON",
        false,
        &mut model,
        control,
    )?;
    apply_group_state_array(
        document,
        default_configuration,
        b"OFF",
        true,
        &mut model,
        control,
    )?;
    control.checkpoint(49, "Optional-content configuration inspected")?;
    Ok(model)
}

fn apply_group_state_array(
    document: &Document,
    configuration: &Dictionary,
    key: &[u8],
    hidden: bool,
    model: &mut OptionalContentModel,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    let Some(groups) = configuration
        .get(key)
        .ok()
        .and_then(|object| resolve_object(document, object).ok())
        .and_then(|object| object.as_array().ok())
    else {
        return Ok(());
    };
    for group in groups {
        control.ensure_not_cancelled()?;
        let identity = object_identity(group);
        if !model.groups.contains(&identity) {
            model.malformed = true;
        }
        if hidden {
            model.hidden_groups.insert(identity);
        } else {
            model.hidden_groups.remove(&identity);
        }
    }
    Ok(())
}

fn inspect_page_privacy(
    document: &Document,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
    optional_content: &OptionalContentModel,
    summary: &mut PrivacyInspectionSummary,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.checkpoint(50, "Inspecting page-level privacy signals")?;
    let total = pages.len();
    for (page_index, (page_number, page_id)) in pages.iter().enumerate() {
        control.ensure_not_cancelled()?;
        let resources = match optional_page_resources(document, *page_id, optional_content, control)
        {
            Ok(resources) => resources,
            Err(error) if error == PDF_JOB_CANCELLED_ERROR => return Err(error),
            Err(_) => OptionalPageResources::default(),
        };
        let mut page_has_optional_content = false;
        let mut page_has_hidden_optional_content = false;
        let mut page_has_zero_opacity = false;
        let mut page_has_content = false;
        let mut invisible_operations = 0_usize;

        match document.get_page_content_with_limit(*page_id, MAX_PAGE_CONTENT_BYTES) {
            Ok(bytes) => {
                page_has_content = bytes.iter().any(|byte| !byte.is_ascii_whitespace());
                match Content::decode_strict(&bytes) {
                    Ok(content) => {
                        let mut render_mode = 0_i64;
                        let mut render_mode_stack = Vec::new();
                        for (operation_index, operation) in content.operations.iter().enumerate() {
                            if operation_index.is_multiple_of(256) {
                                control.ensure_not_cancelled()?;
                            }
                            match operation.operator.as_str() {
                                "q" => render_mode_stack.push(render_mode),
                                "Q" => render_mode = render_mode_stack.pop().unwrap_or(0),
                                "Tr" => {
                                    if let Some(mode) =
                                        operation.operands.first().and_then(pdf_integer)
                                    {
                                        render_mode = mode;
                                    }
                                }
                                "Tj" | "TJ" | "'" | "\"" if matches!(render_mode, 3 | 7) => {
                                    invisible_operations += 1;
                                }
                                "gs" => {
                                    if operation
                                        .operands
                                        .first()
                                        .and_then(|operand| operand.as_name().ok())
                                        .is_some_and(|name| {
                                            resources.zero_opacity_states.contains(name)
                                        })
                                    {
                                        page_has_zero_opacity = true;
                                    }
                                }
                                "Do" => {
                                    if let Some(name) = operation
                                        .operands
                                        .first()
                                        .and_then(|operand| operand.as_name().ok())
                                    {
                                        page_has_optional_content |=
                                            resources.optional_xobjects.contains(name);
                                        page_has_hidden_optional_content |=
                                            resources.hidden_xobjects.contains(name);
                                    }
                                }
                                "BDC" | "DP"
                                    if operation
                                        .operands
                                        .first()
                                        .and_then(|operand| operand.as_name().ok())
                                        .is_some_and(|tag| tag == b"OC") =>
                                {
                                    page_has_optional_content = true;
                                    if let Some(property) = operation.operands.get(1) {
                                        if let Ok(name) = property.as_name() {
                                            page_has_hidden_optional_content |=
                                                resources.hidden_properties.contains(name);
                                        } else {
                                            page_has_hidden_optional_content |=
                                                optional_object_may_be_hidden(
                                                    document,
                                                    property,
                                                    optional_content,
                                                );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(_) => summary.incomplete_page_inspections.push(*page_number),
                }
            }
            Err(_) => summary.incomplete_page_inspections.push(*page_number),
        }

        for (annotation_index, annotation) in document
            .get_page_annotations(*page_id)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            if annotation_index.is_multiple_of(256) {
                control.ensure_not_cancelled()?;
            }
            let flags = annotation
                .get(b"F")
                .and_then(Object::as_i64)
                .unwrap_or_default();
            if flags & (1 | 2 | 32) != 0 {
                summary.hidden_annotation_count += 1;
                summary.hidden_annotation_pages.push(*page_number);
            }
            if let Ok(optional) = annotation.get(b"OC") {
                page_has_optional_content = true;
                page_has_hidden_optional_content |=
                    optional_object_may_be_hidden(document, optional, optional_content);
            }
        }

        if page_has_optional_content {
            summary.optional_content_pages.push(*page_number);
        }
        if page_has_hidden_optional_content {
            summary.hidden_optional_content_pages.push(*page_number);
        }
        if page_has_zero_opacity {
            summary.zero_opacity_pages.push(*page_number);
        }
        if invisible_operations > 0 {
            summary.invisible_text_operations += invisible_operations;
            summary.invisible_text_pages.push(*page_number);
        }
        if page_has_content && page_has_crop_difference(document, *page_id) {
            summary.cropped_content_risk_pages.push(*page_number);
        }
        privacy_loop_checkpoint(
            control,
            50,
            92,
            page_index,
            total,
            1,
            "Inspecting page-level privacy signals",
        )?;
    }
    Ok(())
}

fn optional_page_resources(
    document: &Document,
    page_id: ObjectId,
    optional_content: &OptionalContentModel,
    control: &PdfJobExecutionControl,
) -> Result<OptionalPageResources, String> {
    let resources = page_resources(document, page_id)?;
    let mut result = OptionalPageResources::default();
    for (name, value) in resource_entries(document, &resources, b"Properties", control)? {
        control.ensure_not_cancelled()?;
        if optional_object(document, &value) {
            result.optional_properties.insert(name.clone());
            if optional_object_may_be_hidden(document, &value, optional_content) {
                result.hidden_properties.insert(name);
            }
        }
    }
    for (name, value) in resource_entries(document, &resources, b"XObject", control)? {
        control.ensure_not_cancelled()?;
        let Some(dictionary) = resolve_object(document, &value)
            .ok()
            .and_then(object_dictionary)
        else {
            continue;
        };
        let Ok(optional) = dictionary.get(b"OC") else {
            continue;
        };
        result.optional_xobjects.insert(name.clone());
        if optional_object_may_be_hidden(document, optional, optional_content) {
            result.hidden_xobjects.insert(name);
        }
    }
    for (name, value) in resource_entries(document, &resources, b"ExtGState", control)? {
        control.ensure_not_cancelled()?;
        let Some(dictionary) = resolve_object(document, &value)
            .ok()
            .and_then(object_dictionary)
        else {
            continue;
        };
        let zero_fill = dictionary
            .get(b"ca")
            .ok()
            .and_then(pdf_number)
            .is_some_and(|alpha| alpha <= 0.001);
        let zero_stroke = dictionary
            .get(b"CA")
            .ok()
            .and_then(pdf_number)
            .is_some_and(|alpha| alpha <= 0.001);
        if zero_fill || zero_stroke {
            result.zero_opacity_states.insert(name);
        }
    }
    Ok(result)
}

fn optional_object(document: &Document, object: &Object) -> bool {
    resolve_object(document, object)
        .ok()
        .and_then(object_dictionary)
        .and_then(|dictionary| dictionary.get(b"Type").ok())
        .and_then(|value| value.as_name().ok())
        .is_some_and(|name| name == b"OCG" || name == b"OCMD")
}

fn optional_object_may_be_hidden(
    document: &Document,
    object: &Object,
    optional_content: &OptionalContentModel,
) -> bool {
    if optional_content
        .hidden_groups
        .contains(&object_identity(object))
    {
        return true;
    }
    let Some(dictionary) = resolve_object(document, object)
        .ok()
        .and_then(object_dictionary)
    else {
        return false;
    };
    if has_name(dictionary, b"Type", b"OCG") {
        return optional_content
            .hidden_groups
            .contains(&object_identity(object));
    }
    if !has_name(dictionary, b"Type", b"OCMD") {
        return false;
    }
    let Ok(groups) = dictionary.get(b"OCGs") else {
        return false;
    };
    match resolve_object(document, groups) {
        Ok(Object::Array(values)) => values.iter().any(|value| {
            optional_content
                .hidden_groups
                .contains(&object_identity(value))
        }),
        Ok(value) => optional_content
            .hidden_groups
            .contains(&object_identity(value)),
        Err(_) => false,
    }
}

fn page_has_crop_difference(document: &Document, page_id: ObjectId) -> bool {
    let Ok(Some(media)) = inherited_page_value(document, page_id, b"MediaBox") else {
        return false;
    };
    let Ok(Some(crop)) = inherited_page_value(document, page_id, b"CropBox") else {
        return false;
    };
    let Some(media) = box_coordinates(document, &media) else {
        return false;
    };
    let Some(crop) = box_coordinates(document, &crop) else {
        return false;
    };
    media
        .iter()
        .zip(crop.iter())
        .any(|(left, right)| (left - right).abs() > 0.01)
}

fn box_coordinates(document: &Document, object: &Object) -> Option<[f64; 4]> {
    let values = resolve_object(document, object).ok()?.as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        pdf_number(&values[0])?,
        pdf_number(&values[1])?,
        pdf_number(&values[2])?,
        pdf_number(&values[3])?,
    ])
}

fn build_findings(
    summary: &PrivacyInspectionSummary,
    optional_content: &OptionalContentModel,
) -> Vec<PrivacyFinding> {
    let mut findings = Vec::new();
    if summary.active_content_structures > 0 {
        push_finding(
            &mut findings,
            "active-content",
            PrivacyFindingSeverity::Danger,
            "Scripts or automatic actions detected",
            &format!(
                "The PDF contains {} JavaScript, launch, or automatic-action structure{}. Remove scripts and automatic actions before sharing unless they are explicitly required.",
                summary.active_content_structures,
                plural(summary.active_content_structures)
            ),
            None,
            Some(PrivacyCleanOption::ActiveContent),
        );
    }
    if summary.attachment_structures > 0 {
        push_finding(
            &mut findings,
            "attachments",
            PrivacyFindingSeverity::Warning,
            "Attachments or associated files detected",
            &format!(
                "The PDF contains {} attachment-related structure{}. Embedded files can carry private or executable data independently of the visible pages.",
                summary.attachment_structures,
                plural(summary.attachment_structures)
            ),
            None,
            Some(PrivacyCleanOption::Attachments),
        );
    }
    if summary.web_capture_structures > 0 {
        push_finding(
            &mut findings,
            "web-capture-data",
            PrivacyFindingSeverity::Warning,
            "Web Capture provenance detected",
            &format!(
                "The PDF contains {} Web Capture structure{} that may retain source URLs, digital identifiers, timestamps, or retrieval history.",
                summary.web_capture_structures,
                plural(summary.web_capture_structures)
            ),
            None,
            Some(PrivacyCleanOption::Metadata),
        );
    }
    if summary.embedded_search_indexes > 0 {
        push_finding(
            &mut findings,
            "embedded-search-indexes",
            PrivacyFindingSeverity::Warning,
            "Embedded search indexes detected",
            &format!(
                "The PDF contains {} embedded PDX search-index file{}. Remove attachments unless the index is intentionally distributed with the document.",
                summary.embedded_search_indexes,
                plural(summary.embedded_search_indexes)
            ),
            None,
            Some(PrivacyCleanOption::Attachments),
        );
    }
    if summary.metadata_structures > 0 {
        push_finding(
            &mut findings,
            "metadata",
            PrivacyFindingSeverity::Info,
            "Metadata or private history detected",
            &format!(
                "The PDF contains {} metadata-related structure{}, including document information, identifiers, XMP, or application history.",
                summary.metadata_structures,
                plural(summary.metadata_structures)
            ),
            None,
            Some(PrivacyCleanOption::Metadata),
        );
    }
    if summary.annotation_and_form_structures > 0 {
        push_finding(
            &mut findings,
            "annotations-and-forms",
            PrivacyFindingSeverity::Info,
            "Annotations or form data detected",
            &format!(
                "The PDF contains {} annotation- or form-related structure{}. Comments, links, entered values, and certificate fields may not be part of the page artwork.",
                summary.annotation_and_form_structures,
                plural(summary.annotation_and_form_structures)
            ),
            None,
            Some(PrivacyCleanOption::AnnotationsAndForms),
        );
    }
    if summary.thumbnail_structures > 0 {
        push_finding(
            &mut findings,
            "page-thumbnails",
            PrivacyFindingSeverity::Info,
            "Embedded page thumbnails detected",
            &format!(
                "The PDF contains {} page thumbnail{}. A thumbnail can preserve an older view after page artwork changes.",
                summary.thumbnail_structures,
                plural(summary.thumbnail_structures)
            ),
            None,
            Some(PrivacyCleanOption::Thumbnails),
        );
    }
    if summary.optional_content_groups > 0 {
        let detail = if optional_content.alternate_configurations > 0 {
            format!(
                "The PDF declares {} optional-content group{} and {} alternate visibility configuration{}. Review every layer state before sharing.",
                summary.optional_content_groups,
                plural(summary.optional_content_groups),
                optional_content.alternate_configurations,
                plural(optional_content.alternate_configurations)
            )
        } else {
            format!(
                "The PDF declares {} optional-content group{} used on page{}. Layer content can be shown, hidden, printed, or exported differently by a reader.",
                summary.optional_content_groups,
                plural(summary.optional_content_groups),
                page_list(&summary.optional_content_pages)
            )
        };
        push_finding(
            &mut findings,
            "optional-content",
            PrivacyFindingSeverity::Info,
            "Optional-content layers detected",
            &detail,
            summary.optional_content_pages.first().copied(),
            None,
        );
    }
    if summary.default_hidden_optional_content_groups > 0
        || !summary.hidden_optional_content_pages.is_empty()
    {
        push_finding(
            &mut findings,
            "hidden-optional-content",
            PrivacyFindingSeverity::Warning,
            "Layers can conceal page content",
            &format!(
                "{} optional-content group{} are off in the default configuration, with concealed layer usage detected on page{}. Removing only the layer catalogue could expose that artwork, so Privacy Cleaner does not delete it automatically.",
                summary.default_hidden_optional_content_groups,
                plural(summary.default_hidden_optional_content_groups),
                page_list(&summary.hidden_optional_content_pages)
            ),
            summary.hidden_optional_content_pages.first().copied(),
            None,
        );
    }
    if summary.invisible_text_operations > 0 {
        push_finding(
            &mut findings,
            "invisible-text",
            PrivacyFindingSeverity::Info,
            "Invisible text drawing detected",
            &format!(
                "{} text-showing operation{} use an invisible rendering mode on page{}. This is common in OCR text layers, but hidden text can differ from visible artwork and should be searched and reviewed.",
                summary.invisible_text_operations,
                plural(summary.invisible_text_operations),
                page_list(&summary.invisible_text_pages)
            ),
            summary.invisible_text_pages.first().copied(),
            None,
        );
    }
    if !summary.zero_opacity_pages.is_empty() {
        push_finding(
            &mut findings,
            "zero-opacity-content",
            PrivacyFindingSeverity::Warning,
            "Zero-opacity drawing state detected",
            &format!(
                "Page{} {} invoke a graphics state with zero stroking or non-stroking opacity. Fully transparent artwork may remain extractable even though it is not visible.",
                plural(summary.zero_opacity_pages.len()),
                page_list(&summary.zero_opacity_pages)
            ),
            summary.zero_opacity_pages.first().copied(),
            None,
        );
    }
    if summary.hidden_annotation_count > 0 {
        push_finding(
            &mut findings,
            "hidden-annotations",
            PrivacyFindingSeverity::Warning,
            "Hidden or non-viewing annotations detected",
            &format!(
                "{} annotation{} use Invisible, Hidden, or NoView flags on page{}. Their contents or actions can remain in the file outside the normal page view.",
                summary.hidden_annotation_count,
                plural(summary.hidden_annotation_count),
                page_list(&summary.hidden_annotation_pages)
            ),
            summary.hidden_annotation_pages.first().copied(),
            Some(PrivacyCleanOption::AnnotationsAndForms),
        );
    }
    if !summary.cropped_content_risk_pages.is_empty() {
        push_finding(
            &mut findings,
            "cropped-content-risk",
            PrivacyFindingSeverity::Info,
            "Cropped pages may retain concealed artwork",
            &format!(
                "Page{} {} have crop boxes different from their media boxes. Cropping changes the visible window and does not remove artwork outside it.",
                plural(summary.cropped_content_risk_pages.len()),
                page_list(&summary.cropped_content_risk_pages)
            ),
            summary.cropped_content_risk_pages.first().copied(),
            None,
        );
    }
    if summary.private_extension_structures > 0 {
        push_finding(
            &mut findings,
            "private-extensions",
            PrivacyFindingSeverity::Warning,
            "Private extension data detected",
            &format!(
                "The PDF contains {} declared extension, page-piece, or XX-prefixed private structure{}. Page-piece history is covered by metadata cleaning; declared Extensions data and unknown private keys are reported rather than deleted automatically.",
                summary.private_extension_structures,
                plural(summary.private_extension_structures)
            ),
            None,
            None,
        );
    }
    if optional_content.malformed {
        push_finding(
            &mut findings,
            "optional-content-malformed",
            PrivacyFindingSeverity::Warning,
            "Optional-content configuration is malformed",
            "The layer catalogue could not be interpreted completely. Review the PDF in a specialist preflight tool before relying on any visible layer state.",
            None,
            None,
        );
    }
    if !summary.incomplete_page_inspections.is_empty() {
        push_finding(
            &mut findings,
            "page-inspection-incomplete",
            PrivacyFindingSeverity::Warning,
            "Some page streams were not inspected",
            &format!(
                "Page{} {} could not be decoded within the bounded privacy inspection. Concealed-content signals may be incomplete.",
                plural(summary.incomplete_page_inspections.len()),
                page_list(&summary.incomplete_page_inspections)
            ),
            summary.incomplete_page_inspections.first().copied(),
            None,
        );
    }
    if summary.node_inspection_truncated {
        push_finding(
            &mut findings,
            "node-inspection-limit",
            PrivacyFindingSeverity::Warning,
            "Private-structure inspection reached its limit",
            &format!(
                "The object walk exceeded {MAX_PRIVACY_NODES} direct nodes or 64 nesting levels. Remaining private structures were not classified."
            ),
            None,
            None,
        );
    }
    findings
}

fn push_finding(
    findings: &mut Vec<PrivacyFinding>,
    code: &str,
    severity: PrivacyFindingSeverity,
    title: &str,
    detail: &str,
    page_number: Option<u32>,
    clean_option: Option<PrivacyCleanOption>,
) {
    if findings.len() >= MAX_FINDINGS {
        return;
    }
    findings.push(PrivacyFinding {
        clean_option,
        code: code.to_string(),
        detail: detail.to_string(),
        page_number,
        severity,
        title: title.to_string(),
    });
}

fn normalise_page_lists(summary: &mut PrivacyInspectionSummary) {
    for pages in [
        &mut summary.cropped_content_risk_pages,
        &mut summary.hidden_annotation_pages,
        &mut summary.hidden_optional_content_pages,
        &mut summary.incomplete_page_inspections,
        &mut summary.invisible_text_pages,
        &mut summary.optional_content_pages,
        &mut summary.zero_opacity_pages,
    ] {
        pages.sort_unstable();
        pages.dedup();
    }
}

fn filespec_is_search_index(dictionary: &Dictionary) -> bool {
    [b"UF".as_slice(), b"F".as_slice()]
        .iter()
        .filter_map(|key| dictionary.get(key).ok())
        .filter_map(|value| decode_text_string(value).ok())
        .any(|name| name.trim().to_ascii_lowercase().ends_with(".pdx"))
}

fn is_active_content_dictionary(dictionary: &Dictionary) -> bool {
    dictionary.has(b"JS")
        || dictionary
            .get(b"S")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"JavaScript" || name == b"Launch")
}

fn is_attachment_dictionary(dictionary: &Dictionary) -> bool {
    dictionary.has(b"EF")
        || has_name(dictionary, b"Type", b"Filespec")
        || has_name(dictionary, b"Type", b"EmbeddedFile")
        || has_name(dictionary, b"Subtype", b"FileAttachment")
}

fn is_annotation_dictionary(dictionary: &Dictionary) -> bool {
    has_name(dictionary, b"Type", b"Annot")
        || dictionary
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|name| {
                matches!(
                    name,
                    b"Text"
                        | b"Link"
                        | b"FreeText"
                        | b"Line"
                        | b"Square"
                        | b"Circle"
                        | b"Polygon"
                        | b"PolyLine"
                        | b"Highlight"
                        | b"Underline"
                        | b"Squiggly"
                        | b"StrikeOut"
                        | b"Stamp"
                        | b"Caret"
                        | b"Ink"
                        | b"Popup"
                        | b"FileAttachment"
                        | b"Sound"
                        | b"Movie"
                        | b"Widget"
                        | b"Screen"
                        | b"PrinterMark"
                        | b"TrapNet"
                        | b"Watermark"
                        | b"3D"
                        | b"Redact"
                        | b"RichMedia"
                )
            })
}

fn has_name(dictionary: &Dictionary, key: &[u8], expected: &[u8]) -> bool {
    dictionary
        .get(key)
        .and_then(Object::as_name)
        .is_ok_and(|name| name == expected)
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
    control: &PdfJobExecutionControl,
) -> Result<Vec<(Vec<u8>, Object)>, String> {
    let Some(value) = resources.get(key).ok() else {
        return Ok(Vec::new());
    };
    let dictionary = resolve_object(document, value)?
        .as_dict()
        .map_err(|_| "a page resource category is not a dictionary".to_string())?;
    let mut entries = Vec::with_capacity(dictionary.len());
    for (index, (name, value)) in dictionary.iter().enumerate() {
        if index.is_multiple_of(256) {
            control.ensure_not_cancelled()?;
        }
        entries.push((name.clone(), value.clone()));
    }
    Ok(entries)
}

fn privacy_loop_checkpoint(
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

fn pdf_integer(object: &Object) -> Option<i64> {
    match object {
        Object::Integer(value) => Some(*value),
        _ => None,
    }
}

fn pdf_number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn page_list(pages: &[u32]) -> String {
    if pages.is_empty() {
        return "not mapped to a page".to_string();
    }
    const DISPLAY_LIMIT: usize = 12;
    let mut value = pages
        .iter()
        .take(DISPLAY_LIMIT)
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    if pages.len() > DISPLAY_LIMIT {
        value.push_str(&format!(" and {} more", pages.len() - DISPLAY_LIMIT));
    }
    value
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_layers_invisible_content_indexes_and_private_extensions() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-signals.pdf");
        privacy_signal_fixture()
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();

        let result = inspect_pdf_privacy(InspectPdfPrivacyRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(result.page_count, 1);
        assert_eq!(result.summary.optional_content_groups, 1);
        assert_eq!(result.summary.default_hidden_optional_content_groups, 1);
        assert_eq!(result.summary.hidden_optional_content_pages, vec![1]);
        assert_eq!(result.summary.invisible_text_pages, vec![1]);
        assert_eq!(result.summary.zero_opacity_pages, vec![1]);
        assert_eq!(result.summary.hidden_annotation_pages, vec![1]);
        assert_eq!(result.summary.cropped_content_risk_pages, vec![1]);
        assert!(result.summary.web_capture_structures >= 3);
        assert_eq!(result.summary.embedded_search_indexes, 1);
        assert!(result.summary.private_extension_structures >= 2);
        for code in [
            "web-capture-data",
            "embedded-search-indexes",
            "hidden-optional-content",
            "invisible-text",
            "zero-opacity-content",
            "hidden-annotations",
            "cropped-content-risk",
            "private-extensions",
        ] {
            assert!(
                result.findings.iter().any(|finding| finding.code == code),
                "missing {code}"
            );
        }
    }

    #[test]
    fn keeps_an_ordinary_visible_page_clear_of_concealment_findings() {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 20 30 Td (Visible) Tj ET".to_vec(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
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
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);

        let pages = document.get_pages();
        let mut summary = PrivacyInspectionSummary::default();
        let control = PdfJobExecutionControl::direct();
        inspect_document_structures(&document, &mut summary, &control).unwrap();
        let optional = inspect_optional_content_model(&document, &control).unwrap();
        inspect_page_privacy(&document, &pages, &optional, &mut summary, &control).unwrap();

        assert!(summary.optional_content_pages.is_empty());
        assert!(summary.invisible_text_pages.is_empty());
        assert!(summary.zero_opacity_pages.is_empty());
        assert!(summary.hidden_annotation_pages.is_empty());
        assert!(summary.cropped_content_risk_pages.is_empty());
    }

    #[test]
    fn controlled_inspection_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("controlled-privacy-inspection.pdf");
        privacy_signal_fixture()
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

        let result = run_pdf_privacy_inspection_job_with_control(
            InspectPdfPrivacyRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &control,
        )
        .unwrap();

        assert_eq!(result.file_name, "PDF");
        assert_eq!(result.page_count, 1);
        let progress = progress.lock().unwrap();
        assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(progress
            .iter()
            .any(|(_, stage)| stage.contains("page-level privacy signals")));
        assert_eq!(progress.last().map(|entry| entry.0), Some(99));

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let cancelling_control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |value, _| {
                if value >= 20 {
                    cancellation_flag.store(true, Ordering::Release);
                }
            }),
        );
        let error = inspect_pdf_privacy_with_control(
            InspectPdfPrivacyRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &cancelling_control,
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn inspection_rejects_a_source_changed_at_the_final_gate() {
        let directory = TestDirectory::new();
        let input = directory.path.join("mutated-privacy-inspection.pdf");
        privacy_signal_fixture()
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
                    source
                        .write_all(b"\n% changed during privacy inspection")
                        .unwrap();
                    source.sync_all().unwrap();
                }
            }),
        );

        let error = inspect_pdf_privacy_with_control(
            InspectPdfPrivacyRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &control,
        )
        .unwrap_err();

        assert!(mutated.load(Ordering::Acquire));
        assert!(error.contains("changed while Privacy Inspection"));
    }

    #[test]
    fn privacy_inspection_job_errors_are_content_free() {
        let error = safe_privacy_inspection_job_error(
            "C:\\Private\\Client.pdf could not decrypt with private-password",
        );
        assert_eq!(
            error,
            "The source PDF could not be opened with the supplied password."
        );
        assert!(!error.contains("Client"));
        assert!(!error.contains("private-password"));
    }

    fn privacy_signal_fixture() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let ocg_id = document.add_object(dictionary! {
            "Type" => "OCG",
            "Name" => Object::string_literal("Private layer"),
        });
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"q /Zero gs BT 3 Tr (hidden OCR text) Tj 0 Tr ET /OC /PrivateLayer BDC BT (layer text) Tj ET EMC Q".to_vec(),
        ));
        let hidden_annotation_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![10.into(), 10.into(), 20.into(), 20.into()],
            "Contents" => Object::string_literal("hidden comment"),
            "F" => 2,
            "OC" => ocg_id,
        });
        let index_stream_id = document.add_object(Stream::new(
            dictionary! { "Type" => "EmbeddedFile" },
            b"search index".to_vec(),
        ));
        let index_spec_id = document.add_object(dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal("private-index.pdx"),
            "EF" => dictionary! { "F" => index_stream_id },
        });
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "CropBox" => vec![20.into(), 20.into(), 580.into(), 780.into()],
            "Resources" => dictionary! {
                "Properties" => dictionary! { "PrivateLayer" => ocg_id },
                "ExtGState" => dictionary! {
                    "Zero" => dictionary! { "ca" => 0.0, "CA" => 0.0 },
                },
            },
            "Contents" => content_id,
            "Annots" => vec![hidden_annotation_id.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let names_id = document.add_object(dictionary! {
            "URLS" => dictionary! { "Names" => vec![] },
            "IDS" => dictionary! { "Names" => vec![] },
            "EmbeddedFiles" => dictionary! {
                "Names" => vec![Object::string_literal("private-index.pdx"), index_spec_id.into()],
            },
        });
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Names" => names_id,
            "SpiderInfo" => dictionary! { "V" => 1.0 },
            "PieceInfo" => dictionary! { "PrivateApp" => dictionary! {} },
            "Extensions" => dictionary! { "ADBE" => dictionary! { "BaseVersion" => "1.7", "ExtensionLevel" => 8 } },
            "OCProperties" => dictionary! {
                "OCGs" => vec![ocg_id.into()],
                "D" => dictionary! { "BaseState" => "ON", "OFF" => vec![ocg_id.into()] },
            },
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
                "tufekci-paperworks-privacy-inspection-test-{}-{nonce}",
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
