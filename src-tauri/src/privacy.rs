use crate::file_safety::{TemporaryOutput, ValidatedPdfPaths};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

const MAX_PRIVACY_PAGES: usize = 20_000;
const MAX_PRIVACY_OBJECTS: usize = 1_000_000;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PASSWORD_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrivacyCleanOptions {
    pub(crate) remove_metadata: bool,
    pub(crate) remove_active_content: bool,
    pub(crate) remove_attachments: bool,
    pub(crate) remove_annotations_and_forms: bool,
    pub(crate) remove_thumbnails: bool,
}

impl PrivacyCleanOptions {
    fn any(self) -> bool {
        self.remove_metadata
            || self.remove_active_content
            || self.remove_attachments
            || self.remove_annotations_and_forms
            || self.remove_thumbnails
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPdfPrivacyRequest {
    pub(crate) expected_source_modified_at_ms: Option<u64>,
    pub(crate) expected_source_size: u64,
    pub(crate) input_path: String,
    pub(crate) output_path: String,
    pub(crate) input_password: Option<String>,
    pub(crate) options: PrivacyCleanOptions,
    pub(crate) acknowledge_certificate_signatures: bool,
    #[serde(default)]
    pub(crate) output_protection: Option<PdfOutputProtection>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPdfPrivacyResult {
    pub(crate) output_path: String,
    pub(crate) bytes_written: u64,
    pub(crate) page_count: usize,
    pub(crate) metadata_structures_removed: usize,
    pub(crate) active_content_structures_removed: usize,
    pub(crate) attachment_structures_removed: usize,
    pub(crate) annotation_structures_removed: usize,
    pub(crate) thumbnail_structures_removed: usize,
    pub(crate) web_capture_structures_removed: usize,
    pub(crate) unreachable_objects_pruned: usize,
    pub(crate) encryption: &'static str,
    pub(crate) warnings: Vec<String>,
}

#[derive(Default)]
struct RemovalCounts {
    metadata: usize,
    active_content: usize,
    attachments: usize,
    annotations: usize,
    thumbnails: usize,
    web_capture: usize,
    pruned: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RedactionSanitiseCounts {
    pub(crate) removed_structures: usize,
    pub(crate) pruned_objects: usize,
}

const REDACTION_CATALOGUE_KEYS: &[&[u8]] = &[
    b"AcroForm",
    b"AA",
    b"Collection",
    b"Dests",
    b"Lang",
    b"Legal",
    b"Metadata",
    b"Names",
    b"OCProperties",
    b"OpenAction",
    b"Outlines",
    b"PageLabels",
    b"Perms",
    b"Requirements",
    b"SpiderInfo",
    b"StructTreeRoot",
    b"Threads",
    b"URI",
];

const REDACTION_STRUCTURE_KEYS: &[&[u8]] = &[b"MarkInfo", b"StructParent", b"StructParents"];

#[derive(Default)]
struct ClassifiedIds {
    metadata: HashSet<ObjectId>,
    active_content: HashSet<ObjectId>,
    attachments: HashSet<ObjectId>,
    annotations: HashSet<ObjectId>,
}

#[derive(Clone, Copy)]
enum RemovalCategory {
    Metadata,
    ActiveContent,
    Attachments,
    Annotations,
}

#[cfg(test)]
pub fn clean_pdf_privacy(request: CleanPdfPrivacyRequest) -> Result<CleanPdfPrivacyResult, String> {
    clean_pdf_privacy_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_clean_pdf_privacy_request(
    request: &CleanPdfPrivacyRequest,
) -> Result<(), String> {
    if !request.options.any() {
        return Err("Select at least one privacy category to remove.".to_string());
    }
    if request
        .input_password
        .as_deref()
        .is_some_and(|password| password.len() > MAX_PASSWORD_BYTES)
    {
        return Err(format!(
            "The PDF password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    Ok(())
}

pub(crate) fn clean_pdf_privacy_with_control(
    request: CleanPdfPrivacyRequest,
    control: &PdfJobExecutionControl,
) -> Result<CleanPdfPrivacyResult, String> {
    control.checkpoint(2, "Validating privacy-clean request")?;
    validate_clean_pdf_privacy_request(&request)?;

    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    control.checkpoint(7, "Checking inspected source fingerprint")?;
    let source_metadata = fs::metadata(&paths.input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    if source_metadata.len() != request.expected_source_size
        || modified_at_ms(&source_metadata) != request.expected_source_modified_at_ms
    {
        return Err(
            "The PDF changed on disk after privacy inspection. Inspect it again before cleaning."
                .to_string(),
        );
    }
    control.checkpoint(12, "Opening source PDF")?;
    let mut document = Document::load_with_options(
        &paths.input,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The source PDF could not be parsed: {error}"))?;
    let was_encrypted = document.is_encrypted() || document.was_encrypted();
    if document.is_encrypted() {
        document
            .decrypt(request.input_password.as_deref().unwrap_or_default())
            .map_err(|_| {
                "The PDF could not be decrypted for cleaning. Check its password.".to_string()
            })?;
    }

    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    if page_count > MAX_PRIVACY_PAGES {
        return Err(format!(
            "Privacy Cleaner supports at most {MAX_PRIVACY_PAGES} pages in one PDF."
        ));
    }
    if document.objects.len() > MAX_PRIVACY_OBJECTS {
        return Err(format!(
            "Privacy Cleaner supports at most {MAX_PRIVACY_OBJECTS} indirect objects in one PDF."
        ));
    }
    control.checkpoint(24, "Checking document rewrite safety")?;
    let had_certificate_signature = document_has_certificate_signature(&document);
    ensure_document_rewrite_acknowledged(
        &document,
        &paths.input,
        request.acknowledge_certificate_signatures,
    )?;

    let counts = clean_document_with_control(&mut document, request.options, control)?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    control.checkpoint(79, "Writing prepared cleaned PDF")?;
    document
        .save(prepared.path())
        .map_err(|error| format!("The cleaned PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| format!("The cleaned PDF could not be flushed to disk: {error}"))?;

    control.checkpoint(84, "Reopening prepared cleaned PDF")?;
    let verification = Document::load_with_options(
        prepared.path(),
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The cleaned PDF failed its reopening check: {error}"))?;
    control.checkpoint(87, "Verifying removed privacy structures")?;
    verify_cleaned_pdf(&verification, page_count, request.options)?;

    let protected = if let Some(protection) = request.output_protection.as_ref() {
        let protected = TemporaryOutput::new(&paths.output)?;
        let protection_control =
            control.subrange(89, 94, "Applying AES-256 output protection".to_string());
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            &protection_control,
        )?;
        control.checkpoint(94, "Opening protected cleaned PDF for verification")?;
        let mut protected_verification = Document::load_with_options(
            protected.path(),
            LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
        )
        .map_err(|error| format!("The protected cleaned PDF could not be reopened: {error}"))?;
        if !protected_verification.is_encrypted() {
            return Err(
                "The protected cleaned PDF did not contain AES-256 encryption and was not saved."
                    .to_string(),
            );
        }
        protected_verification
            .decrypt(&protection.open_password)
            .map_err(|_| {
                "The protected cleaned PDF could not be decrypted for verification.".to_string()
            })?;
        control.checkpoint(96, "Repeating privacy verification after decryption")?;
        verify_cleaned_pdf(&protected_verification, page_count, request.options)?;
        Some(protected)
    } else {
        None
    };
    let final_output = protected.as_ref().unwrap_or(&prepared);

    control.checkpoint(98, "Rechecking inspected source before publication")?;
    verify_source_fingerprint(&paths.input, &request)?;
    control.checkpoint(99, "Publishing verified cleaned PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let mut warnings = Vec::new();
    if request.output_protection.is_some() {
        warnings.push(
            "The cleaned copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if was_encrypted {
        warnings.push(
            "The cleaned copy is not password-protected. Use Protect to apply new encryption."
                .to_string(),
        );
    }
    if had_certificate_signature {
        warnings.push(
            "Cleaning changes the PDF and invalidates any existing certificate signature."
                .to_string(),
        );
    }

    Ok(CleanPdfPrivacyResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        bytes_written,
        page_count,
        metadata_structures_removed: counts.metadata,
        active_content_structures_removed: counts.active_content,
        attachment_structures_removed: counts.attachments,
        annotation_structures_removed: counts.annotations,
        thumbnail_structures_removed: counts.thumbnails,
        web_capture_structures_removed: counts.web_capture,
        unreachable_objects_pruned: counts.pruned,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

pub(crate) fn run_pdf_privacy_job_with_control(
    request: CleanPdfPrivacyRequest,
    control: &PdfJobExecutionControl,
) -> Result<CleanPdfPrivacyResult, String> {
    clean_pdf_privacy_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_privacy_job_error(&error)
        }
    })
}

fn verify_source_fingerprint(input: &Path, request: &CleanPdfPrivacyRequest) -> Result<(), String> {
    let metadata = fs::metadata(input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    if metadata.len() != request.expected_source_size
        || modified_at_ms(&metadata) != request.expected_source_modified_at_ms
    {
        return Err(
            "The PDF changed on disk after privacy inspection. Inspect it again before cleaning."
                .to_string(),
        );
    }
    Ok(())
}

fn verify_cleaned_pdf(
    document: &Document,
    page_count: usize,
    options: PrivacyCleanOptions,
) -> Result<(), String> {
    if document.is_encrypted() {
        return Err(
            "The cleaned PDF unexpectedly remained encrypted during verification.".to_string(),
        );
    }
    if document.get_pages().len() != page_count {
        return Err("The cleaned PDF changed the page count and was not saved.".to_string());
    }
    verify_requested_categories_absent(document, options)
}

fn safe_privacy_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed after inspection. Inspect it again before privacy cleaning."
            .to_string();
    }
    if normalised.contains("certificate signature") {
        return "This PDF contains a certificate signature. Confirm the rewrite warning before privacy cleaning."
            .to_string();
    }
    if normalised.contains("qpdf") || normalised.contains("aes-256") {
        return "AES-256 privacy-clean protection could not complete. Install QPDF or add it to PATH, then try again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The PDF could not be opened or protected with the supplied passwords.".to_string();
    }
    "Privacy cleaning failed a structural safety check. Inspect the PDF and try again.".to_string()
}

fn clean_document(document: &mut Document, options: PrivacyCleanOptions) -> RemovalCounts {
    clean_document_with_control(document, options, &PdfJobExecutionControl::direct())
        .expect("direct privacy cleaning cannot be cancelled")
}

fn clean_document_with_control(
    document: &mut Document,
    options: PrivacyCleanOptions,
    control: &PdfJobExecutionControl,
) -> Result<RemovalCounts, String> {
    let classified = classify_indirect_objects(document, options, control)?;
    let mut counts = RemovalCounts::default();

    if options.remove_metadata {
        if document.trailer.remove(b"Info").is_some() {
            counts.metadata += 1;
        }
        if document.trailer.remove(b"ID").is_some() {
            counts.metadata += 1;
        }
    }

    clean_dictionary(&mut document.trailer, &classified, options, &mut counts);
    let object_total = document.objects.len();
    for (index, object) in document.objects.values_mut().enumerate() {
        if index % 128 == 0 {
            control.checkpoint(
                stage_progress(48, 70, index, object_total),
                format!("Cleaning PDF object {} of {object_total}", index + 1),
            )?;
        }
        clean_object_contents(object, &classified, options, &mut counts);
    }

    control.checkpoint(72, "Removing private object references")?;
    counts.metadata += remove_ids(document, &classified.metadata);
    counts.active_content += remove_ids(document, &classified.active_content);
    counts.attachments += remove_ids(document, &classified.attachments);
    counts.annotations += remove_ids(document, &classified.annotations);
    control.checkpoint(76, "Pruning unreachable PDF objects")?;
    counts.pruned = document.prune_objects().len();
    control.ensure_not_cancelled()?;
    Ok(counts)
}

pub(crate) fn sanitise_document_for_redaction(document: &mut Document) -> RedactionSanitiseCounts {
    let options = PrivacyCleanOptions {
        remove_metadata: true,
        remove_active_content: true,
        remove_attachments: true,
        remove_annotations_and_forms: true,
        remove_thumbnails: true,
    };
    let counts = clean_document(document, options);
    let mut removed_structures = counts.metadata
        + counts.active_content
        + counts.attachments
        + counts.annotations
        + counts.thumbnails;

    if let Ok(catalogue) = document.catalog_mut() {
        for key in REDACTION_CATALOGUE_KEYS {
            removed_structures += usize::from(catalogue.remove(key).is_some());
        }
    }
    for object in document.objects.values_mut() {
        removed_structures += remove_redaction_structure_keys(object, 0);
    }
    let additional_pruned = document.prune_objects().len();

    RedactionSanitiseCounts {
        removed_structures,
        pruned_objects: counts.pruned + additional_pruned,
    }
}

pub(crate) fn verify_redaction_sanitised(document: &Document) -> Result<(), String> {
    let options = PrivacyCleanOptions {
        remove_metadata: true,
        remove_active_content: true,
        remove_attachments: true,
        remove_annotations_and_forms: true,
        remove_thumbnails: true,
    };
    verify_requested_categories_absent(document, options).map_err(|_| {
        "The redacted PDF retained a private or interactive structure and was not saved."
            .to_string()
    })?;

    let catalogue = document
        .catalog()
        .map_err(|error| format!("The redacted PDF catalogue is invalid: {error}"))?;
    if REDACTION_CATALOGUE_KEYS
        .iter()
        .any(|key| catalogue.has(key))
        || document_contains_dictionary(document, &|dictionary| {
            REDACTION_STRUCTURE_KEYS
                .iter()
                .any(|key| dictionary.has(key))
        })
    {
        return Err(
            "The redacted PDF retained navigation or document-structure data and was not saved."
                .to_string(),
        );
    }
    Ok(())
}

fn remove_redaction_structure_keys(object: &mut Object, depth: usize) -> usize {
    if depth > 64 {
        return 0;
    }
    match object {
        Object::Dictionary(dictionary) => {
            let mut removed = 0;
            for key in REDACTION_STRUCTURE_KEYS {
                removed += usize::from(dictionary.remove(key).is_some());
            }
            let keys = dictionary
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            for key in keys {
                if let Ok(value) = dictionary.get_mut(&key) {
                    removed += remove_redaction_structure_keys(value, depth + 1);
                }
            }
            removed
        }
        Object::Stream(stream) => {
            let mut removed = 0;
            for key in REDACTION_STRUCTURE_KEYS {
                removed += usize::from(stream.dict.remove(key).is_some());
            }
            let keys = stream
                .dict
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            for key in keys {
                if let Ok(value) = stream.dict.get_mut(&key) {
                    removed += remove_redaction_structure_keys(value, depth + 1);
                }
            }
            removed
        }
        Object::Array(values) => values
            .iter_mut()
            .map(|value| remove_redaction_structure_keys(value, depth + 1))
            .sum(),
        _ => 0,
    }
}

fn classify_indirect_objects(
    document: &Document,
    options: PrivacyCleanOptions,
    control: &PdfJobExecutionControl,
) -> Result<ClassifiedIds, String> {
    let mut ids = ClassifiedIds::default();
    let object_total = document.objects.len();
    for (index, (id, object)) in document.objects.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint(
                stage_progress(28, 46, index, object_total),
                format!("Classifying PDF object {} of {object_total}", index + 1),
            )?;
        }
        let Some(dictionary) = object_dictionary(object) else {
            continue;
        };
        if options.remove_metadata && is_metadata_dictionary(dictionary) {
            ids.metadata.insert(*id);
        }
        if options.remove_active_content && is_active_content_dictionary(dictionary) {
            ids.active_content.insert(*id);
        }
        if options.remove_attachments && is_attachment_dictionary(dictionary) {
            ids.attachments.insert(*id);
        }
        if options.remove_annotations_and_forms && is_annotation_dictionary(dictionary) {
            ids.annotations.insert(*id);
        }
    }
    control.ensure_not_cancelled()?;
    Ok(ids)
}

fn stage_progress(start: u8, end: u8, index: usize, total: usize) -> u8 {
    if total == 0 || end <= start {
        return end;
    }
    let completed = index.min(total) as f64 / total as f64;
    start.saturating_add(((end - start) as f64 * completed).round() as u8)
}

fn remove_ids(document: &mut Document, ids: &HashSet<ObjectId>) -> usize {
    ids.iter()
        .filter(|id| document.objects.remove(id).is_some())
        .count()
}

fn clean_object_contents(
    object: &mut Object,
    classified: &ClassifiedIds,
    options: PrivacyCleanOptions,
    counts: &mut RemovalCounts,
) {
    match object {
        Object::Dictionary(dictionary) => clean_dictionary(dictionary, classified, options, counts),
        Object::Stream(stream) => clean_dictionary(&mut stream.dict, classified, options, counts),
        Object::Array(values) => {
            let mut index = 0;
            while index < values.len() {
                if let Some(category) = removal_category(&values[index], classified, options) {
                    values.remove(index);
                    increment(counts, category);
                } else {
                    clean_object_contents(&mut values[index], classified, options, counts);
                    index += 1;
                }
            }
        }
        _ => {}
    }
}

fn clean_dictionary(
    dictionary: &mut Dictionary,
    classified: &ClassifiedIds,
    options: PrivacyCleanOptions,
    counts: &mut RemovalCounts,
) {
    if options.remove_metadata {
        remove_dictionary_keys(
            dictionary,
            &[b"Metadata", b"PieceInfo", b"LastModified"],
            counts,
            RemovalCategory::Metadata,
        );
        for key in [
            b"SpiderInfo".as_slice(),
            b"URLS".as_slice(),
            b"IDS".as_slice(),
        ] {
            if dictionary.remove(key).is_some() {
                counts.metadata += 1;
                counts.web_capture += 1;
            }
        }
    }
    if options.remove_active_content {
        remove_dictionary_keys(
            dictionary,
            &[b"AA", b"OpenAction", b"JS", b"JavaScript"],
            counts,
            RemovalCategory::ActiveContent,
        );
    }
    if options.remove_attachments {
        remove_dictionary_keys(
            dictionary,
            &[b"EmbeddedFiles", b"EF", b"AF", b"AFRelationship"],
            counts,
            RemovalCategory::Attachments,
        );
    }
    if options.remove_annotations_and_forms {
        remove_dictionary_keys(
            dictionary,
            &[b"Annots", b"AcroForm"],
            counts,
            RemovalCategory::Annotations,
        );
    }
    if options.remove_thumbnails {
        let removed = dictionary.remove(b"Thumb").is_some();
        if removed {
            counts.thumbnails += 1;
        }
    }

    let keys = dictionary
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in keys {
        let category = dictionary
            .get(&key)
            .ok()
            .and_then(|value| removal_category(value, classified, options));
        if let Some(category) = category {
            dictionary.remove(&key);
            increment(counts, category);
            continue;
        }
        if let Ok(value) = dictionary.get_mut(&key) {
            clean_object_contents(value, classified, options, counts);
        }
    }
}

fn remove_dictionary_keys(
    dictionary: &mut Dictionary,
    keys: &[&[u8]],
    counts: &mut RemovalCounts,
    category: RemovalCategory,
) {
    for key in keys {
        if dictionary.remove(key).is_some() {
            increment(counts, category);
        }
    }
}

fn removal_category(
    object: &Object,
    classified: &ClassifiedIds,
    options: PrivacyCleanOptions,
) -> Option<RemovalCategory> {
    if let Object::Reference(id) = object {
        if classified.metadata.contains(id) {
            return Some(RemovalCategory::Metadata);
        }
        if classified.active_content.contains(id) {
            return Some(RemovalCategory::ActiveContent);
        }
        if classified.attachments.contains(id) {
            return Some(RemovalCategory::Attachments);
        }
        if classified.annotations.contains(id) {
            return Some(RemovalCategory::Annotations);
        }
        return None;
    }

    let dictionary = object_dictionary(object)?;
    if options.remove_metadata && is_metadata_dictionary(dictionary) {
        return Some(RemovalCategory::Metadata);
    }
    if options.remove_active_content && is_active_content_dictionary(dictionary) {
        return Some(RemovalCategory::ActiveContent);
    }
    if options.remove_attachments && is_attachment_dictionary(dictionary) {
        return Some(RemovalCategory::Attachments);
    }
    if options.remove_annotations_and_forms && is_annotation_dictionary(dictionary) {
        return Some(RemovalCategory::Annotations);
    }
    None
}

fn increment(counts: &mut RemovalCounts, category: RemovalCategory) {
    match category {
        RemovalCategory::Metadata => counts.metadata += 1,
        RemovalCategory::ActiveContent => counts.active_content += 1,
        RemovalCategory::Attachments => counts.attachments += 1,
        RemovalCategory::Annotations => counts.annotations += 1,
    }
}

fn object_dictionary(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn is_metadata_dictionary(dictionary: &Dictionary) -> bool {
    has_name(dictionary, b"Type", b"Metadata")
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
            .is_ok_and(is_annotation_subtype)
}

fn is_annotation_subtype(name: &[u8]) -> bool {
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
}

fn has_name(dictionary: &Dictionary, key: &[u8], expected: &[u8]) -> bool {
    dictionary
        .get(key)
        .and_then(Object::as_name)
        .is_ok_and(|name| name == expected)
}

fn verify_requested_categories_absent(
    document: &Document,
    options: PrivacyCleanOptions,
) -> Result<(), String> {
    let mut residue = Vec::new();
    if options.remove_metadata
        && (document.trailer.has(b"Info")
            || document.trailer.has(b"ID")
            || document_contains_dictionary(document, &|dictionary| {
                dictionary.has(b"Metadata")
                    || dictionary.has(b"PieceInfo")
                    || dictionary.has(b"LastModified")
                    || dictionary.has(b"SpiderInfo")
                    || dictionary.has(b"URLS")
                    || dictionary.has(b"IDS")
                    || is_metadata_dictionary(dictionary)
            }))
    {
        residue.push("metadata");
    }
    if options.remove_active_content
        && document_contains_dictionary(document, &|dictionary| {
            dictionary.has(b"AA")
                || dictionary.has(b"OpenAction")
                || dictionary.has(b"JavaScript")
                || is_active_content_dictionary(dictionary)
        })
    {
        residue.push("JavaScript or launch actions");
    }
    if options.remove_attachments
        && document_contains_dictionary(document, &|dictionary| {
            dictionary.has(b"EmbeddedFiles")
                || dictionary.has(b"AF")
                || dictionary.has(b"AFRelationship")
                || is_attachment_dictionary(dictionary)
        })
    {
        residue.push("attachments");
    }
    if options.remove_annotations_and_forms
        && document_contains_dictionary(document, &|dictionary| {
            dictionary.has(b"Annots")
                || dictionary.has(b"AcroForm")
                || is_annotation_dictionary(dictionary)
        })
    {
        residue.push("annotations or form fields");
    }
    if options.remove_thumbnails
        && document_contains_dictionary(document, &|dictionary| dictionary.has(b"Thumb"))
    {
        residue.push("page thumbnails");
    }

    if residue.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "The cleaned PDF still contains {} and was not saved.",
            residue.join(", ")
        ))
    }
}

fn document_contains_dictionary(
    document: &Document,
    predicate: &impl Fn(&Dictionary) -> bool,
) -> bool {
    object_contains_dictionary(&Object::Dictionary(document.trailer.clone()), predicate, 0)
        || document
            .objects
            .values()
            .any(|object| object_contains_dictionary(object, predicate, 0))
}

fn object_contains_dictionary(
    object: &Object,
    predicate: &impl Fn(&Dictionary) -> bool,
    depth: usize,
) -> bool {
    if depth > 64 {
        return false;
    }
    match object {
        Object::Dictionary(dictionary) => {
            predicate(dictionary)
                || dictionary
                    .iter()
                    .any(|(_, value)| object_contains_dictionary(value, predicate, depth + 1))
        }
        Object::Stream(stream) => {
            predicate(&stream.dict)
                || stream
                    .dict
                    .iter()
                    .any(|(_, value)| object_contains_dictionary(value, predicate, depth + 1))
        }
        Object::Array(values) => values
            .iter()
            .any(|value| object_contains_dictionary(value, predicate, depth + 1)),
        _ => false,
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
    use crate::job_control::PDF_JOB_CANCELLED_ERROR;
    use lopdf::{dictionary, Stream};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn removes_every_selected_privacy_category_and_preserves_pages() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private.pdf");
        let output = directory.path.join("clean.pdf");
        privacy_fixture().save(&input).unwrap().sync_all().unwrap();
        let source_metadata = fs::metadata(&input).unwrap();

        let result = clean_pdf_privacy(CleanPdfPrivacyRequest {
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: modified_at_ms(&source_metadata),
            expected_source_size: source_metadata.len(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            options: all_options(),
            output_protection: None,
        })
        .unwrap();

        assert_eq!(result.page_count, 1);
        assert_eq!(result.encryption, "None");
        assert!(result.metadata_structures_removed > 0);
        assert!(result.active_content_structures_removed > 0);
        assert!(result.attachment_structures_removed > 0);
        assert!(result.annotation_structures_removed > 0);
        assert!(result.thumbnail_structures_removed > 0);
        assert!(result.web_capture_structures_removed > 0);
        let clean = Document::load(&output).unwrap();
        verify_requested_categories_absent(&clean, all_options()).unwrap();
        assert_eq!(clean.get_pages().len(), 1);

        let original = Document::load(&input).unwrap();
        assert!(verify_requested_categories_absent(&original, all_options()).is_err());
    }

    #[test]
    fn preserves_categories_that_were_not_selected() {
        let mut document = privacy_fixture();
        let options = PrivacyCleanOptions {
            remove_metadata: true,
            remove_active_content: false,
            remove_attachments: false,
            remove_annotations_and_forms: false,
            remove_thumbnails: false,
        };

        clean_document(&mut document, options);

        verify_requested_categories_absent(&document, options).unwrap();
        assert!(document_contains_dictionary(&document, &|dictionary| {
            is_active_content_dictionary(dictionary)
        }));
        assert!(document_contains_dictionary(&document, &|dictionary| {
            is_attachment_dictionary(dictionary)
        }));
        assert!(document_contains_dictionary(&document, &|dictionary| {
            dictionary.has(b"Annots")
        }));
    }

    #[test]
    fn removes_javascript_name_tree_without_removing_attachments() {
        let mut document = privacy_fixture();
        let options = PrivacyCleanOptions {
            remove_metadata: false,
            remove_active_content: true,
            remove_attachments: false,
            remove_annotations_and_forms: false,
            remove_thumbnails: false,
        };

        clean_document(&mut document, options);

        verify_requested_categories_absent(&document, options).unwrap();
        assert!(document_contains_dictionary(&document, &|dictionary| {
            dictionary.has(b"EmbeddedFiles") || is_attachment_dictionary(dictionary)
        }));
        assert!(document_contains_dictionary(&document, &|dictionary| {
            dictionary.has(b"Dests")
        }));
    }

    #[test]
    fn refuses_an_empty_cleaning_request() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private.pdf");
        let output = directory.path.join("clean.pdf");
        privacy_fixture().save(&input).unwrap().sync_all().unwrap();
        let source_metadata = fs::metadata(&input).unwrap();

        let error = clean_pdf_privacy(CleanPdfPrivacyRequest {
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: modified_at_ms(&source_metadata),
            expected_source_size: source_metadata.len(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            options: PrivacyCleanOptions {
                remove_metadata: false,
                remove_active_content: false,
                remove_attachments: false,
                remove_annotations_and_forms: false,
                remove_thumbnails: false,
            },
            output_protection: None,
        })
        .unwrap_err();

        assert!(error.contains("at least one"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_source_that_changed_after_privacy_inspection() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private.pdf");
        let output = directory.path.join("clean.pdf");
        privacy_fixture().save(&input).unwrap().sync_all().unwrap();
        let source_metadata = fs::metadata(&input).unwrap();

        let error = clean_pdf_privacy(CleanPdfPrivacyRequest {
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: modified_at_ms(&source_metadata),
            expected_source_size: source_metadata.len() + 1,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            options: all_options(),
            output_protection: None,
        })
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_source_changed_during_cleaning_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private.pdf");
        let output = directory.path.join("clean.pdf");
        privacy_fixture().save(&input).unwrap().sync_all().unwrap();
        let source_metadata = fs::metadata(&input).unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking inspected source before publication"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during privacy cleaning\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });
        let control = PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress);
        let request = CleanPdfPrivacyRequest {
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: modified_at_ms(&source_metadata),
            expected_source_size: source_metadata.len(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            options: all_options(),
            output_protection: None,
        };

        let error = clean_pdf_privacy_with_control(request, &control).unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn cancellation_during_object_cleaning_never_publishes_the_destination() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private.pdf");
        let output = directory.path.join("clean.pdf");
        privacy_fixture().save(&input).unwrap().sync_all().unwrap();
        let source_metadata = fs::metadata(&input).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress_cancelled = Arc::clone(&cancelled);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 48 {
                    progress_cancelled.store(true, Ordering::Release);
                }
            }),
        );

        let error = clean_pdf_privacy_with_control(
            CleanPdfPrivacyRequest {
                acknowledge_certificate_signatures: false,
                expected_source_modified_at_ms: modified_at_ms(&source_metadata),
                expected_source_size: source_metadata.len(),
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                options: all_options(),
                output_protection: None,
            },
            &control,
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
    }

    #[test]
    fn requires_acknowledgement_before_cleaning_a_signed_pdf() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed-private.pdf");
        let output = directory.path.join("clean.pdf");
        let mut document = privacy_fixture();
        document.add_object(dictionary! {
            "FT" => "Sig",
            "V" => dictionary! {
                "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
            },
        });
        document.save(&input).unwrap().sync_all().unwrap();

        let request = |acknowledge_certificate_signatures| CleanPdfPrivacyRequest {
            acknowledge_certificate_signatures,
            expected_source_modified_at_ms: modified_at_ms(&fs::metadata(&input).unwrap()),
            expected_source_size: fs::metadata(&input).unwrap().len(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            options: all_options(),
            output_protection: None,
        };
        let error = clean_pdf_privacy(request(false)).unwrap_err();
        assert!(error.contains("certificate signature"));
        assert!(!output.exists());

        let result = clean_pdf_privacy(request(true)).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("certificate signature")));
    }

    fn all_options() -> PrivacyCleanOptions {
        PrivacyCleanOptions {
            remove_metadata: true,
            remove_active_content: true,
            remove_attachments: true,
            remove_annotations_and_forms: true,
            remove_thumbnails: true,
        }
    }

    fn privacy_fixture() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 20 30 Td (Visible page) Tj ET".to_vec(),
        ));
        let thumbnail_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 1,
                "Height" => 1,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![255, 255, 255],
        ));
        let attachment_stream_id = document.add_object(Stream::new(
            dictionary! { "Type" => "EmbeddedFile" },
            b"private attachment".to_vec(),
        ));
        let file_spec_id = document.add_object(dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal("private.txt"),
            "EF" => dictionary! { "F" => attachment_stream_id },
        });
        let annotation_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
            "Contents" => Object::string_literal("private comment"),
        });
        let widget_id = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("private-field"),
            "V" => Object::string_literal("private value"),
            "Rect" => vec![20.into(), 20.into(), 80.into(), 40.into()],
        });
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
            "Thumb" => thumbnail_id,
            "Annots" => vec![annotation_id.into(), widget_id.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let metadata_id = document.add_object(Stream::new(
            dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
            b"<xmpmeta>private history</xmpmeta>".to_vec(),
        ));
        let launch_id = document.add_object(dictionary! {
            "Type" => "Action",
            "S" => "Launch",
            "F" => Object::string_literal("private.exe"),
        });
        let names_id = document.add_object(dictionary! {
            "Dests" => dictionary! {
                "Keep" => vec![page_id.into(), Object::Name(b"Fit".to_vec())],
            },
            "EmbeddedFiles" => dictionary! {
                "Names" => vec![Object::string_literal("private.txt"), file_spec_id.into()],
            },
            "JavaScript" => dictionary! {
                "Names" => vec![
                    Object::string_literal("private-script"),
                    Object::Dictionary(dictionary! {
                        "S" => "JavaScript",
                        "JS" => Object::string_literal("app.alert('private')"),
                    }),
                ],
            },
            "URLS" => dictionary! {
                "Names" => vec![Object::string_literal("https://private.example"), page_id.into()],
            },
            "IDS" => dictionary! {
                "Names" => vec![Object::string_literal("private-source-id"), page_id.into()],
            },
        });
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Metadata" => metadata_id,
            "Names" => names_id,
            "OpenAction" => Object::Dictionary(dictionary! {
                "S" => "JavaScript",
                "JS" => Object::string_literal("app.alert('open')"),
            }),
            "AA" => dictionary! { "WC" => launch_id },
            "AF" => vec![file_spec_id.into()],
            "AcroForm" => dictionary! { "Fields" => vec![widget_id.into()] },
            "PieceInfo" => dictionary! { "PrivateApp" => dictionary! {} },
        });
        let info_id = document.add_object(dictionary! {
            "Author" => Object::string_literal("Private Author"),
            "Creator" => Object::string_literal("Private Application"),
        });
        document.trailer.set("Root", catalog_id);
        document.trailer.set("Info", info_id);
        document.trailer.set(
            "ID",
            vec![
                Object::string_literal("private-id-one"),
                Object::string_literal("private-id-two"),
            ],
        );
        document
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-privacy-test",
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
