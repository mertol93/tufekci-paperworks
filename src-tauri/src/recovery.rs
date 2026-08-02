use crate::file_safety::reject_control_characters;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

const SNAPSHOT_VERSION: u8 = 1;
const SNAPSHOT_PREFIX: &str = "session-v1-";
const SNAPSHOT_SUFFIX: &str = ".json";
const MAX_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PDF_PAGES: usize = 25_000;
const MAX_IMPORTED_PDF_SOURCES: usize = 249;
const MAX_MERGE_SOURCES: usize = 250;
const MAX_SPLIT_GROUPS: usize = 250;
const MAX_PAGE_RANGE_LENGTH: usize = 4_096;
const MAX_SPLIT_GROUP_TEXT_LENGTH: usize = MAX_SPLIT_GROUPS * (MAX_PAGE_RANGE_LENGTH + 1);
const MAX_SCAN_FILES: usize = 2_000;
const SNAPSHOTS_TO_KEEP: usize = 3;
const MAX_CLOCK_SKEW_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const SCAN_EXTENSIONS: &[&str] = &[
    "avif", "bmp", "gif", "heic", "heif", "jpeg", "jpg", "png", "tif", "tiff", "webp",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecoverySnapshot {
    version: u8,
    saved_at_unix_ms: u64,
    active_workflow_id: String,
    selected_page: usize,
    zoom: u16,
    document: RecoveryDocument,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
enum RecoveryDocument {
    Pdf {
        name: String,
        source_path: String,
        #[serde(default)]
        imported_sources: Vec<RecoveryPdfSource>,
        pages: Vec<RecoveryPage>,
    },
    Scan {
        name: String,
        source_paths: Vec<String>,
        settings: RecoveryScanSettings,
    },
    Merge {
        name: String,
        sources: Vec<RecoveryMergeSource>,
    },
    Split {
        name: String,
        source_path: String,
        page_groups: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
enum RecoveryPage {
    Source {
        id: String,
        rotation: u16,
        #[serde(default = "primary_source_id")]
        source_id: String,
        source_page: u32,
    },
    Blank {
        id: String,
        height_pt: f64,
        paper_name: String,
        rotation: u16,
        width_pt: f64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RecoveryPdfSource {
    id: String,
    name: String,
    source_path: String,
    certificate_signature: bool,
    certificate_acknowledged: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RecoveryMergeSource {
    id: String,
    source_path: String,
    page_range: String,
}

fn primary_source_id() -> String {
    "primary".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RecoveryScanSettings {
    #[serde(default)]
    auto_crop: bool,
    colour_mode: String,
    #[serde(default)]
    correct_perspective: bool,
    dpi: u16,
    jpeg_quality: u8,
    margin_pt: f64,
    ocr_language: String,
    paper_id: String,
    recognise_text: bool,
    #[serde(default)]
    remove_shadows: bool,
    straighten: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySaveResult {
    saved_at_unix_ms: u64,
}

#[tauri::command]
pub fn save_recovery_snapshot(
    app: tauri::AppHandle,
    snapshot: RecoverySnapshot,
) -> Result<RecoverySaveResult, String> {
    let directory = recovery_directory(&app)?;
    save_snapshot_to_directory(&directory, &snapshot)?;
    Ok(RecoverySaveResult {
        saved_at_unix_ms: snapshot.saved_at_unix_ms,
    })
}

#[tauri::command]
pub fn load_recovery_snapshot(app: tauri::AppHandle) -> Result<Option<RecoverySnapshot>, String> {
    load_snapshot_from_directory(&recovery_directory(&app)?)
}

#[tauri::command]
pub fn clear_recovery_snapshots(app: tauri::AppHandle) -> Result<usize, String> {
    clear_snapshots_from_directory(&recovery_directory(&app)?)
}

fn recovery_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("The application data folder is unavailable: {error}"))?
        .join("recovery");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("The recovery folder could not be created: {error}"))?;
    Ok(directory)
}

fn save_snapshot_to_directory(directory: &Path, snapshot: &RecoverySnapshot) -> Result<(), String> {
    validate_snapshot(snapshot)?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("The recovery folder could not be created: {error}"))?;
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|error| format!("The recovery draft could not be encoded: {error}"))?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("The recovery draft is too large to store safely.".to_string());
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("The system clock is invalid: {error}"))?
        .as_nanos();
    let path = directory.join(format!(
        "{SNAPSHOT_PREFIX}{nonce}-{}{SNAPSHOT_SUFFIX}",
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| format!("The recovery draft could not be created: {error}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(format!(
            "The recovery draft could not be completed: {error}"
        ));
    }
    sync_directory(directory);
    prune_old_snapshots(directory)?;
    Ok(())
}

fn load_snapshot_from_directory(directory: &Path) -> Result<Option<RecoverySnapshot>, String> {
    if !directory.exists() {
        return Ok(None);
    }
    for path in snapshot_paths(directory)? {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_SNAPSHOT_BYTES as u64 => {
                metadata
            }
            _ => continue,
        };
        if metadata.len() == 0 {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let snapshot = match serde_json::from_slice::<RecoverySnapshot>(&bytes) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        if validate_snapshot(&snapshot).is_ok() {
            return Ok(Some(snapshot));
        }
    }
    Ok(None)
}

fn clear_snapshots_from_directory(directory: &Path) -> Result<usize, String> {
    if !directory.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for path in snapshot_paths(directory)? {
        if fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    sync_directory(directory);
    Ok(removed)
}

fn prune_old_snapshots(directory: &Path) -> Result<(), String> {
    let paths = snapshot_paths(directory)?;
    for path in paths.into_iter().skip(SNAPSHOTS_TO_KEEP) {
        let _ = fs::remove_file(path);
    }
    sync_directory(directory);
    Ok(())
}

fn snapshot_paths(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("The recovery folder could not be read: {error}"))?;
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with(SNAPSHOT_PREFIX) && file_name.ends_with(SNAPSHOT_SUFFIX) {
            paths.push(entry.path());
        }
    }
    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    Ok(paths)
}

fn validate_snapshot(snapshot: &RecoverySnapshot) -> Result<(), String> {
    if snapshot.version != SNAPSHOT_VERSION {
        return Err("The recovery draft uses an unsupported version.".to_string());
    }
    if snapshot.saved_at_unix_ms == 0 {
        return Err("The recovery draft has an invalid saved time.".to_string());
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("The system clock is invalid: {error}"))?
        .as_millis() as u64;
    if snapshot.saved_at_unix_ms > now_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
        return Err("The recovery draft has a saved time too far in the future.".to_string());
    }
    validate_short_text("Workflow", &snapshot.active_workflow_id, 64)?;
    if !(25..=400).contains(&snapshot.zoom) {
        return Err("The recovery zoom is outside the supported range.".to_string());
    }

    let page_count = match &snapshot.document {
        RecoveryDocument::Pdf {
            imported_sources,
            name,
            source_path,
            pages,
        } => {
            validate_short_text("Document name", name, 512)?;
            validate_source_path(source_path)?;
            if !has_extension(source_path, &["pdf"]) {
                return Err("The recovery draft does not refer to a PDF source.".to_string());
            }
            if pages.is_empty() || pages.len() > MAX_PDF_PAGES {
                return Err("The recovery draft has an invalid PDF page count.".to_string());
            }
            if imported_sources.len() > MAX_IMPORTED_PDF_SOURCES {
                return Err("The recovery draft has too many imported PDF sources.".to_string());
            }
            let mut source_ids = HashSet::from([primary_source_id()]);
            for source in imported_sources {
                validate_short_text("Imported source identifier", &source.id, 256)?;
                validate_short_text("Imported source name", &source.name, 512)?;
                validate_source_path(&source.source_path)?;
                if !has_extension(&source.source_path, &["pdf"]) {
                    return Err(
                        "The recovery draft contains a non-PDF imported source.".to_string()
                    );
                }
                if source.id == "primary" || !source_ids.insert(source.id.clone()) {
                    return Err(
                        "The recovery draft contains duplicate PDF source identifiers.".to_string(),
                    );
                }
                if source.certificate_acknowledged && !source.certificate_signature {
                    return Err(
                        "The recovery draft contains an invalid signature acknowledgement."
                            .to_string(),
                    );
                }
            }
            let mut page_ids = HashSet::new();
            for page in pages {
                match page {
                    RecoveryPage::Source {
                        id,
                        rotation,
                        source_id,
                        source_page,
                    } => {
                        validate_short_text("Page identifier", id, 512)?;
                        if !page_ids.insert(id) {
                            return Err("The recovery draft contains duplicate page identifiers."
                                .to_string());
                        }
                        validate_rotation(*rotation)?;
                        if !source_ids.contains(source_id) {
                            return Err("The recovery draft refers to an unavailable PDF source."
                                .to_string());
                        }
                        if *source_page == 0 {
                            return Err(
                                "The recovery draft contains an invalid source page.".to_string()
                            );
                        }
                    }
                    RecoveryPage::Blank {
                        id,
                        height_pt,
                        paper_name,
                        rotation,
                        width_pt,
                    } => {
                        validate_short_text("Page identifier", id, 512)?;
                        if !page_ids.insert(id) {
                            return Err("The recovery draft contains duplicate page identifiers."
                                .to_string());
                        }
                        validate_short_text("Paper name", paper_name, 128)?;
                        validate_rotation(*rotation)?;
                        if !valid_dimension(*width_pt) || !valid_dimension(*height_pt) {
                            return Err(
                                "The recovery draft contains invalid blank-page dimensions."
                                    .to_string(),
                            );
                        }
                    }
                }
            }
            pages.len()
        }
        RecoveryDocument::Scan {
            name,
            source_paths,
            settings,
        } => {
            validate_short_text("Document name", name, 512)?;
            if source_paths.is_empty() || source_paths.len() > MAX_SCAN_FILES {
                return Err("The recovery draft has an invalid scan-file count.".to_string());
            }
            for path in source_paths {
                validate_source_path(path)?;
                if !has_extension(path, SCAN_EXTENSIONS) {
                    return Err(
                        "The recovery draft contains an unsupported scan source.".to_string()
                    );
                }
            }
            validate_scan_settings(settings)?;
            source_paths.len()
        }
        RecoveryDocument::Merge { name, sources } => {
            if snapshot.active_workflow_id != "merge" {
                return Err("A merge recovery draft must reopen the Merge workflow.".to_string());
            }
            validate_short_text("Document name", name, 512)?;
            if sources.is_empty() || sources.len() > MAX_MERGE_SOURCES {
                return Err("The recovery draft has an invalid merge-source count.".to_string());
            }
            let mut source_ids = HashSet::new();
            let mut source_paths = HashSet::new();
            for source in sources {
                validate_short_text("Merge source identifier", &source.id, 256)?;
                if !source_ids.insert(source.id.as_str()) {
                    return Err(
                        "The recovery draft contains duplicate merge source identifiers."
                            .to_string(),
                    );
                }
                validate_source_path(&source.source_path)?;
                if !has_extension(&source.source_path, &["pdf"]) {
                    return Err("The recovery draft contains a non-PDF merge source.".to_string());
                }
                if !source_paths.insert(recovery_path_key(&source.source_path)) {
                    return Err(
                        "The recovery draft contains duplicate merge source paths.".to_string()
                    );
                }
                validate_recovery_range_text(&source.page_range)?;
            }
            sources.len()
        }
        RecoveryDocument::Split {
            name,
            source_path,
            page_groups,
        } => {
            if snapshot.active_workflow_id != "split" {
                return Err("A split recovery draft must reopen the Split workflow.".to_string());
            }
            validate_short_text("Document name", name, 512)?;
            validate_source_path(source_path)?;
            if !has_extension(source_path, &["pdf"]) {
                return Err("The recovery draft does not refer to a split PDF.".to_string());
            }
            validate_recovery_split_groups(page_groups)?;
            1
        }
    };
    if snapshot.selected_page == 0 || snapshot.selected_page > page_count {
        return Err("The recovery draft has an invalid selected page.".to_string());
    }
    Ok(())
}

fn validate_scan_settings(settings: &RecoveryScanSettings) -> Result<(), String> {
    if !matches!(
        settings.colour_mode.as_str(),
        "colour" | "greyscale" | "monochrome"
    ) {
        return Err("The recovery draft has an invalid scan colour mode.".to_string());
    }
    if !matches!(settings.dpi, 150 | 300 | 600) {
        return Err("The recovery draft has an invalid scan resolution.".to_string());
    }
    if !(40..=100).contains(&settings.jpeg_quality) {
        return Err("The recovery draft has an invalid JPEG quality.".to_string());
    }
    if !settings.margin_pt.is_finite() || !(0.0..=144.0).contains(&settings.margin_pt) {
        return Err("The recovery draft has an invalid scan margin.".to_string());
    }
    validate_short_text("OCR language", &settings.ocr_language, 64)?;
    validate_short_text("Paper preset", &settings.paper_id, 64)?;
    Ok(())
}

fn validate_source_path(path: &str) -> Result<(), String> {
    reject_control_characters("Recovery source path", path)?;
    if path.is_empty() || path.len() > 32_768 || !Path::new(path).is_absolute() {
        return Err("The recovery draft contains an invalid source path.".to_string());
    }
    Ok(())
}

fn validate_recovery_range_text(range: &str) -> Result<(), String> {
    reject_control_characters("Recovery page range", range)?;
    if range.len() > MAX_PAGE_RANGE_LENGTH {
        return Err(format!(
            "A recovered page range may contain no more than {MAX_PAGE_RANGE_LENGTH} characters."
        ));
    }
    Ok(())
}

fn validate_recovery_split_groups(page_groups: &str) -> Result<(), String> {
    reject_control_characters("Recovery split page groups", page_groups)?;
    if page_groups.len() > MAX_SPLIT_GROUP_TEXT_LENGTH {
        return Err("The recovered split plan is too large.".to_string());
    }
    let mut group_count = 0;
    for group in page_groups
        .split(';')
        .map(str::trim)
        .filter(|group| !group.is_empty())
    {
        group_count += 1;
        if group_count > MAX_SPLIT_GROUPS {
            return Err(format!(
                "A recovered split plan may contain no more than {MAX_SPLIT_GROUPS} page groups."
            ));
        }
        validate_recovery_range_text(group)?;
    }
    Ok(())
}

fn recovery_path_key(path: &str) -> String {
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path.to_string()
    }
}

fn has_extension(path: &str, allowed: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            allowed
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
}

fn validate_short_text(label: &str, value: &str, max_length: usize) -> Result<(), String> {
    reject_control_characters(label, value)?;
    if value.is_empty() || value.len() > max_length {
        return Err(format!("The recovery draft contains an invalid {label}."));
    }
    Ok(())
}

fn validate_rotation(rotation: u16) -> Result<(), String> {
    if matches!(rotation, 0 | 90 | 180 | 270) {
        Ok(())
    } else {
        Err("The recovery draft contains an invalid page rotation.".to_string())
    }
}

fn valid_dimension(value: f64) -> bool {
    value.is_finite() && (1.0..=14_400.0).contains(&value)
}

#[cfg(unix)]
fn sync_directory(directory: &Path) {
    if let Ok(file) = fs::File::open(directory) {
        let _ = file.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn saves_loads_and_rotates_create_new_snapshots() {
        let directory = TestDirectory::new();
        for saved_at in 1..=5 {
            let mut snapshot = pdf_snapshot();
            snapshot.saved_at_unix_ms = saved_at;
            save_snapshot_to_directory(&directory.path, &snapshot).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }

        let paths = snapshot_paths(&directory.path).unwrap();
        assert_eq!(paths.len(), SNAPSHOTS_TO_KEEP);
        let loaded = load_snapshot_from_directory(&directory.path)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.saved_at_unix_ms, 5);
    }

    #[test]
    fn falls_back_when_the_newest_snapshot_is_incomplete() {
        let directory = TestDirectory::new();
        let snapshot = pdf_snapshot();
        save_snapshot_to_directory(&directory.path, &snapshot).unwrap();
        let corrupt_path = directory.path.join(format!(
            "{SNAPSHOT_PREFIX}99999999999999999999-0{SNAPSHOT_SUFFIX}"
        ));
        fs::write(corrupt_path, b"{\"version\":1").unwrap();

        let loaded = load_snapshot_from_directory(&directory.path)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.saved_at_unix_ms, snapshot.saved_at_unix_ms);
    }

    #[test]
    fn schema_does_not_store_passwords_signatures_or_document_bytes() {
        let snapshot = pdf_snapshot();
        let json = serde_json::to_string(&snapshot).unwrap();

        assert!(json.contains("\"kind\":\"pdf\""));
        assert!(json.contains("\"sourcePath\""));
        assert!(json.contains("\"sourcePage\""));
        assert!(json.contains("\"sourceId\""));
        assert!(json.contains("\"importedSources\""));
        assert!(json.contains("\"paperName\""));
        assert!(!json.contains("source_page"));
        assert!(!json.to_ascii_lowercase().contains("password"));
        assert!(!json.contains("pngDataUrl"));
        assert!(!json.contains("Visible page"));
        validate_snapshot(&snapshot).unwrap();
        let decoded = serde_json::from_str::<RecoverySnapshot>(&json).unwrap();
        assert_eq!(decoded.selected_page, snapshot.selected_page);
    }

    #[test]
    fn scan_snapshot_round_trips_settings_and_ordered_sources() {
        let snapshot = scan_snapshot();
        validate_snapshot(&snapshot).unwrap();
        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded = serde_json::from_str::<RecoverySnapshot>(&json).unwrap();

        assert!(json.contains("\"kind\":\"scan\""));
        assert!(json.contains("\"sourcePaths\""));
        assert!(json.contains("\"autoCrop\":true"));
        assert!(json.contains("\"colourMode\":\"greyscale\""));
        assert!(json.contains("\"correctPerspective\":true"));
        assert!(json.contains("\"removeShadows\":false"));
        assert_eq!(decoded.selected_page, 2);
    }

    #[test]
    fn merge_and_split_snapshots_round_trip_without_secrets() {
        for snapshot in [merge_snapshot(), split_snapshot()] {
            validate_snapshot(&snapshot).unwrap();
            let json = serde_json::to_string(&snapshot).unwrap();
            let decoded = serde_json::from_str::<RecoverySnapshot>(&json).unwrap();

            assert!(!json.to_ascii_lowercase().contains("password"));
            assert!(!json.contains("certificateAcknowledged"));
            assert!(!json.contains("outputProtection"));
            assert_eq!(decoded.selected_page, 1);
        }

        let merge_json = serde_json::to_string(&merge_snapshot()).unwrap();
        assert!(merge_json.contains("\"kind\":\"merge\""));
        assert!(merge_json.contains("\"pageRange\":\"3-1\""));
        assert!(merge_json.contains("\"sourcePath\""));

        let split_json = serde_json::to_string(&split_snapshot()).unwrap();
        assert!(split_json.contains("\"kind\":\"split\""));
        assert!(split_json.contains("\"pageGroups\":\"1-3; 7, 9\""));
    }

    #[test]
    fn older_scan_snapshots_default_cleanup_to_off() {
        let mut value = serde_json::to_value(scan_snapshot()).unwrap();
        let settings = value
            .get_mut("document")
            .and_then(|document| document.get_mut("settings"))
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        settings.remove("autoCrop");
        settings.remove("correctPerspective");
        settings.remove("removeShadows");

        let snapshot = serde_json::from_value::<RecoverySnapshot>(value).unwrap();
        validate_snapshot(&snapshot).unwrap();
        let RecoveryDocument::Scan { settings, .. } = snapshot.document else {
            panic!("expected a scan recovery draft");
        };
        assert!(!settings.auto_crop);
        assert!(!settings.correct_perspective);
        assert!(!settings.remove_shadows);
    }

    #[test]
    fn older_pdf_snapshots_default_to_the_primary_source() {
        let mut value = serde_json::to_value(pdf_snapshot()).unwrap();
        let document = value
            .get_mut("document")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        document.remove("importedSources");
        for page in document
            .get_mut("pages")
            .and_then(serde_json::Value::as_array_mut)
            .unwrap()
        {
            if page.get("kind").and_then(serde_json::Value::as_str) == Some("source") {
                page.as_object_mut().unwrap().remove("sourceId");
            }
        }

        let snapshot = serde_json::from_value::<RecoverySnapshot>(value).unwrap();
        validate_snapshot(&snapshot).unwrap();
    }

    #[test]
    fn rejects_unknown_or_invalid_recovery_data() {
        let mut value = serde_json::to_value(pdf_snapshot()).unwrap();
        value.as_object_mut().unwrap().insert(
            "password".to_string(),
            serde_json::Value::String("secret".to_string()),
        );
        assert!(serde_json::from_value::<RecoverySnapshot>(value).is_err());

        let mut snapshot = pdf_snapshot();
        snapshot.zoom = 900;
        assert!(validate_snapshot(&snapshot).is_err());

        let mut snapshot = pdf_snapshot();
        snapshot.saved_at_unix_ms = u64::MAX;
        assert!(validate_snapshot(&snapshot).is_err());

        let mut snapshot = pdf_snapshot();
        if let RecoveryDocument::Pdf { pages, .. } = &mut snapshot.document {
            let first_id = match &pages[0] {
                RecoveryPage::Source { id, .. } => id.clone(),
                RecoveryPage::Blank { id, .. } => id.clone(),
            };
            match &mut pages[1] {
                RecoveryPage::Source { id, .. } | RecoveryPage::Blank { id, .. } => *id = first_id,
            }
        }
        assert!(validate_snapshot(&snapshot).is_err());
    }

    #[test]
    fn rejects_secret_or_unbounded_standalone_plans() {
        let mut value = serde_json::to_value(merge_snapshot()).unwrap();
        value
            .get_mut("document")
            .and_then(|document| document.get_mut("sources"))
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|sources| sources.first_mut())
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .insert(
                "password".to_string(),
                serde_json::Value::String("must not persist".to_string()),
            );
        assert!(serde_json::from_value::<RecoverySnapshot>(value).is_err());

        let mut duplicate = merge_snapshot();
        if let RecoveryDocument::Merge { sources, .. } = &mut duplicate.document {
            sources[1].id = sources[0].id.clone();
        }
        assert!(validate_snapshot(&duplicate).is_err());

        let mut oversized_range = merge_snapshot();
        if let RecoveryDocument::Merge { sources, .. } = &mut oversized_range.document {
            sources[0].page_range = "1".repeat(MAX_PAGE_RANGE_LENGTH + 1);
        }
        assert!(validate_snapshot(&oversized_range).is_err());

        let mut too_many_groups = split_snapshot();
        if let RecoveryDocument::Split { page_groups, .. } = &mut too_many_groups.document {
            *page_groups = std::iter::repeat_n("1", MAX_SPLIT_GROUPS + 1)
                .collect::<Vec<_>>()
                .join(";");
        }
        assert!(validate_snapshot(&too_many_groups).is_err());

        let mut wrong_workflow = split_snapshot();
        wrong_workflow.active_workflow_id = "merge".to_string();
        assert!(validate_snapshot(&wrong_workflow).is_err());
    }

    #[test]
    fn clears_only_recovery_snapshot_files() {
        let directory = TestDirectory::new();
        save_snapshot_to_directory(&directory.path, &pdf_snapshot()).unwrap();
        let unrelated = directory.path.join("keep-me.txt");
        fs::write(&unrelated, b"keep").unwrap();

        assert_eq!(clear_snapshots_from_directory(&directory.path).unwrap(), 1);
        assert!(unrelated.exists());
    }

    fn pdf_snapshot() -> RecoverySnapshot {
        RecoverySnapshot {
            version: SNAPSHOT_VERSION,
            saved_at_unix_ms: 1_750_000_000_000,
            active_workflow_id: "organise".to_string(),
            selected_page: 1,
            zoom: 92,
            document: RecoveryDocument::Pdf {
                name: "Example.pdf".to_string(),
                source_path: if cfg!(windows) {
                    r"C:\Documents\Example.pdf".to_string()
                } else {
                    "/documents/Example.pdf".to_string()
                },
                imported_sources: vec![RecoveryPdfSource {
                    id: "imported-example".to_string(),
                    name: "Appendix.pdf".to_string(),
                    source_path: if cfg!(windows) {
                        r"C:\Documents\Appendix.pdf".to_string()
                    } else {
                        "/documents/Appendix.pdf".to_string()
                    },
                    certificate_signature: true,
                    certificate_acknowledged: true,
                }],
                pages: vec![
                    RecoveryPage::Source {
                        id: "example:source:1".to_string(),
                        rotation: 90,
                        source_id: primary_source_id(),
                        source_page: 1,
                    },
                    RecoveryPage::Blank {
                        id: "example:blank:2".to_string(),
                        height_pt: 842.0,
                        paper_name: "A4".to_string(),
                        rotation: 0,
                        width_pt: 595.0,
                    },
                    RecoveryPage::Source {
                        id: "example:import:3".to_string(),
                        rotation: 0,
                        source_id: "imported-example".to_string(),
                        source_page: 2,
                    },
                ],
            },
        }
    }

    fn scan_snapshot() -> RecoverySnapshot {
        let source = |name: &str| {
            if cfg!(windows) {
                format!(r"C:\Scans\{name}")
            } else {
                format!("/scans/{name}")
            }
        };
        RecoverySnapshot {
            version: SNAPSHOT_VERSION,
            saved_at_unix_ms: 1_750_000_000_000,
            active_workflow_id: "scan".to_string(),
            selected_page: 2,
            zoom: 110,
            document: RecoveryDocument::Scan {
                name: "Two-image scan".to_string(),
                source_paths: vec![source("front.png"), source("back.tiff")],
                settings: RecoveryScanSettings {
                    auto_crop: true,
                    colour_mode: "greyscale".to_string(),
                    correct_perspective: true,
                    dpi: 300,
                    jpeg_quality: 88,
                    margin_pt: 18.0,
                    ocr_language: "eng".to_string(),
                    paper_id: "a4".to_string(),
                    recognise_text: true,
                    remove_shadows: false,
                    straighten: true,
                },
            },
        }
    }

    fn merge_snapshot() -> RecoverySnapshot {
        RecoverySnapshot {
            version: SNAPSHOT_VERSION,
            saved_at_unix_ms: 1_750_000_000_000,
            active_workflow_id: "merge".to_string(),
            selected_page: 1,
            zoom: 92,
            document: RecoveryDocument::Merge {
                name: "Two-PDF merge plan".to_string(),
                sources: vec![
                    RecoveryMergeSource {
                        id: "merge-first".to_string(),
                        source_path: recovery_pdf_path("First.pdf"),
                        page_range: "all".to_string(),
                    },
                    RecoveryMergeSource {
                        id: "merge-second".to_string(),
                        source_path: recovery_pdf_path("Second.pdf"),
                        page_range: "3-1".to_string(),
                    },
                ],
            },
        }
    }

    fn split_snapshot() -> RecoverySnapshot {
        RecoverySnapshot {
            version: SNAPSHOT_VERSION,
            saved_at_unix_ms: 1_750_000_000_000,
            active_workflow_id: "split".to_string(),
            selected_page: 1,
            zoom: 92,
            document: RecoveryDocument::Split {
                name: "Source.pdf split plan".to_string(),
                source_path: recovery_pdf_path("Source.pdf"),
                page_groups: "1-3; 7, 9".to_string(),
            },
        }
    }

    fn recovery_pdf_path(name: &str) -> String {
        if cfg!(windows) {
            format!(r"C:\Documents\{name}")
        } else {
            format!("/documents/{name}")
        }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-recovery-test",
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
