use crate::pdf_jobs::PdfJobKind;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const RECOVERY_DIRECTORY: &str = "active-pdf-jobs";
const RECORD_SCHEMA_VERSION: u8 = 1;
const RECORD_PREFIX: &str = "active-";
const RECORD_SUFFIX: &str = ".json";
const LOCK_SUFFIX: &str = ".lock";
const MAX_DIRECTORY_ENTRIES: usize = 512;
const MAX_RECOVERED_JOBS: usize = 32;
const MAX_RECORD_BYTES: u64 = 8 * 1024;
const MAX_ENTRY_ID_BYTES: usize = 96;
const FUTURE_TIME_TOLERANCE_MS: u64 = 5 * 60 * 1_000;
const ORPHAN_LOCK_RETENTION_MS: u64 = 60 * 60 * 1_000;

static ENTRY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(crate) struct JobRecoveryStore {
    root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecoveredPdfJob {
    pub(crate) entry_id: String,
    pub(crate) kind: PdfJobKind,
    pub(crate) started_at_ms: u64,
    pub(crate) recovered_at_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ActiveJobRecord {
    schema_version: u8,
    entry_id: String,
    kind: PdfJobKind,
    started_at_ms: u64,
}

pub(crate) struct JobRecoveryLease {
    record_path: Option<PathBuf>,
    lock_path: Option<PathBuf>,
    lock_file: Option<File>,
}

impl JobRecoveryStore {
    pub(crate) fn initialise(
        app: &tauri::AppHandle,
    ) -> Result<(Self, Vec<RecoveredPdfJob>), String> {
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("The application data folder is unavailable: {error}"))?;
        fs::create_dir_all(&app_data).map_err(|error| {
            format!("The application data folder could not be created: {error}")
        })?;
        Self::open_directory(&app_data.join(RECOVERY_DIRECTORY))
    }

    pub(crate) fn open_directory(path: &Path) -> Result<(Self, Vec<RecoveredPdfJob>), String> {
        let root = secure_recovery_directory(path)?;
        let store = Self { root };
        let recovered = store.recover_stale_jobs()?;
        Ok((store, recovered))
    }

    pub(crate) fn register(
        &self,
        kind: PdfJobKind,
        started_at_ms: u64,
    ) -> Result<JobRecoveryLease, String> {
        if started_at_ms == 0
            || started_at_ms > timestamp_ms().saturating_add(FUTURE_TIME_TOLERANCE_MS)
        {
            return Err("The PDF job start time is outside the supported bounds.".to_string());
        }

        for _ in 0..32 {
            let sequence = ENTRY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let entry_id = format!("{}-{started_at_ms}-{sequence}", std::process::id());
            let lock_path = self.root.join(lock_file_name(&entry_id));
            let lock_file = match create_and_lock(&lock_path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "The PDF job recovery lock could not be created: {error}"
                    ));
                }
            };
            let record_path = self.root.join(record_file_name(&entry_id));
            let record = ActiveJobRecord {
                schema_version: RECORD_SCHEMA_VERSION,
                entry_id,
                kind,
                started_at_ms,
            };
            if let Err(error) = write_record(&record_path, &record) {
                release_lock(lock_file);
                let _ = remove_file_if_present(&lock_path);
                return Err(error);
            }
            sync_directory(&self.root);
            return Ok(JobRecoveryLease {
                record_path: Some(record_path),
                lock_path: Some(lock_path),
                lock_file: Some(lock_file),
            });
        }
        Err("A unique PDF job recovery entry could not be allocated.".to_string())
    }

    fn recover_stale_jobs(&self) -> Result<Vec<RecoveredPdfJob>, String> {
        let mut records = Vec::new();
        let mut locks = Vec::new();
        let mut record_ids = HashSet::new();
        let entries = fs::read_dir(&self.root).map_err(|error| {
            format!("The PDF job recovery folder could not be inspected: {error}")
        })?;

        for (index, entry) in entries.enumerate() {
            if index >= MAX_DIRECTORY_ENTRIES {
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if let Some(entry_id) = parse_record_file_name(&name) {
                record_ids.insert(entry_id.to_string());
                records.push(entry.path());
            } else if let Some(entry_id) = parse_lock_file_name(&name) {
                locks.push((entry_id.to_string(), entry.path()));
            }
        }

        records.sort();
        let recovered_at_ms = timestamp_ms();
        let mut recovered = Vec::new();
        for record_path in records {
            if let Some(job) = self.recover_record(&record_path, recovered_at_ms)? {
                recovered.push(job);
            }
        }
        for (entry_id, lock_path) in locks {
            if !record_ids.contains(&entry_id) {
                cleanup_orphan_lock(&lock_path);
            }
        }

        recovered.sort_by(|left, right| {
            left.started_at_ms
                .cmp(&right.started_at_ms)
                .then_with(|| left.entry_id.cmp(&right.entry_id))
        });
        if recovered.len() > MAX_RECOVERED_JOBS {
            recovered.drain(..recovered.len() - MAX_RECOVERED_JOBS);
        }
        Ok(recovered)
    }

    fn recover_record(
        &self,
        record_path: &Path,
        recovered_at_ms: u64,
    ) -> Result<Option<RecoveredPdfJob>, String> {
        let Some(name) = record_path.file_name().and_then(|value| value.to_str()) else {
            return Ok(None);
        };
        let Some(entry_id) = parse_record_file_name(name) else {
            return Ok(None);
        };
        let lock_path = self.root.join(lock_file_name(entry_id));
        let lock_file = match claim_lock(&lock_path) {
            Ok(file) => file,
            Err(ClaimLockError::Live) => return Ok(None),
            Err(ClaimLockError::Unsafe) => {
                remove_unsafe_record(record_path);
                return Ok(None);
            }
            Err(ClaimLockError::Io(error)) => {
                return Err(format!(
                    "A PDF job recovery lock could not be inspected: {error}"
                ));
            }
        };

        let record = read_record(record_path)
            .and_then(|record| validate_record(record, entry_id))
            .ok();
        if remove_file_if_present(record_path).is_err() {
            release_lock(lock_file);
            return Err(
                "An interrupted PDF job recovery entry could not be retired safely.".to_string(),
            );
        }
        sync_directory(&self.root);
        release_lock(lock_file);
        let _ = remove_file_if_present(&lock_path);
        sync_directory(&self.root);

        Ok(record.map(|record| RecoveredPdfJob {
            entry_id: record.entry_id,
            kind: record.kind,
            started_at_ms: record.started_at_ms,
            recovered_at_ms,
        }))
    }
}

impl JobRecoveryLease {
    pub(crate) fn complete(mut self) -> Result<(), String> {
        let Some(record_path) = self.record_path.take() else {
            return Ok(());
        };
        if let Err(error) = remove_file_if_present(&record_path) {
            self.record_path = Some(record_path);
            return Err(format!(
                "The completed PDF job recovery entry could not be removed: {error}"
            ));
        }
        if let Some(lock_file) = self.lock_file.take() {
            release_lock(lock_file);
        }
        if let Some(lock_path) = self.lock_path.take() {
            remove_file_if_present(&lock_path).map_err(|error| {
                format!("The completed PDF job recovery lock could not be removed: {error}")
            })?;
            if let Some(root) = lock_path.parent() {
                sync_directory(root);
            }
        }
        Ok(())
    }
}

impl Drop for JobRecoveryLease {
    fn drop(&mut self) {
        // Only an explicit terminal transition removes the record; shutdown leaves it recoverable.
        if let Some(lock_file) = self.lock_file.take() {
            release_lock(lock_file);
        }
    }
}

enum ClaimLockError {
    Live,
    Unsafe,
    Io(io::Error),
}

fn secure_recovery_directory(path: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !is_link_or_reparse(&metadata) && metadata.is_dir() => {}
        Ok(_) => {
            return Err("The PDF job recovery location is not a private directory.".to_string());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| {
                format!("The PDF job recovery folder could not be created: {error}")
            })?;
        }
        Err(error) => {
            return Err(format!(
                "The PDF job recovery folder could not be inspected: {error}"
            ));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!("The PDF job recovery folder permissions could not be secured: {error}")
    })?;
    fs::canonicalize(path)
        .map_err(|error| format!("The PDF job recovery folder could not be opened: {error}"))
}

fn create_and_lock(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    if let Err(error) = file.try_lock() {
        drop(file);
        let _ = remove_file_if_present(path);
        return Err(io::Error::other(error));
    }
    Ok(file)
}

fn claim_lock(path: &Path) -> Result<File, ClaimLockError> {
    for _ in 0..2 {
        match open_existing_lock(path) {
            Ok(file) => {
                return match file.try_lock() {
                    Ok(()) => Ok(file),
                    Err(TryLockError::WouldBlock) => Err(ClaimLockError::Live),
                    Err(TryLockError::Error(error)) => Err(ClaimLockError::Io(error)),
                };
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => match create_and_lock(path) {
                Ok(file) => return Ok(file),
                Err(create_error) if create_error.kind() == io::ErrorKind::AlreadyExists => {
                    continue;
                }
                Err(create_error) => return Err(ClaimLockError::Io(create_error)),
            },
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                return Err(ClaimLockError::Unsafe);
            }
            Err(error) => return Err(ClaimLockError::Io(error)),
        }
    }
    Err(ClaimLockError::Live)
}

fn open_existing_lock(path: &Path) -> io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The recovery lock is not a regular file.",
        ));
    }
    OpenOptions::new().read(true).write(true).open(path)
}

fn write_record(path: &Path, record: &ActiveJobRecord) -> Result<(), String> {
    let encoded = serde_json::to_vec(record)
        .map_err(|error| format!("The PDF job recovery entry could not be encoded: {error}"))?;
    if encoded.is_empty() || encoded.len() as u64 > MAX_RECORD_BYTES {
        return Err("The PDF job recovery entry is outside the supported bounds.".to_string());
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| format!("The PDF job recovery entry could not be created: {error}"))?;
    if let Err(error) = file.write_all(&encoded).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = remove_file_if_present(path);
        return Err(format!(
            "The PDF job recovery entry could not be completed: {error}"
        ));
    }
    Ok(())
}

fn read_record(path: &Path) -> Result<ActiveJobRecord, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("The PDF job recovery entry could not be inspected: {error}"))?;
    if is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_RECORD_BYTES
    {
        return Err("The PDF job recovery entry is unsafe.".to_string());
    }
    let file = File::open(path)
        .map_err(|error| format!("The PDF job recovery entry could not be opened: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("The PDF job recovery entry could not be read: {error}"))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("The PDF job recovery entry is outside the supported bounds.".to_string());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("The PDF job recovery entry is malformed: {error}"))
}

fn validate_record(
    record: ActiveJobRecord,
    expected_entry_id: &str,
) -> Result<ActiveJobRecord, String> {
    if record.schema_version != RECORD_SCHEMA_VERSION
        || record.entry_id != expected_entry_id
        || !valid_entry_id(&record.entry_id)
        || record.started_at_ms == 0
        || record.started_at_ms > timestamp_ms().saturating_add(FUTURE_TIME_TOLERANCE_MS)
    {
        return Err("The PDF job recovery entry is invalid.".to_string());
    }
    Ok(record)
}

fn remove_unsafe_record(path: &Path) {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || metadata.is_file() {
            let _ = remove_file_if_present(path);
        }
    }
}

fn cleanup_orphan_lock(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if is_link_or_reparse(&metadata) || !metadata.is_file() {
        return;
    }
    let old_enough = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|modified| timestamp_ms().checked_sub(modified.as_millis().try_into().ok()?))
        .is_some_and(|age| age >= ORPHAN_LOCK_RETENTION_MS);
    if !old_enough {
        return;
    }
    let Ok(lock_file) = open_existing_lock(path) else {
        return;
    };
    if lock_file.try_lock().is_ok() {
        release_lock(lock_file);
        let _ = remove_file_if_present(path);
    }
}

fn record_file_name(entry_id: &str) -> String {
    format!("{RECORD_PREFIX}{entry_id}{RECORD_SUFFIX}")
}

fn lock_file_name(entry_id: &str) -> String {
    format!("{RECORD_PREFIX}{entry_id}{LOCK_SUFFIX}")
}

fn parse_record_file_name(name: &str) -> Option<&str> {
    let entry_id = name
        .strip_prefix(RECORD_PREFIX)?
        .strip_suffix(RECORD_SUFFIX)?;
    valid_entry_id(entry_id).then_some(entry_id)
}

fn parse_lock_file_name(name: &str) -> Option<&str> {
    let entry_id = name
        .strip_prefix(RECORD_PREFIX)?
        .strip_suffix(LOCK_SUFFIX)?;
    valid_entry_id(entry_id).then_some(entry_id)
}

fn valid_entry_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ENTRY_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
        && value.split('-').count() == 3
        && value.split('-').all(ascii_digits)
}

fn ascii_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn release_lock(file: File) {
    let _ = file.unlock();
    drop(file);
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
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
    use std::collections::BTreeSet;

    #[test]
    fn live_job_is_not_recovered_by_another_instance() {
        let directory = TestDirectory::new();
        let (store, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert!(recovered.is_empty());
        let lease = store.register(PdfJobKind::Scan, timestamp_ms()).unwrap();

        let (_, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert!(recovered.is_empty());

        lease.complete().unwrap();
    }

    #[test]
    fn interrupted_job_is_recovered_exactly_once() {
        let directory = TestDirectory::new();
        let (store, _) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        let started_at_ms = timestamp_ms();
        let lease = store
            .register(PdfJobKind::Certificate, started_at_ms)
            .unwrap();
        drop(lease);

        let (_, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].kind, PdfJobKind::Certificate);
        assert_eq!(recovered[0].started_at_ms, started_at_ms);
        assert!(recovered[0].recovered_at_ms >= started_at_ms);

        let (_, recovered_again) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert!(recovered_again.is_empty());
    }

    #[test]
    fn completed_job_leaves_no_recovery_entry() {
        let directory = TestDirectory::new();
        let (store, _) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        store
            .register(PdfJobKind::Merge, timestamp_ms())
            .unwrap()
            .complete()
            .unwrap();

        let (_, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert!(recovered.is_empty());
        assert!(fs::read_dir(&directory.path).unwrap().next().is_none());
    }

    #[test]
    fn record_schema_contains_only_the_safe_allow_list() {
        let directory = TestDirectory::new();
        let (store, _) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        let lease = store
            .register(PdfJobKind::Protection, timestamp_ms())
            .unwrap();
        let record_path = lease.record_path.as_ref().unwrap();
        let encoded = fs::read_to_string(record_path).unwrap();
        let value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
        let fields = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            fields,
            BTreeSet::from(["entryId", "kind", "schemaVersion", "startedAtMs"])
        );
        for forbidden in [
            "sourcePath",
            "outputPath",
            "password",
            "passphrase",
            "document",
            "ocr",
            "signature",
            "secret",
        ] {
            assert!(!encoded
                .to_ascii_lowercase()
                .contains(&forbidden.to_ascii_lowercase()));
        }
        lease.complete().unwrap();
    }

    #[test]
    fn malformed_unknown_oversized_and_future_records_are_not_surfaced() {
        let directory = TestDirectory::new();
        let now = timestamp_ms();
        let cases = [
            ("7-1-1", b"{".to_vec()),
            (
                "7-1-2",
                format!(
                    r#"{{"schemaVersion":1,"entryId":"7-1-2","kind":"scan","startedAtMs":{now},"password":"secret"}}"#
                )
                .into_bytes(),
            ),
            ("7-1-3", vec![b'x'; MAX_RECORD_BYTES as usize + 1]),
            (
                "7-1-4",
                format!(
                    r#"{{"schemaVersion":1,"entryId":"7-1-4","kind":"scan","startedAtMs":{}}}"#,
                    now + FUTURE_TIME_TOLERANCE_MS + 60_000
                )
                .into_bytes(),
            ),
        ];
        for (entry_id, bytes) in cases {
            fs::write(directory.path.join(record_file_name(entry_id)), bytes).unwrap();
        }

        let (_, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert!(recovered.is_empty());
        assert!(fs::read_dir(&directory.path).unwrap().next().is_none());
    }

    #[test]
    fn only_the_newest_bounded_set_is_restored() {
        let directory = TestDirectory::new();
        let (store, _) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        let base = timestamp_ms().saturating_sub(100);
        for index in 0..(MAX_RECOVERED_JOBS + 5) {
            let lease = store
                .register(PdfJobKind::Batch, base + index as u64)
                .unwrap();
            drop(lease);
        }

        let (_, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert_eq!(recovered.len(), MAX_RECOVERED_JOBS);
        assert_eq!(recovered.first().unwrap().started_at_ms, base + 5);
        assert_eq!(
            recovered.last().unwrap().started_at_ms,
            base + (MAX_RECOVERED_JOBS + 4) as u64
        );
    }

    #[test]
    fn filename_parser_accepts_only_exact_bounded_identifiers() {
        assert_eq!(
            parse_record_file_name("active-12-345-6.json"),
            Some("12-345-6")
        );
        assert_eq!(
            parse_lock_file_name("active-12-345-6.lock"),
            Some("12-345-6")
        );
        for invalid in [
            "active-12-345.json",
            "active-12-345-6-7.json",
            "active-12-name-6.json",
            "prefix-active-12-345-6.json",
            "active-12-345-6.json.bak",
            "active-12-345-6.lock.json",
        ] {
            assert!(parse_record_file_name(invalid).is_none());
        }
    }

    #[cfg(unix)]
    #[test]
    fn linked_record_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.path.join("outside.json");
        fs::write(&target, b"do not remove").unwrap();
        let entry_id = "12-345-6";
        symlink(&target, directory.path.join(record_file_name(entry_id))).unwrap();

        let (_, recovered) = JobRecoveryStore::open_directory(&directory.path).unwrap();
        assert!(recovered.is_empty());
        assert_eq!(fs::read(&target).unwrap(), b"do not remove");
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
                "tufekci-paperworks-job-recovery-test-{}-{nonce}",
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
