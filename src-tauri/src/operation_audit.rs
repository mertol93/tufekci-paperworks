use crate::file_safety::reject_control_characters;
use crate::pdf_jobs::{PdfJobKind, PdfJobStatus};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const AUDIT_DIRECTORY: &str = "operation-audit";
const AUDIT_LOCK_FILE: &str = "history.lock";
const SNAPSHOT_PREFIX: &str = "history-";
const SNAPSHOT_SUFFIX: &str = ".json";
const SNAPSHOT_VERSION: u8 = 1;
const SNAPSHOTS_TO_KEEP: usize = 3;
const MAX_AUDIT_ENTRIES: usize = 500;
const MAX_SNAPSHOT_BYTES: u64 = 512 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 128;
const MAX_CLOCK_SKEW_MS: u64 = 5 * 60 * 1_000;
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_RETRY: Duration = Duration::from_millis(25);
const PERSISTENCE_WARNING: &str = "New activity records may not survive an application restart.";
const RECOVERY_WARNING: &str =
    "An incomplete activity snapshot was skipped and an earlier valid snapshot was restored.";
const UNREADABLE_WARNING: &str =
    "Stored activity history could not be read safely and was not loaded.";

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OperationAuditOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct OperationAuditEntry {
    pub(crate) id: String,
    pub(crate) operation: PdfJobKind,
    pub(crate) outcome: OperationAuditOutcome,
    pub(crate) started_at_ms: u64,
    pub(crate) completed_at_ms: u64,
    pub(crate) duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationAuditReport {
    pub(crate) entries: Vec<OperationAuditEntry>,
    pub(crate) total_entries: usize,
    pub(crate) capacity: usize,
    pub(crate) persistence_warning: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ClearOperationAuditRequest {
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ExportOperationAuditRequest {
    output_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportOperationAuditResult {
    entry_count: usize,
    bytes_written: u64,
}

#[derive(Clone)]
pub(crate) struct OperationAudit {
    inner: Arc<Mutex<AuditStore>>,
    directory: Option<PathBuf>,
}

#[derive(Clone, Default)]
struct AuditStore {
    sequence: u64,
    entries: VecDeque<OperationAuditEntry>,
    pending: Vec<OperationAuditEntry>,
    persistence_warning: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AuditSnapshot {
    schema_version: u8,
    sequence: u64,
    saved_at_ms: u64,
    entries: Vec<OperationAuditEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditExport<'a> {
    schema_version: u8,
    exported_at_ms: u64,
    privacy: &'static str,
    entries: &'a [OperationAuditEntry],
}

struct SnapshotCandidate {
    sequence: u64,
    path: PathBuf,
}

impl OperationAudit {
    pub(crate) fn initialise(app: &tauri::AppHandle) -> Result<Self, String> {
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("The application data folder is unavailable: {error}"))?;
        fs::create_dir_all(&app_data).map_err(|error| {
            format!("The application data folder could not be created: {error}")
        })?;
        Self::open_directory(&app_data.join(AUDIT_DIRECTORY))
    }

    fn open_directory(path: &Path) -> Result<Self, String> {
        let directory = secure_audit_directory(path)?;
        ensure_lock_file(&directory)?;
        let store = with_directory_lock(&directory, || load_store(&directory))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(store)),
            directory: Some(directory),
        })
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuditStore::default())),
            directory: None,
        }
    }

    pub(crate) fn record_terminal(
        &self,
        operation: PdfJobKind,
        status: PdfJobStatus,
        started_at_ms: u64,
        completed_at_ms: u64,
    ) -> Result<(), String> {
        let outcome = match status {
            PdfJobStatus::Succeeded => OperationAuditOutcome::Succeeded,
            PdfJobStatus::Failed => OperationAuditOutcome::Failed,
            PdfJobStatus::Cancelled => OperationAuditOutcome::Cancelled,
            PdfJobStatus::Queued | PdfJobStatus::Running => {
                return Err(
                    "Only a completed operation can be written to activity history.".to_string(),
                );
            }
        };
        let completed_at_ms = completed_at_ms.max(started_at_ms);
        let entry = OperationAuditEntry {
            id: new_event_id(completed_at_ms),
            operation,
            outcome,
            started_at_ms,
            completed_at_ms,
            duration_ms: completed_at_ms.saturating_sub(started_at_ms),
        };
        validate_entry(&entry)?;

        let mut memory = self.lock()?;
        let Some(directory) = self.directory.as_deref() else {
            push_bounded(&mut memory.entries, entry);
            return Ok(());
        };

        let pending = memory.pending.clone();
        match with_directory_lock(directory, || {
            let mut store = load_store(directory)?;
            let mut known_ids = store
                .entries
                .iter()
                .map(|existing| existing.id.clone())
                .collect::<HashSet<_>>();
            for pending_entry in pending.iter().chain(std::iter::once(&entry)) {
                if known_ids.insert(pending_entry.id.clone()) {
                    push_bounded(&mut store.entries, pending_entry.clone());
                }
            }
            store.sequence = store.sequence.saturating_add(1);
            let snapshot = store.snapshot();
            write_snapshot(directory, &snapshot)?;
            store.pending.clear();
            Ok(store)
        }) {
            Ok(store) => {
                *memory = store;
                Ok(())
            }
            Err(error) => {
                push_bounded(&mut memory.entries, entry.clone());
                memory.pending.push(entry);
                if memory.pending.len() > MAX_AUDIT_ENTRIES {
                    memory.pending.remove(0);
                }
                memory.persistence_warning = Some(PERSISTENCE_WARNING.to_string());
                Err(error)
            }
        }
    }

    pub(crate) fn report(&self) -> Result<OperationAuditReport, String> {
        let mut store = self.lock()?;
        if store.pending.is_empty() {
            if let Some(directory) = self.directory.as_deref() {
                match with_directory_lock(directory, || load_store(directory)) {
                    Ok(refreshed) => *store = refreshed,
                    Err(_) => {
                        store.persistence_warning = Some(PERSISTENCE_WARNING.to_string());
                    }
                }
            }
        }
        Ok(report_from_store(&store))
    }

    fn clear(&self) -> Result<usize, String> {
        let mut memory = self.lock()?;
        let count = memory.entries.len();
        let Some(directory) = self.directory.as_deref() else {
            memory.entries.clear();
            memory.pending.clear();
            memory.persistence_warning = None;
            return Ok(count);
        };

        let cleared = with_directory_lock(directory, || {
            let current = load_store(directory)?;
            let mut store = AuditStore {
                sequence: current.sequence.saturating_add(1),
                ..AuditStore::default()
            };
            let snapshot = store.snapshot();
            let retained = write_snapshot(directory, &snapshot)?;
            remove_other_snapshots(directory, &retained)?;
            store.persistence_warning = None;
            Ok((current.entries.len(), store))
        })?;
        *memory = cleared.1;
        Ok(count.max(cleared.0))
    }

    fn lock(&self) -> Result<MutexGuard<'_, AuditStore>, String> {
        self.inner
            .lock()
            .map_err(|_| "The activity history could not be accessed safely.".to_string())
    }
}

impl AuditStore {
    fn snapshot(&self) -> AuditSnapshot {
        AuditSnapshot {
            schema_version: SNAPSHOT_VERSION,
            sequence: self.sequence,
            saved_at_ms: timestamp_ms(),
            entries: self.entries.iter().cloned().collect(),
        }
    }
}

#[tauri::command]
pub(crate) fn list_operation_audit(
    audit: State<'_, OperationAudit>,
) -> Result<OperationAuditReport, String> {
    audit.report()
}

#[tauri::command]
pub(crate) fn clear_operation_audit(
    request: ClearOperationAuditRequest,
    audit: State<'_, OperationAudit>,
) -> Result<usize, String> {
    if !request.confirmed {
        return Err("Confirm that the local activity history should be cleared.".to_string());
    }
    audit.clear()
}

#[tauri::command]
pub(crate) fn export_operation_audit(
    request: ExportOperationAuditRequest,
    audit: State<'_, OperationAudit>,
) -> Result<ExportOperationAuditResult, String> {
    let report = audit.report()?;
    export_report_to_path(&request.output_path, &report)
}

fn export_report_to_path(
    output_path: &str,
    report: &OperationAuditReport,
) -> Result<ExportOperationAuditResult, String> {
    reject_control_characters("Activity export path", output_path)?;
    let requested = PathBuf::from(output_path);
    if !requested.is_absolute()
        || !requested
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("Choose a new .json filename for the activity export.".to_string());
    }
    match fs::symlink_metadata(&requested) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => return Err("The activity export destination already exists.".to_string()),
        Err(error) => {
            return Err(format!(
                "The activity export destination could not be inspected: {error}"
            ));
        }
    }
    let parent = requested
        .parent()
        .ok_or_else(|| "The activity export folder is invalid.".to_string())?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| format!("The activity export folder could not be opened: {error}"))?;
    let file_name = requested
        .file_name()
        .ok_or_else(|| "The activity export filename is invalid.".to_string())?;
    let output = canonical_parent.join(file_name);
    let export = AuditExport {
        schema_version: SNAPSHOT_VERSION,
        exported_at_ms: timestamp_ms(),
        privacy: "This export contains operation type, terminal outcome, and timing only. It contains no filenames, paths, passwords, document content, or job results.",
        entries: &report.entries,
    };
    let bytes = serde_json::to_vec_pretty(&export)
        .map_err(|error| format!("The activity export could not be encoded: {error}"))?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err("The activity export is too large to write safely.".to_string());
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&output)
        .map_err(|error| format!("The activity export could not be created: {error}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&output);
        return Err(format!(
            "The activity export could not be completed: {error}"
        ));
    }
    sync_directory(&canonical_parent);
    Ok(ExportOperationAuditResult {
        entry_count: report.entries.len(),
        bytes_written: bytes.len() as u64,
    })
}

fn secure_audit_directory(path: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !is_link_or_reparse(&metadata) && metadata.is_dir() => {}
        Ok(_) => {
            return Err("The activity-history location is not a private directory.".to_string())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| {
                format!("The activity-history folder could not be created: {error}")
            })?;
        }
        Err(error) => {
            return Err(format!(
                "The activity-history folder could not be inspected: {error}"
            ));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!("The activity-history permissions could not be secured: {error}")
    })?;
    fs::canonicalize(path)
        .map_err(|error| format!("The activity-history folder could not be opened: {error}"))
}

fn ensure_lock_file(directory: &Path) -> Result<(), String> {
    let path = directory.join(AUDIT_LOCK_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !is_link_or_reparse(&metadata) && metadata.is_file() => {}
        Ok(_) => return Err("The activity-history lock is not a regular file.".to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.create_new(true).read(true).write(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&path) {
                Ok(file) => {
                    file.sync_all().map_err(|error| {
                        format!("The activity-history lock could not be completed: {error}")
                    })?;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    return ensure_lock_file(directory);
                }
                Err(error) => {
                    return Err(format!(
                        "The activity-history lock could not be created: {error}"
                    ));
                }
            }
        }
        Err(error) => {
            return Err(format!(
                "The activity-history lock could not be inspected: {error}"
            ));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        format!("The activity-history lock permissions could not be secured: {error}")
    })?;
    Ok(())
}

fn with_directory_lock<T>(
    directory: &Path,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    ensure_lock_file(directory)?;
    let path = directory.join(AUDIT_LOCK_FILE);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("The activity-history lock could not be inspected: {error}"))?;
    if is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err("The activity-history lock is not a regular file.".to_string());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("The activity-history lock could not be opened: {error}"))?;
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(LOCK_RETRY);
            }
            Err(TryLockError::WouldBlock) => {
                return Err("Another application process is updating activity history.".to_string());
            }
            Err(TryLockError::Error(error)) => {
                return Err(format!(
                    "The activity-history lock could not be acquired: {error}"
                ));
            }
        }
    }
    let result = action();
    let _ = file.unlock();
    result
}

fn load_store(directory: &Path) -> Result<AuditStore, String> {
    let (candidates, scan_limited) = snapshot_candidates(directory)?;
    let maximum_sequence = candidates
        .iter()
        .map(|candidate| candidate.sequence)
        .max()
        .unwrap_or(0);
    let mut skipped = 0;
    for candidate in &candidates {
        let snapshot = match read_snapshot(&candidate.path) {
            Ok(snapshot)
                if snapshot.sequence == candidate.sequence
                    && validate_snapshot(&snapshot).is_ok() =>
            {
                snapshot
            }
            _ => {
                skipped += 1;
                continue;
            }
        };
        return Ok(AuditStore {
            sequence: maximum_sequence.max(snapshot.sequence),
            entries: snapshot.entries.into(),
            pending: Vec::new(),
            persistence_warning: if skipped > 0 || scan_limited {
                Some(RECOVERY_WARNING.to_string())
            } else {
                None
            },
        });
    }
    Ok(AuditStore {
        sequence: maximum_sequence,
        entries: VecDeque::new(),
        pending: Vec::new(),
        persistence_warning: if candidates.is_empty() && !scan_limited {
            None
        } else {
            Some(UNREADABLE_WARNING.to_string())
        },
    })
}

fn read_snapshot(path: &Path) -> Result<AuditSnapshot, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("An activity snapshot could not be inspected: {error}"))?;
    if is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_SNAPSHOT_BYTES
    {
        return Err("An activity snapshot is outside the supported bounds.".to_string());
    }
    let file = File::open(path)
        .map_err(|error| format!("An activity snapshot could not be opened: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("An activity snapshot could not be read: {error}"))?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err("An activity snapshot is too large.".to_string());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("An activity snapshot is malformed: {error}"))
}

fn write_snapshot(directory: &Path, snapshot: &AuditSnapshot) -> Result<PathBuf, String> {
    validate_snapshot(snapshot)?;
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|error| format!("The activity snapshot could not be encoded: {error}"))?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err("The activity snapshot is too large to store safely.".to_string());
    }
    let path = directory.join(format!(
        "{SNAPSHOT_PREFIX}{:020}-{}{SNAPSHOT_SUFFIX}",
        snapshot.sequence,
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&path)
        .map_err(|error| format!("The activity snapshot could not be created: {error}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(format!(
            "The activity snapshot could not be completed: {error}"
        ));
    }
    sync_directory(directory);
    prune_snapshots(directory)?;
    Ok(path)
}

fn snapshot_candidates(directory: &Path) -> Result<(Vec<SnapshotCandidate>, bool), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("The activity-history folder could not be read: {error}"))?;
    let mut candidates = Vec::new();
    let mut scan_limited = false;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            scan_limited = true;
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        let Some(sequence) = parse_snapshot_name(&name) else {
            continue;
        };
        candidates.push(SnapshotCandidate {
            sequence,
            path: entry.path(),
        });
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.sequence));
    Ok((candidates, scan_limited))
}

fn prune_snapshots(directory: &Path) -> Result<(), String> {
    let (candidates, _) = snapshot_candidates(directory)?;
    for candidate in candidates.into_iter().skip(SNAPSHOTS_TO_KEEP) {
        let _ = fs::remove_file(candidate.path);
    }
    sync_directory(directory);
    Ok(())
}

fn remove_other_snapshots(directory: &Path, retained: &Path) -> Result<(), String> {
    let (candidates, _) = snapshot_candidates(directory)?;
    for candidate in candidates {
        if candidate.path != retained {
            let _ = fs::remove_file(candidate.path);
        }
    }
    sync_directory(directory);
    Ok(())
}

fn parse_snapshot_name(name: &str) -> Option<u64> {
    let body = name
        .strip_prefix(SNAPSHOT_PREFIX)?
        .strip_suffix(SNAPSHOT_SUFFIX)?;
    let (sequence, pid) = body.split_once('-')?;
    if sequence.len() != 20
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        || pid.is_empty()
        || !pid.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    sequence.parse().ok()
}

fn validate_snapshot(snapshot: &AuditSnapshot) -> Result<(), String> {
    if snapshot.schema_version != SNAPSHOT_VERSION
        || snapshot.sequence == 0
        || snapshot.saved_at_ms == 0
        || snapshot.entries.len() > MAX_AUDIT_ENTRIES
        || snapshot.saved_at_ms > timestamp_ms().saturating_add(MAX_CLOCK_SKEW_MS)
    {
        return Err("The activity snapshot is invalid.".to_string());
    }
    let mut ids = HashSet::new();
    for entry in &snapshot.entries {
        validate_entry(entry)?;
        if !ids.insert(&entry.id) {
            return Err("The activity snapshot contains duplicate entries.".to_string());
        }
    }
    Ok(())
}

fn validate_entry(entry: &OperationAuditEntry) -> Result<(), String> {
    let expected_duration = entry.completed_at_ms.saturating_sub(entry.started_at_ms);
    if !valid_event_id(&entry.id)
        || entry.started_at_ms == 0
        || entry.completed_at_ms < entry.started_at_ms
        || entry.completed_at_ms > timestamp_ms().saturating_add(MAX_CLOCK_SKEW_MS)
        || entry.duration_ms != expected_duration
    {
        return Err("An activity-history entry is invalid.".to_string());
    }
    Ok(())
}

fn valid_event_id(value: &str) -> bool {
    let Some(body) = value.strip_prefix("operation-") else {
        return false;
    };
    let mut parts = body.split('-');
    parts.next().is_some_and(ascii_digits)
        && parts.next().is_some_and(ascii_digits)
        && parts.next().is_some_and(ascii_digits)
        && parts.next().is_none()
}

fn ascii_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn new_event_id(completed_at_ms: u64) -> String {
    let sequence = EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "operation-{}-{completed_at_ms}-{sequence}",
        std::process::id()
    )
}

fn push_bounded(entries: &mut VecDeque<OperationAuditEntry>, entry: OperationAuditEntry) {
    entries.push_back(entry);
    while entries.len() > MAX_AUDIT_ENTRIES {
        entries.pop_front();
    }
}

fn report_from_store(store: &AuditStore) -> OperationAuditReport {
    let entries = store.entries.iter().rev().cloned().collect::<Vec<_>>();
    OperationAuditReport {
        total_entries: entries.len(),
        entries,
        capacity: MAX_AUDIT_ENTRIES,
        persistence_warning: store.persistence_warning.clone(),
    }
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(unix)]
fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) {}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_only_operation_outcome_and_timing() {
        let audit = OperationAudit::in_memory();
        audit
            .record_terminal(
                PdfJobKind::Protection,
                PdfJobStatus::Succeeded,
                1_750_000_000_000,
                1_750_000_001_250,
            )
            .unwrap();
        let report = audit.report().unwrap();
        let value = serde_json::to_value(&report.entries[0]).unwrap();
        let fields = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        assert_eq!(
            fields,
            HashSet::from([
                "completedAtMs",
                "durationMs",
                "id",
                "operation",
                "outcome",
                "startedAtMs"
            ])
        );
        let encoded = serde_json::to_string(&report).unwrap();
        for forbidden in [
            "password",
            "sourcePath",
            "outputPath",
            "documentText",
            "result",
            "error",
            "stage",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn keeps_only_the_newest_five_hundred_entries() {
        let audit = OperationAudit::in_memory();
        for index in 0..(MAX_AUDIT_ENTRIES + 25) {
            audit
                .record_terminal(
                    PdfJobKind::Scan,
                    PdfJobStatus::Succeeded,
                    1_750_000_000_000 + index as u64,
                    1_750_000_000_001 + index as u64,
                )
                .unwrap();
        }
        let report = audit.report().unwrap();
        assert_eq!(report.total_entries, MAX_AUDIT_ENTRIES);
        assert_eq!(
            report.entries.first().unwrap().completed_at_ms,
            1_750_000_000_525
        );
        assert_eq!(
            report.entries.last().unwrap().completed_at_ms,
            1_750_000_000_026
        );
    }

    #[test]
    fn restores_an_older_generation_after_an_interrupted_write() {
        let directory = TestDirectory::new();
        let audit = OperationAudit::open_directory(&directory.path).unwrap();
        audit
            .record_terminal(
                PdfJobKind::Merge,
                PdfJobStatus::Succeeded,
                1_750_000_000_000,
                1_750_000_000_010,
            )
            .unwrap();
        audit
            .record_terminal(
                PdfJobKind::Split,
                PdfJobStatus::Failed,
                1_750_000_001_000,
                1_750_000_001_010,
            )
            .unwrap();
        let (candidates, _) = snapshot_candidates(&directory.path).unwrap();
        fs::write(&candidates[0].path, b"{").unwrap();

        let reopened = OperationAudit::open_directory(&directory.path).unwrap();
        let report = reopened.report().unwrap();
        assert_eq!(report.total_entries, 1);
        assert_eq!(report.entries[0].operation, PdfJobKind::Merge);
        assert_eq!(
            report.persistence_warning.as_deref(),
            Some(RECOVERY_WARNING)
        );
    }

    #[test]
    fn rejects_unknown_snapshot_fields_without_loading_them() {
        let directory = TestDirectory::new();
        ensure_lock_file(&directory.path).unwrap();
        let path = directory
            .path
            .join(format!("{SNAPSHOT_PREFIX}{:020}-1{SNAPSHOT_SUFFIX}", 1));
        fs::write(
            path,
            br#"{"schemaVersion":1,"sequence":1,"savedAtMs":1750000000000,"entries":[],"password":"must-not-load"}"#,
        )
        .unwrap();
        let audit = OperationAudit::open_directory(&directory.path).unwrap();
        let report = audit.report().unwrap();
        assert_eq!(report.total_entries, 0);
        assert_eq!(
            report.persistence_warning.as_deref(),
            Some(UNREADABLE_WARNING)
        );
    }

    #[test]
    fn clearing_writes_an_empty_generation_and_removes_old_history() {
        let directory = TestDirectory::new();
        let audit = OperationAudit::open_directory(&directory.path).unwrap();
        audit
            .record_terminal(
                PdfJobKind::Annotations,
                PdfJobStatus::Cancelled,
                1_750_000_000_000,
                1_750_000_000_010,
            )
            .unwrap();
        audit
            .record_terminal(
                PdfJobKind::Forms,
                PdfJobStatus::Succeeded,
                1_750_000_001_000,
                1_750_000_001_010,
            )
            .unwrap();

        assert_eq!(audit.clear().unwrap(), 2);
        assert_eq!(audit.report().unwrap().total_entries, 0);
        assert_eq!(snapshot_candidates(&directory.path).unwrap().0.len(), 1);
    }

    #[test]
    fn snapshot_schema_rejects_entry_extras() {
        let audit = OperationAudit::in_memory();
        audit
            .record_terminal(
                PdfJobKind::Privacy,
                PdfJobStatus::Succeeded,
                1_750_000_000_000,
                1_750_000_000_010,
            )
            .unwrap();
        let mut snapshot = audit.lock().unwrap().snapshot();
        snapshot.sequence = 1;
        let mut value = serde_json::to_value(snapshot).unwrap();
        value
            .get_mut("entries")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|entries| entries.first_mut())
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .insert(
                "outputPath".to_string(),
                serde_json::Value::String("private.pdf".to_string()),
            );
        assert!(serde_json::from_value::<AuditSnapshot>(value).is_err());
    }

    #[test]
    fn exports_a_create_new_path_free_json_report() {
        let directory = TestDirectory::new();
        let audit = OperationAudit::in_memory();
        audit
            .record_terminal(
                PdfJobKind::Certificate,
                PdfJobStatus::Succeeded,
                1_750_000_000_000,
                1_750_000_002_000,
            )
            .unwrap();
        let report = audit.report().unwrap();
        let output = directory.path.join("activity.json");
        let result = export_report_to_path(&output.to_string_lossy(), &report).unwrap();
        assert_eq!(result.entry_count, 1);
        assert_eq!(result.bytes_written, fs::metadata(&output).unwrap().len());

        let encoded = fs::read_to_string(&output).unwrap();
        let value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
        assert_eq!(value["schemaVersion"], SNAPSHOT_VERSION);
        assert_eq!(value["entries"].as_array().unwrap().len(), 1);
        for forbidden_field in [
            "sourcePath",
            "outputPath",
            "password",
            "documentText",
            "jobResult",
            "jobError",
        ] {
            assert!(!encoded.contains(&format!("\"{forbidden_field}\"")));
        }
        assert!(export_report_to_path(&output.to_string_lossy(), &report).is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), encoded);
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
                "tufekci-paperworks-operation-audit-test-{}-{nonce}",
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
