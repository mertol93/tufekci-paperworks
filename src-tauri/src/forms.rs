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
    decode_text_string, dictionary, Dictionary, Document, LoadOptions, Object, ObjectId, Stream,
    StringFormat,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

const MAX_FIELDS: usize = 2_000;
const MAX_WIDGETS: usize = 5_000;
const MAX_FIELD_DEPTH: usize = 32;
const MAX_FIELD_NAME_CHARACTERS: usize = 512;
const MAX_FIELD_NAME_BYTES: usize = 2_048;
const MAX_VALUE_CHARACTERS: usize = 4_096;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_OPTIONS_PER_FIELD: usize = 1_000;
const MAX_TOTAL_OPTIONS: usize = 20_000;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PAGE_TREE_DEPTH: usize = 32;

const FLAG_READ_ONLY: i64 = 1 << 0;
const FLAG_REQUIRED: i64 = 1 << 1;
const FLAG_MULTILINE: i64 = 1 << 12;
const FLAG_PASSWORD: i64 = 1 << 13;
const FLAG_RADIO: i64 = 1 << 15;
const FLAG_PUSHBUTTON: i64 = 1 << 16;
const FLAG_COMBO: i64 = 1 << 17;
const FLAG_EDIT: i64 = 1 << 18;
const FLAG_MULTI_SELECT: i64 = 1 << 21;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfFormsRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfFormsRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
    acknowledge_certificate_signatures: bool,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    flatten: bool,
    updates: Vec<PdfFormFieldUpdate>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PdfFormFieldUpdate {
    field_id: String,
    values: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum FormFieldKind {
    Text,
    Checkbox,
    Radio,
    Choice,
    Button,
    Signature,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfFormOption {
    label: String,
    value: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalisedRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfFormWidgetInspection {
    widget_id: String,
    page_number: Option<u32>,
    rect: Option<NormalisedRect>,
    export_value: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfFormFieldInspection {
    field_id: String,
    name: String,
    kind: FormFieldKind,
    values: Vec<String>,
    options: Vec<PdfFormOption>,
    widgets: Vec<PdfFormWidgetInspection>,
    editable: bool,
    flattenable: bool,
    read_only: bool,
    required: bool,
    multiline: bool,
    password: bool,
    combo: bool,
    editable_choice: bool,
    multi_select: bool,
    max_length: Option<usize>,
    signature_present: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfFormInspection {
    file_name: String,
    source_size: u64,
    source_modified_at_ms: Option<u64>,
    page_count: usize,
    field_count: usize,
    editable_field_count: usize,
    flattenable_field_count: usize,
    has_xfa: bool,
    was_encrypted: bool,
    certificate_signature: bool,
    fields: Vec<PdfFormFieldInspection>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfFormsResult {
    output_path: String,
    page_count: usize,
    updated_field_count: usize,
    flattened_field_count: usize,
    remaining_field_count: usize,
    bytes_written: u64,
    encryption: &'static str,
    warnings: Vec<String>,
}

struct LoadedFormsPdf {
    document: Document,
    page_count: usize,
    was_encrypted: bool,
}

#[derive(Clone)]
struct ParsedForms {
    acroform: AcroFormLocation,
    fields: Vec<ParsedField>,
    has_xfa: bool,
    warnings: Vec<String>,
}

#[derive(Clone)]
enum AcroFormLocation {
    Direct(Dictionary),
    Reference(ObjectId, Dictionary),
}

#[derive(Clone)]
struct ParsedField {
    object_id: ObjectId,
    name: String,
    kind: FormFieldKind,
    values: Vec<String>,
    options: Vec<PdfFormOption>,
    widgets: Vec<ParsedWidget>,
    flags: i64,
    max_length: Option<usize>,
    signature_present: bool,
}

#[derive(Clone)]
struct ParsedWidget {
    object_id: ObjectId,
    page_id: Option<ObjectId>,
    page_number: Option<u32>,
    rect: Option<PdfRect>,
    visual_rect: Option<NormalisedRect>,
    export_value: Option<String>,
}

#[derive(Clone, Default)]
struct InheritedField {
    field_type: Option<Vec<u8>>,
    flags: i64,
    value: Option<Object>,
    options: Option<Object>,
    max_length: Option<i64>,
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

#[derive(Clone)]
struct ValidatedUpdate {
    field_id: ObjectId,
    values: Vec<String>,
}

#[derive(Clone)]
struct ExpectedUpdatedField {
    name: String,
    values: Vec<String>,
}

#[derive(Clone)]
struct GeneratedWidget {
    field_id: ObjectId,
    widget_id: ObjectId,
    page_id: ObjectId,
    rect: PdfRect,
    appearance_id: ObjectId,
}

#[cfg(test)]
pub fn inspect_pdf_forms(request: InspectPdfFormsRequest) -> Result<PdfFormInspection, String> {
    inspect_pdf_forms_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_forms_request(
    request: &InspectPdfFormsRequest,
) -> Result<(), String> {
    reject_control_characters("Form source path", &request.input_path)?;
    validate_password(request.input_password.as_deref())
}

fn inspect_pdf_forms_with_control(
    request: InspectPdfFormsRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfFormInspection, String> {
    control.checkpoint(2, "Validating form review")?;
    validate_inspect_pdf_forms_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_size = metadata.len();
    let source_modified_at_ms = modified_at_ms(&metadata);
    control.checkpoint(18, "Opening AcroForm structure")?;
    let loaded = load_pdf(&input, request.input_password.as_deref())?;
    control.checkpoint(26, "Inspecting AcroForm field tree")?;
    let parsed = parse_forms_with_control(&loaded.document, control)?;
    let certificate_signature = document_has_certificate_signature(&loaded.document);
    let mut warnings = parsed.warnings.clone();
    if parsed.has_xfa {
        warnings.push(
            "This PDF contains XFA data. XFA forms are not edited or flattened because their dynamic behaviour cannot be reproduced safely."
                .to_string(),
        );
    }
    if certificate_signature {
        warnings.push(
            "Filling or flattening this form rewrites the certificate-signed PDF and invalidates its existing signatures."
                .to_string(),
        );
    }
    if parsed.fields.is_empty() && !parsed.has_xfa {
        warnings.push("This PDF does not contain any AcroForm fields.".to_string());
    }
    let mut fields = Vec::with_capacity(parsed.fields.len());
    for (index, field) in parsed.fields.iter().enumerate() {
        checkpoint_form_loop(
            control,
            index,
            parsed.fields.len(),
            86,
            92,
            "Preparing reviewed form fields",
        )?;
        fields.push(field_inspection(field));
    }
    let editable_field_count = fields.iter().filter(|field| field.editable).count();
    let flattenable_field_count = fields.iter().filter(|field| field.flattenable).count();

    control.checkpoint(94, "Rechecking form source")?;
    verify_source_fingerprint(&input, source_size, source_modified_at_ms)?;
    control.checkpoint(99, "Finalising form review")?;

    Ok(PdfFormInspection {
        file_name: display_name(&input),
        source_size,
        source_modified_at_ms,
        page_count: loaded.page_count,
        field_count: fields.len(),
        editable_field_count,
        flattenable_field_count,
        has_xfa: parsed.has_xfa,
        was_encrypted: loaded.was_encrypted,
        certificate_signature,
        fields,
        warnings,
    })
}

pub(crate) fn run_pdf_form_inspection_job_with_control(
    request: InspectPdfFormsRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfFormInspection, String> {
    inspect_pdf_forms_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_form_inspection_job_error(&error)
        }
    })
}

fn safe_form_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during form review. Open it again before editing."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The form PDF could not be opened with the supplied password.".to_string();
    }
    "The form review failed a structural safety check. Review the source PDF and try again."
        .to_string()
}

#[cfg(test)]
pub fn export_pdf_forms(request: ExportPdfFormsRequest) -> Result<ExportPdfFormsResult, String> {
    export_pdf_forms_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_export_pdf_forms_request(
    request: &ExportPdfFormsRequest,
) -> Result<(), String> {
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    if request.updates.len() > MAX_FIELDS {
        return Err(format!(
            "A form export can update at most {MAX_FIELDS} fields."
        ));
    }
    Ok(())
}

pub(crate) fn export_pdf_forms_with_control(
    request: ExportPdfFormsRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfFormsResult, String> {
    control.checkpoint(1, "Validating form export")?;
    validate_export_pdf_forms_request(&request)?;
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
    control.checkpoint(22, "Inspecting AcroForm structure")?;
    let mut parsed = parse_forms(&loaded.document)?;
    if parsed.has_xfa {
        return Err(
            "XFA forms cannot be filled or flattened safely. Use a PDF containing ordinary AcroForm fields."
                .to_string(),
        );
    }
    if parsed.fields.is_empty() {
        return Err("This PDF does not contain any AcroForm fields to export.".to_string());
    }
    let updates = validate_updates(request.updates, &parsed.fields)?;
    if updates.is_empty() && !request.flatten {
        return Err(
            "Change at least one form field or enable flattening before exporting.".to_string(),
        );
    }
    let expected_updates = expected_updated_fields(&parsed.fields, &updates)?;
    let original_field_count = parsed.fields.len();
    let font_id = loaded.document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    install_form_font(&mut loaded.document, &mut parsed.acroform, font_id)?;
    let mut effective_values = parsed
        .fields
        .iter()
        .map(|field| (field.object_id, field.values.clone()))
        .collect::<HashMap<_, _>>();
    for (index, update) in updates.iter().enumerate() {
        checkpoint_form_loop(
            control,
            index,
            updates.len(),
            30,
            40,
            "Applying reviewed form values",
        )?;
        effective_values.insert(update.field_id, update.values.clone());
        let field = parsed
            .fields
            .iter()
            .find(|field| field.object_id == update.field_id)
            .ok_or_else(|| "A reviewed form field disappeared before export.".to_string())?;
        apply_field_value(&mut loaded.document, field, &update.values)?;
    }

    let update_ids = updates
        .iter()
        .map(|update| update.field_id)
        .collect::<HashSet<_>>();
    let mut generated_widgets = Vec::new();
    let mut substituted_appearances = 0_usize;
    for (index, field) in parsed.fields.iter().enumerate() {
        checkpoint_form_loop(
            control,
            index,
            parsed.fields.len(),
            42,
            58,
            "Generating form appearances",
        )?;
        let should_generate = update_ids.contains(&field.object_id)
            || (request.flatten && field_is_flattenable(field));
        if !should_generate {
            continue;
        }
        let values = effective_values
            .get(&field.object_id)
            .cloned()
            .unwrap_or_default();
        let (generated, substitutions) =
            generate_field_appearances(&mut loaded.document, field, &values, font_id)?;
        generated_widgets.extend(generated);
        substituted_appearances += substitutions;
    }

    let mut flattened_field_ids = HashSet::new();
    if request.flatten {
        for (index, field) in parsed.fields.iter().enumerate() {
            checkpoint_form_loop(
                control,
                index,
                parsed.fields.len(),
                59,
                64,
                "Preparing fields for flattening",
            )?;
            if field_is_flattenable(field)
                && !field.widgets.is_empty()
                && field
                    .widgets
                    .iter()
                    .all(|widget| widget.page_id.is_some() && widget.rect.is_some())
            {
                flattened_field_ids.insert(field.object_id);
            }
        }
        let widgets_to_flatten = generated_widgets
            .iter()
            .filter(|widget| flattened_field_ids.contains(&widget.field_id))
            .cloned()
            .collect::<Vec<_>>();
        flatten_widgets(&mut loaded.document, &widgets_to_flatten)?;
        prune_flattened_fields(
            &mut loaded.document,
            &mut parsed.acroform,
            &flattened_field_ids,
        )?;
    } else {
        write_acroform(&mut loaded.document, &parsed.acroform)?;
    }
    let flattened_field_names = parsed
        .fields
        .iter()
        .filter(|field| flattened_field_ids.contains(&field.object_id))
        .map(|field| field.name.clone())
        .collect::<HashSet<_>>();
    let expected_flattened_marker_pages = generated_widgets
        .iter()
        .filter(|widget| flattened_field_ids.contains(&widget.field_id))
        .map(|widget| widget.page_id)
        .collect::<HashSet<_>>()
        .len();

    loaded.document.prune_objects();
    loaded.document.change_producer("Tüfekci Paperworks");
    control.checkpoint(66, "Writing prepared form PDF")?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    loaded
        .document
        .save(prepared.path())
        .map_err(|error| format!("The completed form PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| {
            format!("The completed form PDF could not be flushed to storage: {error}")
        })?;

    control.checkpoint(72, "Verifying prepared form structure")?;
    let prepared_verified = verify_prepared_form_pdf(
        prepared.path(),
        loaded.page_count,
        request.flatten,
        &updates,
        &flattened_field_ids,
        &generated_widgets,
    )?;
    let expected_remaining_field_count = prepared_verified.fields.len();
    let protected = if let Some(protection) = request.output_protection.as_ref() {
        control.checkpoint(78, "Applying AES-256 output protection")?;
        let protected = TemporaryOutput::new(&paths.output)?;
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            control,
        )?;
        control.checkpoint(88, "Verifying protected form structure")?;
        verify_protected_form_pdf(
            protected.path(),
            &protection.open_password,
            loaded.page_count,
            request.flatten,
            &expected_updates,
            &flattened_field_names,
            expected_flattened_marker_pages,
            expected_remaining_field_count,
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
    control.checkpoint(99, "Publishing verified form PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let remaining_field_count = expected_remaining_field_count;
    let flattened_field_count = flattened_field_ids.len();
    let mut warnings = parsed.warnings;
    if substituted_appearances > 0 {
        warnings.push(format!(
            "{substituted_appearances} form appearance{} contained characters outside the built-in Windows Latin font. The full Unicode value remains in editable fields, while unsupported appearance glyphs use question marks.",
            if substituted_appearances == 1 { "" } else { "s" }
        ));
    }
    if request.flatten {
        warnings.push(format!(
            "{flattened_field_count} supported field{} were flattened into static page content and are no longer editable. Signature fields, push buttons, unsupported fields, and fields without complete widget geometry remain interactive.",
            if flattened_field_count == 1 { "" } else { "s" }
        ));
    }
    if request.output_protection.is_some() {
        warnings.push(
            "The completed form copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if loaded.was_encrypted {
        warnings.push(
            "The completed form copy is not password-protected. Use Protect to apply new encryption."
                .to_string(),
        );
    }
    if had_certificate_signature {
        warnings.push(
            "Form editing changed the PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }
    if original_field_count > 0 && remaining_field_count == 0 {
        warnings.push("All AcroForm fields were removed from the flattened copy.".to_string());
    }

    Ok(ExportPdfFormsResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: loaded.page_count,
        updated_field_count: updates.len(),
        flattened_field_count,
        remaining_field_count,
        bytes_written,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_forms_job_with_control(
    request: ExportPdfFormsRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfFormsResult, String> {
    export_pdf_forms_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_form_job_error(&error)
        }
    })
}

fn safe_form_job_error(error: &str) -> String {
    if error.contains("changed on disk") {
        return "The source PDF changed after review. Review its form fields again before exporting."
            .to_string();
    }
    if error.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before exporting the form."
            .to_string();
    }
    if error.contains("XFA") {
        return "XFA forms cannot be filled or flattened safely. Use a PDF containing ordinary AcroForm fields."
            .to_string();
    }
    if error.contains("QPDF") {
        return "AES-256 form protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if error.to_ascii_lowercase().contains("password")
        || error.to_ascii_lowercase().contains("decrypt")
    {
        return "The form PDF could not be opened or protected with the supplied passwords."
            .to_string();
    }
    if error.contains("destination already exists") {
        return "The destination already exists. Choose a new filename.".to_string();
    }
    if error.contains("cannot be overwritten") {
        return "The source PDF cannot be overwritten. Choose a new filename.".to_string();
    }
    "The form export failed a structural safety check. Review the form and try again.".to_string()
}

fn checkpoint_form_loop(
    control: &PdfJobExecutionControl,
    index: usize,
    total: usize,
    start: u8,
    end: u8,
    stage: &str,
) -> Result<(), String> {
    if !index.is_multiple_of(32) {
        return control.ensure_not_cancelled();
    }
    let span = end.saturating_sub(start);
    let progress = start.saturating_add(
        ((u128::from(span) * index as u128) / total.max(1) as u128)
            .try_into()
            .unwrap_or(span),
    );
    control.checkpoint(
        progress.min(end),
        format!("{stage} ({}/{total})", index + 1),
    )
}

fn expected_updated_fields(
    fields: &[ParsedField],
    updates: &[ValidatedUpdate],
) -> Result<Vec<ExpectedUpdatedField>, String> {
    updates
        .iter()
        .map(|update| {
            let field = fields
                .iter()
                .find(|field| field.object_id == update.field_id)
                .ok_or_else(|| "A reviewed form field disappeared before export.".to_string())?;
            Ok(ExpectedUpdatedField {
                name: field.name.clone(),
                values: update.values.clone(),
            })
        })
        .collect()
}

fn verify_prepared_form_pdf(
    path: &Path,
    expected_page_count: usize,
    flatten: bool,
    updates: &[ValidatedUpdate],
    flattened_field_ids: &HashSet<ObjectId>,
    generated_widgets: &[GeneratedWidget],
) -> Result<ParsedForms, String> {
    let verification = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The completed form PDF failed its reopening check: {error}"))?;
    if verification.is_encrypted() {
        return Err(
            "The completed form PDF unexpectedly remained encrypted and was not saved.".to_string(),
        );
    }
    if verification.get_pages().len() != expected_page_count {
        return Err("The completed form PDF changed the page count and was not saved.".to_string());
    }
    let verified = parse_forms(&verification)?;
    if flatten {
        verify_flattened_fields(
            &verification,
            &verified,
            flattened_field_ids,
            generated_widgets,
        )?;
    } else {
        verify_updated_fields(&verification, &verified, updates)?;
    }
    Ok(verified)
}

#[allow(clippy::too_many_arguments)]
fn verify_protected_form_pdf(
    path: &Path,
    password: &str,
    expected_page_count: usize,
    flatten: bool,
    expected_updates: &[ExpectedUpdatedField],
    flattened_field_names: &HashSet<String>,
    expected_flattened_marker_pages: usize,
    expected_remaining_field_count: usize,
) -> Result<(), String> {
    let mut verification = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The protected form PDF failed its reopening check: {error}"))?;
    if !verification.is_encrypted() {
        return Err(
            "The completed form PDF was not encrypted as requested and was not saved.".to_string(),
        );
    }
    verification.decrypt(password).map_err(|_| {
        "The protected form PDF could not be reopened with its new password.".to_string()
    })?;
    if verification.get_pages().len() != expected_page_count {
        return Err("The protected form PDF changed the page count and was not saved.".to_string());
    }
    let verified = parse_forms(&verification)?;
    if verified.fields.len() != expected_remaining_field_count {
        return Err(
            "The protected form PDF changed its remaining field count and was not saved."
                .to_string(),
        );
    }
    if flatten {
        if verified
            .fields
            .iter()
            .any(|field| flattened_field_names.contains(&field.name))
        {
            return Err(
                "A flattened form field became interactive after protection and the PDF was not saved."
                    .to_string(),
            );
        }
        let marker_count = verification
            .objects
            .values()
            .filter(|object| {
                object
                    .as_stream()
                    .is_ok_and(|stream| stream.dict.has(b"TufekciFlattenedForm"))
            })
            .count();
        if marker_count < expected_flattened_marker_pages {
            return Err("Protected flattened form page content could not be verified.".to_string());
        }
    } else {
        verify_updated_fields_by_name(&verification, &verified, expected_updates)?;
    }
    Ok(())
}

fn verify_updated_fields_by_name(
    document: &Document,
    parsed: &ParsedForms,
    expected_updates: &[ExpectedUpdatedField],
) -> Result<(), String> {
    for expected in expected_updates {
        let mut matches = parsed
            .fields
            .iter()
            .filter(|field| field.name == expected.name);
        let field = matches
            .next()
            .ok_or_else(|| "An updated form field disappeared after protection.".to_string())?;
        if matches.next().is_some() {
            return Err(
                "An updated form field name became ambiguous after protection.".to_string(),
            );
        }
        if field.values != expected.values {
            return Err(
                "An updated form value changed after protection and the PDF was not saved."
                    .to_string(),
            );
        }
        for widget in field
            .widgets
            .iter()
            .filter(|widget| widget.page_id.is_some() && widget.rect.is_some())
        {
            let dictionary = document
                .get_dictionary(widget.object_id)
                .map_err(|error| format!("An updated form widget is invalid: {error}"))?;
            let appearance = dictionary
                .get(b"AP")
                .map_err(|_| "An updated form widget lost its appearance.".to_string())?;
            let appearance = resolved_dictionary(document, appearance)?;
            if !appearance.has(b"N") {
                return Err("An updated form widget lost its normal appearance.".to_string());
            }
        }
    }
    Ok(())
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
            "The source PDF changed on disk after its form fields were reviewed. Review it again before exporting."
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

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedFormsPdf, String> {
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
                "The PDF could not be decrypted for form editing. Check its password.".to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    Ok(LoadedFormsPdf {
        document,
        page_count,
        was_encrypted,
    })
}

fn field_inspection(field: &ParsedField) -> PdfFormFieldInspection {
    let read_only = field.flags & FLAG_READ_ONLY != 0;
    let editable = field_is_editable(field);
    PdfFormFieldInspection {
        field_id: object_id_string(field.object_id),
        name: field.name.clone(),
        kind: field.kind,
        values: field.values.clone(),
        options: field.options.clone(),
        widgets: field
            .widgets
            .iter()
            .map(|widget| PdfFormWidgetInspection {
                widget_id: object_id_string(widget.object_id),
                page_number: widget.page_number,
                rect: widget.visual_rect,
                export_value: widget.export_value.clone(),
            })
            .collect(),
        editable,
        flattenable: field_is_flattenable(field),
        read_only,
        required: field.flags & FLAG_REQUIRED != 0,
        multiline: field.flags & FLAG_MULTILINE != 0,
        password: field.flags & FLAG_PASSWORD != 0,
        combo: field.flags & FLAG_COMBO != 0,
        editable_choice: field.flags & FLAG_EDIT != 0,
        multi_select: field.flags & FLAG_MULTI_SELECT != 0,
        max_length: field.max_length,
        signature_present: field.signature_present,
    }
}

fn field_is_editable(field: &ParsedField) -> bool {
    field.flags & FLAG_READ_ONLY == 0
        && matches!(
            field.kind,
            FormFieldKind::Text
                | FormFieldKind::Checkbox
                | FormFieldKind::Radio
                | FormFieldKind::Choice
        )
}

fn field_is_flattenable(field: &ParsedField) -> bool {
    matches!(
        field.kind,
        FormFieldKind::Text
            | FormFieldKind::Checkbox
            | FormFieldKind::Radio
            | FormFieldKind::Choice
    ) && !field.widgets.is_empty()
        && field
            .widgets
            .iter()
            .all(|widget| widget.page_id.is_some() && widget.rect.is_some())
}

fn parse_forms(document: &Document) -> Result<ParsedForms, String> {
    parse_forms_with_control(document, &PdfJobExecutionControl::direct())
}

fn parse_forms_with_control(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<ParsedForms, String> {
    control.ensure_not_cancelled()?;
    let Some(acroform) = acroform_location(document)? else {
        return Ok(ParsedForms {
            acroform: AcroFormLocation::Direct(Dictionary::new()),
            fields: Vec::new(),
            has_xfa: false,
            warnings: Vec::new(),
        });
    };
    let acroform_dictionary = acroform.dictionary();
    let has_xfa = acroform_dictionary.has(b"XFA");
    let field_ids = reference_array(
        document,
        acroform_dictionary.get(b"Fields"),
        "form field list",
    )?;
    let page_context = PageContext::new_with_control(document, control)?;
    let mut context = FieldParseContext {
        document,
        control,
        page_context,
        fields: Vec::new(),
        visited: HashSet::new(),
        widget_count: 0,
        total_options: 0,
        warnings: Vec::new(),
    };
    for field_id in field_ids {
        parse_field_node(&mut context, field_id, "", InheritedField::default(), 0)?;
    }
    Ok(ParsedForms {
        acroform,
        fields: context.fields,
        has_xfa,
        warnings: context.warnings,
    })
}

impl AcroFormLocation {
    fn dictionary(&self) -> &Dictionary {
        match self {
            Self::Direct(dictionary) | Self::Reference(_, dictionary) => dictionary,
        }
    }

    fn dictionary_mut(&mut self) -> &mut Dictionary {
        match self {
            Self::Direct(dictionary) | Self::Reference(_, dictionary) => dictionary,
        }
    }
}

fn acroform_location(document: &Document) -> Result<Option<AcroFormLocation>, String> {
    let catalog = document
        .catalog()
        .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?;
    match catalog.get(b"AcroForm") {
        Err(_) | Ok(Object::Null) => Ok(None),
        Ok(Object::Dictionary(dictionary)) => {
            Ok(Some(AcroFormLocation::Direct(dictionary.clone())))
        }
        Ok(Object::Reference(id)) => document
            .get_dictionary(*id)
            .cloned()
            .map(|dictionary| Some(AcroFormLocation::Reference(*id, dictionary)))
            .map_err(|error| format!("The PDF AcroForm dictionary is invalid: {error}")),
        Ok(_) => Err("The PDF AcroForm entry is invalid.".to_string()),
    }
}

struct PageContext {
    page_numbers: HashMap<ObjectId, u32>,
    widget_pages: HashMap<ObjectId, ObjectId>,
    geometry: HashMap<ObjectId, PageGeometry>,
}

impl PageContext {
    fn new_with_control(
        document: &Document,
        control: &PdfJobExecutionControl,
    ) -> Result<Self, String> {
        let pages = document.get_pages();
        let page_numbers = pages
            .iter()
            .map(|(number, id)| (*id, *number))
            .collect::<HashMap<_, _>>();
        let mut widget_pages = HashMap::new();
        let mut geometry = HashMap::new();
        let total = pages.len().max(1);
        for (index, page_id) in pages.values().enumerate() {
            control.checkpoint(
                30 + (((index + 1) * 18 / total).min(18)) as u8,
                format!("Inspecting form page {} of {}", index + 1, pages.len()),
            )?;
            geometry.insert(*page_id, page_geometry(document, *page_id)?);
            let page = document
                .get_dictionary(*page_id)
                .map_err(|error| format!("A form page is invalid: {error}"))?;
            for annotation in object_array(document, page.get(b"Annots"), "page annotation list")? {
                control.ensure_not_cancelled()?;
                if let Ok(annotation_id) = annotation.as_reference() {
                    widget_pages.insert(annotation_id, *page_id);
                }
            }
        }
        Ok(Self {
            page_numbers,
            widget_pages,
            geometry,
        })
    }
}

struct FieldParseContext<'a> {
    document: &'a Document,
    control: &'a PdfJobExecutionControl,
    page_context: PageContext,
    fields: Vec<ParsedField>,
    visited: HashSet<ObjectId>,
    widget_count: usize,
    total_options: usize,
    warnings: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn parse_field_node(
    context: &mut FieldParseContext<'_>,
    field_id: ObjectId,
    parent_name: &str,
    inherited: InheritedField,
    depth: usize,
) -> Result<(), String> {
    let node_number = context.visited.len() + 1;
    context.control.checkpoint(
        50 + ((node_number.min(MAX_FIELDS + MAX_WIDGETS) * 34 / (MAX_FIELDS + MAX_WIDGETS)) as u8),
        format!("Inspecting form field node {node_number}"),
    )?;
    if depth >= MAX_FIELD_DEPTH {
        return Err(format!(
            "The AcroForm field tree exceeds the supported depth of {MAX_FIELD_DEPTH}."
        ));
    }
    if !context.visited.insert(field_id) {
        return Err(
            "The AcroForm field tree contains a cycle or repeated field object.".to_string(),
        );
    }
    if context.visited.len() > MAX_FIELDS + MAX_WIDGETS {
        return Err("The AcroForm field tree is too large to inspect safely.".to_string());
    }
    let dictionary = context
        .document
        .get_dictionary(field_id)
        .map_err(|error| format!("An AcroForm field is invalid: {error}"))?;
    let inherited = inherit_field(dictionary, inherited)?;
    let partial_name = dictionary
        .get(b"T")
        .ok()
        .map(decode_limited_text)
        .transpose()?
        .unwrap_or_default();
    let name = qualified_name(parent_name, &partial_name);
    let kid_ids = reference_array(
        context.document,
        dictionary.get(b"Kids"),
        "field child list",
    )?;
    let all_widget_kids = !kid_ids.is_empty()
        && kid_ids.iter().all(|kid_id| {
            context
                .document
                .get_dictionary(*kid_id)
                .is_ok_and(|kid| is_widget_dictionary(kid) && !kid.has(b"T") && !kid.has(b"FT"))
        });
    let is_widget = is_widget_dictionary(dictionary);
    let terminal = is_widget || kid_ids.is_empty() || all_widget_kids;

    if terminal {
        if context.fields.len() >= MAX_FIELDS {
            return Err(format!(
                "A PDF can contain at most {MAX_FIELDS} supported form fields."
            ));
        }
        let widget_ids = if is_widget {
            vec![field_id]
        } else if all_widget_kids {
            kid_ids.clone()
        } else {
            Vec::new()
        };
        context.widget_count += widget_ids.len();
        if context.widget_count > MAX_WIDGETS {
            return Err(format!(
                "A PDF can contain at most {MAX_WIDGETS} form widgets."
            ));
        }
        let kind = classify_field(inherited.field_type.as_deref(), inherited.flags);
        let mut widgets = Vec::with_capacity(widget_ids.len());
        for widget_id in widget_ids {
            context.control.ensure_not_cancelled()?;
            widgets.push(parse_widget(context, widget_id)?);
        }
        context.control.ensure_not_cancelled()?;
        let options = parse_options(inherited.options.as_ref(), context.control)?;
        context.total_options += options.len();
        if options.len() > MAX_OPTIONS_PER_FIELD || context.total_options > MAX_TOTAL_OPTIONS {
            return Err("The AcroForm contains too many choice or button options.".to_string());
        }
        let mut field_options = options;
        if matches!(kind, FormFieldKind::Checkbox | FormFieldKind::Radio) {
            for value in widgets
                .iter()
                .filter_map(|widget| widget.export_value.clone())
            {
                if !field_options.iter().any(|option| option.value == value) {
                    field_options.push(PdfFormOption {
                        label: value.clone(),
                        value,
                    });
                }
            }
            if kind == FormFieldKind::Checkbox && field_options.is_empty() {
                field_options.push(PdfFormOption {
                    label: "Checked".to_string(),
                    value: "Yes".to_string(),
                });
            }
        }
        let values = field_values(kind, inherited.value.as_ref())?;
        let field_name = if name.is_empty() {
            format!("Unnamed field {}", context.fields.len() + 1)
        } else {
            validate_field_name(name)?
        };
        context.fields.push(ParsedField {
            object_id: field_id,
            name: field_name,
            kind,
            values,
            options: field_options,
            widgets,
            flags: inherited.flags,
            max_length: inherited
                .max_length
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
                .map(|value| value.min(MAX_VALUE_CHARACTERS)),
            signature_present: kind == FormFieldKind::Signature
                && inherited
                    .value
                    .as_ref()
                    .is_some_and(|value| !matches!(value, Object::Null)),
        });
        return Ok(());
    }

    for kid_id in kid_ids {
        parse_field_node(context, kid_id, &name, inherited.clone(), depth + 1)?;
    }
    Ok(())
}

fn inherit_field(
    dictionary: &Dictionary,
    mut inherited: InheritedField,
) -> Result<InheritedField, String> {
    if let Ok(field_type) = dictionary.get(b"FT") {
        inherited.field_type = Some(
            field_type
                .as_name()
                .map_err(|_| "An AcroForm field has an invalid type.".to_string())?
                .to_vec(),
        );
    }
    if let Ok(flags) = dictionary.get(b"Ff") {
        inherited.flags = flags
            .as_i64()
            .map_err(|_| "An AcroForm field has invalid flags.".to_string())?;
    }
    if let Ok(value) = dictionary.get(b"V") {
        inherited.value = Some(value.clone());
    }
    if let Ok(options) = dictionary.get(b"Opt") {
        inherited.options = Some(options.clone());
    }
    if let Ok(max_length) = dictionary.get(b"MaxLen") {
        inherited.max_length = Some(
            max_length
                .as_i64()
                .map_err(|_| "A text form field has an invalid maximum length.".to_string())?,
        );
    }
    Ok(inherited)
}

fn parse_widget(
    context: &FieldParseContext<'_>,
    widget_id: ObjectId,
) -> Result<ParsedWidget, String> {
    let dictionary = context
        .document
        .get_dictionary(widget_id)
        .map_err(|error| format!("An AcroForm widget is invalid: {error}"))?;
    if !is_widget_dictionary(dictionary) {
        return Err("An AcroForm widget child is not a Widget annotation.".to_string());
    }
    let page_id = context
        .page_context
        .widget_pages
        .get(&widget_id)
        .copied()
        .or_else(|| dictionary.get(b"P").and_then(Object::as_reference).ok());
    let page_number = page_id.and_then(|id| context.page_context.page_numbers.get(&id).copied());
    let rect = dictionary
        .get(b"Rect")
        .ok()
        .map(parse_pdf_rect)
        .transpose()?;
    let visual_rect = page_id
        .zip(rect)
        .and_then(|(id, rect)| {
            context
                .page_context
                .geometry
                .get(&id)
                .map(|geometry| (*geometry, rect))
        })
        .map(|(geometry, rect)| rect_to_visual(geometry, rect));
    Ok(ParsedWidget {
        object_id: widget_id,
        page_id,
        page_number,
        rect,
        visual_rect,
        export_value: widget_export_value(context.document, dictionary),
    })
}

fn is_widget_dictionary(dictionary: &Dictionary) -> bool {
    dictionary
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|name| name == b"Widget")
}

fn classify_field(field_type: Option<&[u8]>, flags: i64) -> FormFieldKind {
    match field_type {
        Some(b"Tx") => FormFieldKind::Text,
        Some(b"Ch") => FormFieldKind::Choice,
        Some(b"Sig") => FormFieldKind::Signature,
        Some(b"Btn") if flags & FLAG_PUSHBUTTON != 0 => FormFieldKind::Button,
        Some(b"Btn") if flags & FLAG_RADIO != 0 => FormFieldKind::Radio,
        Some(b"Btn") => FormFieldKind::Checkbox,
        Some(_) | None => FormFieldKind::Unsupported,
    }
}

fn field_values(kind: FormFieldKind, value: Option<&Object>) -> Result<Vec<String>, String> {
    if kind == FormFieldKind::Signature {
        return Ok(Vec::new());
    }
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    object_text_values(value)
}

fn object_text_values(value: &Object) -> Result<Vec<String>, String> {
    match value {
        Object::Null => Ok(Vec::new()),
        Object::String(_, _) => Ok(vec![decode_limited_text(value)?]),
        Object::Name(name) if name == b"Off" => Ok(Vec::new()),
        Object::Name(name) => Ok(vec![String::from_utf8_lossy(name).into_owned()]),
        Object::Array(values) => values
            .iter()
            .map(|value| match value {
                Object::String(_, _) => decode_limited_text(value),
                Object::Name(name) => Ok(String::from_utf8_lossy(name).into_owned()),
                _ => Err("A form field contains an invalid array value.".to_string()),
            })
            .collect(),
        _ => Err("A form field contains an unsupported value type.".to_string()),
    }
}

fn parse_options(
    value: Option<&Object>,
    control: &PdfJobExecutionControl,
) -> Result<Vec<PdfFormOption>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .map_err(|_| "A choice field has an invalid option list.".to_string())?;
    let mut options = Vec::with_capacity(values.len());
    for option in values {
        control.ensure_not_cancelled()?;
        options.push(match option {
            Object::String(_, _) => {
                let value = decode_limited_text(option)?;
                Ok(PdfFormOption {
                    label: value.clone(),
                    value,
                })
            }
            Object::Array(pair) if pair.len() == 2 => Ok(PdfFormOption {
                value: decode_limited_text(&pair[0])?,
                label: decode_limited_text(&pair[1])?,
            }),
            _ => Err("A choice field contains an invalid option.".to_string()),
        }?);
    }
    Ok(options)
}

fn widget_export_value(document: &Document, widget: &Dictionary) -> Option<String> {
    let appearance = resolved_dictionary(document, widget.get(b"AP").ok()?).ok()?;
    let normal = resolved_dictionary(document, appearance.get(b"N").ok()?).ok()?;
    normal
        .iter()
        .map(|(name, _)| name)
        .find(|name| name.as_slice() != b"Off")
        .map(|name| String::from_utf8_lossy(name).into_owned())
}

fn qualified_name(parent: &str, partial: &str) -> String {
    match (parent.is_empty(), partial.is_empty()) {
        (true, _) => partial.to_string(),
        (_, true) => parent.to_string(),
        (false, false) => format!("{parent}.{partial}"),
    }
}

fn validate_field_name(name: String) -> Result<String, String> {
    if name.chars().count() > MAX_FIELD_NAME_CHARACTERS || name.len() > MAX_FIELD_NAME_BYTES {
        return Err("An AcroForm field name is too long to process safely.".to_string());
    }
    Ok(name)
}

fn decode_limited_text(value: &Object) -> Result<String, String> {
    let value = decode_text_string(value)
        .map_err(|_| "An AcroForm text string could not be decoded.".to_string())?;
    validate_form_value(value)
}

fn validate_form_value(value: String) -> Result<String, String> {
    let value = value.replace("\r\n", "\n").replace('\r', "\n");
    if value.chars().count() > MAX_VALUE_CHARACTERS || value.len() > MAX_VALUE_BYTES {
        return Err("A form field value is too long to process safely.".to_string());
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err("A form field value contains unsupported control characters.".to_string());
    }
    Ok(value)
}

fn reference_array(
    document: &Document,
    value: Result<&Object, lopdf::Error>,
    label: &str,
) -> Result<Vec<ObjectId>, String> {
    let value = match value {
        Err(_) | Ok(Object::Null) => return Ok(Vec::new()),
        Ok(value) => value,
    };
    let array = match value {
        Object::Array(array) => array,
        Object::Reference(id) => document
            .get_object(*id)
            .and_then(Object::as_array)
            .map_err(|error| format!("The PDF {label} is invalid: {error}"))?,
        _ => return Err(format!("The PDF {label} is not an array.")),
    };
    array
        .iter()
        .map(|value| {
            value
                .as_reference()
                .map_err(|_| format!("The PDF {label} contains a direct or invalid object."))
        })
        .collect()
}

fn object_array(
    document: &Document,
    value: Result<&Object, lopdf::Error>,
    label: &str,
) -> Result<Vec<Object>, String> {
    let value = match value {
        Err(_) | Ok(Object::Null) => return Ok(Vec::new()),
        Ok(value) => value,
    };
    match value {
        Object::Array(array) => Ok(array.clone()),
        Object::Reference(id) => document
            .get_object(*id)
            .and_then(Object::as_array)
            .cloned()
            .map_err(|error| format!("The PDF {label} is invalid: {error}")),
        _ => Err(format!("The PDF {label} is not an array.")),
    }
}

fn resolved_dictionary(document: &Document, value: &Object) -> Result<Dictionary, String> {
    match value {
        Object::Dictionary(dictionary) => Ok(dictionary.clone()),
        Object::Reference(id) => document
            .get_dictionary(*id)
            .cloned()
            .map_err(|error| format!("A referenced form dictionary is invalid: {error}")),
        _ => Err("A form dictionary entry is invalid.".to_string()),
    }
}

fn object_id_string(id: ObjectId) -> String {
    format!("{}-{}", id.0, id.1)
}

fn parse_object_id(value: &str) -> Result<ObjectId, String> {
    let (object, generation) = value
        .split_once('-')
        .ok_or_else(|| "A form field identifier is invalid.".to_string())?;
    let object = object
        .parse::<u32>()
        .map_err(|_| "A form field identifier is invalid.".to_string())?;
    let generation = generation
        .parse::<u16>()
        .map_err(|_| "A form field identifier is invalid.".to_string())?;
    Ok((object, generation))
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<PageGeometry, String> {
    let page_box = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "A form page does not define a crop or media box.".to_string())?;
    let page_box = dereference_object(document, &page_box, "form page box")?;
    let coordinates = page_box
        .as_array()
        .map_err(|_| "A form page box is not an array.".to_string())?;
    if coordinates.len() != 4 {
        return Err("A form page box must contain four coordinates.".to_string());
    }
    let values = coordinates
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, _>>()?;
    let width = values[2] - values[0];
    let height = values[3] - values[1];
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("A form page has invalid dimensions.".to_string());
    }
    let rotation = match inherited_page_value(document, page_id, b"Rotate")? {
        Some(value) => dereference_object(document, &value, "form page rotation")?
            .as_i64()
            .map_err(|_| "A form page has an invalid rotation.".to_string())?,
        None => 0,
    }
    .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("A form page has an unsupported rotation.".to_string());
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
        _ => Err("A form page box contains a non-numeric value.".to_string()),
    }
}

fn parse_pdf_rect(value: &Object) -> Result<PdfRect, String> {
    let values = value
        .as_array()
        .map_err(|_| "A form widget rectangle is not an array.".to_string())?;
    if values.len() != 4 {
        return Err("A form widget rectangle must contain four coordinates.".to_string());
    }
    let coordinates = values
        .iter()
        .map(pdf_number_value)
        .collect::<Result<Vec<_>, _>>()?;
    let left = coordinates[0].min(coordinates[2]);
    let right = coordinates[0].max(coordinates[2]);
    let bottom = coordinates[1].min(coordinates[3]);
    let top = coordinates[1].max(coordinates[3]);
    if !left.is_finite()
        || !right.is_finite()
        || !bottom.is_finite()
        || !top.is_finite()
        || right - left < 0.5
        || top - bottom < 0.5
    {
        return Err("A form widget has invalid dimensions.".to_string());
    }
    Ok(PdfRect {
        left,
        bottom,
        right,
        top,
    })
}

fn rect_to_visual(geometry: PageGeometry, rect: PdfRect) -> NormalisedRect {
    let corners = [
        PdfPoint {
            x: rect.left,
            y: rect.bottom,
        },
        PdfPoint {
            x: rect.left,
            y: rect.top,
        },
        PdfPoint {
            x: rect.right,
            y: rect.bottom,
        },
        PdfPoint {
            x: rect.right,
            y: rect.top,
        },
    ];
    let visual = corners
        .into_iter()
        .map(|point| pdf_to_visual(geometry, point))
        .collect::<Vec<_>>();
    let left = visual
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let right = visual
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let top = visual
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let bottom = visual
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    NormalisedRect {
        x: (left / geometry.visual_width).clamp(0.0, 1.0),
        y: (top / geometry.visual_height).clamp(0.0, 1.0),
        width: ((right - left) / geometry.visual_width).clamp(0.0, 1.0),
        height: ((bottom - top) / geometry.visual_height).clamp(0.0, 1.0),
    }
}

fn pdf_to_visual(geometry: PageGeometry, point: PdfPoint) -> PdfPoint {
    let x = point.x - geometry.page.left;
    let y = point.y - geometry.page.bottom;
    let (visual_x, visual_bottom) = match geometry.rotation {
        90 => (y, geometry.page.width - x),
        180 => (geometry.page.width - x, geometry.page.height - y),
        270 => (geometry.page.height - y, x),
        _ => (x, y),
    };
    PdfPoint {
        x: visual_x,
        y: geometry.visual_height - visual_bottom,
    }
}

impl PdfRect {
    fn width(self) -> f64 {
        self.right - self.left
    }

    fn height(self) -> f64 {
        self.top - self.bottom
    }
}

fn validate_updates(
    updates: Vec<PdfFormFieldUpdate>,
    fields: &[ParsedField],
) -> Result<Vec<ValidatedUpdate>, String> {
    if updates.len() > MAX_FIELDS {
        return Err(format!(
            "At most {MAX_FIELDS} form fields can be updated at once."
        ));
    }
    let fields_by_id = fields
        .iter()
        .map(|field| (field.object_id, field))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut validated = Vec::with_capacity(updates.len());
    for update in updates {
        let field_id = parse_object_id(&update.field_id)?;
        if !seen.insert(field_id) {
            return Err("A form field update was submitted more than once.".to_string());
        }
        let field = fields_by_id
            .get(&field_id)
            .copied()
            .ok_or_else(|| "A form field update refers to an unknown field.".to_string())?;
        if !field_is_editable(field) {
            return Err(format!(
                "The form field “{}” is read-only or unsupported.",
                field.name
            ));
        }
        let values = update
            .values
            .into_iter()
            .map(validate_form_value)
            .collect::<Result<Vec<_>, _>>()?;
        validate_field_update(field, &values)?;
        if values != field.values {
            validated.push(ValidatedUpdate { field_id, values });
        }
    }
    Ok(validated)
}

fn validate_field_update(field: &ParsedField, values: &[String]) -> Result<(), String> {
    let empty = values.is_empty() || values.iter().all(|value| value.is_empty());
    if field.flags & FLAG_REQUIRED != 0 && empty {
        return Err(format!(
            "The required form field “{}” cannot be empty.",
            field.name
        ));
    }
    match field.kind {
        FormFieldKind::Text => {
            if values.len() > 1 {
                return Err(format!(
                    "The text field “{}” accepts one value.",
                    field.name
                ));
            }
            if let Some(max_length) = field.max_length {
                if values
                    .first()
                    .is_some_and(|value| value.chars().count() > max_length)
                {
                    return Err(format!(
                        "The text field “{}” accepts at most {max_length} characters.",
                        field.name
                    ));
                }
            }
            if field.flags & FLAG_MULTILINE == 0
                && values
                    .first()
                    .is_some_and(|value| value.contains(['\r', '\n']))
            {
                return Err(format!(
                    "The text field “{}” does not allow multiple lines.",
                    field.name
                ));
            }
        }
        FormFieldKind::Checkbox | FormFieldKind::Radio => {
            if values.len() > 1 {
                return Err(format!(
                    "The button field “{}” accepts one choice.",
                    field.name
                ));
            }
            if let Some(value) = values.first() {
                if !value.is_empty() && !field.options.iter().any(|option| option.value == *value) {
                    return Err(format!(
                        "The button field “{}” received an unknown choice.",
                        field.name
                    ));
                }
            }
        }
        FormFieldKind::Choice => {
            if field.flags & FLAG_MULTI_SELECT == 0 && values.len() > 1 {
                return Err(format!(
                    "The choice field “{}” accepts one value.",
                    field.name
                ));
            }
            if field.flags & FLAG_EDIT == 0 {
                for value in values {
                    if !value.is_empty()
                        && !field.options.iter().any(|option| option.value == *value)
                    {
                        return Err(format!(
                            "The choice field “{}” received an unknown option.",
                            field.name
                        ));
                    }
                }
            }
        }
        _ => {
            return Err(format!("The form field “{}” cannot be edited.", field.name));
        }
    }
    Ok(())
}

fn install_form_font(
    document: &mut Document,
    acroform: &mut AcroFormLocation,
    font_id: ObjectId,
) -> Result<(), String> {
    let mut resources = match acroform.dictionary().get(b"DR") {
        Ok(value) => resolved_dictionary(document, value)?,
        Err(_) => Dictionary::new(),
    };
    let mut fonts = match resources.get(b"Font") {
        Ok(value) => resolved_dictionary(document, value)?,
        Err(_) => Dictionary::new(),
    };
    fonts.set("TufekciHelv", font_id);
    resources.set("Font", fonts);
    let dictionary = acroform.dictionary_mut();
    dictionary.set("DR", resources);
    dictionary.set(
        "DA",
        Object::String(
            b"/TufekciHelv 10 Tf 0 0 0 rg".to_vec(),
            StringFormat::Literal,
        ),
    );
    dictionary.set("NeedAppearances", false);
    Ok(())
}

fn apply_field_value(
    document: &mut Document,
    field: &ParsedField,
    values: &[String],
) -> Result<(), String> {
    let mut dictionary = document
        .get_dictionary(field.object_id)
        .map_err(|error| format!("The form field “{}” is invalid: {error}", field.name))?
        .clone();
    match field.kind {
        FormFieldKind::Text => {
            dictionary.set(
                "V",
                form_text_string(values.first().map(String::as_str).unwrap_or_default()),
            );
            dictionary.remove(b"RV");
        }
        FormFieldKind::Choice => {
            if field.flags & FLAG_MULTI_SELECT != 0 {
                dictionary.set(
                    "V",
                    Object::Array(values.iter().map(|value| form_text_string(value)).collect()),
                );
                let indexes = values
                    .iter()
                    .filter_map(|value| {
                        field
                            .options
                            .iter()
                            .position(|option| option.value == *value)
                            .and_then(|index| i64::try_from(index).ok())
                            .map(Object::Integer)
                    })
                    .collect::<Vec<_>>();
                dictionary.set("I", Object::Array(indexes));
            } else {
                dictionary.set(
                    "V",
                    form_text_string(values.first().map(String::as_str).unwrap_or_default()),
                );
                dictionary.remove(b"I");
            }
        }
        FormFieldKind::Checkbox | FormFieldKind::Radio => {
            let value = values.first().filter(|value| !value.is_empty());
            dictionary.set(
                "V",
                Object::Name(
                    value.map_or_else(|| b"Off".to_vec(), |value| value.as_bytes().to_vec()),
                ),
            );
        }
        _ => {
            return Err(format!(
                "The form field “{}” cannot be updated.",
                field.name
            ))
        }
    }
    document
        .objects
        .insert(field.object_id, Object::Dictionary(dictionary));
    Ok(())
}

fn generate_field_appearances(
    document: &mut Document,
    field: &ParsedField,
    values: &[String],
    font_id: ObjectId,
) -> Result<(Vec<GeneratedWidget>, usize), String> {
    let mut generated = Vec::new();
    let mut substitutions = 0_usize;
    for widget in &field.widgets {
        let (Some(page_id), Some(rect)) = (widget.page_id, widget.rect) else {
            continue;
        };
        match field.kind {
            FormFieldKind::Text | FormFieldKind::Choice => {
                let display_values = if field.kind == FormFieldKind::Choice {
                    values
                        .iter()
                        .map(|value| {
                            field
                                .options
                                .iter()
                                .find(|option| option.value == *value)
                                .map(|option| option.label.clone())
                                .unwrap_or_else(|| value.clone())
                        })
                        .collect::<Vec<_>>()
                } else if field.flags & FLAG_PASSWORD != 0 {
                    values
                        .iter()
                        .map(|value| "*".repeat(value.chars().count()))
                        .collect()
                } else {
                    values.to_vec()
                };
                let (appearance_id, substituted) = text_widget_appearance(
                    document,
                    rect,
                    &display_values,
                    field.flags & FLAG_MULTILINE != 0 || field.flags & FLAG_MULTI_SELECT != 0,
                    font_id,
                )?;
                substitutions += usize::from(substituted);
                set_single_widget_appearance(document, widget.object_id, appearance_id)?;
                generated.push(GeneratedWidget {
                    field_id: field.object_id,
                    widget_id: widget.object_id,
                    page_id,
                    rect,
                    appearance_id,
                });
            }
            FormFieldKind::Checkbox | FormFieldKind::Radio => {
                let on_value = widget
                    .export_value
                    .clone()
                    .or_else(|| field.options.first().map(|option| option.value.clone()))
                    .unwrap_or_else(|| "Yes".to_string());
                let selected = values.first().is_some_and(|value| *value == on_value);
                let (off_id, on_id) =
                    button_widget_appearances(document, rect, field.kind == FormFieldKind::Radio)?;
                set_button_widget_appearance(
                    document,
                    widget.object_id,
                    &on_value,
                    selected,
                    off_id,
                    on_id,
                )?;
                generated.push(GeneratedWidget {
                    field_id: field.object_id,
                    widget_id: widget.object_id,
                    page_id,
                    rect,
                    appearance_id: if selected { on_id } else { off_id },
                });
            }
            _ => {}
        }
    }
    Ok((generated, substitutions))
}

fn set_single_widget_appearance(
    document: &mut Document,
    widget_id: ObjectId,
    appearance_id: ObjectId,
) -> Result<(), String> {
    let mut widget = document
        .get_dictionary(widget_id)
        .map_err(|error| format!("A form widget is invalid: {error}"))?
        .clone();
    let mut appearance = match widget.get(b"AP") {
        Ok(value) => resolved_dictionary(document, value).unwrap_or_default(),
        Err(_) => Dictionary::new(),
    };
    appearance.set("N", appearance_id);
    widget.set("AP", appearance);
    document
        .objects
        .insert(widget_id, Object::Dictionary(widget));
    Ok(())
}

fn set_button_widget_appearance(
    document: &mut Document,
    widget_id: ObjectId,
    on_value: &str,
    selected: bool,
    off_id: ObjectId,
    on_id: ObjectId,
) -> Result<(), String> {
    let mut widget = document
        .get_dictionary(widget_id)
        .map_err(|error| format!("A button form widget is invalid: {error}"))?
        .clone();
    let mut states = Dictionary::new();
    states.set("Off", off_id);
    states.set(on_value.as_bytes().to_vec(), on_id);
    let mut appearance = match widget.get(b"AP") {
        Ok(value) => resolved_dictionary(document, value).unwrap_or_default(),
        Err(_) => Dictionary::new(),
    };
    appearance.set("N", states);
    widget.set("AP", appearance);
    widget.set(
        "AS",
        Object::Name(if selected {
            on_value.as_bytes().to_vec()
        } else {
            b"Off".to_vec()
        }),
    );
    document
        .objects
        .insert(widget_id, Object::Dictionary(widget));
    Ok(())
}

fn text_widget_appearance(
    document: &mut Document,
    rect: PdfRect,
    values: &[String],
    multiline: bool,
    font_id: ObjectId,
) -> Result<(ObjectId, bool), String> {
    let width = rect.width();
    let height = rect.height();
    let font_size = (height * if multiline { 0.28 } else { 0.5 }).clamp(6.0, 14.0);
    let padding = 3.0_f64.min(width / 5.0).min(height / 5.0);
    let max_lines = if multiline {
        ((height - padding * 2.0) / (font_size * 1.2))
            .floor()
            .max(1.0) as usize
    } else {
        1
    }
    .min(64);
    let joined = if multiline {
        values.join("\n")
    } else {
        values.first().cloned().unwrap_or_default()
    };
    let lines = wrap_text(
        &joined,
        (width - padding * 2.0).max(1.0),
        font_size,
        max_lines,
    );
    let mut substitutions = false;
    let mut operations = widget_base_operations(width, height, false);
    operations.extend([
        Operation::new("q", vec![]),
        Operation::new(
            "re",
            vec![
                pdf_real(padding),
                pdf_real(padding),
                pdf_real((width - padding * 2.0).max(0.1)),
                pdf_real((height - padding * 2.0).max(0.1)),
            ],
        ),
        Operation::new("W", vec![]),
        Operation::new("n", vec![]),
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"TufekciHelv".to_vec()), pdf_real(font_size)],
        ),
        Operation::new("rg", vec![pdf_real(0.08), pdf_real(0.1), pdf_real(0.14)]),
    ]);
    for (index, line) in lines.iter().enumerate() {
        let baseline = height - padding - font_size - index as f64 * font_size * 1.2;
        if baseline < padding - font_size * 0.2 {
            break;
        }
        let (encoded, substituted) = encode_win_ansi(line);
        substitutions |= substituted;
        operations.push(Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                pdf_real(padding),
                pdf_real(baseline),
            ],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(encoded, StringFormat::Literal)],
        ));
    }
    operations.extend([Operation::new("ET", vec![]), Operation::new("Q", vec![])]);
    let mut fonts = Dictionary::new();
    fonts.set("TufekciHelv", font_id);
    let mut resources = Dictionary::new();
    resources.set("Font", fonts);
    let appearance = add_form_appearance(document, rect, resources, operations)?;
    Ok((appearance, substitutions))
}

fn button_widget_appearances(
    document: &mut Document,
    rect: PdfRect,
    radio: bool,
) -> Result<(ObjectId, ObjectId), String> {
    let width = rect.width();
    let height = rect.height();
    let off_operations = widget_base_operations(width, height, radio);
    let off_id = add_form_appearance(document, rect, Dictionary::new(), off_operations)?;
    let mut on_operations = widget_base_operations(width, height, radio);
    if radio {
        on_operations.extend(circle_path(
            width / 2.0,
            height / 2.0,
            width.min(height) * 0.23,
        ));
        on_operations.push(Operation::new(
            "rg",
            vec![pdf_real(0.1), pdf_real(0.25), pdf_real(0.65)],
        ));
        on_operations.push(Operation::new("f", vec![]));
    } else {
        let inset = width.min(height) * 0.23;
        on_operations.extend([
            Operation::new("q", vec![]),
            Operation::new("RG", vec![pdf_real(0.1), pdf_real(0.25), pdf_real(0.65)]),
            Operation::new(
                "w",
                vec![pdf_real((width.min(height) * 0.12).clamp(1.2, 3.5))],
            ),
            Operation::new("J", vec![1.into()]),
            Operation::new("m", vec![pdf_real(inset), pdf_real(height * 0.5)]),
            Operation::new("l", vec![pdf_real(width * 0.44), pdf_real(inset)]),
            Operation::new("l", vec![pdf_real(width - inset), pdf_real(height - inset)]),
            Operation::new("S", vec![]),
            Operation::new("Q", vec![]),
        ]);
    }
    let on_id = add_form_appearance(document, rect, Dictionary::new(), on_operations)?;
    Ok((off_id, on_id))
}

fn widget_base_operations(width: f64, height: f64, radio: bool) -> Vec<Operation> {
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("rg", vec![1.into(), 1.into(), 1.into()]),
        Operation::new("RG", vec![pdf_real(0.43), pdf_real(0.48), pdf_real(0.57)]),
        Operation::new("w", vec![pdf_real(1.0)]),
    ];
    if radio {
        operations.extend(circle_path(
            width / 2.0,
            height / 2.0,
            (width.min(height) / 2.0 - 0.7).max(0.2),
        ));
        operations.push(Operation::new("B", vec![]));
    } else {
        operations.extend([
            Operation::new(
                "re",
                vec![
                    pdf_real(0.5),
                    pdf_real(0.5),
                    pdf_real((width - 1.0).max(0.1)),
                    pdf_real((height - 1.0).max(0.1)),
                ],
            ),
            Operation::new("B", vec![]),
        ]);
    }
    operations.push(Operation::new("Q", vec![]));
    operations
}

fn circle_path(centre_x: f64, centre_y: f64, radius: f64) -> Vec<Operation> {
    let kappa = 0.552_284_749_8;
    vec![
        Operation::new("m", vec![pdf_real(centre_x + radius), pdf_real(centre_y)]),
        Operation::new(
            "c",
            vec![
                pdf_real(centre_x + radius),
                pdf_real(centre_y + kappa * radius),
                pdf_real(centre_x + kappa * radius),
                pdf_real(centre_y + radius),
                pdf_real(centre_x),
                pdf_real(centre_y + radius),
            ],
        ),
        Operation::new(
            "c",
            vec![
                pdf_real(centre_x - kappa * radius),
                pdf_real(centre_y + radius),
                pdf_real(centre_x - radius),
                pdf_real(centre_y + kappa * radius),
                pdf_real(centre_x - radius),
                pdf_real(centre_y),
            ],
        ),
        Operation::new(
            "c",
            vec![
                pdf_real(centre_x - radius),
                pdf_real(centre_y - kappa * radius),
                pdf_real(centre_x - kappa * radius),
                pdf_real(centre_y - radius),
                pdf_real(centre_x),
                pdf_real(centre_y - radius),
            ],
        ),
        Operation::new(
            "c",
            vec![
                pdf_real(centre_x + kappa * radius),
                pdf_real(centre_y - radius),
                pdf_real(centre_x + radius),
                pdf_real(centre_y - kappa * radius),
                pdf_real(centre_x + radius),
                pdf_real(centre_y),
            ],
        ),
    ]
}

fn add_form_appearance(
    document: &mut Document,
    rect: PdfRect,
    resources: Dictionary,
    operations: Vec<Operation>,
) -> Result<ObjectId, String> {
    let content = Content { operations }
        .encode()
        .map_err(|error| format!("A form appearance could not be encoded: {error}"))?;
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => Object::Array(vec![0.into(), 0.into(), pdf_real(rect.width()), pdf_real(rect.height())]),
            "Resources" => resources,
            "TufekciFormAppearance" => true,
        },
        content,
    );
    stream
        .compress()
        .map_err(|error| format!("A form appearance could not be compressed: {error}"))?;
    Ok(document.add_object(stream))
}

fn flatten_widgets(document: &mut Document, widgets: &[GeneratedWidget]) -> Result<(), String> {
    let mut by_page: HashMap<ObjectId, Vec<&GeneratedWidget>> = HashMap::new();
    for widget in widgets {
        by_page.entry(widget.page_id).or_default().push(widget);
    }
    for (page_id, page_widgets) in by_page {
        let mut page = document
            .get_dictionary(page_id)
            .map_err(|error| format!("A form page is invalid: {error}"))?
            .clone();
        let mut resources = match page.get(b"Resources") {
            Ok(value) => resolved_dictionary(document, value)?,
            Err(_) => Dictionary::new(),
        };
        let mut xobjects = match resources.get(b"XObject") {
            Ok(value) => resolved_dictionary(document, value)?,
            Err(_) => Dictionary::new(),
        };
        let mut operations = Vec::new();
        for widget in &page_widgets {
            let resource_name = format!("TufekciForm{}", widget.appearance_id.0).into_bytes();
            xobjects.set(resource_name.clone(), widget.appearance_id);
            operations.extend([
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    vec![
                        1.into(),
                        0.into(),
                        0.into(),
                        1.into(),
                        pdf_real(widget.rect.left),
                        pdf_real(widget.rect.bottom),
                    ],
                ),
                Operation::new("Do", vec![Object::Name(resource_name)]),
                Operation::new("Q", vec![]),
            ]);
        }
        resources.set("XObject", xobjects);
        page.set("Resources", resources);
        let content = Content { operations }
            .encode()
            .map_err(|error| format!("Flattened form content could not be encoded: {error}"))?;
        let marker = page_widgets
            .iter()
            .map(|widget| object_id_string(widget.widget_id))
            .collect::<Vec<_>>()
            .join(",");
        let content_id = document.add_object(Stream::new(
            dictionary! {
                "TufekciFlattenedForm" => Object::String(marker.into_bytes(), StringFormat::Literal),
            },
            content,
        ));
        let existing_contents = page.get(b"Contents").ok().cloned();
        page.set(
            "Contents",
            append_content_stream(document, existing_contents, content_id)?,
        );
        let removed = page_widgets
            .iter()
            .map(|widget| widget.widget_id)
            .collect::<HashSet<_>>();
        let annotations = object_array(document, page.get(b"Annots"), "page annotation list")?
            .into_iter()
            .filter(|annotation| {
                annotation
                    .as_reference()
                    .map_or(true, |id| !removed.contains(&id))
            })
            .collect::<Vec<_>>();
        if annotations.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", Object::Array(annotations));
        }
        document.objects.insert(page_id, Object::Dictionary(page));
    }
    Ok(())
}

fn append_content_stream(
    document: &mut Document,
    existing: Option<Object>,
    content_id: ObjectId,
) -> Result<Object, String> {
    let new_content = Object::Reference(content_id);
    match existing {
        None | Some(Object::Null) => Ok(new_content),
        Some(Object::Reference(id)) => Ok(Object::Array(vec![Object::Reference(id), new_content])),
        Some(Object::Array(mut contents)) => {
            contents.push(new_content);
            Ok(Object::Array(contents))
        }
        Some(Object::Stream(stream)) => {
            let original_id = document.add_object(stream);
            Ok(Object::Array(vec![
                Object::Reference(original_id),
                new_content,
            ]))
        }
        Some(_) => Err("A form page has an invalid content stream.".to_string()),
    }
}

fn prune_flattened_fields(
    document: &mut Document,
    acroform: &mut AcroFormLocation,
    flattened: &HashSet<ObjectId>,
) -> Result<(), String> {
    let root_ids = reference_array(
        document,
        acroform.dictionary().get(b"Fields"),
        "form field list",
    )?;
    let mut visited = HashSet::new();
    let retained = root_ids
        .into_iter()
        .filter_map(
            |id| match retain_form_node(document, id, flattened, &mut visited, 0) {
                Ok(true) => Some(Ok(Object::Reference(id))),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .collect::<Result<Vec<_>, String>>()?;
    acroform
        .dictionary_mut()
        .set("Fields", Object::Array(retained));
    if acroform
        .dictionary()
        .get(b"Fields")
        .and_then(Object::as_array)
        .is_ok_and(Vec::is_empty)
    {
        document
            .catalog_mut()
            .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?
            .remove(b"AcroForm");
    } else {
        write_acroform(document, acroform)?;
    }
    Ok(())
}

fn retain_form_node(
    document: &mut Document,
    field_id: ObjectId,
    flattened: &HashSet<ObjectId>,
    visited: &mut HashSet<ObjectId>,
    depth: usize,
) -> Result<bool, String> {
    if flattened.contains(&field_id) {
        return Ok(false);
    }
    if depth >= MAX_FIELD_DEPTH || !visited.insert(field_id) {
        return Err("The AcroForm field tree could not be pruned safely.".to_string());
    }
    let mut dictionary = document
        .get_dictionary(field_id)
        .map_err(|error| format!("An AcroForm field is invalid: {error}"))?
        .clone();
    let kids = reference_array(document, dictionary.get(b"Kids"), "field child list")?;
    if !kids.is_empty() {
        let mut retained = Vec::new();
        for kid_id in kids {
            if flattened.contains(&kid_id) {
                continue;
            }
            let kid = document
                .get_dictionary(kid_id)
                .map_err(|error| format!("An AcroForm child is invalid: {error}"))?;
            if is_widget_dictionary(kid)
                || retain_form_node(document, kid_id, flattened, visited, depth + 1)?
            {
                retained.push(Object::Reference(kid_id));
            }
        }
        if retained.is_empty()
            && dictionary.get(b"FT").is_err()
            && !is_widget_dictionary(&dictionary)
        {
            return Ok(false);
        }
        dictionary.set("Kids", Object::Array(retained));
        document
            .objects
            .insert(field_id, Object::Dictionary(dictionary));
    }
    Ok(true)
}

fn write_acroform(document: &mut Document, acroform: &AcroFormLocation) -> Result<(), String> {
    match acroform {
        AcroFormLocation::Direct(dictionary) => {
            document
                .catalog_mut()
                .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?
                .set("AcroForm", dictionary.clone());
        }
        AcroFormLocation::Reference(id, dictionary) => {
            document
                .objects
                .insert(*id, Object::Dictionary(dictionary.clone()));
            document
                .catalog_mut()
                .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?
                .set("AcroForm", *id);
        }
    }
    Ok(())
}

fn verify_updated_fields(
    document: &Document,
    parsed: &ParsedForms,
    updates: &[ValidatedUpdate],
) -> Result<(), String> {
    for update in updates {
        let field = parsed
            .fields
            .iter()
            .find(|field| field.object_id == update.field_id)
            .ok_or_else(|| "An updated form field disappeared during verification.".to_string())?;
        if field.values != update.values {
            return Err(format!(
                "The form field “{}” changed during verification ({} entries and {} characters reopened instead of {} entries and {} characters) and the PDF was not saved.",
                field.name,
                field.values.len(),
                field.values.iter().map(|value| value.chars().count()).sum::<usize>(),
                update.values.len(),
                update.values.iter().map(|value| value.chars().count()).sum::<usize>()
            ));
        }
        for widget in field
            .widgets
            .iter()
            .filter(|widget| widget.page_id.is_some() && widget.rect.is_some())
        {
            let dictionary = document
                .get_dictionary(widget.object_id)
                .map_err(|error| format!("An updated form widget is invalid: {error}"))?;
            let appearance = dictionary
                .get(b"AP")
                .map_err(|_| "An updated form widget lost its appearance.".to_string())?;
            let appearance = resolved_dictionary(document, appearance)?;
            if !appearance.has(b"N") {
                return Err("An updated form widget lost its normal appearance.".to_string());
            }
        }
    }
    Ok(())
}

fn verify_flattened_fields(
    document: &Document,
    parsed: &ParsedForms,
    flattened: &HashSet<ObjectId>,
    generated_widgets: &[GeneratedWidget],
) -> Result<(), String> {
    if parsed
        .fields
        .iter()
        .any(|field| flattened.contains(&field.object_id))
    {
        return Err(
            "A flattened form field remained interactive and the PDF was not saved.".to_string(),
        );
    }
    let flattened_widgets = generated_widgets
        .iter()
        .filter(|widget| flattened.contains(&widget.field_id))
        .collect::<Vec<_>>();
    let mut page_annotations = HashSet::new();
    for page_id in document.get_pages().values() {
        let page = document
            .get_dictionary(*page_id)
            .map_err(|error| format!("A verified form page is invalid: {error}"))?;
        for annotation in object_array(document, page.get(b"Annots"), "page annotation list")? {
            if let Ok(annotation_id) = annotation.as_reference() {
                page_annotations.insert(annotation_id);
            }
        }
    }
    if flattened_widgets
        .iter()
        .any(|widget| page_annotations.contains(&widget.widget_id))
    {
        return Err(
            "A flattened form widget remained on a page and the PDF was not saved.".to_string(),
        );
    }
    let marker_count = document
        .objects
        .values()
        .filter(|object| {
            object
                .as_stream()
                .is_ok_and(|stream| stream.dict.has(b"TufekciFlattenedForm"))
        })
        .count();
    let expected_pages = flattened_widgets
        .iter()
        .map(|widget| widget.page_id)
        .collect::<HashSet<_>>()
        .len();
    if marker_count < expected_pages {
        return Err("Flattened form page content could not be verified.".to_string());
    }
    Ok(())
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

fn form_text_string(value: &str) -> Object {
    let mut bytes = vec![0xfe, 0xff];
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
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
    use lopdf::text_string;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn inspects_hierarchical_typed_fields_and_rotated_widget_geometry() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();

        let inspection = inspect_pdf_forms(InspectPdfFormsRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(inspection.page_count, 2);
        assert_eq!(inspection.field_count, 6);
        assert_eq!(inspection.editable_field_count, 5);
        assert_eq!(inspection.flattenable_field_count, 5);
        assert!(!inspection.has_xfa);
        let name = inspection
            .fields
            .iter()
            .find(|field| field.name == "identity.name")
            .unwrap();
        assert_eq!(name.kind, FormFieldKind::Text);
        assert_eq!(name.values, vec!["Old name"]);
        assert!(name.required);
        let choice = field(&inspection, "country");
        assert_eq!(choice.kind, FormFieldKind::Choice);
        assert_eq!(
            choice.options,
            vec![
                PdfFormOption {
                    label: "United Kingdom".to_string(),
                    value: "GB".to_string(),
                },
                PdfFormOption {
                    label: "Türkiye".to_string(),
                    value: "TR".to_string(),
                }
            ]
        );
        let rotated = choice.widgets[0].rect.unwrap();
        assert!((0.0..=1.0).contains(&rotated.x));
        assert!((0.0..=1.0).contains(&rotated.y));
        assert!(rotated.width > 0.0 && rotated.height > 0.0);
        let radio = field(&inspection, "delivery");
        assert_eq!(radio.widgets.len(), 2);
        assert_eq!(
            radio
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["Email", "Post"]
        );
    }

    #[test]
    fn controlled_form_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_progress = Arc::clone(&observed);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            observed_for_progress.lock().unwrap().push((value, stage));
        });

        let report = inspect_pdf_forms_with_control(
            InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap();

        assert_eq!(report.field_count, 6);
        let observed = observed.lock().unwrap();
        assert!(observed
            .iter()
            .any(|(_, stage)| stage == "Inspecting form field node 2"));
        assert_eq!(observed.last().map(|(value, _)| *value), Some(99));
        drop(observed);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let cancel_progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Inspecting form field node 2" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = inspect_pdf_forms_with_control(
            InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(cancelled, cancel_progress),
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn form_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking form source"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during form review\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });

        let error = inspect_pdf_forms_with_control(
            InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
    }

    #[test]
    fn fills_fields_generates_appearances_and_preserves_direct_annotations() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        let output = directory.path.join("completed.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(input.clone());

        let result = export_pdf_forms(ExportPdfFormsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            flatten: false,
            updates: completed_updates(&inspection),
        })
        .unwrap();

        assert_eq!(result.updated_field_count, 5);
        assert_eq!(result.flattened_field_count, 0);
        assert_eq!(result.remaining_field_count, 6);
        assert!(result.bytes_written > 0);
        let reopened = Document::load(&output).unwrap();
        let parsed = parse_forms(&reopened).unwrap();
        assert_eq!(parsed.fields.len(), 6);
        assert_eq!(
            parsed_field(&parsed, "identity.name").values,
            vec!["Tüfekci"]
        );
        assert_eq!(
            parsed_field(&parsed, "notes").values,
            vec!["First line\nSecond line"]
        );
        assert_eq!(parsed_field(&parsed, "accepted").values, vec!["Yes"]);
        assert_eq!(parsed_field(&parsed, "delivery").values, vec!["Post"]);
        assert_eq!(parsed_field(&parsed, "country").values, vec!["TR"]);
        assert!(parsed
            .fields
            .iter()
            .filter(|field| field_is_editable(field))
            .flat_map(|field| &field.widgets)
            .all(|widget| reopened
                .get_dictionary(widget.object_id)
                .is_ok_and(|dictionary| dictionary.has(b"AP"))));
        assert!(page_has_direct_annotation(&reopened, 1));
        let acroform = acroform_location(&reopened).unwrap().unwrap();
        assert!(acroform
            .dictionary()
            .get(b"NeedAppearances")
            .is_ok_and(|value| matches!(value, Object::Boolean(false))));
    }

    #[test]
    fn flattens_supported_fields_but_keeps_push_buttons_and_unrelated_annotations() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        let output = directory.path.join("flattened.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(input.clone());

        let result = export_pdf_forms(ExportPdfFormsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            flatten: true,
            updates: completed_updates(&inspection),
        })
        .unwrap();

        assert_eq!(result.updated_field_count, 5);
        assert_eq!(result.flattened_field_count, 5);
        assert_eq!(result.remaining_field_count, 1);
        let reopened = Document::load(&output).unwrap();
        let parsed = parse_forms(&reopened).unwrap();
        assert_eq!(parsed.fields.len(), 1);
        assert_eq!(parsed.fields[0].kind, FormFieldKind::Button);
        assert!(page_has_direct_annotation(&reopened, 1));
        assert!(reopened.objects.values().any(|object| object
            .as_stream()
            .is_ok_and(|stream| stream.dict.has(b"TufekciFlattenedForm"))));
    }

    #[test]
    fn rejects_invalid_values_and_xfa_forms() {
        let document = fixture(false);
        let parsed = parse_forms(&document).unwrap();
        let required = parsed_field(&parsed, "identity.name");
        assert!(validate_field_update(required, &[String::new()])
            .unwrap_err()
            .contains("required"));
        let choice = parsed_field(&parsed, "country");
        assert!(validate_field_update(choice, &["Unknown".to_string()])
            .unwrap_err()
            .contains("unknown option"));

        let directory = TestDirectory::new();
        let input = directory.path.join("xfa.pdf");
        let output = directory.path.join("completed.pdf");
        let mut xfa = fixture(false);
        let form_id = xfa
            .catalog()
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_reference()
            .unwrap();
        xfa.get_dictionary_mut(form_id).unwrap().set(
            "XFA",
            Object::String(b"dynamic".to_vec(), StringFormat::Literal),
        );
        xfa.save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(input.clone());
        assert!(inspection.has_xfa);
        let error = export_pdf_forms(ExportPdfFormsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            flatten: false,
            updates: Vec::new(),
        })
        .unwrap_err();
        assert!(error.contains("XFA"));
        assert!(!output.exists());
    }

    #[test]
    fn requires_acknowledgement_before_rewriting_a_signed_form() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed-form.pdf");
        let output = directory.path.join("completed.pdf");
        fixture(true).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(input.clone());
        assert!(inspection.certificate_signature);

        let error = export_pdf_forms(ExportPdfFormsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            flatten: false,
            updates: vec![update(&inspection, "identity.name", &["Changed"])],
        })
        .unwrap_err();

        assert!(error.contains("certificate signature"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_form_source_that_changed_after_review() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        let output = directory.path.join("completed.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(input.clone());
        let mut bytes = fs::read(&input).unwrap();
        bytes.extend_from_slice(b"\n% changed after review\n");
        fs::write(&input, bytes).unwrap();

        let error = export_pdf_forms(ExportPdfFormsRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            flatten: false,
            updates: vec![update(&inspection, "identity.name", &["Changed"])],
        })
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_form_source_changed_during_export_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form.pdf");
        let output = directory.path.join("completed.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect(input.clone());
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking source PDF before publication"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during form export\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });
        let control = PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress);

        let error = export_pdf_forms_with_control(
            ExportPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                output_protection: None,
                acknowledge_certificate_signatures: false,
                expected_source_size: inspection.source_size,
                expected_source_modified_at_ms: inspection.source_modified_at_ms,
                flatten: false,
                updates: vec![update(&inspection, "identity.name", &["Changed"])],
            },
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    fn inspect(path: PathBuf) -> PdfFormInspection {
        inspect_pdf_forms(InspectPdfFormsRequest {
            input_path: path.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap()
    }

    fn field<'a>(inspection: &'a PdfFormInspection, name: &str) -> &'a PdfFormFieldInspection {
        inspection
            .fields
            .iter()
            .find(|field| field.name == name)
            .unwrap()
    }

    fn parsed_field<'a>(parsed: &'a ParsedForms, name: &str) -> &'a ParsedField {
        parsed
            .fields
            .iter()
            .find(|field| field.name == name)
            .unwrap()
    }

    fn update(inspection: &PdfFormInspection, name: &str, values: &[&str]) -> PdfFormFieldUpdate {
        PdfFormFieldUpdate {
            field_id: field(inspection, name).field_id.clone(),
            values: values.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    fn completed_updates(inspection: &PdfFormInspection) -> Vec<PdfFormFieldUpdate> {
        vec![
            update(inspection, "identity.name", &["Tüfekci"]),
            update(inspection, "notes", &["First line\nSecond line"]),
            update(inspection, "accepted", &["Yes"]),
            update(inspection, "delivery", &["Post"]),
            update(inspection, "country", &["TR"]),
        ]
    }

    fn page_has_direct_annotation(document: &Document, page_number: u32) -> bool {
        let page_id = document.get_pages()[&page_number];
        let page = document.get_dictionary(page_id).unwrap();
        object_array(document, page.get(b"Annots"), "page annotations")
            .unwrap()
            .iter()
            .any(|annotation| matches!(annotation, Object::Dictionary(_)))
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
            page_ids.push(document.add_object(page));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => 2,
                "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            }),
        );

        let identity_id = document.new_object_id();
        let name_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Parent" => identity_id,
            "FT" => "Tx",
            "Ff" => FLAG_REQUIRED,
            "T" => text_string("name"),
            "V" => text_string("Old name"),
            "Rect" => vec![72.into(), 680.into(), 300.into(), 712.into()],
            "P" => page_ids[0],
        });
        document.objects.insert(
            identity_id,
            Object::Dictionary(dictionary! {
                "T" => text_string("identity"),
                "Kids" => vec![Object::Reference(name_id)],
            }),
        );
        let notes_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "Ff" => FLAG_MULTILINE,
            "MaxLen" => 80,
            "T" => text_string("notes"),
            "V" => text_string("Line one"),
            "Rect" => vec![72.into(), 520.into(), 360.into(), 640.into()],
            "P" => page_ids[0],
        });
        let accepted_appearance = state_appearance(&mut document, "Yes", false);
        let accepted_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Btn",
            "T" => text_string("accepted"),
            "V" => "Off",
            "AS" => "Off",
            "Rect" => vec![72.into(), 470.into(), 92.into(), 490.into()],
            "P" => page_ids[0],
            "AP" => accepted_appearance,
        });

        let delivery_id = document.new_object_id();
        let email_appearance = state_appearance(&mut document, "Email", true);
        let email_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Parent" => delivery_id,
            "Rect" => vec![72.into(), 420.into(), 92.into(), 440.into()],
            "P" => page_ids[0],
            "AP" => email_appearance,
            "AS" => "Email",
        });
        let post_appearance = state_appearance(&mut document, "Post", true);
        let post_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Parent" => delivery_id,
            "Rect" => vec![100.into(), 100.into(), 122.into(), 122.into()],
            "P" => page_ids[1],
            "AP" => post_appearance,
            "AS" => "Off",
        });
        document.objects.insert(
            delivery_id,
            Object::Dictionary(dictionary! {
                "FT" => "Btn",
                "Ff" => FLAG_RADIO,
                "T" => text_string("delivery"),
                "V" => "Email",
                "Kids" => vec![Object::Reference(email_id), Object::Reference(post_id)],
            }),
        );

        let country_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Ch",
            "Ff" => FLAG_COMBO,
            "T" => text_string("country"),
            "V" => text_string("GB"),
            "Opt" => vec![
                Object::Array(vec![text_string("GB"), text_string("United Kingdom")]),
                Object::Array(vec![text_string("TR"), text_string("Türkiye")]),
            ],
            "Rect" => vec![160.into(), 100.into(), 360.into(), 132.into()],
            "P" => page_ids[1],
        });
        let submit_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Btn",
            "Ff" => FLAG_PUSHBUTTON,
            "T" => text_string("submit"),
            "Rect" => vec![160.into(), 160.into(), 260.into(), 194.into()],
            "P" => page_ids[1],
        });

        let mut fields = vec![
            Object::Reference(identity_id),
            Object::Reference(notes_id),
            Object::Reference(accepted_id),
            Object::Reference(delivery_id),
            Object::Reference(country_id),
            Object::Reference(submit_id),
        ];
        let page_one_annotations = vec![
            Object::Dictionary(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Text",
                "Rect" => vec![20.into(), 20.into(), 40.into(), 40.into()],
                "Contents" => text_string("Unrelated direct annotation"),
            }),
            Object::Reference(name_id),
            Object::Reference(notes_id),
            Object::Reference(accepted_id),
            Object::Reference(email_id),
        ];
        let mut page_two_annotations = vec![
            Object::Reference(post_id),
            Object::Reference(country_id),
            Object::Reference(submit_id),
        ];

        if signed {
            let signature_id = document.add_object(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Widget",
                "FT" => "Sig",
                "T" => text_string("certificate"),
                "Rect" => vec![300.into(), 160.into(), 480.into(), 210.into()],
                "P" => page_ids[1],
                "V" => dictionary! {
                    "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                    "Contents" => Object::String(vec![1, 2, 3], StringFormat::Hexadecimal),
                },
            });
            fields.push(Object::Reference(signature_id));
            page_two_annotations.push(Object::Reference(signature_id));
        }

        let mut page_one = document.get_dictionary(page_ids[0]).unwrap().clone();
        page_one.set("Annots", Object::Array(page_one_annotations));
        document
            .objects
            .insert(page_ids[0], Object::Dictionary(page_one));
        let mut page_two = document.get_dictionary(page_ids[1]).unwrap().clone();
        page_two.set("Annots", Object::Array(page_two_annotations));
        document
            .objects
            .insert(page_ids[1], Object::Dictionary(page_two));

        let acroform_id = document.add_object(dictionary! {
            "Fields" => fields,
            "NeedAppearances" => true,
        });
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    fn state_appearance(document: &mut Document, on_name: &str, radio: bool) -> Dictionary {
        let rect = PdfRect {
            left: 0.0,
            bottom: 0.0,
            right: 20.0,
            top: 20.0,
        };
        let off_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
            },
            Vec::new(),
        ));
        let on_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
                "Radio" => radio,
            },
            Content {
                operations: widget_base_operations(rect.width(), rect.height(), radio),
            }
            .encode()
            .unwrap(),
        ));
        let mut states = Dictionary::new();
        states.set("Off", off_id);
        states.set(on_name.as_bytes().to_vec(), on_id);
        dictionary! { "N" => states }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path =
                crate::test_support::create_unique_test_directory("tufekci-paperworks-form-test");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
