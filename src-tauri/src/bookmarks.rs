mod printed_contents;

use crate::file_safety::{
    canonical_pdf_input, reject_control_characters, TemporaryOutput, ValidatedPdfPaths,
};
use crate::health::{document_has_certificate_signature, ensure_document_rewrite_acknowledged};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::protection::{
    lock_pdf_changes_with_control, validate_pdf_output_protection, PdfOutputProtection,
};
use lopdf::{decode_text_string, text_string, Dictionary, Document, LoadOptions, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use printed_contents::{
    add_printed_contents_pages, validate_printed_contents_options, verify_printed_contents,
    PrintedContentsVerification,
};

pub(crate) const MAX_BOOKMARKS: usize = 2_000;
const MAX_BOOKMARK_DEPTH: u8 = 6;
const MAX_TITLE_CHARACTERS: usize = 256;
const MAX_TITLE_BYTES: usize = 1_024;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_NAMED_DESTINATION_NODES: usize = 4_000;
const MAX_DESTINATION_DEPTH: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPdfBookmarksRequest {
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportPdfBookmarksRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    #[serde(default)]
    output_protection: Option<PdfOutputProtection>,
    acknowledge_certificate_signatures: bool,
    expected_source_size: u64,
    expected_source_modified_at_ms: Option<u64>,
    bookmarks: Vec<PdfBookmarkEntry>,
    #[serde(default)]
    printed_contents: Option<PrintedContentsOptions>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PrintedContentsOptions {
    title: String,
    maximum_level: u8,
    add_bookmark: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PdfBookmarkEntry {
    pub(crate) title: String,
    pub(crate) page_number: Option<u32>,
    pub(crate) level: u8,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) open: bool,
    pub(crate) colour: [f32; 3],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfBookmarkInspection {
    file_name: String,
    source_size: u64,
    source_modified_at_ms: Option<u64>,
    page_count: usize,
    bookmark_count: usize,
    unresolved_bookmark_count: usize,
    was_encrypted: bool,
    certificate_signature: bool,
    bookmarks: Vec<PdfBookmarkEntry>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPdfBookmarksResult {
    output_path: String,
    page_count: usize,
    bookmark_count: usize,
    contents_page_count: usize,
    printed_entry_count: usize,
    bytes_written: u64,
    encryption: &'static str,
    warnings: Vec<String>,
}

struct LoadedBookmarksPdf {
    document: Document,
    page_count: usize,
    was_encrypted: bool,
}

pub(crate) struct InspectedBookmarks {
    pub(crate) entries: Vec<PdfBookmarkEntry>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone)]
struct BookmarkNode {
    entry: PdfBookmarkEntry,
    children: Vec<usize>,
}

#[cfg(test)]
pub fn inspect_pdf_bookmarks(
    request: InspectPdfBookmarksRequest,
) -> Result<PdfBookmarkInspection, String> {
    inspect_pdf_bookmarks_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_inspect_pdf_bookmarks_request(
    request: &InspectPdfBookmarksRequest,
) -> Result<(), String> {
    reject_control_characters("Bookmark source path", &request.input_path)?;
    validate_password(request.input_password.as_deref())
}

fn inspect_pdf_bookmarks_with_control(
    request: InspectPdfBookmarksRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfBookmarkInspection, String> {
    control.checkpoint(2, "Validating bookmark review")?;
    validate_inspect_pdf_bookmarks_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;
    let source_size = metadata.len();
    let source_modified_at_ms = modified_at_ms(&metadata);
    control.checkpoint(18, "Opening bookmark structure")?;
    let loaded = load_pdf(&input, request.input_password.as_deref())?;
    control.checkpoint(32, "Inspecting bookmark destinations")?;
    let certificate_signature = document_has_certificate_signature(&loaded.document);
    let inspected = inspect_document_bookmarks_with_control(&loaded.document, control)?;
    let unresolved_bookmark_count = inspected
        .entries
        .iter()
        .filter(|entry| entry.page_number.is_none())
        .count();
    let mut warnings = inspected.warnings;
    if unresolved_bookmark_count > 0 {
        warnings.push(format!(
            "{unresolved_bookmark_count} bookmark{} use an unsupported, missing, or external destination. Assign a page before exporting the edited tree.",
            if unresolved_bookmark_count == 1 { "" } else { "s" }
        ));
    }
    if certificate_signature {
        warnings.push(
            "Editing bookmarks rewrites this certificate-signed PDF and invalidates its existing signatures."
                .to_string(),
        );
    }

    control.checkpoint(94, "Rechecking bookmark source")?;
    verify_source_fingerprint(&input, source_size, source_modified_at_ms)?;
    control.checkpoint(99, "Finalising bookmark review")?;

    Ok(PdfBookmarkInspection {
        file_name: display_name(&input),
        source_size,
        source_modified_at_ms,
        page_count: loaded.page_count,
        bookmark_count: inspected.entries.len(),
        unresolved_bookmark_count,
        was_encrypted: loaded.was_encrypted,
        certificate_signature,
        bookmarks: inspected.entries,
        warnings,
    })
}

pub(crate) fn run_pdf_bookmark_inspection_job_with_control(
    request: InspectPdfBookmarksRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfBookmarkInspection, String> {
    inspect_pdf_bookmarks_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_bookmark_inspection_job_error(&error)
        }
    })
}

fn safe_bookmark_inspection_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "The source PDF changed during bookmark review. Open it again before editing."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The bookmark PDF could not be opened with the supplied password.".to_string();
    }
    "The bookmark review failed a structural safety check. Review the source PDF and try again."
        .to_string()
}

#[cfg(test)]
pub fn export_pdf_bookmarks(
    request: ExportPdfBookmarksRequest,
) -> Result<ExportPdfBookmarksResult, String> {
    export_pdf_bookmarks_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_export_pdf_bookmarks_request(
    request: &ExportPdfBookmarksRequest,
) -> Result<(), String> {
    validate_password(request.input_password.as_deref())?;
    validate_pdf_output_protection(request.output_protection.as_ref())?;
    ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    validate_bookmarks(request.bookmarks.clone(), usize::MAX)?;
    validate_printed_contents_options(request.printed_contents.as_ref(), &request.bookmarks)?;
    Ok(())
}

pub(crate) fn export_pdf_bookmarks_with_control(
    request: ExportPdfBookmarksRequest,
    control: &PdfJobExecutionControl,
) -> Result<ExportPdfBookmarksResult, String> {
    control.checkpoint(1, "Validating bookmark export")?;
    validate_export_pdf_bookmarks_request(&request)?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(8, "Opening and decrypting source PDF")?;
    let mut loaded = load_pdf(&paths.input, request.input_password.as_deref())?;
    control.checkpoint(18, "Checking document rewrite safety")?;
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
    control.checkpoint(28, "Validating bookmark tree")?;
    let bookmarks = validate_bookmarks(request.bookmarks, loaded.page_count)?;
    let printed_contents = request
        .printed_contents
        .as_ref()
        .map(|options| {
            add_printed_contents_pages(
                &mut loaded.document,
                loaded.page_count,
                &bookmarks,
                options,
                control,
            )
        })
        .transpose()?;
    let output_bookmarks = printed_contents
        .as_ref()
        .map(|contents| contents.output_bookmarks.clone())
        .unwrap_or(bookmarks);
    let contents_page_count = printed_contents
        .as_ref()
        .map_or(0, |contents| contents.verification.page_count);
    let printed_entry_count = printed_contents
        .as_ref()
        .map_or(0, |contents| contents.verification.entry_count);
    let final_page_count = loaded.page_count + contents_page_count;
    control.checkpoint(50, "Building bookmark outline")?;
    replace_bookmarks(&mut loaded.document, &output_bookmarks)?;
    loaded.document.prune_objects();
    loaded.document.change_producer("Tüfekci Paperworks");

    control.checkpoint(62, "Writing prepared bookmarked PDF")?;
    let prepared = TemporaryOutput::new(&paths.output)?;
    loaded
        .document
        .save(prepared.path())
        .map_err(|error| format!("The bookmarked PDF could not be written: {error}"))?
        .sync_all()
        .map_err(|error| format!("The bookmarked PDF could not be flushed to storage: {error}"))?;
    control.checkpoint(72, "Verifying prepared bookmark structure")?;
    verify_bookmarked_pdf(
        prepared.path(),
        None,
        false,
        final_page_count,
        had_form_fields,
        &output_bookmarks,
        printed_contents
            .as_ref()
            .map(|contents| &contents.verification),
    )?;

    let protected = if let Some(protection) = request.output_protection.as_ref() {
        control.checkpoint(79, "Applying AES-256 output protection")?;
        let protected = TemporaryOutput::new(&paths.output)?;
        lock_pdf_changes_with_control(
            prepared.path(),
            protected.path(),
            &protection.open_password,
            &protection.owner_password,
            control,
        )?;
        control.checkpoint(89, "Verifying protected bookmark structure")?;
        verify_bookmarked_pdf(
            protected.path(),
            Some(&protection.open_password),
            true,
            final_page_count,
            had_form_fields,
            &output_bookmarks,
            printed_contents
                .as_ref()
                .map(|contents| &contents.verification),
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
    control.checkpoint(99, "Publishing verified bookmarked PDF")?;
    let bytes_written = final_output.persist(&paths.output)?;
    let mut warnings = printed_contents
        .as_ref()
        .map_or_else(Vec::new, |contents| contents.warnings.clone());
    warnings.push(
        "Bookmark destinations in the new copy use whole-page Fit targets. Specialist zoom coordinates and external destinations are not retained."
            .to_string(),
    );
    if output_bookmarks.is_empty() {
        warnings.push("All bookmarks were removed from the new copy.".to_string());
    }
    if request.output_protection.is_some() {
        warnings.push(
            "The bookmarked copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
                .to_string(),
        );
    } else if loaded.was_encrypted {
        warnings.push(
            "The bookmarked copy is not password-protected. Use Protect to apply new encryption."
                .to_string(),
        );
    }
    if had_certificate_signature {
        warnings.push(
            "Bookmark editing changed the PDF and invalidated its existing certificate signatures."
                .to_string(),
        );
    }

    Ok(ExportPdfBookmarksResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        page_count: final_page_count,
        bookmark_count: output_bookmarks.len(),
        contents_page_count,
        printed_entry_count,
        bytes_written,
        encryption: if request.output_protection.is_some() {
            "AES-256"
        } else {
            "None"
        },
        warnings,
    })
}

fn verify_bookmarked_pdf(
    path: &Path,
    password: Option<&str>,
    expected_encrypted: bool,
    expected_page_count: usize,
    expected_form_fields: bool,
    expected_bookmarks: &[PdfBookmarkEntry],
    expected_printed_contents: Option<&PrintedContentsVerification>,
) -> Result<(), String> {
    let mut verification = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|error| format!("The bookmarked PDF failed its reopening check: {error}"))?;
    let encrypted = verification.is_encrypted();
    if encrypted != expected_encrypted {
        return Err(if expected_encrypted {
            "The bookmarked PDF was not encrypted as requested and was not saved.".to_string()
        } else {
            "The bookmarked PDF unexpectedly remained encrypted and was not saved.".to_string()
        });
    }
    if encrypted {
        verification
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "The protected bookmarked PDF could not be reopened with its new password."
                    .to_string()
            })?;
    }
    if verification.get_pages().len() != expected_page_count {
        return Err("The bookmarked PDF changed the page count and was not saved.".to_string());
    }
    if expected_form_fields
        && !verification
            .catalog()
            .is_ok_and(|catalog| catalog.has(b"AcroForm"))
    {
        return Err("The bookmarked PDF lost its form structure and was not saved.".to_string());
    }
    if let Some(expected) = expected_printed_contents {
        verify_printed_contents(&verification, expected)?;
    }
    let verified_bookmarks = inspect_document_bookmarks(&verification)?.entries;
    if verified_bookmarks != expected_bookmarks {
        return Err(
            "The bookmark tree changed during verification and the PDF was not saved.".to_string(),
        );
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
            "The source PDF changed on disk after its bookmarks were reviewed. Review it again before exporting."
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

fn load_pdf(path: &Path, password: Option<&str>) -> Result<LoadedBookmarksPdf, String> {
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
                "The PDF could not be decrypted for bookmark editing. Check its password."
                    .to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    Ok(LoadedBookmarksPdf {
        document,
        page_count,
        was_encrypted,
    })
}

fn validate_bookmarks(
    bookmarks: Vec<PdfBookmarkEntry>,
    page_count: usize,
) -> Result<Vec<PdfBookmarkEntry>, String> {
    if bookmarks.len() > MAX_BOOKMARKS {
        return Err(format!(
            "A bookmark tree can contain at most {MAX_BOOKMARKS} entries."
        ));
    }
    let mut validated: Vec<PdfBookmarkEntry> = Vec::with_capacity(bookmarks.len());
    for (index, mut entry) in bookmarks.into_iter().enumerate() {
        entry.title = entry.title.trim().to_string();
        if entry.title.is_empty() {
            return Err(format!("Bookmark {} needs a title.", index + 1));
        }
        if entry.title.chars().count() > MAX_TITLE_CHARACTERS || entry.title.len() > MAX_TITLE_BYTES
        {
            return Err(format!(
                "Bookmark {} is too long. Use at most {MAX_TITLE_CHARACTERS} characters.",
                index + 1
            ));
        }
        if entry.title.contains(['\r', '\n', '\0']) {
            return Err(format!(
                "Bookmark {} cannot contain line breaks or null characters.",
                index + 1
            ));
        }
        let page_number = entry
            .page_number
            .ok_or_else(|| format!("Bookmark {} needs a page destination.", index + 1))?;
        if page_number == 0 || page_number as usize > page_count {
            return Err(format!(
                "Bookmark {} points outside this PDF's {} pages.",
                index + 1,
                page_count
            ));
        }
        if entry.level > MAX_BOOKMARK_DEPTH {
            return Err(format!(
                "Bookmark {} is nested too deeply. The maximum level is {}.",
                index + 1,
                MAX_BOOKMARK_DEPTH + 1
            ));
        }
        if index == 0 && entry.level != 0 {
            return Err("The first bookmark must be at the top level.".to_string());
        }
        if let Some(previous) = validated.last() {
            if entry.level > previous.level + 1 {
                return Err(format!(
                    "Bookmark {} skips a nesting level. Indent it one level at a time.",
                    index + 1
                ));
            }
        }
        if entry
            .colour
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(format!(
                "Bookmark {} has an invalid text colour.",
                index + 1
            ));
        }
        validated.push(entry);
    }

    for index in 0..validated.len() {
        let has_children = validated
            .get(index + 1)
            .is_some_and(|next| next.level > validated[index].level);
        if !has_children {
            validated[index].open = true;
        }
    }
    Ok(validated)
}

pub(crate) fn replace_bookmarks(
    document: &mut Document,
    bookmarks: &[PdfBookmarkEntry],
) -> Result<(), String> {
    document.bookmarks.clear();
    document.bookmark_table.clear();
    document.max_bookmark_id = 0;
    {
        let catalog = document
            .catalog_mut()
            .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?;
        catalog.remove(b"Outlines");
        if bookmarks.is_empty()
            && catalog
                .get(b"PageMode")
                .and_then(Object::as_name)
                .is_ok_and(|mode| mode == b"UseOutlines")
        {
            catalog.remove(b"PageMode");
        }
    }
    if bookmarks.is_empty() {
        return Ok(());
    }

    let pages = document.get_pages();
    let mut nodes: Vec<BookmarkNode> = bookmarks
        .iter()
        .cloned()
        .map(|entry| BookmarkNode {
            entry,
            children: Vec::new(),
        })
        .collect();
    let mut roots = Vec::new();
    let mut parent_stack: Vec<usize> = Vec::new();
    for index in 0..nodes.len() {
        let level = nodes[index].entry.level as usize;
        parent_stack.truncate(level);
        if level == 0 {
            roots.push(index);
        } else {
            let parent = *parent_stack
                .get(level - 1)
                .ok_or_else(|| "The bookmark hierarchy is invalid.".to_string())?;
            nodes[parent].children.push(index);
        }
        parent_stack.push(index);
    }

    let root_id = document.add_object(Object::Null);
    let (first, last, descendant_count) =
        build_outline_siblings(document, &nodes, &roots, root_id, &pages)?;
    let mut root = Dictionary::new();
    root.set("Type", "Outlines");
    if let Some(first) = first {
        root.set("First", first);
    }
    if let Some(last) = last {
        root.set("Last", last);
    }
    root.set("Count", descendant_count as i64);
    document.objects.insert(root_id, Object::Dictionary(root));
    let catalog = document
        .catalog_mut()
        .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?;
    catalog.set("Outlines", root_id);
    catalog.set("PageMode", "UseOutlines");
    Ok(())
}

fn build_outline_siblings(
    document: &mut Document,
    nodes: &[BookmarkNode],
    indices: &[usize],
    parent_id: ObjectId,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
) -> Result<(Option<ObjectId>, Option<ObjectId>, usize), String> {
    if indices.is_empty() {
        return Ok((None, None, 0));
    }
    let object_ids = indices
        .iter()
        .map(|_| document.add_object(Object::Null))
        .collect::<Vec<_>>();
    let mut descendant_count = indices.len();

    for (position, node_index) in indices.iter().copied().enumerate() {
        let node = &nodes[node_index];
        let object_id = object_ids[position];
        let page_number = node
            .entry
            .page_number
            .ok_or_else(|| "A bookmark lost its page destination.".to_string())?;
        let page_id = *pages
            .get(&page_number)
            .ok_or_else(|| format!("Bookmark page {page_number} disappeared during export."))?;
        let mut dictionary = Dictionary::new();
        dictionary.set("Title", text_string(&node.entry.title));
        dictionary.set("Parent", parent_id);
        dictionary.set(
            "Dest",
            Object::Array(vec![
                Object::Reference(page_id),
                Object::Name(b"Fit".to_vec()),
            ]),
        );
        dictionary.set(
            "F",
            i64::from(u8::from(node.entry.italic) | (u8::from(node.entry.bold) << 1)),
        );
        dictionary.set(
            "C",
            node.entry
                .colour
                .iter()
                .copied()
                .map(Object::Real)
                .collect::<Vec<_>>(),
        );
        if position > 0 {
            dictionary.set("Prev", object_ids[position - 1]);
        }
        if position + 1 < object_ids.len() {
            dictionary.set("Next", object_ids[position + 1]);
        }
        if !node.children.is_empty() {
            let (first, last, child_count) =
                build_outline_siblings(document, nodes, &node.children, object_id, pages)?;
            if let Some(first) = first {
                dictionary.set("First", first);
            }
            if let Some(last) = last {
                dictionary.set("Last", last);
            }
            dictionary.set(
                "Count",
                if node.entry.open {
                    child_count as i64
                } else {
                    -(child_count as i64)
                },
            );
            descendant_count += child_count;
        }
        document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
    }

    Ok((
        object_ids.first().copied(),
        object_ids.last().copied(),
        descendant_count,
    ))
}

pub(crate) fn inspect_document_bookmarks(
    document: &Document,
) -> Result<InspectedBookmarks, String> {
    inspect_document_bookmarks_with_control(document, &PdfJobExecutionControl::direct())
}

pub(crate) fn inspect_document_bookmarks_with_control(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<InspectedBookmarks, String> {
    control.ensure_not_cancelled()?;
    let catalog = document
        .catalog()
        .map_err(|error| format!("The PDF catalogue is invalid: {error}"))?;
    let Some(outlines) = catalog.get(b"Outlines").ok().cloned() else {
        return Ok(InspectedBookmarks {
            entries: Vec::new(),
            warnings: Vec::new(),
        });
    };
    let root = resolved_dictionary(document, &outlines)?;
    let Some(first) = root.get(b"First").ok().cloned() else {
        return Ok(InspectedBookmarks {
            entries: Vec::new(),
            warnings: vec!["The PDF has an empty bookmark root.".to_string()],
        });
    };
    let pages_by_id = document
        .get_pages()
        .into_iter()
        .map(|(number, id)| (id, number))
        .collect::<HashMap<_, _>>();
    control.checkpoint(35, "Inspecting named bookmark destinations")?;
    let named_destinations = collect_named_destinations(document, control)?;
    let mut entries = Vec::new();
    let mut visited = HashSet::new();
    let mut warnings = Vec::new();
    walk_outline_siblings(
        document,
        first,
        0,
        &pages_by_id,
        &named_destinations,
        &mut visited,
        &mut entries,
        &mut warnings,
        control,
    )?;
    warnings.sort();
    warnings.dedup();
    Ok(InspectedBookmarks { entries, warnings })
}

#[allow(clippy::too_many_arguments)]
fn walk_outline_siblings(
    document: &Document,
    first: Object,
    level: u8,
    pages_by_id: &HashMap<ObjectId, u32>,
    named_destinations: &HashMap<Vec<u8>, Object>,
    visited: &mut HashSet<ObjectId>,
    entries: &mut Vec<PdfBookmarkEntry>,
    warnings: &mut Vec<String>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if level > MAX_BOOKMARK_DEPTH {
        return Err(format!(
            "The bookmark tree exceeds the supported depth of {} levels.",
            MAX_BOOKMARK_DEPTH + 1
        ));
    }
    let mut current = Some(first);
    while let Some(object) = current {
        let bookmark_number = entries.len() + 1;
        control.checkpoint(
            45 + ((bookmark_number.min(MAX_BOOKMARKS) * 35 / MAX_BOOKMARKS) as u8),
            format!("Inspecting bookmark {bookmark_number}"),
        )?;
        if entries.len() >= MAX_BOOKMARKS {
            return Err(format!(
                "The PDF contains more than {MAX_BOOKMARKS} bookmarks and cannot be edited safely."
            ));
        }
        if let Ok(id) = object.as_reference() {
            if !visited.insert(id) {
                return Err(
                    "The PDF bookmark tree contains a cycle and cannot be edited safely."
                        .to_string(),
                );
            }
        }
        let node = resolved_dictionary(document, &object)?;
        current = node.get(b"Next").ok().cloned();
        let (title, title_replaced) = bookmark_title(document, &node);
        if title_replaced {
            warnings.push(
                "One or more empty, invalid, or oversized bookmark titles were replaced for safe editing."
                    .to_string(),
            );
        }
        let title = if title.chars().count() > MAX_TITLE_CHARACTERS {
            warnings.push(
                "One or more unusually long bookmark titles were shortened for safe editing."
                    .to_string(),
            );
            title.chars().take(MAX_TITLE_CHARACTERS).collect()
        } else {
            title
        };
        let page_number = outline_destination(&node).and_then(|destination| {
            resolve_destination_page(
                document,
                &destination,
                pages_by_id,
                named_destinations,
                &mut HashSet::new(),
                0,
            )
        });
        let format = node.get(b"F").and_then(Object::as_i64).unwrap_or_default();
        let colour = node
            .get(b"C")
            .ok()
            .and_then(bookmark_colour)
            .unwrap_or([0.0, 0.0, 0.0]);
        let open = node
            .get(b"Count")
            .and_then(Object::as_i64)
            .map_or(true, |count| count >= 0);
        entries.push(PdfBookmarkEntry {
            title,
            page_number,
            level,
            bold: format & 2 != 0,
            italic: format & 1 != 0,
            open,
            colour,
        });
        if let Ok(child) = node.get(b"First") {
            walk_outline_siblings(
                document,
                child.clone(),
                level + 1,
                pages_by_id,
                named_destinations,
                visited,
                entries,
                warnings,
                control,
            )?;
        }
    }
    Ok(())
}

fn outline_destination(node: &Dictionary) -> Option<Object> {
    if let Ok(destination) = node.get(b"Dest") {
        return Some(destination.clone());
    }
    let action = node.get(b"A").ok()?;
    match action {
        Object::Dictionary(dictionary) => {
            if dictionary
                .get(b"S")
                .and_then(Object::as_name)
                .is_ok_and(|name| name == b"GoTo")
            {
                dictionary.get(b"D").ok().cloned()
            } else {
                None
            }
        }
        _ => Some(action.clone()),
    }
}

fn bookmark_title(document: &Document, node: &Dictionary) -> (String, bool) {
    let Some(value) = node
        .get(b"Title")
        .ok()
        .and_then(|value| document.dereference(value).ok().map(|(_, value)| value))
    else {
        return ("Untitled bookmark".to_string(), true);
    };
    if matches!(value, Object::String(bytes, _) if bytes.len() > MAX_TITLE_BYTES * 4) {
        return ("Untitled bookmark".to_string(), true);
    }
    let Ok(decoded) = decode_text_string(value) else {
        return ("Untitled bookmark".to_string(), true);
    };
    let title = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        ("Untitled bookmark".to_string(), true)
    } else {
        (title, false)
    }
}

fn resolve_destination_page(
    document: &Document,
    destination: &Object,
    pages_by_id: &HashMap<ObjectId, u32>,
    named_destinations: &HashMap<Vec<u8>, Object>,
    visited: &mut HashSet<ObjectId>,
    depth: usize,
) -> Option<u32> {
    if depth >= MAX_DESTINATION_DEPTH {
        return None;
    }
    match destination {
        Object::Reference(id) => {
            if let Some(page) = pages_by_id.get(id) {
                return Some(*page);
            }
            if !visited.insert(*id) {
                return None;
            }
            let object = document.get_object(*id).ok()?;
            resolve_destination_page(
                document,
                object,
                pages_by_id,
                named_destinations,
                visited,
                depth + 1,
            )
        }
        Object::Array(values) => values.first().and_then(|value| {
            resolve_destination_page(
                document,
                value,
                pages_by_id,
                named_destinations,
                visited,
                depth + 1,
            )
        }),
        Object::Dictionary(dictionary) => dictionary.get(b"D").ok().and_then(|value| {
            resolve_destination_page(
                document,
                value,
                pages_by_id,
                named_destinations,
                visited,
                depth + 1,
            )
        }),
        Object::String(name, _) | Object::Name(name) => {
            named_destinations.get(name).and_then(|value| {
                resolve_destination_page(
                    document,
                    value,
                    pages_by_id,
                    named_destinations,
                    visited,
                    depth + 1,
                )
            })
        }
        _ => None,
    }
}

fn collect_named_destinations(
    document: &Document,
    control: &PdfJobExecutionControl,
) -> Result<HashMap<Vec<u8>, Object>, String> {
    let mut destinations = HashMap::new();
    let Ok(catalog) = document.catalog() else {
        return Ok(destinations);
    };
    if let Ok(old_destinations) = catalog.get(b"Dests") {
        if let Ok(dictionary) = resolved_dictionary(document, old_destinations) {
            for (name, value) in dictionary.iter().take(MAX_NAMED_DESTINATION_NODES) {
                control.ensure_not_cancelled()?;
                destinations.insert(name.clone(), value.clone());
            }
        }
    }
    let names_tree = catalog
        .get(b"Names")
        .ok()
        .and_then(|names| resolved_dictionary(document, names).ok())
        .and_then(|names| names.get(b"Dests").ok().cloned());
    if let Some(names_tree) = names_tree {
        let mut visited = HashSet::new();
        let mut nodes = 0;
        collect_name_tree(
            document,
            &names_tree,
            &mut destinations,
            &mut visited,
            &mut nodes,
            control,
        )?;
    }
    Ok(destinations)
}

fn collect_name_tree(
    document: &Document,
    object: &Object,
    destinations: &mut HashMap<Vec<u8>, Object>,
    visited: &mut HashSet<ObjectId>,
    nodes: &mut usize,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.ensure_not_cancelled()?;
    if *nodes >= MAX_NAMED_DESTINATION_NODES {
        return Ok(());
    }
    if let Ok(id) = object.as_reference() {
        if !visited.insert(id) {
            return Ok(());
        }
    }
    let Ok(dictionary) = resolved_dictionary(document, object) else {
        return Ok(());
    };
    *nodes += 1;
    if let Ok(names) = dictionary.get(b"Names").and_then(Object::as_array) {
        for pair in names.chunks_exact(2) {
            control.ensure_not_cancelled()?;
            if destinations.len() >= MAX_NAMED_DESTINATION_NODES {
                break;
            }
            if let Ok(name) = pair[0].as_str() {
                destinations.insert(name.to_vec(), pair[1].clone());
            }
        }
    }
    if let Ok(children) = dictionary.get(b"Kids").and_then(Object::as_array) {
        for child in children {
            collect_name_tree(document, child, destinations, visited, nodes, control)?;
        }
    }
    Ok(())
}

fn resolved_dictionary(document: &Document, object: &Object) -> Result<Dictionary, String> {
    let (_, resolved) = document
        .dereference(object)
        .map_err(|error| format!("A bookmark object could not be resolved: {error}"))?;
    match resolved {
        Object::Dictionary(dictionary) => Ok(dictionary.clone()),
        Object::Stream(stream) => Ok(stream.dict.clone()),
        _ => Err("A bookmark object is not a dictionary.".to_string()),
    }
}

fn bookmark_colour(object: &Object) -> Option<[f32; 3]> {
    let values = object.as_array().ok()?;
    if values.len() != 3 {
        return None;
    }
    let colour = [
        values[0].as_f32().ok()?,
        values[1].as_f32().ok()?,
        values[2].as_f32().ok()?,
    ];
    colour
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .then_some(colour)
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
    use lopdf::{dictionary, Stream, StringFormat};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn inspects_nested_unicode_bookmarks_and_styles() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let mut document = fixture(false);
        let expected = vec![
            entry("Overview", 1, 0),
            PdfBookmarkEntry {
                title: "Tüfekci notes".to_string(),
                page_number: Some(2),
                level: 1,
                bold: true,
                italic: true,
                open: true,
                colour: [0.1, 0.2, 0.3],
            },
            entry("Appendix", 3, 0),
        ];
        replace_bookmarks(&mut document, &expected).unwrap();
        document.save(&input).unwrap().sync_all().unwrap();

        let result = inspect_pdf_bookmarks(InspectPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();

        assert_eq!(result.page_count, 3);
        assert_eq!(result.bookmark_count, 3);
        assert_eq!(result.unresolved_bookmark_count, 0);
        assert_eq!(result.bookmarks, expected);
    }

    #[test]
    fn controlled_bookmark_review_reports_progress_and_honours_cancellation() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let mut document = fixture(false);
        replace_bookmarks(
            &mut document,
            &[
                entry("First", 1, 0),
                entry("Second", 2, 0),
                entry("Third", 3, 0),
            ],
        )
        .unwrap();
        document.save(&input).unwrap().sync_all().unwrap();

        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_progress = Arc::clone(&observed);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            observed_for_progress.lock().unwrap().push((value, stage));
        });
        let report = inspect_pdf_bookmarks_with_control(
            InspectPdfBookmarksRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress),
        )
        .unwrap();

        assert_eq!(report.bookmark_count, 3);
        let observed = observed.lock().unwrap();
        assert!(observed
            .iter()
            .any(|(_, stage)| stage == "Inspecting bookmark 2"));
        assert_eq!(observed.last().map(|(value, _)| *value), Some(99));
        drop(observed);

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let cancel_progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Inspecting bookmark 2" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });
        let error = inspect_pdf_bookmarks_with_control(
            InspectPdfBookmarksRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            },
            &PdfJobExecutionControl::new(cancelled, cancel_progress),
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn bookmark_review_rejects_a_source_changed_before_report_delivery() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking bookmark source"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during bookmark review\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });

        let error = inspect_pdf_bookmarks_with_control(
            InspectPdfBookmarksRequest {
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
    fn exports_and_reopens_an_exact_edited_tree() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("bookmarked.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let bookmarks = vec![
            entry("Chapter 1", 1, 0),
            entry("Section 1.1", 2, 1),
            entry("Chapter 2", 3, 0),
        ];

        let result = export_pdf_bookmarks(ExportPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: fs::metadata(&input).unwrap().len(),
            expected_source_modified_at_ms: modified_at_ms(&fs::metadata(&input).unwrap()),
            bookmarks: bookmarks.clone(),
            printed_contents: None,
        })
        .unwrap();

        assert_eq!(result.bookmark_count, 3);
        assert!(result.bytes_written > 0);
        let reopened = Document::load(&output).unwrap();
        assert_eq!(reopened.get_pages().len(), 3);
        assert_eq!(
            inspect_document_bookmarks(&reopened).unwrap().entries,
            bookmarks
        );
    }

    #[test]
    fn prints_linked_unicode_contents_and_shifts_the_outline() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("contents.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let original_bytes = fs::read(&input).unwrap();
        let bookmarks = vec![
            entry("GiriÅŸ", 1, 0),
            entry("TÃ¼fekci iÅŸ akÄ±ÅŸÄ±", 2, 1),
            entry("RÃ©sumÃ©", 3, 0),
        ];

        let result = export_pdf_bookmarks(ExportPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: fs::metadata(&input).unwrap().len(),
            expected_source_modified_at_ms: modified_at_ms(&fs::metadata(&input).unwrap()),
            bookmarks: bookmarks.clone(),
            printed_contents: Some(PrintedContentsOptions {
                title: "Ä°Ã§indekiler".to_string(),
                maximum_level: 6,
                add_bookmark: true,
            }),
        })
        .unwrap();

        assert_eq!(fs::read(&input).unwrap(), original_bytes);
        assert_eq!(result.page_count, 4);
        assert_eq!(result.contents_page_count, 1);
        assert_eq!(result.printed_entry_count, 3);
        assert_eq!(result.bookmark_count, 4);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("3 linked entries")));
        let reopened = Document::load(&output).unwrap();
        let pages = reopened.get_pages();
        let contents_page = reopened.get_object(pages[&1]).unwrap().as_dict().unwrap();
        assert!(matches!(
            contents_page.get(b"PaperworksPrintedContents"),
            Ok(Object::Boolean(true))
        ));
        assert_eq!(
            contents_page
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let mut expected = bookmarks
            .into_iter()
            .map(|mut bookmark| {
                bookmark.page_number = bookmark.page_number.map(|page| page + 1);
                bookmark
            })
            .collect::<Vec<_>>();
        expected.insert(
            0,
            PdfBookmarkEntry {
                title: "Ä°Ã§indekiler".to_string(),
                page_number: Some(1),
                level: 0,
                bold: true,
                italic: false,
                open: true,
                colour: [0.09, 0.31, 0.65],
            },
        );
        assert_eq!(
            inspect_document_bookmarks(&reopened).unwrap().entries,
            expected
        );
    }

    #[test]
    fn paginates_large_contents_without_adding_a_sidebar_entry() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("contents.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let bookmarks = (0..45)
            .map(|index| entry(&format!("Heading {}", index + 1), index % 3 + 1, 0))
            .collect::<Vec<_>>();

        let result = export_pdf_bookmarks(ExportPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: fs::metadata(&input).unwrap().len(),
            expected_source_modified_at_ms: modified_at_ms(&fs::metadata(&input).unwrap()),
            bookmarks,
            printed_contents: Some(PrintedContentsOptions {
                title: "Contents".to_string(),
                maximum_level: 0,
                add_bookmark: false,
            }),
        })
        .unwrap();

        assert_eq!(result.contents_page_count, 2);
        assert_eq!(result.printed_entry_count, 45);
        assert_eq!(result.page_count, 5);
        assert_eq!(result.bookmark_count, 45);
        let reopened = Document::load(&output).unwrap();
        let pages = reopened.get_pages();
        for page_number in [1, 2] {
            assert!(matches!(
                reopened
                    .get_object(pages[&page_number])
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .get(b"PaperworksPrintedContents"),
                Ok(Object::Boolean(true))
            ));
        }
        assert_eq!(
            inspect_document_bookmarks(&reopened).unwrap().entries[0].page_number,
            Some(3)
        );
    }

    #[test]
    fn cancellation_during_contents_pagination_publishes_nothing() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("contents.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let bookmarks = (0..45)
            .map(|index| entry(&format!("Heading {}", index + 1), index % 3 + 1, 0))
            .collect::<Vec<_>>();
        let metadata = fs::metadata(&input).unwrap();
        let request = ExportPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: metadata.len(),
            expected_source_modified_at_ms: modified_at_ms(&metadata),
            bookmarks,
            printed_contents: Some(PrintedContentsOptions {
                title: "Contents".to_string(),
                maximum_level: 0,
                add_bookmark: false,
            }),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_progress = Arc::clone(&cancelled);
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Building printed contents page 2 of 2" {
                cancelled_for_progress.store(true, Ordering::Release);
            }
        });

        let error = export_pdf_bookmarks_with_control(
            request,
            &PdfJobExecutionControl::new(cancelled, progress),
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(!output.exists());
    }

    #[test]
    fn validates_printed_contents_title_depth_and_selected_entries() {
        let bookmarks = vec![entry("Nested", 1, 1)];
        assert!(validate_printed_contents_options(
            Some(&PrintedContentsOptions {
                title: " ".to_string(),
                maximum_level: 1,
                add_bookmark: false,
            }),
            &bookmarks,
        )
        .unwrap_err()
        .contains("need a title"));
        assert!(validate_printed_contents_options(
            Some(&PrintedContentsOptions {
                title: "Contents".to_string(),
                maximum_level: MAX_BOOKMARK_DEPTH + 1,
                add_bookmark: false,
            }),
            &bookmarks,
        )
        .unwrap_err()
        .contains("at most level"));
        assert!(validate_printed_contents_options(
            Some(&PrintedContentsOptions {
                title: "Contents".to_string(),
                maximum_level: 0,
                add_bookmark: false,
            }),
            &bookmarks,
        )
        .unwrap_err()
        .contains("at least one bookmark"));
    }

    #[test]
    fn reports_unresolved_named_destinations() {
        let mut document = fixture(false);
        let page_id = document.get_pages()[&1];
        let root_id = document.add_object(Object::Null);
        let child_id = document.add_object(dictionary! {
            "Title" => text_string("Missing destination"),
            "Parent" => root_id,
            "Dest" => Object::String(b"missing".to_vec(), StringFormat::Literal),
        });
        document.objects.insert(
            root_id,
            Object::Dictionary(dictionary! {
                "Type" => "Outlines",
                "First" => child_id,
                "Last" => child_id,
                "Count" => 1,
            }),
        );
        document.catalog_mut().unwrap().set("Outlines", root_id);
        assert!(page_id.0 > 0);

        let inspected = inspect_document_bookmarks(&document).unwrap();
        assert_eq!(inspected.entries.len(), 1);
        assert_eq!(inspected.entries[0].page_number, None);
    }

    #[test]
    fn validates_page_targets_and_hierarchy() {
        assert!(validate_bookmarks(vec![entry("Bad", 4, 0)], 3)
            .unwrap_err()
            .contains("outside"));
        let mut nested = entry("Bad nesting", 1, 2);
        nested.level = 2;
        assert!(validate_bookmarks(vec![nested], 3)
            .unwrap_err()
            .contains("top level"));
    }

    #[test]
    fn requires_acknowledgement_before_rewriting_a_signed_pdf() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed.pdf");
        let output = directory.path.join("bookmarked.pdf");
        fixture(true).save(&input).unwrap().sync_all().unwrap();

        let error = export_pdf_bookmarks(ExportPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: fs::metadata(&input).unwrap().len(),
            expected_source_modified_at_ms: modified_at_ms(&fs::metadata(&input).unwrap()),
            bookmarks: vec![entry("Start", 1, 0)],
            printed_contents: None,
        })
        .unwrap_err();

        assert!(error.contains("certificate signature"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_source_that_changed_after_review() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("bookmarked.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let inspection = inspect_pdf_bookmarks(InspectPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            input_password: None,
        })
        .unwrap();
        let mut bytes = fs::read(&input).unwrap();
        bytes.extend_from_slice(b"\n% changed after review\n");
        fs::write(&input, bytes).unwrap();

        let error = export_pdf_bookmarks(ExportPdfBookmarksRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: None,
            output_protection: None,
            acknowledge_certificate_signatures: false,
            expected_source_size: inspection.source_size,
            expected_source_modified_at_ms: inspection.source_modified_at_ms,
            bookmarks: vec![entry("Start", 1, 0)],
            printed_contents: None,
        })
        .unwrap_err();

        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_source_changed_during_export_before_publication() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("bookmarked.pdf");
        fixture(false).save(&input).unwrap().sync_all().unwrap();
        let metadata = fs::metadata(&input).unwrap();
        let expected_size = metadata.len();
        let expected_modified_at_ms = modified_at_ms(&metadata);
        let changed = Arc::new(AtomicBool::new(false));
        let changed_for_progress = Arc::clone(&changed);
        let input_for_progress = input.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |_, stage| {
            if stage == "Rechecking source PDF before publication"
                && !changed_for_progress.swap(true, Ordering::AcqRel)
            {
                let mut bytes = fs::read(&input_for_progress).unwrap();
                bytes.extend_from_slice(b"\n% changed during bookmark export\n");
                fs::write(&input_for_progress, bytes).unwrap();
            }
        });
        let control = PdfJobExecutionControl::new(Arc::new(AtomicBool::new(false)), progress);

        let error = export_pdf_bookmarks_with_control(
            ExportPdfBookmarksRequest {
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                output_protection: None,
                acknowledge_certificate_signatures: false,
                expected_source_size: expected_size,
                expected_source_modified_at_ms: expected_modified_at_ms,
                bookmarks: vec![entry("Start", 1, 0)],
                printed_contents: None,
            },
            &control,
        )
        .unwrap_err();

        assert!(changed.load(Ordering::Acquire));
        assert!(error.contains("changed on disk"));
        assert!(!output.exists());
    }

    fn entry(title: &str, page_number: u32, level: u8) -> PdfBookmarkEntry {
        PdfBookmarkEntry {
            title: title.to_string(),
            page_number: Some(page_number),
            level,
            bold: false,
            italic: false,
            open: true,
            colour: [0.0, 0.0, 0.0],
        }
    }

    fn fixture(signed: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let mut page_ids = Vec::new();
        for _ in 0..3 {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => Dictionary::new(),
                "Contents" => content_id,
            });
            page_ids.push(page_id);
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => 3,
            }),
        );
        let mut catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        };
        if signed {
            let signature_id = document.add_object(dictionary! {
                "FT" => "Sig",
                "V" => dictionary! {
                    "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                    "Contents" => Object::String(vec![1, 2, 3], StringFormat::Hexadecimal),
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
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "tufekci-paperworks-bookmark-test-{}-{nonce}",
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
