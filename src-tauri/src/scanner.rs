use crate::child_process::ManagedChild;
use crate::file_safety::reject_control_characters;
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
#[cfg(target_os = "linux")]
use std::fs::File;
use std::io::Read;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Manager;

const MAX_DEVICE_ID_BYTES: usize = 4 * 1024;
const MAX_CAPTURE_PAGES: u16 = 200;
const MAX_CAPTURE_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ADAPTER_OUTPUT_BYTES: usize = 1024 * 1024;
const CAPTURE_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const CAPTURE_PREFIX: &str = "capture-";
#[cfg(any(target_os = "linux", target_os = "windows"))]
const SCANNER_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(any(target_os = "linux", target_os = "windows"))]
const SCANNER_CAPTURE_TIMEOUT: Duration = Duration::from_secs(30 * 60 + 10);
#[cfg(target_os = "macos")]
const IMAGE_CAPTURE_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(50);
#[cfg(target_os = "macos")]
const IMAGE_CAPTURE_CAPTURE_TIMEOUT: Duration = Duration::from_secs(30 * 60 + 10);
#[cfg(any(target_os = "macos", test))]
const IMAGE_CAPTURE_PROTOCOL_VERSION: u16 = 1;
#[cfg(target_os = "macos")]
const IMAGE_CAPTURE_HELPER_NAME: &str = "tufekci-paperworks-scanner";
static CAPTURE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
pub enum ScannerBackend {
    Wia,
    ImageCapture,
    Sane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
pub enum ScannerDiscoveryStatus {
    BackendUnavailable,
    DevicesFound,
    DiscoveryFailed,
    NoDevices,
    UnsupportedPlatform,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScannerSource {
    Flatbed,
    Feeder,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScannerColourMode {
    Colour,
    Greyscale,
    Monochrome,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannerDevice {
    id: String,
    name: String,
    manufacturer: String,
    model: String,
    backend: ScannerBackend,
    flatbed: bool,
    feeder: bool,
    duplex: bool,
    supported_dpi: Vec<u16>,
    colour_modes: Vec<ScannerColourMode>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannerDiscovery {
    backend: ScannerBackend,
    backend_name: &'static str,
    available: bool,
    status: ScannerDiscoveryStatus,
    detail: String,
    devices: Vec<ScannerDevice>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CaptureScannerPagesRequest {
    pub(crate) device_id: String,
    pub(crate) source: ScannerSource,
    pub(crate) duplex: bool,
    pub(crate) dpi: u16,
    pub(crate) colour_mode: ScannerColourMode,
    pub(crate) paper_width_mm: f64,
    pub(crate) paper_height_mm: f64,
    pub(crate) page_limit: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannerCaptureResult {
    pub(crate) capture_id: String,
    pub(crate) page_count: usize,
    pub(crate) paths: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

struct AdapterCaptureResult {
    paths: Vec<PathBuf>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CaptureOutputFingerprint {
    path: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
}

struct CaptureWorkspace {
    path: PathBuf,
    keep: bool,
}

impl CaptureWorkspace {
    fn new(path: PathBuf) -> Self {
        Self { path, keep: false }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for CaptureWorkspace {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[tauri::command]
pub async fn list_scanners() -> Result<ScannerDiscovery, String> {
    tauri::async_runtime::spawn_blocking(discover_platform_scanners)
        .await
        .map_err(|error| format!("The scanner discovery task could not complete: {error}"))?
}

pub(crate) fn run_scanner_capture_job_with_control(
    request: CaptureScannerPagesRequest,
    root: &Path,
    control: &PdfJobExecutionControl,
) -> Result<ScannerCaptureResult, String> {
    capture_scanner_pages_with_control(request, root, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_scanner_capture_job_error(&error)
        }
    })
}

fn capture_scanner_pages_with_control(
    request: CaptureScannerPagesRequest,
    root: &Path,
    control: &PdfJobExecutionControl,
) -> Result<ScannerCaptureResult, String> {
    capture_scanner_pages_with_control_and_adapter(request, root, control, capture_platform_scanner)
}

fn capture_scanner_pages_with_control_and_adapter<F>(
    request: CaptureScannerPagesRequest,
    root: &Path,
    control: &PdfJobExecutionControl,
    adapter: F,
) -> Result<ScannerCaptureResult, String>
where
    F: FnOnce(
        &CaptureScannerPagesRequest,
        &Path,
        &PdfJobExecutionControl,
    ) -> Result<AdapterCaptureResult, String>,
{
    control.checkpoint(2, "Checking connected-scanner settings")?;
    validate_capture_request(&request)?;
    control.checkpoint(6, "Preparing a private capture workspace")?;
    cleanup_old_captures(root);
    let (capture_id, directory) = create_capture_directory(root)?;
    let mut workspace = CaptureWorkspace::new(directory);
    control.checkpoint(14, "Connecting to the selected scanner")?;
    let captured = adapter(&request, workspace.path(), control)?;
    control.checkpoint(88, "Checking the captured page count")?;
    if captured.paths.len() > usize::from(request.page_limit) {
        return Err("The scanner returned more pages than the requested limit.".to_string());
    }
    let paths = validate_capture_outputs(workspace.path(), captured.paths)?;
    let fingerprints = capture_output_fingerprints(&paths)?;
    control.checkpoint(96, "Rechecking the captured pages")?;
    verify_capture_output_fingerprints(&fingerprints)?;
    control.checkpoint(99, "Finalising the scanner capture")?;
    workspace.keep();
    Ok(ScannerCaptureResult {
        capture_id,
        page_count: paths.len(),
        paths: paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        warnings: captured.warnings,
    })
}

pub(crate) fn validate_capture_request(request: &CaptureScannerPagesRequest) -> Result<(), String> {
    reject_control_characters("Scanner device ID", &request.device_id)?;
    if request.device_id.is_empty() || request.device_id.len() > MAX_DEVICE_ID_BYTES {
        return Err("Choose a valid connected scanner.".to_string());
    }
    if request.duplex && request.source != ScannerSource::Feeder {
        return Err("Duplex scanning is available only with a document feeder.".to_string());
    }
    if !(75..=1_200).contains(&request.dpi) {
        return Err("Scanner resolution must be between 75 and 1,200 DPI.".to_string());
    }
    for (label, value) in [
        ("Paper width", request.paper_width_mm),
        ("Paper height", request.paper_height_mm),
    ] {
        if !value.is_finite() || !(10.0..=500.0).contains(&value) {
            return Err(format!("{label} is outside the supported scanner range."));
        }
    }
    if request.page_limit == 0 || request.page_limit > MAX_CAPTURE_PAGES {
        return Err(format!(
            "A scanner capture may contain between 1 and {MAX_CAPTURE_PAGES} pages."
        ));
    }
    if request.source == ScannerSource::Flatbed && request.page_limit != 1 {
        return Err("A flatbed capture produces one page at a time.".to_string());
    }
    Ok(())
}

pub(crate) fn initialise_scanner_capture_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("The application data folder is unavailable: {error}"))?
        .join("scanner-captures");
    fs::create_dir_all(&root)
        .map_err(|error| format!("The scanner capture folder could not be created: {error}"))?;
    fs::canonicalize(&root)
        .map_err(|error| format!("The scanner capture folder could not be opened: {error}"))
}

fn create_capture_directory(root: &Path) -> Result<(String, PathBuf), String> {
    let now = unix_time_millis();
    for _ in 0..100 {
        let sequence = CAPTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let capture_id = format!("{CAPTURE_PREFIX}{now}-{}-{sequence}", std::process::id());
        let directory = root.join(&capture_id);
        match fs::create_dir(&directory) {
            Ok(()) => return Ok((capture_id, directory)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "A scanner capture workspace could not be created: {error}"
                ));
            }
        }
    }
    Err("A unique scanner capture workspace could not be created.".to_string())
}

fn validate_capture_outputs(directory: &Path, paths: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() || paths.len() > usize::from(MAX_CAPTURE_PAGES) {
        return Err("The scanner did not return a valid page set.".to_string());
    }
    let canonical_directory = fs::canonicalize(directory)
        .map_err(|error| format!("The scanner capture folder could not be verified: {error}"))?;
    let mut verified = Vec::with_capacity(paths.len());
    let mut seen = HashSet::new();
    for path in paths {
        let canonical = fs::canonicalize(&path)
            .map_err(|error| format!("A captured scanner page could not be opened: {error}"))?;
        if canonical.parent() != Some(canonical_directory.as_path())
            || !seen.insert(canonical.clone())
        {
            return Err("A scanner returned an invalid or repeated capture path.".to_string());
        }
        let supported_extension = canonical
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                [
                    "bmp", "jpeg", "jpg", "pbm", "pgm", "png", "pnm", "ppm", "tif", "tiff",
                ]
                .iter()
                .any(|supported| value.eq_ignore_ascii_case(supported))
            });
        let metadata = fs::metadata(&canonical)
            .map_err(|error| format!("A captured scanner page could not be inspected: {error}"))?;
        if !supported_extension
            || !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_CAPTURE_FILE_BYTES
        {
            return Err("A scanner returned an unsupported or unsafe image file.".to_string());
        }
        verified.push(canonical);
    }
    Ok(verified)
}

fn capture_output_fingerprints(paths: &[PathBuf]) -> Result<Vec<CaptureOutputFingerprint>, String> {
    paths
        .iter()
        .map(|path| {
            let metadata = fs::metadata(path).map_err(|error| {
                format!("A captured scanner page could not be inspected: {error}")
            })?;
            Ok(CaptureOutputFingerprint {
                path: path.clone(),
                bytes: metadata.len(),
                modified: metadata.modified().ok(),
            })
        })
        .collect()
}

fn verify_capture_output_fingerprints(
    fingerprints: &[CaptureOutputFingerprint],
) -> Result<(), String> {
    for expected in fingerprints {
        let current = capture_output_fingerprints(std::slice::from_ref(&expected.path))?
            .pop()
            .expect("one capture fingerprint was requested");
        if current != *expected {
            return Err(
                "A captured scanner page changed before it could be returned safely.".to_string(),
            );
        }
    }
    Ok(())
}

fn safe_scanner_capture_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed before it could be returned") {
        return "A captured scanner page changed before delivery. Capture the pages again."
            .to_string();
    }
    if normalised.contains("timeout") || normalised.contains("exceeded") {
        return "Connected scanner capture exceeded its safety timeout and was stopped."
            .to_string();
    }
    if normalised.contains("not supported") {
        return "Connected scanner capture is not supported on this operating system.".to_string();
    }
    if normalised.contains("wia") {
        return "Windows WIA could not complete the scanner capture. Check the device, driver, source and paper, then try again."
            .to_string();
    }
    if normalised.contains("image capture") {
        return "macOS Image Capture could not complete the scanner capture. Check the device, source and paper, then try again."
            .to_string();
    }
    if normalised.contains("sane") || normalised.contains("scanimage") {
        return "Linux SANE could not complete the scanner capture. Check the device, backend, source and paper, then try again."
            .to_string();
    }
    "Connected scanner capture could not complete safely. Check the scanner, source, paper and page limit, then try again."
        .to_string()
}

fn cleanup_old_captures(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let old_enough = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > CAPTURE_RETENTION);
        if file_type.is_dir()
            && !file_type.is_symlink()
            && name.starts_with(CAPTURE_PREFIX)
            && old_enough
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(target_os = "windows")]
fn discover_platform_scanners() -> Result<ScannerDiscovery, String> {
    discover_wia_scanners()
}

#[cfg(target_os = "linux")]
fn discover_platform_scanners() -> Result<ScannerDiscovery, String> {
    discover_sane_scanners()
}

#[cfg(target_os = "macos")]
fn discover_platform_scanners() -> Result<ScannerDiscovery, String> {
    discover_image_capture_scanners()
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn discover_platform_scanners() -> Result<ScannerDiscovery, String> {
    Ok(ScannerDiscovery {
        backend: ScannerBackend::Sane,
        backend_name: "Connected scanner",
        available: false,
        status: ScannerDiscoveryStatus::UnsupportedPlatform,
        detail: "Connected scanning is not supported on this operating system.".to_string(),
        devices: Vec::new(),
    })
}

#[cfg(target_os = "windows")]
fn capture_platform_scanner(
    request: &CaptureScannerPagesRequest,
    directory: &Path,
    control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    capture_wia_scanner(request, directory, control)
}

#[cfg(target_os = "linux")]
fn capture_platform_scanner(
    request: &CaptureScannerPagesRequest,
    directory: &Path,
    control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    capture_sane_scanner(request, directory, control)
}

#[cfg(target_os = "macos")]
fn capture_platform_scanner(
    request: &CaptureScannerPagesRequest,
    directory: &Path,
    control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    capture_image_capture_scanner(request, directory, control)
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn capture_platform_scanner(
    _request: &CaptureScannerPagesRequest,
    _directory: &Path,
    _control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    Err("Connected scanning is not supported on this operating system.".to_string())
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ImageCaptureDiscoveryReport {
    protocol_version: u16,
    devices: Vec<ImageCaptureDeviceRecord>,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ImageCaptureDeviceRecord {
    id: String,
    name: String,
    manufacturer: String,
    model: String,
    flatbed: bool,
    feeder: bool,
    duplex: bool,
    supported_dpi: Vec<u16>,
    colour_modes: Vec<ScannerColourMode>,
    detail: Option<String>,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ImageCaptureCaptureReport {
    protocol_version: u16,
    paths: Vec<PathBuf>,
    warnings: Vec<String>,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageCaptureCaptureRequest<'a> {
    protocol_version: u16,
    device_id: &'a str,
    source: ScannerSource,
    duplex: bool,
    dpi: u16,
    colour_mode: ScannerColourMode,
    paper_width_mm: f64,
    paper_height_mm: f64,
    page_limit: u16,
    output_directory: &'a str,
}

#[cfg(target_os = "macos")]
fn discover_image_capture_scanners() -> Result<ScannerDiscovery, String> {
    let helper = match image_capture_bridge_path() {
        Ok(path) => path,
        Err(error) => {
            return Ok(ScannerDiscovery {
                backend: ScannerBackend::ImageCapture,
                backend_name: "macOS Image Capture",
                available: false,
                status: ScannerDiscoveryStatus::BackendUnavailable,
                detail: error,
                devices: Vec::new(),
            });
        }
    };
    let output =
        match run_image_capture_bridge(&helper, "discover", &[], IMAGE_CAPTURE_DISCOVERY_TIMEOUT) {
            Ok(output) => output,
            Err(error) => {
                return Ok(ScannerDiscovery {
                    backend: ScannerBackend::ImageCapture,
                    backend_name: "macOS Image Capture",
                    available: true,
                    status: ScannerDiscoveryStatus::DiscoveryFailed,
                    detail: format!("Image Capture scanner discovery failed: {error}"),
                    devices: Vec::new(),
                });
            }
        };
    let report = parse_image_capture_discovery(&output)?;
    let devices = report
        .devices
        .into_iter()
        .filter_map(image_capture_device_from_record)
        .collect::<Vec<_>>();
    Ok(ScannerDiscovery {
        backend: ScannerBackend::ImageCapture,
        backend_name: "macOS Image Capture",
        available: true,
        status: if devices.is_empty() {
            ScannerDiscoveryStatus::NoDevices
        } else {
            ScannerDiscoveryStatus::DevicesFound
        },
        detail: if devices.is_empty() {
            "Image Capture is available, but no connected scanners were found.".to_string()
        } else {
            format!(
                "{} connected scanner{} found through Image Capture.",
                devices.len(),
                if devices.len() == 1 { "" } else { "s" }
            )
        },
        devices,
    })
}

#[cfg(target_os = "macos")]
fn capture_image_capture_scanner(
    request: &CaptureScannerPagesRequest,
    directory: &Path,
    control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    control.checkpoint(18, "Configuring macOS Image Capture")?;
    let helper = image_capture_bridge_path()?;
    let output_directory = directory
        .to_str()
        .ok_or_else(|| "The scanner capture path is not valid Unicode.".to_string())?;
    let helper_request = ImageCaptureCaptureRequest {
        protocol_version: IMAGE_CAPTURE_PROTOCOL_VERSION,
        device_id: &request.device_id,
        source: request.source,
        duplex: request.duplex,
        dpi: request.dpi,
        colour_mode: request.colour_mode,
        paper_width_mm: request.paper_width_mm,
        paper_height_mm: request.paper_height_mm,
        page_limit: request.page_limit,
        output_directory,
    };
    let input = serde_json::to_vec(&helper_request)
        .map_err(|error| format!("The Image Capture request could not be encoded: {error}"))?;
    control.checkpoint(24, "Waiting for macOS Image Capture pages")?;
    let output = run_image_capture_bridge_with_control(
        &helper,
        "capture",
        &input,
        IMAGE_CAPTURE_CAPTURE_TIMEOUT,
        control,
    )?;
    let report = parse_image_capture_capture(&output)?;
    if report.paths.len() > usize::from(request.page_limit) {
        return Err("Image Capture returned more pages than the requested limit.".to_string());
    }
    let warnings = report
        .warnings
        .into_iter()
        .filter(|warning| {
            !warning.is_empty()
                && warning.len() <= 4 * 1024
                && reject_control_characters("Image Capture warning", warning).is_ok()
        })
        .collect();
    Ok(AdapterCaptureResult {
        paths: report.paths,
        warnings,
    })
}

#[cfg(any(target_os = "macos", test))]
fn parse_image_capture_discovery(bytes: &[u8]) -> Result<ImageCaptureDiscoveryReport, String> {
    let report: ImageCaptureDiscoveryReport = serde_json::from_slice(bytes)
        .map_err(|error| format!("Image Capture returned an invalid device list: {error}"))?;
    if report.protocol_version != IMAGE_CAPTURE_PROTOCOL_VERSION {
        return Err("The packaged Image Capture bridge uses an unsupported protocol.".to_string());
    }
    Ok(report)
}

#[cfg(any(target_os = "macos", test))]
fn parse_image_capture_capture(bytes: &[u8]) -> Result<ImageCaptureCaptureReport, String> {
    let report: ImageCaptureCaptureReport = serde_json::from_slice(bytes)
        .map_err(|error| format!("Image Capture returned an invalid capture report: {error}"))?;
    if report.protocol_version != IMAGE_CAPTURE_PROTOCOL_VERSION {
        return Err("The packaged Image Capture bridge uses an unsupported protocol.".to_string());
    }
    Ok(report)
}

#[cfg(any(target_os = "macos", test))]
fn image_capture_device_from_record(record: ImageCaptureDeviceRecord) -> Option<ScannerDevice> {
    let valid_text = |label: &str, value: &str, maximum: usize| {
        value.len() <= maximum && reject_control_characters(label, value).is_ok()
    };
    if record.id.is_empty()
        || record.id.len() > MAX_DEVICE_ID_BYTES
        || !valid_text("Image Capture device ID", &record.id, MAX_DEVICE_ID_BYTES)
        || !valid_text("Image Capture device name", &record.name, 1024)
        || !valid_text("Image Capture manufacturer", &record.manufacturer, 1024)
        || !valid_text("Image Capture model", &record.model, 1024)
        || record
            .detail
            .as_deref()
            .is_some_and(|detail| !valid_text("Image Capture detail", detail, 4 * 1024))
    {
        return None;
    }
    let name = if record.name.trim().is_empty() {
        "Image Capture scanner".to_string()
    } else {
        record.name
    };
    let model = if record.model.trim().is_empty() {
        name.clone()
    } else {
        record.model
    };
    Some(ScannerDevice {
        id: record.id,
        name,
        manufacturer: record.manufacturer,
        model,
        backend: ScannerBackend::ImageCapture,
        flatbed: record.flatbed,
        feeder: record.feeder,
        duplex: record.duplex && record.feeder,
        supported_dpi: normalise_dpi(record.supported_dpi),
        colour_modes: normalise_colour_modes(record.colour_modes),
        detail: record.detail.filter(|detail| !detail.trim().is_empty()),
    })
}

#[cfg(target_os = "macos")]
fn image_capture_bridge_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("The application executable could not be located: {error}"))?;
    let executable_directory = executable
        .parent()
        .ok_or_else(|| "The application executable folder is unavailable.".to_string())?;
    let bundled_candidates = image_capture_bridge_names()
        .into_iter()
        .map(|name| executable_directory.join(name))
        .collect::<Vec<_>>();
    #[cfg(debug_assertions)]
    let candidates = {
        let mut candidates = bundled_candidates;
        let source_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        candidates.extend(
            image_capture_bridge_names()
                .into_iter()
                .map(|name| source_directory.join(name)),
        );
        candidates
    };
    #[cfg(not(debug_assertions))]
    let candidates = bundled_candidates;
    for candidate in candidates {
        let Ok(metadata) = fs::symlink_metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                continue;
            }
        }
        return fs::canonicalize(&candidate).map_err(|error| {
            format!("The packaged Image Capture bridge could not be opened: {error}")
        });
    }
    Err("The packaged macOS Image Capture scanner bridge is missing or is not executable. Rebuild the macOS application bundle.".to_string())
}

#[cfg(target_os = "macos")]
fn image_capture_bridge_names() -> Vec<String> {
    let architecture = match std::env::consts::ARCH {
        "aarch64" => "aarch64-apple-darwin",
        "x86_64" => "x86_64-apple-darwin",
        _ => "universal-apple-darwin",
    };
    vec![
        IMAGE_CAPTURE_HELPER_NAME.to_string(),
        format!("{IMAGE_CAPTURE_HELPER_NAME}-{architecture}"),
        format!("{IMAGE_CAPTURE_HELPER_NAME}-universal-apple-darwin"),
    ]
}

#[cfg(target_os = "macos")]
fn run_image_capture_bridge(
    helper: &Path,
    operation: &'static str,
    input: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    run_image_capture_bridge_with_control(
        helper,
        operation,
        input,
        timeout,
        &PdfJobExecutionControl::direct(),
    )
}

#[cfg(target_os = "macos")]
fn run_image_capture_bridge_with_control(
    helper: &Path,
    operation: &'static str,
    input: &[u8],
    timeout: Duration,
    control: &PdfJobExecutionControl,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new(helper);
    command
        .arg(operation)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(&mut command)
        .map_err(|error| format!("The Image Capture bridge could not start: {error}"))?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| "The Image Capture result pipe could not be opened.".to_string())?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| "The Image Capture diagnostic pipe could not be opened.".to_string())?;
    let stdout_thread = std::thread::spawn(move || read_bounded_adapter_pipe(stdout));
    let stderr_thread = std::thread::spawn(move || read_bounded_adapter_pipe(stderr));
    if let Some(mut stdin) = child.take_stdin() {
        if let Err(error) = stdin.write_all(input) {
            let _ = child.terminate_tree();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(format!(
                "The Image Capture bridge could not receive its request: {error}"
            ));
        }
    }

    let status = wait_for_scanner_process(&mut child, timeout, "Image Capture", control);
    let stdout = stdout_thread
        .join()
        .map_err(|_| "The Image Capture result reader stopped unexpectedly.".to_string())?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| "The Image Capture diagnostic reader stopped unexpectedly.".to_string())?;
    if matches!(status.as_ref(), Err(error) if error == PDF_JOB_CANCELLED_ERROR) {
        return Err(PDF_JOB_CANCELLED_ERROR.to_string());
    }
    let stdout = stdout?;
    let stderr = stderr?;
    let status = status?;
    if !status.success() {
        return Err(format!(
            "The Image Capture bridge could not complete the request: {}",
            bounded_diagnostic(&stderr)
        ));
    }
    if stdout.is_empty() {
        return Err("The Image Capture bridge returned no result.".to_string());
    }
    Ok(stdout)
}

fn read_bounded_adapter_pipe<R: Read>(reader: R) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut reader = reader.take((MAX_ADAPTER_OUTPUT_BYTES + 1) as u64);
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Scanner adapter output could not be read: {error}"))?;
    if bytes.len() > MAX_ADAPTER_OUTPUT_BYTES {
        return Err("The scanner adapter returned too much output.".to_string());
    }
    Ok(bytes)
}

fn wait_for_scanner_process(
    child: &mut ManagedChild,
    timeout: Duration,
    adapter_name: &str,
    control: &PdfJobExecutionControl,
) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if control.ensure_not_cancelled().is_err() {
            let _ = child.terminate_tree();
            let _ = child.wait();
            return Err(PDF_JOB_CANCELLED_ERROR.to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                return Err(format!(
                    "The {adapter_name} operation exceeded its safety timeout and was stopped."
                ));
            }
            Err(error) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                return Err(format!(
                    "The {adapter_name} operation could not be monitored safely: {error}"
                ));
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WiaScannerRecord {
    id: String,
    name: String,
    manufacturer: String,
    model: String,
    flatbed: bool,
    feeder: bool,
    duplex: bool,
    supported_dpi: Vec<u16>,
    colour_modes: Vec<ScannerColourMode>,
    detail: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WiaCaptureRecord {
    paths: Vec<PathBuf>,
    warnings: Vec<String>,
}

#[cfg(target_os = "windows")]
fn discover_wia_scanners() -> Result<ScannerDiscovery, String> {
    let output = match run_powershell_json(WIA_DISCOVERY_SCRIPT, &[], SCANNER_DISCOVERY_TIMEOUT) {
        Ok(output) => output,
        Err(error) => {
            return Ok(ScannerDiscovery {
                backend: ScannerBackend::Wia,
                backend_name: "Windows Image Acquisition (WIA)",
                available: false,
                status: ScannerDiscoveryStatus::BackendUnavailable,
                detail: error,
                devices: Vec::new(),
            });
        }
    };
    let records: Vec<WiaScannerRecord> = serde_json::from_slice(&output)
        .map_err(|error| format!("Windows WIA returned an invalid device list: {error}"))?;
    let devices = records
        .into_iter()
        .map(|record| ScannerDevice {
            id: record.id,
            name: record.name,
            manufacturer: record.manufacturer,
            model: record.model,
            backend: ScannerBackend::Wia,
            flatbed: record.flatbed,
            feeder: record.feeder,
            duplex: record.duplex,
            supported_dpi: normalise_dpi(record.supported_dpi),
            colour_modes: normalise_colour_modes(record.colour_modes),
            detail: record.detail,
        })
        .collect::<Vec<_>>();
    Ok(ScannerDiscovery {
        backend: ScannerBackend::Wia,
        backend_name: "Windows Image Acquisition (WIA)",
        available: true,
        status: if devices.is_empty() {
            ScannerDiscoveryStatus::NoDevices
        } else {
            ScannerDiscoveryStatus::DevicesFound
        },
        detail: if devices.is_empty() {
            "WIA is available, but no connected scanners were found.".to_string()
        } else {
            format!(
                "{} connected scanner{} found through WIA.",
                devices.len(),
                if devices.len() == 1 { "" } else { "s" }
            )
        },
        devices,
    })
}

#[cfg(target_os = "windows")]
fn capture_wia_scanner(
    request: &CaptureScannerPagesRequest,
    directory: &Path,
    control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    control.checkpoint(18, "Configuring Windows WIA")?;
    let output_directory = directory
        .to_str()
        .ok_or_else(|| "The scanner capture path is not valid Unicode.".to_string())?;
    let page_limit = if request.source == ScannerSource::Flatbed {
        1
    } else {
        request.page_limit
    };
    let source = match request.source {
        ScannerSource::Flatbed => "flatbed",
        ScannerSource::Feeder => "feeder",
    };
    let colour_mode = match request.colour_mode {
        ScannerColourMode::Colour => "colour",
        ScannerColourMode::Greyscale => "greyscale",
        ScannerColourMode::Monochrome => "monochrome",
    };
    let owned_variables = vec![
        ("TP_SCANNER_ID".to_string(), request.device_id.clone()),
        (
            "TP_SCANNER_OUTPUT".to_string(),
            output_directory.to_string(),
        ),
        ("TP_SCANNER_SOURCE".to_string(), source.to_string()),
        (
            "TP_SCANNER_DUPLEX".to_string(),
            if request.duplex { "1" } else { "0" }.to_string(),
        ),
        ("TP_SCANNER_DPI".to_string(), request.dpi.to_string()),
        ("TP_SCANNER_COLOUR".to_string(), colour_mode.to_string()),
        (
            "TP_SCANNER_WIDTH_MM".to_string(),
            request.paper_width_mm.to_string(),
        ),
        (
            "TP_SCANNER_HEIGHT_MM".to_string(),
            request.paper_height_mm.to_string(),
        ),
        ("TP_SCANNER_PAGE_LIMIT".to_string(), page_limit.to_string()),
    ];
    let borrowed_variables = owned_variables
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    control.checkpoint(24, "Waiting for Windows WIA pages")?;
    let output = run_powershell_json_with_control(
        WIA_CAPTURE_SCRIPT,
        &borrowed_variables,
        SCANNER_CAPTURE_TIMEOUT,
        control,
    )?;
    let record: WiaCaptureRecord = serde_json::from_slice(&output)
        .map_err(|error| format!("Windows WIA returned an invalid capture report: {error}"))?;
    Ok(AdapterCaptureResult {
        paths: record.paths,
        warnings: record.warnings,
    })
}

#[cfg(target_os = "windows")]
fn run_powershell_json(
    script: &str,
    variables: &[(&str, &str)],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    run_powershell_json_with_control(
        script,
        variables,
        timeout,
        &PdfJobExecutionControl::direct(),
    )
}

#[cfg(target_os = "windows")]
fn run_powershell_json_with_control(
    script: &str,
    variables: &[(&str, &str)],
    timeout: Duration,
    control: &PdfJobExecutionControl,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in variables {
        command.env(name, value);
    }
    let mut child = ManagedChild::spawn(&mut command)
        .map_err(|error| format!("Windows WIA could not be started: {error}"))?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| "Windows WIA result output could not be opened.".to_string())?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| "Windows WIA diagnostic output could not be opened.".to_string())?;
    let stdout_thread = std::thread::spawn(move || read_bounded_adapter_pipe(stdout));
    let stderr_thread = std::thread::spawn(move || read_bounded_adapter_pipe(stderr));
    let write_result = child
        .take_stdin()
        .ok_or_else(|| "Windows WIA input could not be opened.".to_string())
        .and_then(|mut stdin| {
            stdin
                .write_all(script.as_bytes())
                .map_err(|error| format!("Windows WIA could not receive its request: {error}"))
        });
    if let Err(error) = write_result {
        let _ = child.terminate_tree();
        let _ = child.wait();
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        return Err(error);
    }
    let status = wait_for_scanner_process(&mut child, timeout, "Windows WIA", control);
    let stdout = stdout_thread
        .join()
        .map_err(|_| "The Windows WIA result reader stopped unexpectedly.".to_string())?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| "The Windows WIA diagnostic reader stopped unexpectedly.".to_string())?;
    if matches!(status.as_ref(), Err(error) if error == PDF_JOB_CANCELLED_ERROR) {
        return Err(PDF_JOB_CANCELLED_ERROR.to_string());
    }
    let stdout = stdout?;
    let stderr = stderr?;
    let status = status?;
    if !status.success() {
        return Err(format!(
            "Windows WIA could not complete the request: {}",
            bounded_diagnostic(&stderr)
        ));
    }
    if stdout.is_empty() {
        return Err("Windows WIA returned no result.".to_string());
    }
    Ok(stdout)
}

#[cfg(target_os = "linux")]
fn discover_sane_scanners() -> Result<ScannerDiscovery, String> {
    let mut command = Command::new("scanimage");
    command
        .arg("--formatted-device-list")
        .arg("%d\t%v\t%m\t%t\n")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = match run_sane_command(&mut command, SCANNER_DISCOVERY_TIMEOUT) {
        Ok(output) => output,
        Err(SaneCommandError::Start(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ScannerDiscovery {
                backend: ScannerBackend::Sane,
                backend_name: "Scanner Access Now Easy (SANE)",
                available: false,
                status: ScannerDiscoveryStatus::BackendUnavailable,
                detail: "Install the SANE scanimage command and an appropriate scanner backend to enable connected scanning.".to_string(),
                devices: Vec::new(),
            });
        }
        Err(error) => {
            return Err(format!(
                "SANE scanner discovery could not complete: {}",
                error.detail()
            ))
        }
    };
    if !output.status.success() {
        return Ok(ScannerDiscovery {
            backend: ScannerBackend::Sane,
            backend_name: "Scanner Access Now Easy (SANE)",
            available: true,
            status: ScannerDiscoveryStatus::DiscoveryFailed,
            detail: format!(
                "SANE is installed but device discovery failed: {}",
                bounded_diagnostic(&output.stderr)
            ),
            devices: Vec::new(),
        });
    }
    let records = parse_sane_device_list(&output.stdout);
    let devices = records
        .into_iter()
        .map(|record| sane_device_from_record(record))
        .collect::<Vec<_>>();
    Ok(ScannerDiscovery {
        backend: ScannerBackend::Sane,
        backend_name: "Scanner Access Now Easy (SANE)",
        available: true,
        status: if devices.is_empty() {
            ScannerDiscoveryStatus::NoDevices
        } else {
            ScannerDiscoveryStatus::DevicesFound
        },
        detail: if devices.is_empty() {
            "SANE is available, but no connected scanners were found.".to_string()
        } else {
            format!(
                "{} connected scanner{} found through SANE.",
                devices.len(),
                if devices.len() == 1 { "" } else { "s" }
            )
        },
        devices,
    })
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct SaneDeviceRecord {
    id: String,
    vendor: String,
    model: String,
    kind: String,
}

#[cfg(target_os = "linux")]
fn sane_device_from_record(record: SaneDeviceRecord) -> ScannerDevice {
    let mut command = Command::new("scanimage");
    command
        .args(["--device-name", &record.id, "--help"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let help = run_sane_command(&mut command, SCANNER_DISCOVERY_TIMEOUT)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            let mut bytes = output.stdout;
            bytes.push(b'\n');
            bytes.extend_from_slice(&output.stderr);
            String::from_utf8_lossy(&bytes).into_owned()
        })
        .unwrap_or_default();
    let source_values = extract_sane_option_values(&help, "--source");
    let mode_values = extract_sane_option_values(&help, "--mode");
    let flatbed = source_values.is_empty()
        || source_values
            .iter()
            .any(|value| value.to_ascii_lowercase().contains("flatbed"));
    let feeder = source_values.iter().any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("adf") || value.contains("feeder")
    });
    let duplex = source_values
        .iter()
        .any(|value| value.to_ascii_lowercase().contains("duplex"));
    let colour_modes = sane_colour_modes(&mode_values);
    let supported_dpi = sane_preferred_dpi(&help);
    ScannerDevice {
        id: record.id,
        name: [record.vendor.as_str(), record.model.as_str()]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        manufacturer: record.vendor,
        model: record.model,
        backend: ScannerBackend::Sane,
        flatbed,
        feeder,
        duplex,
        supported_dpi,
        colour_modes,
        detail: (!record.kind.is_empty()).then_some(record.kind),
    }
}

#[cfg(target_os = "linux")]
fn capture_sane_scanner(
    request: &CaptureScannerPagesRequest,
    directory: &Path,
    control: &PdfJobExecutionControl,
) -> Result<AdapterCaptureResult, String> {
    control.checkpoint(18, "Inspecting the selected SANE scanner")?;
    let mut help_command = Command::new("scanimage");
    help_command
        .args(["--device-name", &request.device_id, "--help"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let help_output =
        run_sane_command_with_control(&mut help_command, SCANNER_DISCOVERY_TIMEOUT, control)
            .map_err(|error| {
                sane_capture_error("SANE could not inspect the selected scanner", error)
            })?;
    if !help_output.status.success() {
        return Err(format!(
            "SANE could not inspect the selected scanner: {}",
            bounded_diagnostic(&help_output.stderr)
        ));
    }
    let mut help_bytes = help_output.stdout;
    help_bytes.push(b'\n');
    help_bytes.extend_from_slice(&help_output.stderr);
    let help = String::from_utf8_lossy(&help_bytes);
    let source_values = extract_sane_option_values(&help, "--source");
    let source = select_sane_source(&source_values, request.source, request.duplex)?;
    let mode_values = extract_sane_option_values(&help, "--mode");
    let mode = select_sane_mode(&mode_values, request.colour_mode)?;
    let mut command = Command::new("scanimage");
    command
        .arg("--device-name")
        .arg(&request.device_id)
        .arg("--format=pnm")
        .arg("--resolution")
        .arg(request.dpi.to_string())
        .arg("--mode")
        .arg(mode)
        .arg("-x")
        .arg(format!("{:.3}", request.paper_width_mm))
        .arg("-y")
        .arg(format!("{:.3}", request.paper_height_mm));
    if let Some(source) = source {
        command.arg("--source").arg(source);
    }

    if request.source == ScannerSource::Feeder {
        control.checkpoint(24, "Waiting for SANE feeder pages")?;
        let pattern = directory.join("scan-%04d.pnm");
        command
            .arg(format!("--batch={}", pattern.to_string_lossy()))
            .arg(format!("--batch-count={}", request.page_limit))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = run_sane_command_with_control(&mut command, SCANNER_CAPTURE_TIMEOUT, control)
            .map_err(|error| {
                sane_capture_error("SANE could not complete the feeder scan", error)
            })?;
        if !output.status.success() {
            return Err(format!(
                "SANE could not complete the feeder scan: {}",
                bounded_diagnostic(&output.stderr)
            ));
        }
    } else {
        control.checkpoint(24, "Waiting for the SANE flatbed page")?;
        let path = directory.join("scan-0001.pnm");
        let file = File::create(&path)
            .map_err(|error| format!("The flatbed scan file could not be created: {error}"))?;
        command.stdout(Stdio::from(file)).stderr(Stdio::piped());
        let output = run_sane_command_with_control(&mut command, SCANNER_CAPTURE_TIMEOUT, control)
            .map_err(|error| {
                sane_capture_error("SANE could not complete the flatbed scan", error)
            })?;
        if !output.status.success() {
            return Err(format!(
                "SANE could not complete the flatbed scan: {}",
                bounded_diagnostic(&output.stderr)
            ));
        }
    }

    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("The SANE capture folder could not be read: {error}"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("scan-") && value.ends_with(".pnm"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(AdapterCaptureResult {
        paths,
        warnings: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
enum SaneCommandError {
    Start(std::io::Error),
    Runtime(String),
}

#[cfg(target_os = "linux")]
impl SaneCommandError {
    fn detail(&self) -> String {
        match self {
            Self::Start(error) => format!("the command could not be started: {error}"),
            Self::Runtime(error) => error.clone(),
        }
    }

    fn is_cancelled(&self) -> bool {
        matches!(self, Self::Runtime(error) if error == PDF_JOB_CANCELLED_ERROR)
    }
}

#[cfg(target_os = "linux")]
fn sane_capture_error(context: &str, error: SaneCommandError) -> String {
    if error.is_cancelled() {
        PDF_JOB_CANCELLED_ERROR.to_string()
    } else {
        format!("{context}: {}", error.detail())
    }
}

#[cfg(target_os = "linux")]
fn run_sane_command(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, SaneCommandError> {
    run_sane_command_with_control(command, timeout, &PdfJobExecutionControl::direct())
}

#[cfg(target_os = "linux")]
fn run_sane_command_with_control(
    command: &mut Command,
    timeout: Duration,
    control: &PdfJobExecutionControl,
) -> Result<std::process::Output, SaneCommandError> {
    command.stdin(Stdio::null());
    let mut child = ManagedChild::spawn(command).map_err(SaneCommandError::Start)?;
    let stdout_reader = child
        .take_stdout()
        .map(|pipe| std::thread::spawn(move || read_bounded_adapter_pipe(pipe)));
    let stderr_reader = child
        .take_stderr()
        .map(|pipe| std::thread::spawn(move || read_bounded_adapter_pipe(pipe)));
    let status = wait_for_scanner_process(&mut child, timeout, "SANE", control)
        .map_err(SaneCommandError::Runtime);
    let stdout = finish_adapter_pipe(stdout_reader).map_err(SaneCommandError::Runtime);
    let stderr = finish_adapter_pipe(stderr_reader).map_err(SaneCommandError::Runtime);
    if matches!(status.as_ref(), Err(error) if error.is_cancelled()) {
        return Err(SaneCommandError::Runtime(
            PDF_JOB_CANCELLED_ERROR.to_string(),
        ));
    }
    Ok(std::process::Output {
        status: status?,
        stdout: stdout?,
        stderr: stderr?,
    })
}

#[cfg(target_os = "linux")]
fn finish_adapter_pipe(
    reader: Option<std::thread::JoinHandle<Result<Vec<u8>, String>>>,
) -> Result<Vec<u8>, String> {
    reader
        .map(|reader| {
            reader
                .join()
                .map_err(|_| "The SANE output reader stopped unexpectedly.".to_string())?
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

#[cfg(any(target_os = "linux", test))]
fn parse_sane_device_list(bytes: &[u8]) -> Vec<SaneDeviceRecord> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').map(str::trim).collect::<Vec<_>>();
            (parts.len() >= 4 && !parts[0].is_empty()).then(|| SaneDeviceRecord {
                id: parts[0].to_string(),
                vendor: parts[1].to_string(),
                model: parts[2].to_string(),
                kind: parts[3..].join(" "),
            })
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn extract_sane_option_values(help: &str, option: &str) -> Vec<String> {
    help.lines()
        .find_map(|line| {
            let start = line.find(option)? + option.len();
            let mut values = line[start..].trim();
            if values.starts_with('=') {
                values = values[1..].trim();
            }
            if let Some(index) = values.find(" [") {
                values = &values[..index];
            }
            let values = values
                .split('|')
                .map(|value| value.trim().trim_matches('"'))
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(values)
        })
        .unwrap_or_default()
}

#[cfg(any(target_os = "linux", test))]
fn sane_preferred_dpi(help: &str) -> Vec<u16> {
    let Some(line) = help.lines().find(|line| line.contains("--resolution")) else {
        return vec![150, 300, 600];
    };
    let numbers = line
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|value| value.parse::<u16>().ok())
        .filter(|value| (75..=1_200).contains(value))
        .collect::<Vec<_>>();
    if line.contains("..") && numbers.len() >= 2 {
        let minimum = numbers[0].min(numbers[1]);
        let maximum = numbers[0].max(numbers[1]);
        let values = [150, 300, 600]
            .into_iter()
            .filter(|value| *value >= minimum && *value <= maximum)
            .collect::<Vec<_>>();
        if !values.is_empty() {
            return values;
        }
    }
    let values = [150, 300, 600]
        .into_iter()
        .filter(|value| numbers.contains(value))
        .collect::<Vec<_>>();
    if values.is_empty() {
        vec![150, 300, 600]
    } else {
        values
    }
}

#[cfg(any(target_os = "linux", test))]
fn sane_colour_modes(values: &[String]) -> Vec<ScannerColourMode> {
    let mut modes = Vec::new();
    for value in values {
        let value = value.to_ascii_lowercase();
        if value.contains("color") || value.contains("colour") {
            modes.push(ScannerColourMode::Colour);
        } else if value.contains("gray") || value.contains("grey") {
            modes.push(ScannerColourMode::Greyscale);
        } else if value.contains("lineart") || value.contains("line art") || value.contains("black")
        {
            modes.push(ScannerColourMode::Monochrome);
        }
    }
    normalise_colour_modes(modes)
}

#[cfg(any(target_os = "linux", test))]
fn select_sane_source(
    values: &[String],
    source: ScannerSource,
    duplex: bool,
) -> Result<Option<String>, String> {
    if values.is_empty() && source == ScannerSource::Flatbed {
        return Ok(None);
    }
    let selected = match (source, duplex) {
        (ScannerSource::Flatbed, _) => values
            .iter()
            .find(|value| value.to_ascii_lowercase().contains("flatbed")),
        (ScannerSource::Feeder, true) => values
            .iter()
            .find(|value| value.to_ascii_lowercase().contains("duplex")),
        (ScannerSource::Feeder, false) => values.iter().find(|value| {
            let value = value.to_ascii_lowercase();
            (value.contains("adf") || value.contains("feeder"))
                && !value.contains("duplex")
                && !value.contains("back")
        }),
    };
    selected
        .cloned()
        .map(Some)
        .ok_or_else(|| match (source, duplex) {
            (ScannerSource::Flatbed, _) => {
                "The selected SANE scanner has no flatbed source.".to_string()
            }
            (ScannerSource::Feeder, true) => {
                "The selected SANE scanner has no duplex feeder source.".to_string()
            }
            (ScannerSource::Feeder, false) => {
                "The selected SANE scanner has no document feeder source.".to_string()
            }
        })
}

#[cfg(any(target_os = "linux", test))]
fn select_sane_mode(values: &[String], requested: ScannerColourMode) -> Result<String, String> {
    if values.is_empty() {
        return Ok(match requested {
            ScannerColourMode::Colour => "Color",
            ScannerColourMode::Greyscale => "Gray",
            ScannerColourMode::Monochrome => "Lineart",
        }
        .to_string());
    }
    let selected = values.iter().find(|value| {
        let value = value.to_ascii_lowercase();
        match requested {
            ScannerColourMode::Colour => value.contains("color") || value.contains("colour"),
            ScannerColourMode::Greyscale => value.contains("gray") || value.contains("grey"),
            ScannerColourMode::Monochrome => {
                value.contains("lineart") || value.contains("line art") || value.contains("black")
            }
        }
    });
    selected
        .cloned()
        .ok_or_else(|| "The selected scanner does not offer the requested colour mode.".to_string())
}

fn normalise_dpi(values: Vec<u16>) -> Vec<u16> {
    let mut values = values
        .into_iter()
        .filter(|value| (75..=1_200).contains(value))
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        vec![150, 300, 600]
    } else {
        values
    }
}

fn normalise_colour_modes(values: Vec<ScannerColourMode>) -> Vec<ScannerColourMode> {
    let mut normalised = Vec::new();
    for mode in [
        ScannerColourMode::Colour,
        ScannerColourMode::Greyscale,
        ScannerColourMode::Monochrome,
    ] {
        if values.contains(&mode) {
            normalised.push(mode);
        }
    }
    if normalised.is_empty() {
        vec![
            ScannerColourMode::Colour,
            ScannerColourMode::Greyscale,
            ScannerColourMode::Monochrome,
        ]
    } else {
        normalised
    }
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    const LIMIT: usize = 8 * 1024;
    let bytes = &bytes[..bytes.len().min(LIMIT)];
    let diagnostic = String::from_utf8_lossy(bytes).trim().to_string();
    if diagnostic.is_empty() {
        "No diagnostic detail was returned.".to_string()
    } else {
        diagnostic
    }
}

#[cfg(target_os = "windows")]
const WIA_DISCOVERY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'

function Get-WiaProperty($properties, [int]$propertyId) {
  foreach ($property in @($properties)) {
    if ([int]$property.PropertyID -eq $propertyId) { return $property }
  }
  return $null
}

function Get-DeviceInfoValue($info, [string]$name, [string]$fallback) {
  try {
    $value = [string]$info.Properties[$name].Value
    if (-not [string]::IsNullOrWhiteSpace($value)) { return $value }
  } catch {}
  return $fallback
}

function Get-PropertyValues($property) {
  $values = @()
  if ($null -eq $property) { return $values }
  try {
    foreach ($value in @($property.SubTypeValues)) { $values += [int]$value }
  } catch {}
  return @($values)
}

function Get-PreferredDpi($property) {
  $values = @(Get-PropertyValues $property | Where-Object { $_ -ge 75 -and $_ -le 1200 })
  if ($values.Count -gt 0) {
    $preferred = @(@(150, 300, 600) | Where-Object { $values -contains $_ })
    if ($preferred.Count -gt 0) { return @($preferred) }
    return @($values | Sort-Object -Unique | Select-Object -First 12)
  }
  try {
    $minimum = [int]$property.SubTypeMin
    $maximum = [int]$property.SubTypeMax
    $preferred = @(@(150, 300, 600) | Where-Object { $_ -ge $minimum -and $_ -le $maximum })
    if ($preferred.Count -gt 0) { return @($preferred) }
  } catch {}
  return @(150, 300, 600)
}

function Get-ColourModes($property) {
  $values = @(Get-PropertyValues $property)
  if ($values.Count -eq 0) { return @('colour', 'greyscale', 'monochrome') }
  $modes = @()
  if ($values -contains 3) { $modes += 'colour' }
  if ($values -contains 2) { $modes += 'greyscale' }
  if ($values -contains 0) { $modes += 'monochrome' }
  if ($modes.Count -eq 0) { return @('colour', 'greyscale', 'monochrome') }
  return @($modes)
}

$manager = New-Object -ComObject WIA.DeviceManager
$devices = @()
foreach ($info in @($manager.DeviceInfos)) {
  if ([int]$info.Type -ne 1) { continue }
  $name = Get-DeviceInfoValue $info 'Name' 'WIA scanner'
  $manufacturer = Get-DeviceInfoValue $info 'Manufacturer' ''
  $model = Get-DeviceInfoValue $info 'Description' $name
  $flatbed = $true
  $feeder = $true
  $duplex = $false
  $dpi = @(150, 300, 600)
  $colourModes = @('colour', 'greyscale', 'monochrome')
  $detail = $null
  try {
    $device = $info.Connect()
    $item = $device.Items.Item(1)
    $capabilityProperty = Get-WiaProperty $device.Properties 3086
    if ($null -eq $capabilityProperty) { $capabilityProperty = Get-WiaProperty $item.Properties 3086 }
    if ($null -ne $capabilityProperty) {
      $capabilities = [int]$capabilityProperty.Value
      $feeder = (($capabilities -band 1) -ne 0)
      $flatbed = (($capabilities -band 2) -ne 0)
      $duplex = (($capabilities -band 4) -ne 0)
    } else {
      $detail = 'WIA did not report source capabilities; the driver will confirm them when scanning starts.'
    }
    $dpi = @(Get-PreferredDpi (Get-WiaProperty $item.Properties 6147))
    $colourModes = @(Get-ColourModes (Get-WiaProperty $item.Properties 4103))
  } catch {
    $detail = 'The scanner was listed, but WIA could not open it to inspect capabilities.'
  }
  $devices += [pscustomobject]@{
    id = [string]$info.DeviceID
    name = $name
    manufacturer = $manufacturer
    model = $model
    flatbed = [bool]$flatbed
    feeder = [bool]$feeder
    duplex = [bool]$duplex
    supportedDpi = @($dpi)
    colourModes = @($colourModes)
    detail = $detail
  }
}
[Console]::Out.Write((ConvertTo-Json -InputObject @($devices) -Compress -Depth 6))
"#;

#[cfg(target_os = "windows")]
const WIA_CAPTURE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'

function Get-WiaProperty($properties, [int]$propertyId) {
  foreach ($property in @($properties)) {
    if ([int]$property.PropertyID -eq $propertyId) { return $property }
  }
  return $null
}

function Set-WiaProperty($properties, [int]$propertyId, $value) {
  $property = Get-WiaProperty $properties $propertyId
  if ($null -eq $property -or [bool]$property.IsReadOnly) { return $false }
  try { $property.Value = $value; return $true } catch { return $false }
}

$manager = New-Object -ComObject WIA.DeviceManager
$deviceInfo = $null
foreach ($candidate in @($manager.DeviceInfos)) {
  if ([int]$candidate.Type -eq 1 -and [string]$candidate.DeviceID -ceq $env:TP_SCANNER_ID) {
    $deviceInfo = $candidate
    break
  }
}
if ($null -eq $deviceInfo) { throw 'The selected WIA scanner is no longer connected.' }

$device = $deviceInfo.Connect()
if ($device.Items.Count -lt 1) { throw 'The WIA scanner has no acquisition item.' }
$item = $device.Items.Item(1)
$warnings = @()
$sourceValue = if ($env:TP_SCANNER_SOURCE -eq 'feeder') { 1 } else { 2 }
if ($env:TP_SCANNER_DUPLEX -eq '1') { $sourceValue = $sourceValue -bor 4 }
$sourceSet = (Set-WiaProperty $device.Properties 3088 $sourceValue)
if (-not $sourceSet) { $sourceSet = Set-WiaProperty $item.Properties 3088 $sourceValue }
if (-not $sourceSet) { throw 'The WIA driver did not accept the requested flatbed or feeder source.' }

$dpi = [int]$env:TP_SCANNER_DPI
$xResolutionSet = Set-WiaProperty $item.Properties 6147 $dpi
$yResolutionSet = Set-WiaProperty $item.Properties 6148 $dpi
if (-not ($xResolutionSet -and $yResolutionSet)) {
  $warnings += 'The WIA driver kept its default resolution because the requested DPI was unavailable.'
}
$dataType = switch ($env:TP_SCANNER_COLOUR) {
  'colour' { 3 }
  'greyscale' { 2 }
  default { 0 }
}
if (-not (Set-WiaProperty $item.Properties 4103 $dataType)) {
  $warnings += 'The WIA driver kept its default colour mode.'
}
$widthPixels = [Math]::Max(1, [int][Math]::Round(([double]$env:TP_SCANNER_WIDTH_MM / 25.4) * $dpi))
$heightPixels = [Math]::Max(1, [int][Math]::Round(([double]$env:TP_SCANNER_HEIGHT_MM / 25.4) * $dpi))
$widthSet = Set-WiaProperty $item.Properties 6151 $widthPixels
$heightSet = Set-WiaProperty $item.Properties 6152 $heightPixels
if (-not ($widthSet -and $heightSet)) {
  $warnings += 'The WIA driver kept its own page-size or automatic-crop setting.'
}

$pageLimit = [int]$env:TP_SCANNER_PAGE_LIMIT
$paths = @()
$bmpFormat = '{B96B3CAB-0728-11D3-9D7B-0000F81EF32E}'
for ($page = 1; $page -le $pageLimit; $page++) {
  if ($env:TP_SCANNER_SOURCE -eq 'feeder') {
    [void](Set-WiaProperty $item.Properties 3096 1)
  }
  try {
    $image = $item.Transfer($bmpFormat)
    if ($null -eq $image) { throw 'The WIA driver returned no image.' }
    $path = Join-Path $env:TP_SCANNER_OUTPUT ('scan-{0:D4}.bmp' -f $page)
    $image.SaveFile($path)
    $paths += $path
  } catch {
    $code = [uint32]([int64]$_.Exception.HResult -band 0xffffffffL)
    if ($env:TP_SCANNER_SOURCE -eq 'feeder' -and $paths.Count -gt 0 -and $code -eq 0x80210003) {
      break
    }
    throw
  }
  if ($env:TP_SCANNER_SOURCE -eq 'flatbed') { break }
}
if ($paths.Count -eq 0) { throw 'The WIA scanner did not capture a page.' }
[Console]::Out.Write((ConvertTo-Json -InputObject ([pscustomobject]@{
  paths = @($paths)
  warnings = @($warnings)
}) -Compress -Depth 5))
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    #[test]
    fn discovery_statuses_have_stable_wire_values() {
        let values = [
            (
                ScannerDiscoveryStatus::BackendUnavailable,
                "\"backend-unavailable\"",
            ),
            (ScannerDiscoveryStatus::DevicesFound, "\"devices-found\""),
            (
                ScannerDiscoveryStatus::DiscoveryFailed,
                "\"discovery-failed\"",
            ),
            (ScannerDiscoveryStatus::NoDevices, "\"no-devices\""),
            (
                ScannerDiscoveryStatus::UnsupportedPlatform,
                "\"unsupported-platform\"",
            ),
        ];
        for (status, expected) in values {
            assert_eq!(serde_json::to_string(&status).unwrap(), expected);
        }
    }

    #[test]
    fn validates_scanner_source_and_page_limits() {
        let request = CaptureScannerPagesRequest {
            device_id: "test:scanner".to_string(),
            source: ScannerSource::Flatbed,
            duplex: false,
            dpi: 300,
            colour_mode: ScannerColourMode::Colour,
            paper_width_mm: 210.0,
            paper_height_mm: 297.0,
            page_limit: 1,
        };
        assert!(validate_capture_request(&request).is_ok());
        assert!(validate_capture_request(&CaptureScannerPagesRequest {
            duplex: true,
            ..request.clone()
        })
        .unwrap_err()
        .contains("Duplex"));
        assert!(validate_capture_request(&CaptureScannerPagesRequest {
            page_limit: 2,
            ..request
        })
        .unwrap_err()
        .contains("flatbed"));
    }

    #[test]
    fn controlled_scanner_capture_reports_progress_and_keeps_verified_pages() {
        let root = TestDirectory::new();
        let reports = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let captured_reports = Arc::clone(&reports);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, stage| {
                captured_reports.lock().unwrap().push((progress, stage));
            }),
        );

        let result = capture_scanner_pages_with_control_and_adapter(
            test_capture_request(),
            &root.path,
            &control,
            |_, directory, execution_control| {
                execution_control.checkpoint(52, "Receiving scanner page 1")?;
                let page = directory.join("scan-0001.png");
                RgbImage::from_pixel(40, 60, Rgb([230, 230, 230]))
                    .save(&page)
                    .unwrap();
                Ok(AdapterCaptureResult {
                    paths: vec![page],
                    warnings: vec!["The test adapter retained its reviewed defaults.".to_string()],
                })
            },
        )
        .unwrap();

        assert_eq!(result.page_count, 1);
        assert_eq!(result.paths.len(), 1);
        assert!(Path::new(&result.paths[0]).exists());
        assert_eq!(result.warnings.len(), 1);
        let serialised = serde_json::to_string(&result).unwrap();
        assert!(!serialised.contains("test:scanner"));
        assert!(!serialised.contains("deviceId"));
        let reports = reports.lock().unwrap();
        assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(reports
            .iter()
            .any(|(_, stage)| stage == "Receiving scanner page 1"));
        assert_eq!(reports.last().map(|entry| entry.0), Some(99));
    }

    #[test]
    fn cancelled_scanner_capture_removes_its_private_workspace() {
        let root = TestDirectory::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |progress, _| {
                if progress >= 14 {
                    cancellation_flag.store(true, Ordering::Release);
                }
            }),
        );

        let error = capture_scanner_pages_with_control_and_adapter(
            test_capture_request(),
            &root.path,
            &control,
            |_, _, _| unreachable!("capture was cancelled before the adapter"),
        )
        .unwrap_err();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert_eq!(fs::read_dir(&root.path).unwrap().count(), 0);
    }

    #[test]
    fn scanner_capture_rejects_a_page_changed_before_delivery() {
        let root = TestDirectory::new();
        let captured_path = Arc::new(Mutex::new(None::<PathBuf>));
        let path_to_change = Arc::clone(&captured_path);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| {
                if progress == 96 {
                    let path = path_to_change.lock().unwrap().clone().unwrap();
                    let mut page = fs::OpenOptions::new().append(true).open(path).unwrap();
                    std::io::Write::write_all(&mut page, b"changed after capture validation")
                        .unwrap();
                }
            }),
        );

        let error = capture_scanner_pages_with_control_and_adapter(
            test_capture_request(),
            &root.path,
            &control,
            |_, directory, _| {
                let page = directory.join("scan-0001.png");
                RgbImage::from_pixel(40, 60, Rgb([230, 230, 230]))
                    .save(&page)
                    .unwrap();
                *captured_path.lock().unwrap() = Some(page.clone());
                Ok(AdapterCaptureResult {
                    paths: vec![page],
                    warnings: Vec::new(),
                })
            },
        )
        .unwrap_err();

        assert!(error.contains("changed before it could be returned"));
        assert_eq!(fs::read_dir(&root.path).unwrap().count(), 0);
    }

    #[test]
    fn scanner_capture_job_errors_are_content_free() {
        let error = safe_scanner_capture_job_error(
            "Windows WIA exposed C:\\Private\\client-passport.bmp for device secret-id",
        );
        assert_eq!(
            error,
            "Windows WIA could not complete the scanner capture. Check the device, driver, source and paper, then try again."
        );
        assert!(!error.contains("passport"));
        assert!(!error.contains("secret-id"));
    }

    #[test]
    fn scanner_process_wait_honours_job_cancellation() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("scanner::tests::scanner_wait_helper")
            .arg("--nocapture")
            .env("PAPERWORKS_SCANNER_WAIT_TEST_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = ManagedChild::spawn(&mut command).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let control = PdfJobExecutionControl::new(cancelled, Arc::new(|_, _| {}));
        let started = Instant::now();
        let cancellation_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            cancellation_flag.store(true, Ordering::Release);
        });

        let error = wait_for_scanner_process(
            &mut child,
            Duration::from_secs(30),
            "test scanner",
            &control,
        )
        .unwrap_err();
        cancellation_thread.join().unwrap();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn scanner_wait_helper() {
        if std::env::var_os("PAPERWORKS_SCANNER_WAIT_TEST_CHILD").is_none() {
            return;
        }
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn parses_sane_devices_and_backend_option_values() {
        let devices = parse_sane_device_list(
            b"airscan:e0:Office\tExample Corp\tDocument 500\tvirtual scanner\n",
        );
        assert_eq!(
            devices,
            vec![SaneDeviceRecord {
                id: "airscan:e0:Office".to_string(),
                vendor: "Example Corp".to_string(),
                model: "Document 500".to_string(),
                kind: "virtual scanner".to_string(),
            }]
        );
        let help = "  --source Flatbed|ADF Front|ADF Duplex [Flatbed]\n  --mode Lineart|Gray|Color [Color]";
        assert_eq!(
            extract_sane_option_values(help, "--source"),
            vec!["Flatbed", "ADF Front", "ADF Duplex"]
        );
        assert_eq!(
            select_sane_source(
                &extract_sane_option_values(help, "--source"),
                ScannerSource::Feeder,
                true,
            )
            .unwrap(),
            Some("ADF Duplex".to_string())
        );
        assert_eq!(
            select_sane_mode(
                &extract_sane_option_values(help, "--mode"),
                ScannerColourMode::Greyscale,
            )
            .unwrap(),
            "Gray"
        );
        assert_eq!(
            sane_colour_modes(&extract_sane_option_values(help, "--mode")),
            vec![
                ScannerColourMode::Colour,
                ScannerColourMode::Greyscale,
                ScannerColourMode::Monochrome,
            ]
        );
    }

    #[test]
    fn derives_preferred_resolutions_from_ranges_and_lists() {
        assert_eq!(
            sane_preferred_dpi("--resolution 75..600dpi [300]"),
            vec![150, 300, 600]
        );
        assert_eq!(
            sane_preferred_dpi("--resolution 100|200|300|400dpi [300]"),
            vec![300]
        );
    }

    #[test]
    fn parses_versioned_image_capture_devices_without_trusting_driver_text() {
        let report = parse_image_capture_discovery(
            br#"{
                "protocolVersion": 1,
                "devices": [{
                    "id": "imagecapture:office-1",
                    "name": "Office Scanner",
                    "manufacturer": "Example",
                    "model": "Scan 500",
                    "flatbed": true,
                    "feeder": true,
                    "duplex": true,
                    "supportedDpi": [600, 300, 300, 5000],
                    "colourModes": ["monochrome", "colour"],
                    "detail": "USB"
                }]
            }"#,
        )
        .unwrap();
        let device =
            image_capture_device_from_record(report.devices.into_iter().next().unwrap()).unwrap();
        assert_eq!(device.backend, ScannerBackend::ImageCapture);
        assert_eq!(device.supported_dpi, vec![300, 600]);
        assert_eq!(
            device.colour_modes,
            vec![ScannerColourMode::Colour, ScannerColourMode::Monochrome]
        );
        assert!(device.flatbed && device.feeder && device.duplex);

        let invalid = ImageCaptureDeviceRecord {
            id: "imagecapture:bad\nrecord".to_string(),
            name: "Bad scanner".to_string(),
            manufacturer: String::new(),
            model: String::new(),
            flatbed: true,
            feeder: false,
            duplex: false,
            supported_dpi: vec![300],
            colour_modes: vec![ScannerColourMode::Colour],
            detail: None,
        };
        assert!(image_capture_device_from_record(invalid).is_none());
    }

    #[test]
    fn rejects_incompatible_image_capture_protocols_and_unknown_fields() {
        let valid = parse_image_capture_capture(
            br#"{"protocolVersion":1,"paths":["/tmp/scan-0001.tiff"],"warnings":["bounded"]}"#,
        )
        .unwrap();
        assert_eq!(valid.paths, vec![PathBuf::from("/tmp/scan-0001.tiff")]);
        assert_eq!(valid.warnings, vec!["bounded"]);
        assert!(
            parse_image_capture_discovery(br#"{"protocolVersion":2,"devices":[]}"#)
                .unwrap_err()
                .contains("unsupported protocol")
        );
        assert!(parse_image_capture_capture(
            br#"{"protocolVersion":1,"paths":[],"warnings":[],"unexpected":true}"#
        )
        .unwrap_err()
        .contains("invalid capture report"));
    }

    #[test]
    fn serialises_image_capture_requests_in_the_native_protocol_shape() {
        let request = ImageCaptureCaptureRequest {
            protocol_version: IMAGE_CAPTURE_PROTOCOL_VERSION,
            device_id: "imagecapture:office-1",
            source: ScannerSource::Feeder,
            duplex: true,
            dpi: 300,
            colour_mode: ScannerColourMode::Greyscale,
            paper_width_mm: 210.0,
            paper_height_mm: 297.0,
            page_limit: 25,
            output_directory: "/tmp/capture",
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["protocolVersion"], IMAGE_CAPTURE_PROTOCOL_VERSION);
        assert_eq!(value["deviceId"], "imagecapture:office-1");
        assert_eq!(value["source"], "feeder");
        assert_eq!(value["colourMode"], "greyscale");
        assert_eq!(value["pageLimit"], 25);
        assert_eq!(value["outputDirectory"], "/tmp/capture");
    }

    #[test]
    fn accepts_only_non_empty_capture_files_inside_the_session() {
        let directory = TestDirectory::new();
        let page = directory.path.join("scan-0001.pnm");
        fs::write(&page, b"P1\n1 1\n0\n").unwrap();
        assert_eq!(
            validate_capture_outputs(&directory.path, vec![page.clone()]).unwrap(),
            vec![fs::canonicalize(page).unwrap()]
        );
        let outside = directory.path.parent().unwrap().join("outside.pnm");
        fs::write(&outside, b"P1\n1 1\n0\n").unwrap();
        assert!(validate_capture_outputs(&directory.path, vec![outside.clone()]).is_err());
        let _ = fs::remove_file(outside);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn wia_discovery_returns_a_typed_report_without_requiring_hardware() {
        let report = discover_wia_scanners().unwrap();
        assert_eq!(report.backend, ScannerBackend::Wia);
        assert_eq!(report.backend_name, "Windows Image Acquisition (WIA)");
    }

    #[test]
    #[ignore = "requires a private connected scanner and PAPERWORKS_SCANNER_DEVICE_ID"]
    fn live_connected_scanner_capture() {
        let discovery = discover_platform_scanners().unwrap();
        let device_id = std::env::var("PAPERWORKS_SCANNER_DEVICE_ID").unwrap_or_else(|_| {
            let available = discovery
                .devices
                .iter()
                .map(|device| format!("{} ({})", device.id, device.name))
                .collect::<Vec<_>>()
                .join(", ");
            panic!("set PAPERWORKS_SCANNER_DEVICE_ID to one of the discovered devices: {available}")
        });
        let source = match std::env::var("PAPERWORKS_SCANNER_SOURCE")
            .unwrap_or_else(|_| "flatbed".to_string())
            .as_str()
        {
            "flatbed" => ScannerSource::Flatbed,
            "feeder" => ScannerSource::Feeder,
            value => panic!("unsupported PAPERWORKS_SCANNER_SOURCE: {value}"),
        };
        let duplex = std::env::var("PAPERWORKS_SCANNER_DUPLEX").is_ok_and(|value| value == "1");
        let colour_mode = match std::env::var("PAPERWORKS_SCANNER_COLOUR")
            .unwrap_or_else(|_| "colour".to_string())
            .as_str()
        {
            "colour" => ScannerColourMode::Colour,
            "greyscale" => ScannerColourMode::Greyscale,
            "monochrome" => ScannerColourMode::Monochrome,
            value => panic!("unsupported PAPERWORKS_SCANNER_COLOUR: {value}"),
        };
        let dpi = std::env::var("PAPERWORKS_SCANNER_DPI")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(300);
        let page_limit = if source == ScannerSource::Flatbed {
            1
        } else {
            std::env::var("PAPERWORKS_SCANNER_PAGE_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2)
        };
        let paper_width_mm = std::env::var("PAPERWORKS_SCANNER_WIDTH_MM")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(210.0);
        let paper_height_mm = std::env::var("PAPERWORKS_SCANNER_HEIGHT_MM")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(297.0);
        let request = CaptureScannerPagesRequest {
            device_id: device_id.clone(),
            source,
            duplex,
            dpi,
            colour_mode,
            paper_width_mm,
            paper_height_mm,
            page_limit,
        };
        validate_capture_request(&request).unwrap();
        let device = discovery
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .expect("the configured scanner was not discovered");
        assert!(
            (source == ScannerSource::Flatbed && device.flatbed)
                || (source == ScannerSource::Feeder && device.feeder)
        );
        assert!(!duplex || device.duplex);

        let directory = TestDirectory::new();
        let capture =
            capture_platform_scanner(&request, &directory.path, &PdfJobExecutionControl::direct())
                .unwrap();
        let paths = validate_capture_outputs(&directory.path, capture.paths).unwrap();
        assert!(!paths.is_empty());
        assert!(paths.len() <= usize::from(page_limit));
        for path in paths {
            let image = image::ImageReader::open(path)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .decode()
                .unwrap();
            assert!(image.width() > 0 && image.height() > 0);
        }
    }

    fn test_capture_request() -> CaptureScannerPagesRequest {
        CaptureScannerPagesRequest {
            device_id: "test:scanner".to_string(),
            source: ScannerSource::Flatbed,
            duplex: false,
            dpi: 300,
            colour_mode: ScannerColourMode::Colour,
            paper_width_mm: 210.0,
            paper_height_mm: 297.0,
            page_limit: 1,
        }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-scanner-test",
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
