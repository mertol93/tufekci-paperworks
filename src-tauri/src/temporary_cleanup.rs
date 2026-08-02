use crate::file_safety::paths_are_equal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const REGISTRY_DIRECTORY: &str = "temporary-workspaces";
const RECORD_VERSION: u8 = 1;
const MAX_REGISTRY_ENTRIES: usize = 4_096;
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_LEASE_ID_BYTES: usize = 96;
const DIRECTORY_TOKEN_FILE: &str = ".paperworks-owner";
const SCANNER_CAPTURE_RETENTION_DAYS: u8 = 7;
const FUTURE_TIME_TOLERANCE_MS: u64 = 5 * 60 * 1_000;
const ORPHAN_LOCK_RETENTION_MS: u64 = 60 * 60 * 1_000;

static LEASE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static REGISTRY: OnceLock<TemporaryRegistry> = OnceLock::new();

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TemporaryKind {
    OutputFile,
    BatchDirectory,
    CertificateWorkspace,
    CertificatePassfile,
    PyHankoPasswordBridge,
    OcrProgressPlugin,
    OcrUserWords,
    ScanRaster,
}

fn is_directory_kind(kind: TemporaryKind) -> bool {
    matches!(
        kind,
        TemporaryKind::BatchDirectory
            | TemporaryKind::CertificateWorkspace
            | TemporaryKind::PyHankoPasswordBridge
    )
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TemporaryCleanupStatus {
    completed_at_ms: u64,
    inspected_leases: usize,
    removed_files: usize,
    removed_directories: usize,
    removed_orphan_locks: usize,
    missing_targets: usize,
    active_leases: usize,
    rejected_leases: usize,
    errors: usize,
    scan_limited: bool,
    scanner_capture_retention_days: u8,
}

#[derive(Clone)]
struct TemporaryRegistry {
    root: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LeaseRecord {
    version: u8,
    lease_id: String,
    kind: TemporaryKind,
    target_path: String,
    owner_pid: u32,
    created_at_ms: u64,
    ownership_token: String,
}

pub(crate) struct TemporaryLease {
    target_path: PathBuf,
    kind: TemporaryKind,
    ownership_token: String,
    record_path: Option<PathBuf>,
    lock_path: Option<PathBuf>,
    lock_file: Option<File>,
    remove_target_on_drop: bool,
}

enum RemovedTarget {
    File,
    Directory,
    Missing,
}

pub(crate) fn initialise(app: &tauri::AppHandle) -> Result<TemporaryCleanupStatus, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("The application data folder is unavailable: {error}"))?;
    fs::create_dir_all(&app_data)
        .map_err(|error| format!("The application data folder could not be created: {error}"))?;
    let root = secure_registry_directory(&app_data.join(REGISTRY_DIRECTORY))?;
    let registry = TemporaryRegistry { root };

    if let Some(existing) = REGISTRY.get() {
        if !paths_are_equal(&existing.root, &registry.root) {
            return Err(
                "The temporary-workspace registry was already initialised elsewhere.".to_string(),
            );
        }
    } else {
        REGISTRY.set(registry.clone()).map_err(|_| {
            "The temporary-workspace registry could not be initialised.".to_string()
        })?;
    }

    Ok(registry.cleanup())
}

#[tauri::command]
pub(crate) fn temporary_cleanup_status(
    status: tauri::State<'_, TemporaryCleanupStatus>,
) -> TemporaryCleanupStatus {
    status.inner().clone()
}

pub(crate) fn register_temporary_path(
    proposed_path: &Path,
    kind: TemporaryKind,
) -> Result<TemporaryLease, String> {
    let target_path = canonical_new_target(proposed_path)?;
    let owner_pid = std::process::id();
    validate_owned_name(&target_path, kind, owner_pid)?;

    let Some(registry) = REGISTRY.get() else {
        return Ok(TemporaryLease::unregistered(target_path, kind));
    };
    registry.register(target_path, kind, owner_pid)
}

impl TemporaryLease {
    fn unregistered(target_path: PathBuf, kind: TemporaryKind) -> Self {
        Self {
            target_path,
            kind,
            ownership_token: String::new(),
            record_path: None,
            lock_path: None,
            lock_file: None,
            remove_target_on_drop: true,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.target_path
    }

    pub(crate) fn write_directory_ownership_token(&self) -> Result<(), String> {
        if !is_directory_kind(self.kind) {
            return Err(
                "Only an app-owned temporary directory may have an ownership token.".to_string(),
            );
        }
        let token_path = self.target_path.join(DIRECTORY_TOKEN_FILE);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&token_path).map_err(|error| {
            format!("The temporary-directory ownership token could not be created: {error}")
        })?;
        file.write_all(self.ownership_token.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| {
                format!("The temporary-directory ownership token could not be completed: {error}")
            })
    }

    pub(crate) fn cancel_without_target_cleanup(&mut self) {
        self.remove_target_on_drop = false;
        self.remove_registry_files();
    }

    fn remove_registry_files(&mut self) {
        let record_removed = self
            .record_path
            .as_ref()
            .is_none_or(|path| remove_file_if_present(path).is_ok());
        if !record_removed {
            return;
        }
        self.record_path = None;
        if let Some(lock_file) = self.lock_file.take() {
            let _ = lock_file.unlock();
            drop(lock_file);
        }
        if let Some(lock_path) = self.lock_path.take() {
            let _ = remove_file_if_present(&lock_path);
        }
    }
}

impl Drop for TemporaryLease {
    fn drop(&mut self) {
        if !self.remove_target_on_drop {
            self.remove_registry_files();
            return;
        }
        if remove_live_target(&self.target_path, self.kind).is_ok() {
            self.remove_registry_files();
        }
    }
}

impl TemporaryRegistry {
    fn register(
        &self,
        target_path: PathBuf,
        kind: TemporaryKind,
        owner_pid: u32,
    ) -> Result<TemporaryLease, String> {
        let target_path = canonical_new_target(&target_path)?;
        validate_owned_name(&target_path, kind, owner_pid)?;
        for _ in 0..32 {
            let now = unix_time_millis();
            let sequence = LEASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let lease_id = format!("{owner_pid}-{now}-{sequence}");
            let lock_path = self.root.join(lock_file_name(&lease_id));
            let mut lock_options = OpenOptions::new();
            lock_options.create_new(true).read(true).write(true);
            #[cfg(unix)]
            lock_options.mode(0o600);
            let lock_file = match lock_options.open(&lock_path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "The temporary-workspace lease could not be created: {error}"
                    ));
                }
            };
            if let Err(error) = lock_file.try_lock() {
                drop(lock_file);
                let _ = remove_file_if_present(&lock_path);
                return Err(format!(
                    "The temporary-workspace lease could not be locked: {error}"
                ));
            }

            let record_path = self.root.join(record_file_name(&lease_id));
            let record = LeaseRecord {
                version: RECORD_VERSION,
                lease_id: lease_id.clone(),
                kind,
                target_path: target_path.to_string_lossy().into_owned(),
                owner_pid,
                created_at_ms: now,
                ownership_token: lease_id.clone(),
            };
            if let Err(error) = write_record(&record_path, &record) {
                let _ = lock_file.unlock();
                drop(lock_file);
                let _ = remove_file_if_present(&lock_path);
                return Err(error);
            }
            sync_directory(&self.root);
            return Ok(TemporaryLease {
                target_path,
                kind,
                ownership_token: lease_id,
                record_path: Some(record_path),
                lock_path: Some(lock_path),
                lock_file: Some(lock_file),
                remove_target_on_drop: true,
            });
        }
        Err("A unique temporary-workspace lease could not be allocated.".to_string())
    }

    fn cleanup(&self) -> TemporaryCleanupStatus {
        let mut status = TemporaryCleanupStatus {
            completed_at_ms: unix_time_millis(),
            scanner_capture_retention_days: SCANNER_CAPTURE_RETENTION_DAYS,
            ..TemporaryCleanupStatus::default()
        };
        let Ok(entries) = fs::read_dir(&self.root) else {
            status.errors += 1;
            return status;
        };
        let mut records = Vec::new();
        let mut locks = Vec::new();
        let mut record_ids = HashSet::new();
        for (index, entry) in entries.enumerate() {
            if index >= MAX_REGISTRY_ENTRIES {
                status.scan_limited = true;
                break;
            }
            let Ok(entry) = entry else {
                status.errors += 1;
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                status.rejected_leases += 1;
                continue;
            };
            if let Some(lease_id) = parse_record_file_name(&name) {
                record_ids.insert(lease_id.to_string());
                records.push(entry.path());
            } else if let Some(lease_id) = parse_lock_file_name(&name) {
                locks.push((lease_id.to_string(), entry.path()));
            }
        }

        for record_path in records {
            self.cleanup_record(&record_path, &mut status);
        }
        if !status.scan_limited {
            for (lease_id, lock_path) in locks {
                if !record_ids.contains(&lease_id) {
                    cleanup_orphan_lock(&lock_path, &mut status);
                }
            }
        }
        status.completed_at_ms = unix_time_millis();
        status
    }

    fn cleanup_record(&self, record_path: &Path, status: &mut TemporaryCleanupStatus) {
        status.inspected_leases += 1;
        let Some(file_name) = record_path.file_name().and_then(|value| value.to_str()) else {
            status.rejected_leases += 1;
            return;
        };
        let Some(lease_id) = parse_record_file_name(file_name) else {
            status.rejected_leases += 1;
            return;
        };
        let lock_path = self.root.join(lock_file_name(lease_id));
        let lock_file = match open_existing_lock(&lock_path) {
            Ok(file) => file,
            Err(_) => {
                status.rejected_leases += 1;
                return;
            }
        };
        match lock_file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                status.active_leases += 1;
                return;
            }
            Err(TryLockError::Error(_)) => {
                status.errors += 1;
                return;
            }
        }

        let record = match read_record(record_path) {
            Ok(record) => record,
            Err(_) => {
                status.rejected_leases += 1;
                finish_stale_lease(record_path, &lock_path, lock_file);
                return;
            }
        };
        if validate_record(&record, lease_id).is_err() {
            status.rejected_leases += 1;
            finish_stale_lease(record_path, &lock_path, lock_file);
            return;
        }
        let target_path = PathBuf::from(&record.target_path);
        match remove_stale_target(&target_path, &record) {
            Ok(RemovedTarget::File) => status.removed_files += 1,
            Ok(RemovedTarget::Directory) => status.removed_directories += 1,
            Ok(RemovedTarget::Missing) => status.missing_targets += 1,
            Err(_) => {
                status.errors += 1;
                let _ = lock_file.unlock();
                return;
            }
        }
        finish_stale_lease(record_path, &lock_path, lock_file);
    }
}

fn secure_registry_directory(path: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !is_link_or_reparse(&metadata) && metadata.is_dir() => {}
        Ok(_) => {
            return Err("The temporary-workspace registry is not a private directory.".to_string());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| {
                format!("The temporary-workspace registry could not be created: {error}")
            })?;
        }
        Err(error) => {
            return Err(format!(
                "The temporary-workspace registry could not be inspected: {error}"
            ));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!("The temporary-workspace registry permissions could not be secured: {error}")
    })?;
    fs::canonicalize(path)
        .map_err(|error| format!("The temporary-workspace registry could not be opened: {error}"))
}

fn canonical_new_target(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("The temporary-workspace path is empty.".to_string());
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| "The temporary-workspace filename is invalid.".to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "The temporary-workspace parent folder is invalid.".to_string())?;
    let canonical_parent = fs::canonicalize(parent).map_err(|error| {
        format!("The temporary-workspace parent folder could not be opened: {error}")
    })?;
    let target = canonical_parent.join(file_name);
    let encoded = target
        .to_str()
        .ok_or_else(|| "The temporary-workspace path is not valid Unicode.".to_string())?;
    if encoded.len() > MAX_PATH_BYTES || encoded.contains(['\r', '\n', '\0']) {
        return Err("The temporary-workspace path is outside the supported bounds.".to_string());
    }
    match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(target),
        Ok(_) => Err("The temporary-workspace target already exists. Try again.".to_string()),
        Err(error) => Err(format!(
            "The temporary-workspace target could not be inspected: {error}"
        )),
    }
}

fn validate_record(record: &LeaseRecord, lease_id: &str) -> Result<(), String> {
    if record.version != RECORD_VERSION
        || record.lease_id != lease_id
        || record.ownership_token != lease_id
        || record.target_path.len() > MAX_PATH_BYTES
        || record.target_path.contains(['\r', '\n', '\0'])
        || record.owner_pid == 0
        || record.created_at_ms == 0
        || record.created_at_ms > unix_time_millis().saturating_add(FUTURE_TIME_TOLERANCE_MS)
    {
        return Err("The temporary-workspace lease is invalid.".to_string());
    }
    let path = Path::new(&record.target_path);
    if !path.is_absolute() {
        return Err("The temporary-workspace target is not absolute.".to_string());
    }
    validate_parent_path(path)?;
    validate_owned_name(path, record.kind, record.owner_pid)
}

fn validate_parent_path(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "The temporary-workspace target has no parent.".to_string())?;
    let canonical = fs::canonicalize(parent)
        .map_err(|error| format!("The temporary-workspace parent could not be opened: {error}"))?;
    if !paths_are_equal(&canonical, parent) {
        return Err("The temporary-workspace parent path changed after registration.".to_string());
    }
    Ok(())
}

fn validate_owned_name(path: &Path, kind: TemporaryKind, owner_pid: u32) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The temporary-workspace filename is invalid.".to_string())?;
    let pid = owner_pid.to_string();
    let valid = match kind {
        TemporaryKind::OutputFile => {
            let Some(body) = name
                .strip_prefix('.')
                .and_then(|value| value.strip_suffix(".paperworks.tmp"))
            else {
                return Err("The temporary PDF filename is invalid.".to_string());
            };
            let mut parts = body.rsplitn(3, '.');
            let nonce = parts.next().unwrap_or_default();
            let embedded_pid = parts.next().unwrap_or_default();
            let original_name = parts.next().unwrap_or_default();
            !original_name.is_empty() && embedded_pid == pid && ascii_digits(nonce)
        }
        TemporaryKind::BatchDirectory => {
            validate_hyphenated_name(name, ".paperworks-batch-", &pid, None)
        }
        TemporaryKind::CertificateWorkspace => {
            validate_hyphenated_name(name, ".paperworks-certificate-", &pid, None)
        }
        TemporaryKind::CertificatePassfile => {
            validate_hyphenated_name(name, ".tufekci-paperworks-pyhk-", &pid, Some(".secret"))
        }
        TemporaryKind::PyHankoPasswordBridge => {
            validate_hyphenated_name(name, ".tufekci-paperworks-pyhk-bridge-", &pid, None)
        }
        TemporaryKind::OcrProgressPlugin => {
            validate_dotted_name(name, ".ocr-progress.", &pid, ".py")
        }
        TemporaryKind::OcrUserWords => validate_dotted_name(name, ".ocr-user-words.", &pid, ".txt"),
        TemporaryKind::ScanRaster => validate_dotted_name(name, ".scan-normalised.", &pid, ".png"),
    };
    if valid {
        Ok(())
    } else {
        Err("The temporary-workspace filename is not app-owned.".to_string())
    }
}

fn validate_hyphenated_name(name: &str, prefix: &str, pid: &str, suffix: Option<&str>) -> bool {
    let Some(body) = name.strip_prefix(prefix) else {
        return false;
    };
    let body = if let Some(suffix) = suffix {
        let Some(body) = body.strip_suffix(suffix) else {
            return false;
        };
        body
    } else {
        body
    };
    let mut parts = body.split('-');
    parts.next() == Some(pid)
        && parts.next().is_some_and(ascii_digits)
        && parts.next().is_some_and(ascii_digits)
        && parts.next().is_none()
}

fn validate_dotted_name(name: &str, prefix: &str, pid: &str, suffix: &str) -> bool {
    let Some(body) = name
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
    else {
        return false;
    };
    let mut parts = body.split('.');
    parts.next() == Some(pid) && parts.next().is_some_and(ascii_digits) && parts.next().is_none()
}

fn ascii_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn write_record(path: &Path, record: &LeaseRecord) -> Result<(), String> {
    let encoded = serde_json::to_vec(record)
        .map_err(|error| format!("The temporary-workspace lease could not be encoded: {error}"))?;
    if encoded.len() as u64 > MAX_RECORD_BYTES {
        return Err("The temporary-workspace lease is too large.".to_string());
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| {
        format!("The temporary-workspace lease record could not be created: {error}")
    })?;
    if let Err(error) = file.write_all(&encoded).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = remove_file_if_present(path);
        return Err(format!(
            "The temporary-workspace lease record could not be completed: {error}"
        ));
    }
    Ok(())
}

fn read_record(path: &Path) -> Result<LeaseRecord, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!("The temporary-workspace lease could not be inspected: {error}")
    })?;
    if is_link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err("The temporary-workspace lease record is unsafe.".to_string());
    }
    let file = File::open(path)
        .map_err(|error| format!("The temporary-workspace lease could not be opened: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("The temporary-workspace lease could not be read: {error}"))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("The temporary-workspace lease record is too large.".to_string());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("The temporary-workspace lease is malformed: {error}"))
}

fn remove_stale_target(path: &Path, record: &LeaseRecord) -> Result<RemovedTarget, String> {
    validate_parent_path(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RemovedTarget::Missing);
        }
        Err(error) => {
            return Err(format!(
                "The stale temporary-workspace target could not be inspected: {error}"
            ));
        }
    };
    if is_link_or_reparse(&metadata) {
        return Err("A stale temporary-workspace target is a link or reparse point.".to_string());
    }
    if is_directory_kind(record.kind) {
        if !metadata.is_dir() {
            return Err("The stale temporary-workspace target is not a directory.".to_string());
        }
        verify_directory_ownership(path, &record.ownership_token)?;
        fs::remove_dir_all(path).map_err(|error| {
            format!("The stale temporary-workspace directory could not be removed: {error}")
        })?;
        Ok(RemovedTarget::Directory)
    } else {
        if !metadata.is_file() {
            return Err("The stale temporary-workspace target is not a file.".to_string());
        }
        fs::remove_file(path).map_err(|error| {
            format!("The stale temporary-workspace file could not be removed: {error}")
        })?;
        Ok(RemovedTarget::File)
    }
}

fn verify_directory_ownership(path: &Path, expected: &str) -> Result<(), String> {
    let token_path = path.join(DIRECTORY_TOKEN_FILE);
    let metadata = match fs::symlink_metadata(&token_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut entries = fs::read_dir(path).map_err(|error| {
                format!("The stale temporary-workspace directory could not be inspected: {error}")
            })?;
            if entries.next().is_none() {
                return Ok(());
            }
            return Err(
                "The stale temporary-workspace directory has no ownership token.".to_string(),
            );
        }
        Err(error) => {
            return Err(format!(
                "The stale temporary-directory ownership token could not be inspected: {error}"
            ));
        }
    };
    if is_link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > 128 {
        return Err("The stale temporary-directory ownership token is unsafe.".to_string());
    }
    let token = fs::read_to_string(&token_path).map_err(|error| {
        format!("The stale temporary-directory ownership token could not be read: {error}")
    })?;
    if token != expected {
        return Err("The stale temporary-directory ownership token does not match.".to_string());
    }
    Ok(())
}

fn remove_live_target(path: &Path, kind: TemporaryKind) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "The temporary-workspace target could not be inspected: {error}"
            ));
        }
    };
    if is_link_or_reparse(&metadata) {
        return Err("The temporary-workspace target is a link or reparse point.".to_string());
    }
    if is_directory_kind(kind) {
        if !metadata.is_dir() {
            return Err("The temporary-workspace target is not a directory.".to_string());
        }
        fs::remove_dir_all(path).map_err(|error| {
            format!("The temporary-workspace directory could not be removed: {error}")
        })
    } else {
        if !metadata.is_file() {
            return Err("The temporary-workspace target is not a file.".to_string());
        }
        fs::remove_file(path)
            .map_err(|error| format!("The temporary-workspace file could not be removed: {error}"))
    }
}

fn finish_stale_lease(record_path: &Path, lock_path: &Path, lock_file: File) {
    let record_removed = remove_file_if_present(record_path).is_ok();
    let _ = lock_file.unlock();
    drop(lock_file);
    if record_removed {
        let _ = remove_file_if_present(lock_path);
    }
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn record_file_name(lease_id: &str) -> String {
    format!("lease-{lease_id}.json")
}

fn lock_file_name(lease_id: &str) -> String {
    format!("lease-{lease_id}.lock")
}

fn parse_record_file_name(name: &str) -> Option<&str> {
    let lease_id = name.strip_prefix("lease-")?.strip_suffix(".json")?;
    valid_lease_id(lease_id).then_some(lease_id)
}

fn parse_lock_file_name(name: &str) -> Option<&str> {
    let lease_id = name.strip_prefix("lease-")?.strip_suffix(".lock")?;
    valid_lease_id(lease_id).then_some(lease_id)
}

fn valid_lease_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LEASE_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
        && value.split('-').count() == 3
        && value.split('-').all(ascii_digits)
}

fn open_existing_lock(path: &Path) -> io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The lease lock is not a regular file.",
        ));
    }
    OpenOptions::new().read(true).write(true).open(path)
}

fn cleanup_orphan_lock(path: &Path, status: &mut TemporaryCleanupStatus) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => {
            status.errors += 1;
            return;
        }
    };
    let old_enough = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .and_then(|modified| unix_time_millis().checked_sub(modified.as_millis().try_into().ok()?))
        .is_some_and(|age| age >= ORPHAN_LOCK_RETENTION_MS);
    if !old_enough {
        return;
    }
    let lock_file = match open_existing_lock(path) {
        Ok(file) => file,
        Err(_) => {
            status.rejected_leases += 1;
            return;
        }
    };
    match lock_file.try_lock() {
        Ok(()) => {
            let _ = lock_file.unlock();
            drop(lock_file);
            if remove_file_if_present(path).is_ok() {
                status.removed_orphan_locks += 1;
            } else {
                status.errors += 1;
            }
        }
        Err(TryLockError::WouldBlock) => {}
        Err(TryLockError::Error(_)) => status.errors += 1,
    }
}

fn unix_time_millis() -> u64 {
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn skips_a_live_locked_lease_then_cleans_it_on_drop() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let target = temporary_output_path(&directory.path);
        let lease = registry
            .register(
                target.clone(),
                TemporaryKind::OutputFile,
                std::process::id(),
            )
            .unwrap();
        fs::write(&target, b"temporary PDF").unwrap();

        let status = registry.cleanup();
        assert_eq!(status.active_leases, 1);
        assert!(target.exists());

        drop(lease);
        assert!(!target.exists());
        assert_eq!(lease_record_count(&registry.root), 0);
    }

    #[test]
    fn removes_an_unlocked_file_after_a_simulated_crash() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let target = temporary_output_path(&directory.path);
        let mut lease = registry
            .register(
                target.clone(),
                TemporaryKind::OutputFile,
                std::process::id(),
            )
            .unwrap();
        fs::write(&target, b"temporary PDF").unwrap();
        lease.remove_target_on_drop = false;
        lease.lock_file.take();
        std::mem::forget(lease);

        let status = registry.cleanup();
        assert_eq!(status.removed_files, 1);
        assert!(!target.exists());
        assert_eq!(lease_record_count(&registry.root), 0);
    }

    #[test]
    fn removes_only_a_batch_directory_with_its_matching_token() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let target = directory
            .path
            .join(format!(".paperworks-batch-{}-123-0", std::process::id()));
        let mut lease = registry
            .register(
                target.clone(),
                TemporaryKind::BatchDirectory,
                std::process::id(),
            )
            .unwrap();
        fs::create_dir(&target).unwrap();
        lease.write_directory_ownership_token().unwrap();
        fs::write(target.join("prepared.pdf"), b"temporary PDF").unwrap();
        lease.remove_target_on_drop = false;
        lease.lock_file.take();
        std::mem::forget(lease);

        let status = registry.cleanup();
        assert_eq!(status.removed_directories, 1);
        assert!(!target.exists());
    }

    #[test]
    fn removes_a_password_bridge_directory_after_a_simulated_crash() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let target = directory.path.join(format!(
            ".tufekci-paperworks-pyhk-bridge-{}-321-0",
            std::process::id()
        ));
        let mut lease = registry
            .register(
                target.clone(),
                TemporaryKind::PyHankoPasswordBridge,
                std::process::id(),
            )
            .unwrap();
        fs::create_dir(&target).unwrap();
        lease.write_directory_ownership_token().unwrap();
        fs::write(target.join("sitecustomize.py"), b"import getpass").unwrap();
        lease.remove_target_on_drop = false;
        lease.lock_file.take();
        std::mem::forget(lease);

        let status = registry.cleanup();
        assert_eq!(status.removed_directories, 1);
        assert!(!target.exists());
    }

    #[test]
    fn rejects_a_batch_directory_with_a_changed_token() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let target = directory
            .path
            .join(format!(".paperworks-batch-{}-456-0", std::process::id()));
        let mut lease = registry
            .register(
                target.clone(),
                TemporaryKind::BatchDirectory,
                std::process::id(),
            )
            .unwrap();
        fs::create_dir(&target).unwrap();
        lease.write_directory_ownership_token().unwrap();
        fs::write(target.join(DIRECTORY_TOKEN_FILE), b"changed").unwrap();
        fs::write(target.join("prepared.pdf"), b"temporary PDF").unwrap();
        lease.remove_target_on_drop = false;
        lease.lock_file.take();
        std::mem::forget(lease);

        let status = registry.cleanup();
        assert_eq!(status.errors, 1);
        assert!(target.exists());
    }

    #[test]
    fn rejects_a_forged_scanner_capture_lease() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let capture = directory.path.join("capture-123-456-1");
        fs::create_dir(&capture).unwrap();
        fs::write(capture.join("page-1.png"), b"image").unwrap();
        let lease_id = format!("{}-123-1", std::process::id());
        let lock_path = registry.root.join(lock_file_name(&lease_id));
        File::create(&lock_path).unwrap();
        let record = LeaseRecord {
            version: RECORD_VERSION,
            lease_id: lease_id.clone(),
            kind: TemporaryKind::BatchDirectory,
            target_path: capture.to_string_lossy().into_owned(),
            owner_pid: std::process::id(),
            created_at_ms: unix_time_millis(),
            ownership_token: lease_id.clone(),
        };
        write_record(&registry.root.join(record_file_name(&lease_id)), &record).unwrap();

        let status = registry.cleanup();
        assert_eq!(status.rejected_leases, 1);
        assert!(capture.exists());
    }

    #[test]
    fn rejects_unknown_fields_without_touching_the_target() {
        let directory = TestDirectory::new();
        let registry = directory.registry();
        let target = temporary_output_path(&directory.path);
        fs::write(&target, b"keep").unwrap();
        let lease_id = format!("{}-789-1", std::process::id());
        File::create(registry.root.join(lock_file_name(&lease_id))).unwrap();
        let record_path = registry.root.join(record_file_name(&lease_id));
        fs::write(
            &record_path,
            format!(
                r#"{{"version":1,"leaseId":"{lease_id}","kind":"output-file","targetPath":"{}","ownerPid":{},"createdAtMs":{},"ownershipToken":"{lease_id}","password":"secret"}}"#,
                target.to_string_lossy().replace('\\', "\\\\"),
                std::process::id(),
                unix_time_millis()
            ),
        )
        .unwrap();

        let status = registry.cleanup();
        assert_eq!(status.rejected_leases, 1);
        assert_eq!(fs::read(&target).unwrap(), b"keep");
    }

    #[test]
    fn accepts_only_exact_app_owned_temporary_names() {
        let directory = TestDirectory::new();
        let pid = std::process::id();
        assert!(validate_owned_name(
            &directory
                .path
                .join(format!(".document.pdf.{pid}.123.paperworks.tmp")),
            TemporaryKind::OutputFile,
            pid
        )
        .is_ok());
        assert!(validate_owned_name(
            &directory
                .path
                .join(format!(".tufekci-paperworks-pyhk-{pid}-123-0.secret")),
            TemporaryKind::CertificatePassfile,
            pid
        )
        .is_ok());
        assert!(validate_owned_name(
            &directory
                .path
                .join(format!(".tufekci-paperworks-pyhk-bridge-{pid}-123-0")),
            TemporaryKind::PyHankoPasswordBridge,
            pid
        )
        .is_ok());
        assert!(validate_owned_name(
            &directory
                .path
                .join(format!(".paperworks-certificate-{pid}-123-0")),
            TemporaryKind::CertificateWorkspace,
            pid
        )
        .is_ok());
        assert!(validate_owned_name(
            &directory.path.join("capture-123-456-1"),
            TemporaryKind::BatchDirectory,
            pid
        )
        .is_err());
        assert!(validate_owned_name(
            &directory
                .path
                .join(format!(".ocr-user-words.{pid}.123.txt.exe")),
            TemporaryKind::OcrUserWords,
            pid
        )
        .is_err());
        assert!(validate_owned_name(
            &directory.path.join(format!(".ocr-progress.{pid}.123.py")),
            TemporaryKind::OcrProgressPlugin,
            pid
        )
        .is_ok());
    }

    fn temporary_output_path(parent: &Path) -> PathBuf {
        parent.join(format!(
            ".document.pdf.{}.123.paperworks.tmp",
            std::process::id()
        ))
    }

    fn lease_record_count(root: &Path) -> usize {
        fs::read_dir(root)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| parse_record_file_name(name).is_some())
            })
            .count()
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let nonce = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tufekci-paperworks-temporary-cleanup-test-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn registry(&self) -> TemporaryRegistry {
            let root = self.path.join("registry");
            fs::create_dir(&root).unwrap();
            TemporaryRegistry {
                root: fs::canonicalize(root).unwrap(),
            }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
