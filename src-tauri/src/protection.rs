use crate::child_process::ManagedChild;
use crate::file_safety::{reject_control_characters, TemporaryOutput, ValidatedPdfPaths};
use crate::health::ensure_pdf_rewrite_acknowledged;
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::runtime_capabilities::current_capabilities;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

const MAX_PASSWORD_BYTES: usize = 127;
const MAX_QPDF_DIAGNOSTIC_BYTES: usize = 1024 * 1024;
const QPDF_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionCapabilities {
    provider: &'static str,
    available: bool,
    version: Option<String>,
    encryption: &'static str,
    max_password_bytes: usize,
    supports_password_removal: bool,
    permissions_are_advisory: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectPdfRequest {
    input_path: String,
    output_path: String,
    input_password: Option<String>,
    open_password: String,
    owner_password: String,
    print_permission: PrintPermission,
    modification_permission: ModificationPermission,
    allow_copying: bool,
    acknowledge_certificate_signatures: bool,
    expected_source_modified_at_ms: Option<u64>,
    expected_source_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProtectionRequest {
    input_path: String,
    output_path: String,
    password: String,
    acknowledge_certificate_signatures: bool,
    expected_source_modified_at_ms: Option<u64>,
    expected_source_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", content = "request", rename_all = "lowercase")]
pub enum ProtectionPdfJobRequest {
    Protect(ProtectPdfRequest),
    Remove(RemoveProtectionRequest),
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrintPermission {
    None,
    Low,
    Full,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModificationPermission {
    None,
    Assembly,
    Form,
    Annotate,
    All,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionResult {
    output_path: String,
    bytes_written: u64,
    encryption: &'static str,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PdfOutputProtection {
    pub(crate) open_password: String,
    pub(crate) owner_password: String,
}

#[tauri::command]
pub fn protection_capabilities() -> ProtectionCapabilities {
    if !current_capabilities().password_protection() {
        return ProtectionCapabilities {
            provider: "QPDF",
            available: false,
            version: None,
            encryption: "AES-256",
            max_password_bytes: MAX_PASSWORD_BYTES,
            supports_password_removal: false,
            permissions_are_advisory: true,
        };
    }
    let version = Command::new("qpdf")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            first_output_line(&output.stdout).or_else(|| first_output_line(&output.stderr))
        });

    ProtectionCapabilities {
        provider: "QPDF",
        available: version.is_some(),
        version,
        encryption: "AES-256",
        max_password_bytes: MAX_PASSWORD_BYTES,
        supports_password_removal: true,
        permissions_are_advisory: true,
    }
}

#[cfg(test)]
pub fn protect_pdf(request: ProtectPdfRequest) -> Result<ProtectionResult, String> {
    protect_pdf_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_protection_pdf_job_request(
    request: &ProtectionPdfJobRequest,
) -> Result<(), String> {
    match request {
        ProtectionPdfJobRequest::Protect(request) => validate_protect_pdf_request(request),
        ProtectionPdfJobRequest::Remove(request) => validate_remove_protection_request(request),
    }
}

pub(crate) fn run_protection_pdf_job_with_control(
    request: ProtectionPdfJobRequest,
    control: &PdfJobExecutionControl,
) -> Result<ProtectionResult, String> {
    match request {
        ProtectionPdfJobRequest::Protect(request) => protect_pdf_with_control(request, control),
        ProtectionPdfJobRequest::Remove(request) => {
            remove_pdf_protection_with_control(request, control)
        }
    }
}

fn validate_protect_pdf_request(request: &ProtectPdfRequest) -> Result<(), String> {
    reject_control_characters("Input path", &request.input_path)?;
    reject_control_characters("Output path", &request.output_path)?;
    validate_password("Opening password", &request.open_password, false)?;
    validate_password("Administrator password", &request.owner_password, false)?;

    if request.open_password == request.owner_password
        && !matches!(request.modification_permission, ModificationPermission::All)
    {
        return Err(
            "Use a different administrator password when restricting document changes.".to_string(),
        );
    }

    if let Some(password) = request.input_password.as_deref() {
        validate_password("Current password", password, true)?;
    }
    if request.expected_source_size == 0 {
        return Err("Review the source PDF again before adding protection.".to_string());
    }
    Ok(())
}

fn validate_remove_protection_request(request: &RemoveProtectionRequest) -> Result<(), String> {
    reject_control_characters("Input path", &request.input_path)?;
    reject_control_characters("Output path", &request.output_path)?;
    validate_password("Current password", &request.password, true)?;
    if request.expected_source_size == 0 {
        return Err("Review the source PDF again before removing protection.".to_string());
    }
    Ok(())
}

fn protect_pdf_with_control(
    request: ProtectPdfRequest,
    control: &PdfJobExecutionControl,
) -> Result<ProtectionResult, String> {
    protect_pdf_with_runner(request, control, &run_qpdf_with_control)
}

fn protect_pdf_with_runner<F>(
    request: ProtectPdfRequest,
    control: &PdfJobExecutionControl,
    runner: &F,
) -> Result<ProtectionResult, String>
where
    F: Fn(&[String], &[&str], &PdfJobExecutionControl) -> Result<(), String>,
{
    control.checkpoint(1, "Validating password protection request")?;
    validate_protect_pdf_request(&request)?;

    control.checkpoint(5, "Opening reviewed source PDF")?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(10, "Checking certificate-signature risk")?;
    ensure_pdf_rewrite_acknowledged(
        &request.input_path,
        request.input_password.as_deref(),
        request.acknowledge_certificate_signatures,
    )?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(18, "Preparing AES-256 output")?;
    let temporary = TemporaryOutput::new(&paths.output)?;
    let input_path = paths.input.to_string_lossy().into_owned();
    let output_path = paths.output.to_string_lossy().into_owned();
    let temporary_path = temporary.path().to_string_lossy().into_owned();
    let mut arguments = vec![
        input_path.clone(),
        temporary_path.clone(),
        "--warning-exit-0".to_string(),
        "--password-mode=unicode".to_string(),
    ];

    if let Some(password) = request
        .input_password
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        arguments.push(format!("--password={password}"));
    }

    arguments.extend([
        "--encrypt".to_string(),
        request.open_password.clone(),
        request.owner_password.clone(),
        "256".to_string(),
        format!("--print={}", request.print_permission.as_qpdf_value()),
        format!(
            "--modify={}",
            request.modification_permission.as_qpdf_value()
        ),
        format!(
            "--extract={}",
            if request.allow_copying { "y" } else { "n" }
        ),
        "--accessibility=y".to_string(),
        "--".to_string(),
    ]);

    let sensitive_values = [
        request.open_password.as_str(),
        request.owner_password.as_str(),
        request.input_password.as_deref().unwrap_or_default(),
        input_path.as_str(),
        output_path.as_str(),
        temporary_path.as_str(),
    ];
    control.checkpoint(25, "Encrypting PDF with AES-256")?;
    runner(&arguments, &sensitive_values, control)?;
    control.checkpoint(78, "Reopening encrypted output")?;
    verify_pdf_with_runner(
        temporary.path(),
        Some(&request.open_password),
        &sensitive_values,
        control,
        runner,
    )?;
    control.checkpoint(95, "Rechecking source PDF before publication")?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(99, "Publishing verified protected PDF")?;
    let bytes_written = temporary.persist(&paths.output)?;

    Ok(ProtectionResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        bytes_written,
        encryption: "AES-256",
    })
}

#[cfg(test)]
pub fn remove_pdf_protection(request: RemoveProtectionRequest) -> Result<ProtectionResult, String> {
    remove_pdf_protection_with_control(request, &PdfJobExecutionControl::direct())
}

fn remove_pdf_protection_with_control(
    request: RemoveProtectionRequest,
    control: &PdfJobExecutionControl,
) -> Result<ProtectionResult, String> {
    control.checkpoint(1, "Validating password-removal request")?;
    validate_remove_protection_request(&request)?;

    control.checkpoint(5, "Opening reviewed protected PDF")?;
    let paths = ValidatedPdfPaths::new(&request.input_path, &request.output_path)?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(10, "Checking certificate-signature risk")?;
    ensure_pdf_rewrite_acknowledged(
        &request.input_path,
        Some(&request.password),
        request.acknowledge_certificate_signatures,
    )?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(18, "Preparing unlocked output")?;
    let temporary = TemporaryOutput::new(&paths.output)?;
    let input_path = paths.input.to_string_lossy().into_owned();
    let output_path = paths.output.to_string_lossy().into_owned();
    let temporary_path = temporary.path().to_string_lossy().into_owned();
    let arguments = vec![
        input_path.clone(),
        temporary_path.clone(),
        "--warning-exit-0".to_string(),
        "--password-mode=unicode".to_string(),
        format!("--password={}", request.password),
        "--decrypt".to_string(),
    ];
    let sensitive_values = [
        request.password.as_str(),
        input_path.as_str(),
        output_path.as_str(),
        temporary_path.as_str(),
    ];

    control.checkpoint(25, "Removing PDF encryption")?;
    run_qpdf_with_control(&arguments, &sensitive_values, control)?;
    control.checkpoint(78, "Reopening unlocked output")?;
    verify_pdf_with_control(temporary.path(), None, &sensitive_values, control)?;
    control.checkpoint(95, "Rechecking source PDF before publication")?;
    verify_source_fingerprint(
        &paths.input,
        request.expected_source_size,
        request.expected_source_modified_at_ms,
    )?;
    control.checkpoint(99, "Publishing verified unlocked PDF")?;
    let bytes_written = temporary.persist(&paths.output)?;

    Ok(ProtectionResult {
        output_path: paths.output.to_string_lossy().into_owned(),
        bytes_written,
        encryption: "None",
    })
}

#[cfg(test)]
pub(crate) fn lock_pdf_changes(
    input: &Path,
    output: &Path,
    open_password: &str,
    owner_password: &str,
) -> Result<(), String> {
    lock_pdf_changes_with_control(
        input,
        output,
        open_password,
        owner_password,
        &PdfJobExecutionControl::direct(),
    )
}

pub(crate) fn lock_pdf_changes_with_control(
    input: &Path,
    output: &Path,
    open_password: &str,
    owner_password: &str,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    lock_pdf_changes_from_source_with_control(
        input,
        None,
        output,
        open_password,
        owner_password,
        control,
    )
}

pub(crate) fn decrypt_pdf_to_path_with_control(
    input: &Path,
    output: &Path,
    password: &str,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    validate_password("Current password", password, true)?;
    let input_path = input.to_string_lossy().into_owned();
    let output_path = output.to_string_lossy().into_owned();
    let arguments = decrypt_pdf_arguments(&input_path, &output_path, password);
    let sensitive_values = [password, input_path.as_str(), output_path.as_str()];
    run_qpdf_with_control(&arguments, &sensitive_values, control)?;
    verify_pdf_with_control(output, None, &sensitive_values, control)
}

pub(crate) fn lock_pdf_changes_from_source_with_control(
    input: &Path,
    input_password: Option<&str>,
    output: &Path,
    open_password: &str,
    owner_password: &str,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    validate_password("Opening password", open_password, false)?;
    validate_password("Administrator password", owner_password, false)?;
    if let Some(input_password) = input_password {
        validate_password("Current password", input_password, true)?;
    }
    if open_password == owner_password {
        return Err(
            "Use a different administrator password when restricting document changes.".to_string(),
        );
    }

    let arguments = lock_pdf_arguments(
        &input.to_string_lossy(),
        &output.to_string_lossy(),
        input_password,
        open_password,
        owner_password,
    );
    let input_path = input.to_string_lossy();
    let output_path = output.to_string_lossy();
    let input_password = input_password.unwrap_or_default();
    let sensitive_values = [
        input_password,
        open_password,
        owner_password,
        input_path.as_ref(),
        output_path.as_ref(),
    ];
    run_qpdf_with_control(&arguments, &sensitive_values, control)?;
    verify_pdf_with_control(output, Some(open_password), &sensitive_values, control)
}

fn decrypt_pdf_arguments(input: &str, output: &str, password: &str) -> Vec<String> {
    vec![
        input.to_string(),
        output.to_string(),
        "--warning-exit-0".to_string(),
        "--password-mode=unicode".to_string(),
        format!("--password={password}"),
        "--decrypt".to_string(),
    ]
}

fn lock_pdf_arguments(
    input: &str,
    output: &str,
    input_password: Option<&str>,
    open_password: &str,
    owner_password: &str,
) -> Vec<String> {
    let mut arguments = vec![
        input.to_string(),
        output.to_string(),
        "--warning-exit-0".to_string(),
        "--password-mode=unicode".to_string(),
    ];
    if let Some(input_password) = input_password {
        arguments.push(format!("--password={input_password}"));
    }
    arguments.extend([
        "--encrypt".to_string(),
        open_password.to_string(),
        owner_password.to_string(),
        "256".to_string(),
        "--print=full".to_string(),
        "--modify=none".to_string(),
        "--extract=y".to_string(),
        "--accessibility=y".to_string(),
        "--".to_string(),
    ]);
    arguments
}

pub(crate) fn validate_pdf_output_protection(
    protection: Option<&PdfOutputProtection>,
) -> Result<(), String> {
    let Some(protection) = protection else {
        return Ok(());
    };
    validate_password("Opening password", &protection.open_password, false)?;
    validate_password("Administrator password", &protection.owner_password, false)?;
    if protection.open_password == protection.owner_password {
        return Err(
            "Use a different administrator password when restricting document changes.".to_string(),
        );
    }
    Ok(())
}

impl PrintPermission {
    fn as_qpdf_value(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Full => "full",
        }
    }
}

impl ModificationPermission {
    fn as_qpdf_value(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Assembly => "assembly",
            Self::Form => "form",
            Self::Annotate => "annotate",
            Self::All => "all",
        }
    }
}

fn run_qpdf_with_control(
    arguments: &[String],
    sensitive_values: &[&str],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.ensure_not_cancelled()?;
    let mut command = Command::new("qpdf");
    command.arg("@-");
    let mut input = Vec::new();
    for argument in arguments {
        writeln!(&mut input, "{argument}")
            .map_err(|error| format!("QPDF input could not be prepared: {error}"))?;
    }
    let output = run_child_with_control(&mut command, &input, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            format!(
                "QPDF could not complete. Install QPDF or add it to PATH, then try again: {error}"
            )
        }
    })?;

    if output.status.success() {
        return Ok(());
    }

    let detail = first_output_line(&output.stderr)
        .or_else(|| first_output_line(&output.stdout))
        .unwrap_or_else(|| "QPDF returned an unknown error.".to_string());
    Err(redact_sensitive_values(&detail, sensitive_values))
}

fn verify_pdf_with_control(
    path: &Path,
    password: Option<&str>,
    sensitive_values: &[&str],
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    verify_pdf_with_runner(
        path,
        password,
        sensitive_values,
        control,
        &run_qpdf_with_control,
    )
}

fn verify_pdf_with_runner<F>(
    path: &Path,
    password: Option<&str>,
    sensitive_values: &[&str],
    control: &PdfJobExecutionControl,
    runner: &F,
) -> Result<(), String>
where
    F: Fn(&[String], &[&str], &PdfJobExecutionControl) -> Result<(), String>,
{
    let mut arguments = vec![
        path.to_string_lossy().into_owned(),
        "--warning-exit-0".to_string(),
        "--password-mode=unicode".to_string(),
    ];

    if let Some(password) = password {
        arguments.push(format!("--password={password}"));
    }
    arguments.push("--check".to_string());

    match runner(&arguments, sensitive_values, control) {
        Err(error) if error == PDF_JOB_CANCELLED_ERROR => Err(error),
        Err(error) => Err(format!("The output PDF failed verification: {error}")),
        Ok(()) => Ok(()),
    }
}

fn run_child_with_control(
    command: &mut Command,
    input: &[u8],
    control: &PdfJobExecutionControl,
) -> Result<Output, String> {
    control.ensure_not_cancelled()?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(command)
        .map_err(|error| format!("The external PDF tool could not be started: {error}"))?;
    let stdout_reader = child.take_stdout().map(read_child_output);
    let stderr_reader = child.take_stderr().map(read_child_output);

    let write_result = child
        .take_stdin()
        .ok_or_else(|| "The external PDF tool input could not be prepared.".to_string())
        .and_then(|mut stdin| {
            stdin.write_all(input).map_err(|error| {
                format!("The external PDF tool input could not be written: {error}")
            })
        });
    if let Err(error) = write_result {
        let _ = child.terminate_tree();
        let _ = child.wait();
        let _ = finish_child_output(stdout_reader);
        let _ = finish_child_output(stderr_reader);
        return Err(error);
    }

    let deadline = Instant::now() + QPDF_TIMEOUT;
    loop {
        if let Err(cancelled) = control.ensure_not_cancelled() {
            let _ = child.terminate_tree();
            let _ = child.wait();
            let _ = finish_child_output(stdout_reader);
            let _ = finish_child_output(stderr_reader);
            return Err(cancelled);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Output {
                    status,
                    stdout: finish_child_output(stdout_reader),
                    stderr: finish_child_output(stderr_reader),
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(75)),
            Ok(None) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                let _ = finish_child_output(stdout_reader);
                let _ = finish_child_output(stderr_reader);
                return Err(
                    "The external PDF tool exceeded its 30-minute safety timeout and was stopped."
                        .to_string(),
                );
            }
            Err(error) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                let _ = finish_child_output(stdout_reader);
                let _ = finish_child_output(stderr_reader);
                return Err(format!(
                    "The external PDF tool could not be monitored safely: {error}"
                ));
            }
        }
    }
}

fn read_child_output<R>(mut pipe: R) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        {
            let mut limited = pipe.by_ref().take((MAX_QPDF_DIAGNOSTIC_BYTES + 1) as u64);
            let _ = limited.read_to_end(&mut output);
        }
        output.truncate(MAX_QPDF_DIAGNOSTIC_BYTES);
        let _ = io::copy(&mut pipe, &mut io::sink());
        output
    })
}

fn finish_child_output(reader: Option<thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

fn verify_source_fingerprint(
    path: &Path,
    expected_size: u64,
    expected_modified_at_ms: Option<u64>,
) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("The reviewed source PDF could not be checked: {error}"))?;
    if !metadata.is_file()
        || metadata.len() != expected_size
        || modified_at_ms(&metadata) != expected_modified_at_ms
    {
        return Err(
            "The source PDF changed after it was reviewed. Review it again before changing protection."
                .to_string(),
        );
    }
    Ok(())
}

fn modified_at_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

pub(crate) fn validate_password(
    label: &str,
    password: &str,
    allow_empty: bool,
) -> Result<(), String> {
    if !allow_empty && password.is_empty() {
        return Err(format!("{label} is required."));
    }
    reject_control_characters(label, password)?;

    if password.len() > MAX_PASSWORD_BYTES {
        return Err(format!(
            "{label} must be no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
        ));
    }

    Ok(())
}

fn redact_sensitive_values(message: &str, sensitive_values: &[&str]) -> String {
    sensitive_values
        .iter()
        .filter(|value| !value.is_empty())
        .fold(message.to_string(), |redacted, value| {
            redacted.replace(value, "[redacted]")
        })
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn rejects_line_breaks_in_passwords() {
        let error = validate_password("Password", "line one\nline two", false).unwrap_err();
        assert!(error.contains("line breaks"));
    }

    #[test]
    fn rejects_passwords_over_the_pdf_limit() {
        let password = "a".repeat(MAX_PASSWORD_BYTES + 1);
        let error = validate_password("Password", &password, false).unwrap_err();
        assert!(error.contains("127 UTF-8 bytes"));
    }

    #[test]
    fn counts_unicode_passwords_as_utf8_bytes() {
        let password = "\u{00e9}".repeat(64);
        assert!(validate_password("Password", &password, false).is_err());
    }

    #[test]
    fn redacts_every_non_empty_secret() {
        let message = redact_sensitive_values(
            "user-secret and owner-secret",
            &["user-secret", "owner-secret", ""],
        );
        assert_eq!(message, "[redacted] and [redacted]");
    }

    #[test]
    fn maps_permissions_to_qpdf_values() {
        assert_eq!(PrintPermission::Low.as_qpdf_value(), "low");
        assert_eq!(ModificationPermission::Assembly.as_qpdf_value(), "assembly");
    }

    #[test]
    fn batch_helpers_decrypt_inputs_and_reprotect_with_distinct_passwords() {
        assert_eq!(
            decrypt_pdf_arguments("source.pdf", "unlocked.pdf", "current-secret"),
            [
                "source.pdf",
                "unlocked.pdf",
                "--warning-exit-0",
                "--password-mode=unicode",
                "--password=current-secret",
                "--decrypt",
            ]
        );
        let arguments = lock_pdf_arguments(
            "source.pdf",
            "protected.pdf",
            Some("current-secret"),
            "opening-secret",
            "administrator-secret",
        );
        assert_eq!(arguments[4], "--password=current-secret");
        assert_eq!(arguments[5], "--encrypt");
        assert_eq!(arguments[6], "opening-secret");
        assert_eq!(arguments[7], "administrator-secret");
        assert!(arguments.contains(&"--modify=none".to_string()));
    }

    #[test]
    fn protection_workflows_require_signed_source_acknowledgement() {
        let directory = TestDirectory::new();
        let input = directory.path.join("signed.pdf");
        let protected_output = directory.path.join("protected.pdf");
        let unlocked_output = directory.path.join("unlocked.pdf");
        signed_document().save(&input).unwrap().sync_all().unwrap();
        let source_metadata = fs::metadata(&input).unwrap();
        let source_modified_at_ms = modified_at_ms(&source_metadata);
        let source_size = source_metadata.len();

        let protect_error = protect_pdf(ProtectPdfRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: protected_output.to_string_lossy().into_owned(),
            input_password: None,
            open_password: "opening-password".to_string(),
            owner_password: "administrator-password".to_string(),
            print_permission: PrintPermission::Full,
            modification_permission: ModificationPermission::None,
            allow_copying: true,
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: source_modified_at_ms,
            expected_source_size: source_size,
        })
        .unwrap_err();
        assert!(protect_error.contains("certificate signature"));
        assert!(!protected_output.exists());

        let remove_error = remove_pdf_protection(RemoveProtectionRequest {
            input_path: input.to_string_lossy().into_owned(),
            output_path: unlocked_output.to_string_lossy().into_owned(),
            password: String::new(),
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: source_modified_at_ms,
            expected_source_size: source_size,
        })
        .unwrap_err();
        assert!(remove_error.contains("certificate signature"));
        assert!(!unlocked_output.exists());
    }

    #[test]
    fn cancellation_stops_a_running_external_pdf_process() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(|_, _| {});
        let control = PdfJobExecutionControl::new(Arc::clone(&cancelled), progress);
        let cancellation = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            cancelled.store(true, Ordering::Release);
        });
        let mut command = long_running_test_command();
        let started = Instant::now();
        let error = run_child_with_control(&mut command, b"", &control).unwrap_err();
        cancellation.join().unwrap();

        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn source_changed_after_protection_verification_is_never_published() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("protected.pdf");
        unsigned_document()
            .save(&input)
            .unwrap()
            .sync_all()
            .unwrap();
        let metadata = fs::metadata(&input).unwrap();
        let progress_values = Arc::new(Mutex::new(Vec::new()));
        let recorded_progress = Arc::clone(&progress_values);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |progress, _| recorded_progress.lock().unwrap().push(progress)),
        );
        let calls = AtomicUsize::new(0);
        let runner = |arguments: &[String],
                      _sensitive_values: &[&str],
                      _control: &PdfJobExecutionControl| {
            if calls.fetch_add(1, Ordering::AcqRel) == 0 {
                fs::copy(&arguments[0], &arguments[1])
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            } else {
                fs::write(&input, b"source changed after verification")
                    .map_err(|error| error.to_string())
            }
        };

        let error = protect_pdf_with_runner(
            ProtectPdfRequest {
                input_path: input.to_string_lossy().into_owned(),
                output_path: output.to_string_lossy().into_owned(),
                input_password: None,
                open_password: "opening-password".to_string(),
                owner_password: "administrator-password".to_string(),
                print_permission: PrintPermission::Full,
                modification_permission: ModificationPermission::None,
                allow_copying: true,
                acknowledge_certificate_signatures: false,
                expected_source_modified_at_ms: modified_at_ms(&metadata),
                expected_source_size: metadata.len(),
            },
            &control,
            &runner,
        )
        .unwrap_err();

        assert!(error.contains("changed after it was reviewed"));
        assert_eq!(calls.load(Ordering::Acquire), 2);
        assert!(!output.exists());
        let progress = progress_values.lock().unwrap();
        assert!(progress.windows(2).all(|values| values[0] <= values[1]));
        assert_eq!(progress.last(), Some(&95));
    }

    #[cfg(windows)]
    fn long_running_test_command() -> Command {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 30",
        ]);
        command
    }

    #[cfg(not(windows))]
    fn long_running_test_command() -> Command {
        let mut command = Command::new("sleep");
        command.arg("30");
        command
    }

    fn unsigned_document() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
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
        let catalogue_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalogue_id);
        document
    }

    fn signed_document() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
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
        let signature_id = document.add_object(dictionary! {
            "FT" => "Sig",
            "V" => dictionary! {
                "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
            },
        });
        let catalogue_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => dictionary! { "Fields" => vec![signature_id.into()] },
        });
        document.trailer.set("Root", catalogue_id);
        document
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-protection-test",
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
