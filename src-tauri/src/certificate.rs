use crate::child_process::ManagedChild;
use crate::file_safety::{
    canonical_pdf_input, paths_are_equal, reject_control_characters, validated_new_pdf_output,
    TemporaryOutput,
};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::runtime_capabilities::current_capabilities;
use crate::temporary_cleanup::{register_temporary_path, TemporaryKind, TemporaryLease};
use lopdf::{decode_text_string, Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::{Host, Url};
use zeroize::Zeroizing;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

const ENGINE_TIMEOUT: Duration = Duration::from_secs(8);
const SIGN_TIMEOUT: Duration = Duration::from_secs(240);
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(150);
const ENGINE_OUTPUT_LIMIT: usize = 32 * 1024;
const VALIDATION_OUTPUT_LIMIT: usize = 512 * 1024;
const REPORT_TEXT_LIMIT: usize = 64 * 1024;
const MAX_CERTIFICATE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TRUST_ROOT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TRUST_ROOTS: usize = 16;
const MAX_PASSPHRASE_BYTES: usize = 1024;
const MAX_FIELD_NAME_BYTES: usize = 64;
const MAX_TIMESTAMP_URL_BYTES: usize = 2048;
const MAX_SIGNATURE_INSPECTION_NODES: usize = 2_000_000;
const MAX_SIGNATURE_REPORT_FIELDS: usize = 512;
const MAX_SIGNATURE_FIELD_TEXT_BYTES: usize = 1024;
const PYHANKO_PASSWORD_BRIDGE_SOURCE: &[u8] = include_bytes!("pyhanko_password_bridge.py");
const PYHANKO_PASSWORD_BRIDGE_FILE: &str = "sitecustomize.py";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateCapabilities {
    available: bool,
    provider: &'static str,
    version: Option<String>,
    passfile_supported: bool,
    detail: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CertificatePosition {
    Left,
    Centre,
    Right,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CertificateSignRequest {
    input_path: String,
    #[serde(default)]
    input_password: Option<String>,
    output_path: String,
    pkcs12_path: String,
    pkcs12_passphrase: String,
    pkcs12_passphrase_confirmation: String,
    visible: bool,
    page_number: Option<u32>,
    position: Option<CertificatePosition>,
    field_name: String,
    timestamp_url: Option<String>,
    embed_validation_info: bool,
    #[serde(default)]
    trust_roots: Vec<String>,
}

impl std::fmt::Debug for CertificateSignRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CertificateSignRequest { redacted: true }")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InspectCertificateRequest {
    pub(crate) input_path: String,
    #[serde(default)]
    pub(crate) input_password: Option<String>,
    #[serde(default)]
    pub(crate) trust_roots: Vec<String>,
}

impl std::fmt::Debug for InspectCertificateRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("InspectCertificateRequest { redacted: true }")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CertificateValidationState {
    Unsigned,
    Valid,
    Invalid,
    Indeterminate,
    Unavailable,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateSignatureField {
    name: String,
    signed: bool,
    kind: &'static str,
    reason: Option<String>,
    location: Option<String>,
    signing_time: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateValidationReport {
    input_path: String,
    encrypted: bool,
    signature_count: usize,
    timestamp_count: usize,
    fields: Vec<CertificateSignatureField>,
    state: CertificateValidationState,
    intact: Option<bool>,
    trusted: Option<bool>,
    engine_version: Option<String>,
    summary: String,
    details: String,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateSignResult {
    output_path: String,
    bytes_written: u64,
    encrypted: bool,
    field_name: String,
    visible: bool,
    timestamped: bool,
    validation: CertificateValidationReport,
    warnings: Vec<String>,
}

#[derive(Default)]
struct SignatureStructure {
    fields: Vec<CertificateSignatureField>,
    field_limit_reached: bool,
    signature_count: usize,
    timestamp_count: usize,
    valid_byte_ranges: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ValidationSignals {
    intact: Option<bool>,
    trusted: Option<bool>,
    timestamp_present: bool,
}

struct SignCommandArguments<'a> {
    input: &'a Path,
    output: &'a Path,
    pkcs12: &'a Path,
    passfile: &'a Path,
    field_spec: &'a str,
    timestamp_url: Option<&'a str>,
    embed_validation_info: bool,
    trust_roots: &'a [PathBuf],
}

#[derive(Debug)]
struct CapturedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}

#[derive(Debug)]
enum CommandRunError {
    Setup(String),
    Start(std::io::Error),
    Input(std::io::Error),
    Monitor(std::io::Error),
    Cancelled,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceFingerprint {
    bytes: u64,
    modified: Option<SystemTime>,
    sha256: [u8; 32],
}

#[tauri::command]
pub fn certificate_capabilities() -> CertificateCapabilities {
    if !current_capabilities().certificate_signing() {
        return CertificateCapabilities {
            available: false,
            provider: "pyHanko",
            version: None,
            passfile_supported: false,
            detail:
                "Certificate signing requires a desktop engine and is unavailable on this platform."
                    .to_string(),
        };
    }
    inspect_capabilities()
}

#[cfg(test)]
pub fn certificate_sign_pdf(
    request: CertificateSignRequest,
) -> Result<CertificateSignResult, String> {
    certificate_sign_pdf_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn validate_certificate_sign_request(
    request: &CertificateSignRequest,
) -> Result<(), String> {
    validate_optional_pdf_password(request.input_password.as_deref())?;
    validate_secret("Certificate passphrase", &request.pkcs12_passphrase)?;
    validate_secret(
        "Certificate passphrase confirmation",
        &request.pkcs12_passphrase_confirmation,
    )?;
    if request.pkcs12_passphrase != request.pkcs12_passphrase_confirmation {
        return Err("The certificate passphrases do not match.".to_string());
    }
    validate_field_name(&request.field_name)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let output = validated_new_pdf_output(&request.output_path)?;
    if paths_are_equal(&input, &output) {
        return Err("The source PDF cannot be overwritten. Choose a new filename.".to_string());
    }
    let pkcs12 = validate_pkcs12_file(&request.pkcs12_path)?;
    if paths_are_equal(&input, &pkcs12) || paths_are_equal(&output, &pkcs12) {
        return Err(
            "The certificate file must be separate from the source and destination PDF files."
                .to_string(),
        );
    }
    validate_trust_roots(&request.trust_roots)?;
    let timestamp_url = request
        .timestamp_url
        .as_deref()
        .map(validate_timestamp_url)
        .transpose()?;
    if request.embed_validation_info && timestamp_url.is_none() {
        return Err(
            "A trusted timestamp URL is required when embedding long-term validation information."
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn validate_inspect_certificate_request(
    request: &InspectCertificateRequest,
) -> Result<(), String> {
    validate_optional_pdf_password(request.input_password.as_deref())?;
    canonical_pdf_input(&request.input_path)?;
    validate_trust_roots(&request.trust_roots)?;
    Ok(())
}

pub(crate) fn certificate_sign_pdf_with_control(
    request: CertificateSignRequest,
    control: &PdfJobExecutionControl,
) -> Result<CertificateSignResult, String> {
    control.checkpoint(1, "Validating certificate signing request")?;
    validate_certificate_sign_request(&request)?;
    let CertificateSignRequest {
        input_path,
        input_password,
        output_path,
        pkcs12_path,
        pkcs12_passphrase,
        pkcs12_passphrase_confirmation,
        visible,
        page_number,
        position,
        field_name,
        timestamp_url,
        embed_validation_info,
        trust_roots,
    } = request;

    let input_password = input_password.map(Zeroizing::new);
    let passphrase = Zeroizing::new(pkcs12_passphrase);
    let confirmation = Zeroizing::new(pkcs12_passphrase_confirmation);
    validate_secret("Certificate passphrase", &passphrase)?;
    validate_secret("Certificate passphrase confirmation", &confirmation)?;
    if passphrase.as_str() != confirmation.as_str() {
        return Err("The certificate passphrases do not match.".to_string());
    }
    validate_field_name(&field_name)?;

    control.checkpoint(8, "Checking the local certificate engine")?;
    let capabilities = inspect_capabilities_with_control(Some(control))?;
    if !capabilities.available || !capabilities.passfile_supported {
        return Err(capabilities.detail);
    }

    let input = canonical_pdf_input(&input_path)?;
    let opening_source_fingerprint = source_fingerprint(&input, Some(control))?;
    let output = validated_new_pdf_output(&output_path)?;
    if paths_are_equal(&input, &output) {
        return Err("The source PDF cannot be overwritten. Choose a new filename.".to_string());
    }
    let pkcs12 = validate_pkcs12_file(&pkcs12_path)?;
    let pkcs12_fingerprint = source_fingerprint(&pkcs12, Some(control))?;
    if paths_are_equal(&input, &pkcs12) || paths_are_equal(&output, &pkcs12) {
        return Err(
            "The certificate file must be separate from the source and destination PDF files."
                .to_string(),
        );
    }
    let trust_roots = validate_trust_roots(&trust_roots)?;
    let trust_fingerprints = trust_roots
        .iter()
        .map(|path| {
            source_fingerprint(path, Some(control)).map(|fingerprint| (path.clone(), fingerprint))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let timestamp_url = timestamp_url
        .as_deref()
        .map(validate_timestamp_url)
        .transpose()?;
    if embed_validation_info && timestamp_url.is_none() {
        return Err(
            "A trusted timestamp URL is required when embedding long-term validation information."
                .to_string(),
        );
    }

    control.checkpoint(16, "Opening source PDF and certificate")?;
    let LoadedCertificatePdf {
        document,
        encrypted: source_encrypted,
    } = load_certificate_pdf(
        &input,
        input_password.as_ref().map(|password| password.as_str()),
    )?;
    let pyhanko_password = if source_encrypted {
        Some(
            input_password
                .as_ref()
                .map_or("", |password| password.as_str()),
        )
    } else {
        None
    };
    let source_structure = inspect_signature_structure_with_control(
        &document,
        fs::metadata(&input)
            .map_err(|error| format!("The source PDF could not be inspected: {error}"))?
            .len(),
        control,
    )?;
    let field_spec = if visible {
        visible_field_spec(
            &document,
            page_number.ok_or_else(|| {
                "Choose a page for the visible certificate signature.".to_string()
            })?,
            position.ok_or_else(|| {
                "Choose a position for the visible certificate signature.".to_string()
            })?,
            &field_name,
        )?
    } else {
        field_name.clone()
    };

    control.checkpoint(28, "Preparing the private signing workspace")?;
    let workspace = TemporaryCertificateWorkspace::new()?;
    let signing_input =
        workspace.snapshot_file(&input, "source.pdf", opening_source_fingerprint, control)?;
    let signing_identity =
        workspace.snapshot_file(&pkcs12, "signer.p12", pkcs12_fingerprint, control)?;
    let signing_trust_roots = trust_fingerprints
        .iter()
        .enumerate()
        .map(|(index, (path, fingerprint))| {
            workspace.snapshot_file(path, &format!("trust-{index}.cer"), *fingerprint, control)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let temporary = TemporaryOutput::new(&output)?;
    let passfile = TemporaryPassfile::new(passphrase.as_bytes())?;
    let arguments = build_sign_arguments(SignCommandArguments {
        input: &signing_input,
        output: temporary.path(),
        pkcs12: &signing_identity,
        passfile: passfile.path(),
        field_spec: &field_spec,
        timestamp_url: timestamp_url.as_deref(),
        embed_validation_info,
        trust_roots: &signing_trust_roots,
    });
    control.checkpoint(35, "Applying certificate signature")?;
    let sign_output = run_pyhanko_with_control(
        &arguments,
        VALIDATION_OUTPUT_LIMIT,
        SIGN_TIMEOUT,
        pyhanko_password,
        Some(control),
    )
    .map_err(|error| {
        controlled_command_error("pyHanko could not complete certificate signing", error)
    })?;
    drop(passfile);
    if !sign_output.status.success() {
        return Err(format!(
            "pyHanko did not create a certificate-signed PDF: {}",
            command_diagnostic(
                &sign_output,
                &[passphrase.as_str(), pyhanko_password.unwrap_or_default()],
            )
        ));
    }

    control.checkpoint(68, "Reopening signed PDF")?;
    let temporary_metadata = fs::metadata(temporary.path())
        .map_err(|error| format!("The signed PDF was not created: {error}"))?;
    if !temporary_metadata.is_file() || temporary_metadata.len() == 0 {
        return Err("pyHanko returned success without creating a signed PDF.".to_string());
    }
    let LoadedCertificatePdf {
        document: signed_document,
        encrypted: signed_encrypted,
    } = load_certificate_pdf(temporary.path(), pyhanko_password)
        .map_err(|error| format!("The signed PDF could not be reopened safely: {error}"))?;
    if signed_encrypted != source_encrypted {
        return Err(
            "The signed PDF did not preserve the source document's encryption state and was not published."
                .to_string(),
        );
    }
    let signed_structure = inspect_signature_structure_with_control(
        &signed_document,
        temporary_metadata.len(),
        control,
    )?;
    if signed_structure.signature_count <= source_structure.signature_count {
        return Err("The output PDF did not contain a new certificate signature.".to_string());
    }
    if signed_structure.valid_byte_ranges == 0 {
        return Err(
            "The new certificate signature did not contain a complete PDF byte range.".to_string(),
        );
    }

    control.checkpoint(78, "Validating signature integrity and trust")?;
    let validation_output = run_pyhanko_with_control(
        &build_validation_arguments(temporary.path(), &signing_trust_roots, false),
        VALIDATION_OUTPUT_LIMIT,
        VALIDATE_TIMEOUT,
        pyhanko_password,
        Some(control),
    )
    .map_err(|error| {
        controlled_command_error(
            "The signed PDF was created temporarily, but pyHanko could not validate it",
            error,
        )
    })?;
    let validation_text = output_text(&validation_output);
    let signals = parse_validation_signals(&validation_text, validation_output.status.success());
    if signals.intact != Some(true) {
        return Err(format!(
            "The signed PDF failed post-signature integrity verification and was not published: {}",
            command_diagnostic(&validation_output, &[pyhanko_password.unwrap_or_default()])
        ));
    }

    let mut warnings = Vec::new();
    if signals.trusted != Some(true) {
        warnings.push(
            "The certificate signature is structurally intact, but pyHanko did not establish a trusted certificate chain. Add the appropriate root certificate and validate again before relying on signer identity."
                .to_string(),
        );
    }
    if timestamp_url.is_none() {
        warnings.push(
            "No trusted timestamp was requested. The signing time may be self-reported rather than independently proven."
                .to_string(),
        );
    }

    control.checkpoint(94, "Rechecking source PDF before publication")?;
    verify_source_fingerprint(&input, opening_source_fingerprint, control)?;
    verify_signing_identity_fingerprint(&pkcs12, pkcs12_fingerprint, control)?;
    for (path, fingerprint) in &trust_fingerprints {
        verify_trust_root_fingerprint(path, *fingerprint, control)?;
    }
    control.checkpoint(99, "Publishing verified certificate-signed PDF")?;
    let bytes_written = temporary.persist(&output)?;
    let validation = report_from_output(
        &output,
        signed_structure,
        capabilities.version,
        &validation_output,
        source_encrypted,
        &[pyhanko_password.unwrap_or_default()],
        warnings.clone(),
    );
    Ok(CertificateSignResult {
        output_path: output.to_string_lossy().into_owned(),
        bytes_written,
        encrypted: source_encrypted,
        field_name,
        visible,
        timestamped: timestamp_url.is_some(),
        validation,
        warnings,
    })
}

pub(crate) fn run_certificate_sign_job_with_control(
    request: CertificateSignRequest,
    control: &PdfJobExecutionControl,
) -> Result<CertificateSignResult, String> {
    certificate_sign_pdf_with_control(request, control)
        .map(job_safe_certificate_result)
        .map_err(|error| {
            if error == PDF_JOB_CANCELLED_ERROR {
                error
            } else {
                safe_certificate_job_error(&error)
            }
        })
}

fn controlled_command_error(context: &str, error: CommandRunError) -> String {
    if matches!(error, CommandRunError::Cancelled) {
        PDF_JOB_CANCELLED_ERROR.to_string()
    } else {
        format!("{context}: {}", error.detail())
    }
}

fn source_fingerprint(
    path: &Path,
    control: Option<&PdfJobExecutionControl>,
) -> Result<SourceFingerprint, String> {
    let opening_metadata = fs::metadata(path)
        .map_err(|error| format!("A certificate input could not be inspected: {error}"))?;
    if !opening_metadata.is_file() {
        return Err("A certificate input is no longer an ordinary file.".to_string());
    }
    let mut file = File::open(path)
        .map_err(|error| format!("A certificate input could not be read: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes_read = 0_u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("A certificate input could not be read: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes_read = bytes_read.saturating_add(count as u64);
        if let Some(control) = control {
            control.ensure_not_cancelled()?;
        }
    }
    let closing_metadata = file
        .metadata()
        .map_err(|error| format!("A certificate input could not be rechecked: {error}"))?;
    if opening_metadata.len() != bytes_read
        || closing_metadata.len() != bytes_read
        || opening_metadata.modified().ok() != closing_metadata.modified().ok()
    {
        return Err("A certificate input changed while it was being inspected.".to_string());
    }
    Ok(SourceFingerprint {
        bytes: bytes_read,
        modified: closing_metadata.modified().ok(),
        sha256: hasher.finalize().into(),
    })
}

fn verify_source_fingerprint(
    path: &Path,
    expected: SourceFingerprint,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if source_fingerprint(path, Some(control))? != expected {
        return Err(
            "The source PDF changed on disk during certificate signing. Choose it again before signing."
                .to_string(),
        );
    }
    Ok(())
}

fn verify_validation_source_fingerprint(
    path: &Path,
    expected: SourceFingerprint,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if source_fingerprint(path, Some(control))? != expected {
        return Err(
            "The source PDF changed on disk during certificate validation. Choose it again before validating."
                .to_string(),
        );
    }
    Ok(())
}

fn verify_trust_root_fingerprint(
    path: &Path,
    expected: SourceFingerprint,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if source_fingerprint(path, Some(control))? != expected {
        return Err(
            "A trust-root certificate changed on disk during certificate validation. Choose it again before validating."
                .to_string(),
        );
    }
    Ok(())
}

fn verify_signing_identity_fingerprint(
    path: &Path,
    expected: SourceFingerprint,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if source_fingerprint(path, Some(control))? != expected {
        return Err(
            "The signing certificate changed on disk during certificate signing. Choose it again before signing."
                .to_string(),
        );
    }
    Ok(())
}

fn job_safe_certificate_result(mut result: CertificateSignResult) -> CertificateSignResult {
    result.output_path = Path::new(&result.output_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "certificate-signed.pdf".to_string());
    result.field_name.clear();
    result.validation.input_path.clear();
    result.validation.fields.clear();
    result.validation.details.clear();
    result
}

fn safe_certificate_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("signing certificate changed") {
        return "The signing certificate changed during certificate signing. Choose it again before signing."
            .to_string();
    }
    if normalised.contains("trust-root certificate changed") {
        return "A trust certificate changed during certificate signing. Choose it again before signing."
            .to_string();
    }
    if normalised.contains("source pdf changed") {
        return "The source PDF changed during certificate signing. Choose it again before signing."
            .to_string();
    }
    if normalised.contains("certificate input changed") {
        return "A certificate-signing input changed during its safety review. Choose the PDF, signing certificate and trust certificates again."
            .to_string();
    }
    if normalised.contains("not found on path")
        || normalised.contains("certificate engine")
        || normalised.contains("pyhanko")
    {
        return "Certificate signing could not use the local pyHanko engine. Check Certificate Studio readiness and try again."
            .to_string();
    }
    if normalised.contains("passphrase") || normalised.contains("pkcs#12") {
        return "The signing certificate or its passphrase could not be accepted.".to_string();
    }
    if normalised.contains("pdf password") || normalised.contains("document's encryption state") {
        return "The source PDF password or its protected output could not be accepted safely."
            .to_string();
    }
    if normalised.contains("timestamp") {
        return "The trusted timestamp service could not complete certificate signing.".to_string();
    }
    "Certificate signing failed a safety or cryptographic verification check. Review the settings and try again."
        .to_string()
}

#[cfg(test)]
pub fn inspect_certificate_signatures(
    request: InspectCertificateRequest,
) -> Result<CertificateValidationReport, String> {
    inspect_certificate_signatures_with_control(request, &PdfJobExecutionControl::direct())
}

pub(crate) fn run_certificate_validation_job_with_control(
    request: InspectCertificateRequest,
    control: &PdfJobExecutionControl,
) -> Result<CertificateValidationReport, String> {
    let sensitive_paths = certificate_validation_sensitive_paths(&request);
    inspect_certificate_signatures_with_control(request, control)
        .map(|report| job_safe_certificate_validation_report(report, &sensitive_paths))
        .map_err(|error| {
            if error == PDF_JOB_CANCELLED_ERROR {
                error
            } else {
                safe_certificate_validation_job_error(&error)
            }
        })
}

pub(crate) fn inspect_certificate_signatures_with_control(
    request: InspectCertificateRequest,
    control: &PdfJobExecutionControl,
) -> Result<CertificateValidationReport, String> {
    control.checkpoint(1, "Validating certificate review request")?;
    validate_inspect_certificate_request(&request)?;
    let InspectCertificateRequest {
        input_path,
        input_password,
        trust_roots,
    } = request;
    let input_password = input_password.map(Zeroizing::new);

    control.checkpoint(8, "Opening PDF for certificate review")?;
    let input = canonical_pdf_input(&input_path)?;
    let source_snapshot = source_fingerprint(&input, Some(control))?;
    let trust_roots = validate_trust_roots(&trust_roots)?;
    let trust_fingerprints = trust_roots
        .iter()
        .map(|path| {
            source_fingerprint(path, Some(control)).map(|fingerprint| (path.clone(), fingerprint))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let LoadedCertificatePdf {
        document,
        encrypted: source_encrypted,
    } = load_certificate_pdf(
        &input,
        input_password.as_ref().map(|password| password.as_str()),
    )?;
    let pyhanko_password = if source_encrypted {
        Some(
            input_password
                .as_ref()
                .map_or("", |password| password.as_str()),
        )
    } else {
        None
    };

    control.checkpoint(24, "Inspecting certificate signature fields")?;
    let structure =
        inspect_signature_structure_with_control(&document, source_snapshot.bytes, control)?;
    if structure.signature_count == 0 {
        let warnings = signature_field_limit_warnings(&structure);
        let report = CertificateValidationReport {
            input_path: input.to_string_lossy().into_owned(),
            encrypted: source_encrypted,
            signature_count: 0,
            timestamp_count: 0,
            fields: structure.fields,
            state: CertificateValidationState::Unsigned,
            intact: None,
            trusted: None,
            engine_version: None,
            summary: "No filled certificate signatures were found in this PDF.".to_string(),
            details: "Unsigned signature fields may still be listed above.".to_string(),
            warnings,
        };
        return finish_certificate_validation(
            report,
            &input,
            source_snapshot,
            &trust_fingerprints,
            control,
        );
    }

    control.checkpoint(42, "Checking the local certificate validation engine")?;
    let capabilities = inspect_capabilities_with_control(Some(control))?;
    if !capabilities.available {
        let mut warnings = signature_field_limit_warnings(&structure);
        warnings.push(
            "Structural signature detection does not prove cryptographic integrity or signer identity."
                .to_string(),
        );
        let report = CertificateValidationReport {
            input_path: input.to_string_lossy().into_owned(),
            encrypted: source_encrypted,
            signature_count: structure.signature_count,
            timestamp_count: structure.timestamp_count,
            fields: structure.fields,
            state: CertificateValidationState::Unavailable,
            intact: None,
            trusted: None,
            engine_version: capabilities.version,
            summary: "Certificate signatures were found, but pyHanko is unavailable for cryptographic validation.".to_string(),
            details: capabilities.detail,
            warnings,
        };
        return finish_certificate_validation(
            report,
            &input,
            source_snapshot,
            &trust_fingerprints,
            control,
        );
    }

    control.checkpoint(58, "Validating signature integrity and trust")?;
    let workspace = TemporaryCertificateWorkspace::new()?;
    let validation_input =
        workspace.snapshot_file(&input, "source.pdf", source_snapshot, control)?;
    let validation_trust_roots = trust_fingerprints
        .iter()
        .enumerate()
        .map(|(index, (path, fingerprint))| {
            workspace.snapshot_file(path, &format!("trust-{index}.cer"), *fingerprint, control)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let validation_output = run_pyhanko_with_control(
        &build_validation_arguments(&validation_input, &validation_trust_roots, true),
        VALIDATION_OUTPUT_LIMIT,
        VALIDATE_TIMEOUT,
        pyhanko_password,
        Some(control),
    )
    .map_err(|error| controlled_command_error("pyHanko validation could not complete", error))?;
    control.checkpoint(92, "Reviewing certificate validation report")?;
    let report = report_from_output(
        &input,
        structure,
        capabilities.version,
        &validation_output,
        source_encrypted,
        &[pyhanko_password.unwrap_or_default()],
        Vec::new(),
    );
    finish_certificate_validation(
        report,
        &input,
        source_snapshot,
        &trust_fingerprints,
        control,
    )
}

fn finish_certificate_validation(
    report: CertificateValidationReport,
    input: &Path,
    source_fingerprint: SourceFingerprint,
    trust_fingerprints: &[(PathBuf, SourceFingerprint)],
    control: &PdfJobExecutionControl,
) -> Result<CertificateValidationReport, String> {
    control.checkpoint(98, "Rechecking certificate validation inputs")?;
    verify_validation_source_fingerprint(input, source_fingerprint, control)?;
    for (path, fingerprint) in trust_fingerprints {
        verify_trust_root_fingerprint(path, *fingerprint, control)?;
    }
    control.ensure_not_cancelled()?;
    Ok(report)
}

fn certificate_validation_sensitive_paths(request: &InspectCertificateRequest) -> Vec<String> {
    let mut paths = vec![request.input_path.clone()];
    paths.extend(request.trust_roots.iter().cloned());
    for path in std::iter::once(&request.input_path).chain(request.trust_roots.iter()) {
        if let Ok(canonical) = fs::canonicalize(path) {
            paths.push(canonical.to_string_lossy().into_owned());
        }
    }
    let mut variants = Vec::new();
    for path in paths {
        variants.push(path.clone());
        variants.push(path.replace('\\', "/"));
        variants.push(path.replace('/', "\\"));
    }
    variants.retain(|path| !path.is_empty());
    variants.sort_by_key(|path| std::cmp::Reverse(path.len()));
    variants.dedup();
    variants
}

fn job_safe_certificate_validation_report(
    mut report: CertificateValidationReport,
    sensitive_paths: &[String],
) -> CertificateValidationReport {
    report.input_path = "PDF".to_string();
    redact_sensitive_paths(&mut report.summary, sensitive_paths);
    report.details.clear();
    for warning in &mut report.warnings {
        redact_sensitive_paths(warning, sensitive_paths);
    }
    for field in &mut report.fields {
        redact_sensitive_paths(&mut field.name, sensitive_paths);
        if let Some(reason) = &mut field.reason {
            redact_sensitive_paths(reason, sensitive_paths);
        }
        if let Some(location) = &mut field.location {
            redact_sensitive_paths(location, sensitive_paths);
        }
        if let Some(signing_time) = &mut field.signing_time {
            redact_sensitive_paths(signing_time, sensitive_paths);
        }
    }
    report
}

fn signature_field_limit_warnings(structure: &SignatureStructure) -> Vec<String> {
    structure
        .field_limit_reached
        .then(|| {
            format!(
                "Only the first {MAX_SIGNATURE_REPORT_FIELDS} signature fields are listed because the PDF exceeds the bounded report limit."
            )
        })
        .into_iter()
        .collect()
}

fn redact_sensitive_paths(value: &mut String, sensitive_paths: &[String]) {
    for path in sensitive_paths {
        if value.contains(path) {
            *value = value.replace(path, "[local file]");
        }
    }
}

fn safe_certificate_validation_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed on disk") {
        return "A certificate validation input changed during the review. Choose the PDF and trust certificates again."
            .to_string();
    }
    if normalised.contains("pdf password") || normalised.contains("encrypted") {
        return "The source PDF password could not be accepted for certificate validation."
            .to_string();
    }
    if normalised.contains("trust-root") || normalised.contains("trust certificate") {
        return "A selected trust certificate could not be read safely. Choose it again."
            .to_string();
    }
    if normalised.contains("not found on path")
        || normalised.contains("certificate engine")
        || normalised.contains("pyhanko")
    {
        return "Certificate validation could not use the local pyHanko engine. Check Certificate Studio readiness and try again."
            .to_string();
    }
    "Certificate validation could not complete a bounded integrity and trust review. Review the PDF and try again."
        .to_string()
}

fn inspect_capabilities() -> CertificateCapabilities {
    inspect_capabilities_with_control(None).unwrap_or_else(|error| CertificateCapabilities {
        available: false,
        provider: "pyHanko",
        version: None,
        passfile_supported: false,
        detail: error,
    })
}

fn inspect_capabilities_with_control(
    control: Option<&PdfJobExecutionControl>,
) -> Result<CertificateCapabilities, String> {
    let version_output = match run_pyhanko_with_control(
        &["--version".to_string()],
        ENGINE_OUTPUT_LIMIT,
        ENGINE_TIMEOUT,
        None,
        control,
    ) {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return Ok(CertificateCapabilities {
                available: false,
                provider: "pyHanko",
                version: None,
                passfile_supported: false,
                detail: format!(
                    "pyHanko is installed but did not start correctly: {}",
                    command_diagnostic(&output, &[])
                ),
            })
        }
        Err(CommandRunError::Cancelled) => return Err(PDF_JOB_CANCELLED_ERROR.to_string()),
        Err(error) => {
            return Ok(CertificateCapabilities {
                available: false,
                provider: "pyHanko",
                version: None,
                passfile_supported: false,
                detail: format!(
                    "Install pyHanko and pyhanko-cli, then make sure 'pyhanko' is on PATH. {}",
                    error.detail()
                ),
            })
        }
    };
    let version = first_output_line(&version_output.stdout)
        .or_else(|| first_output_line(&version_output.stderr));
    let help_arguments = [
        "sign".to_string(),
        "addsig".to_string(),
        "pkcs12".to_string(),
        "--help".to_string(),
    ];
    let help_output = match run_pyhanko_with_control(
        &help_arguments,
        ENGINE_OUTPUT_LIMIT,
        ENGINE_TIMEOUT,
        None,
        control,
    ) {
        Ok(output) => output,
        Err(CommandRunError::Cancelled) => return Err(PDF_JOB_CANCELLED_ERROR.to_string()),
        Err(error) => {
            return Ok(CertificateCapabilities {
                available: true,
                provider: "pyHanko",
                version,
                passfile_supported: false,
                detail: format!(
                    "pyHanko is ready for certificate validation, but its PKCS#12 signing command could not be inspected safely: {}",
                    error.detail()
                ),
            })
        }
    };
    let help = output_text(&help_output);
    let passfile_supported = help_output.status.success() && help.contains("--passfile");
    Ok(CertificateCapabilities {
        available: true,
        provider: "pyHanko",
        version,
        passfile_supported,
        detail: if passfile_supported {
            "pyHanko is ready for PKCS#12 signing and certificate validation.".to_string()
        } else {
            "pyHanko is ready for certificate validation, but the installed CLI does not expose the required PKCS#12 --passfile option. Upgrade pyHanko and pyhanko-cli before signing."
                .to_string()
        },
    })
}

struct LoadedCertificatePdf {
    document: Document,
    encrypted: bool,
}

fn load_certificate_pdf(
    path: &Path,
    input_password: Option<&str>,
) -> Result<LoadedCertificatePdf, String> {
    let mut document = Document::load(path)
        .map_err(|error| format!("The PDF could not be read safely: {error}"))?;
    let encrypted = document.is_encrypted() || document.was_encrypted();
    if document.is_encrypted() {
        document = Document::load_with_password(path, input_password.unwrap_or_default())
            .map_err(|_| "The source PDF password was not accepted.".to_string())?;
    }
    if document.get_pages().is_empty() {
        return Err("The PDF does not contain any readable pages.".to_string());
    }
    Ok(LoadedCertificatePdf {
        document,
        encrypted,
    })
}

fn validate_pkcs12_file(path: &str) -> Result<PathBuf, String> {
    reject_control_characters("Certificate path", path)?;
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("The PKCS#12 certificate could not be opened: {error}"))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("The PKCS#12 certificate could not be inspected: {error}"))?;
    let supported_extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("p12") || value.eq_ignore_ascii_case("pfx")
        });
    if !metadata.is_file() || !supported_extension {
        return Err("Choose an existing .p12 or .pfx PKCS#12 certificate file.".to_string());
    }
    if metadata.len() == 0 || metadata.len() > MAX_CERTIFICATE_BYTES {
        return Err(
            "The PKCS#12 certificate file is empty or exceeds the 64 MB safety limit.".to_string(),
        );
    }
    Ok(canonical)
}

fn validate_trust_roots(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    if paths.len() > MAX_TRUST_ROOTS {
        return Err(format!(
            "Choose no more than {MAX_TRUST_ROOTS} trust-root certificates."
        ));
    }
    let mut result = Vec::with_capacity(paths.len());
    let mut seen = HashSet::new();
    for path in paths {
        reject_control_characters("Trust-root path", path)?;
        let canonical = fs::canonicalize(path)
            .map_err(|error| format!("A trust-root certificate could not be opened: {error}"))?;
        let metadata = fs::metadata(&canonical)
            .map_err(|error| format!("A trust-root certificate could not be inspected: {error}"))?;
        let supported_extension = canonical
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                ["cer", "crt", "der", "pem"]
                    .iter()
                    .any(|extension| value.eq_ignore_ascii_case(extension))
            });
        if !metadata.is_file() || !supported_extension {
            return Err(
                "Trust roots must be existing PEM, CRT, CER or DER certificate files.".to_string(),
            );
        }
        if metadata.len() == 0 || metadata.len() > MAX_TRUST_ROOT_BYTES {
            return Err(
                "A trust-root certificate is empty or exceeds the 16 MB safety limit.".to_string(),
            );
        }
        if seen.insert(canonical.clone()) {
            result.push(canonical);
        }
    }
    Ok(result)
}

fn validate_secret(label: &str, value: &str) -> Result<(), String> {
    reject_control_characters(label, value)?;
    if value.len() > MAX_PASSPHRASE_BYTES {
        return Err(format!(
            "{label} must fit within {MAX_PASSPHRASE_BYTES} UTF-8 bytes."
        ));
    }
    Ok(())
}

fn validate_optional_pdf_password(value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        validate_secret("PDF password", value)?;
    }
    Ok(())
}

fn validate_field_name(value: &str) -> Result<(), String> {
    reject_control_characters("Signature field name", value)?;
    if value.is_empty()
        || value.len() > MAX_FIELD_NAME_BYTES
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(
            "The signature field name must contain 1-64 ASCII letters, numbers, dots, hyphens or underscores."
                .to_string(),
        );
    }
    Ok(())
}

fn validate_timestamp_url(value: &str) -> Result<String, String> {
    reject_control_characters("Timestamp URL", value)?;
    if value.is_empty()
        || value.len() > MAX_TIMESTAMP_URL_BYTES
        || value.chars().any(char::is_whitespace)
    {
        return Err("Enter a valid timestamp-service URL no longer than 2,048 bytes.".to_string());
    }
    let url = Url::parse(value).map_err(|_| "Enter a valid timestamp-service URL.".to_string())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Timestamp URLs cannot contain credentials, a query or a fragment.".to_string(),
        );
    }
    let secure = url.scheme() == "https";
    let local_http = url.scheme() == "http"
        && match url.host() {
            Some(Host::Domain("localhost")) => true,
            Some(Host::Ipv4(address)) => address.is_loopback(),
            Some(Host::Ipv6(address)) => address.is_loopback(),
            _ => false,
        };
    if !secure && !local_http {
        return Err(
            "Timestamp services must use HTTPS. Plain HTTP is accepted only for a loopback test service."
                .to_string(),
        );
    }
    if url.host().is_none() {
        return Err("The timestamp-service URL must include a host.".to_string());
    }
    Ok(url.to_string())
}

fn visible_field_spec(
    document: &Document,
    page_number: u32,
    position: CertificatePosition,
    field_name: &str,
) -> Result<String, String> {
    let pages = document.get_pages();
    let page_id = pages.get(&page_number).copied().ok_or_else(|| {
        format!(
            "Signature page {page_number} is outside this {}-page PDF.",
            pages.len()
        )
    })?;
    let (page, rotation) = page_geometry(document, page_id)?;
    let rectangle = certificate_rectangle(page, rotation, position);
    let rectangle = integer_certificate_rectangle(rectangle)?;
    Ok(format!(
        "{page_number}/{},{},{},{}/{field_name}",
        rectangle[0], rectangle[1], rectangle[2], rectangle[3]
    ))
}

fn integer_certificate_rectangle(rectangle: [f64; 4]) -> Result<[i64; 4], String> {
    if rectangle
        .iter()
        .any(|value| !value.is_finite() || *value < i64::MIN as f64 || *value > i64::MAX as f64)
    {
        return Err(
            "The signature rectangle is outside the supported coordinate range.".to_string(),
        );
    }
    let rounded = [
        rectangle[0].ceil() as i64,
        rectangle[1].ceil() as i64,
        rectangle[2].floor() as i64,
        rectangle[3].floor() as i64,
    ];
    if rounded[2] <= rounded[0] || rounded[3] <= rounded[1] {
        return Err("The signature page is too small for a visible certificate field.".to_string());
    }
    Ok(rounded)
}

#[derive(Clone, Copy, Debug)]
struct PageBox {
    left: f64,
    bottom: f64,
    width: f64,
    height: f64,
}

fn page_geometry(document: &Document, page_id: ObjectId) -> Result<(PageBox, i64), String> {
    let page_box = inherited_page_value(document, page_id, b"CropBox")?
        .or(inherited_page_value(document, page_id, b"MediaBox")?)
        .ok_or_else(|| "The signature page does not define a crop or media box.".to_string())?;
    let page_box = resolve_object(document, &page_box)?;
    let coordinates = page_box
        .as_array()
        .map_err(|_| "The signature page box is not an array.".to_string())?;
    if coordinates.len() != 4 {
        return Err("The signature page box must contain four coordinates.".to_string());
    }
    let values = coordinates
        .iter()
        .map(pdf_number)
        .collect::<Result<Vec<_>, _>>()?;
    let width = values[2] - values[0];
    let height = values[3] - values[1];
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Err("The signature page has invalid dimensions.".to_string());
    }
    let rotation = inherited_page_value(document, page_id, b"Rotate")?
        .as_ref()
        .and_then(|value| resolve_object(document, value).ok())
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err("The signature page has an unsupported rotation.".to_string());
    }
    Ok((
        PageBox {
            left: values[0],
            bottom: values[1],
            width,
            height,
        },
        rotation,
    ))
}

fn inherited_page_value(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<Object>, String> {
    let mut current = page_id;
    let mut visited = HashSet::new();
    for _ in 0..256 {
        if !visited.insert(current) {
            return Err("The page tree is cyclic.".to_string());
        }
        let dictionary = document
            .get_dictionary(current)
            .map_err(|error| format!("The signature page tree is invalid: {error}"))?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value.clone()));
        }
        match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => current = parent,
            Err(_) => return Ok(None),
        }
    }
    Err("The signature page tree is too deeply nested.".to_string())
}

fn resolve_object<'a>(document: &'a Document, object: &'a Object) -> Result<&'a Object, String> {
    match object {
        Object::Reference(id) => document
            .get_object(*id)
            .map_err(|error| format!("A referenced PDF object is invalid: {error}")),
        value => Ok(value),
    }
}

fn pdf_number(value: &Object) -> Result<f64, String> {
    match value {
        Object::Integer(number) => Ok(*number as f64),
        Object::Real(number) => Ok(f64::from(*number)),
        _ => Err("The signature page box contains a non-numeric value.".to_string()),
    }
}

fn certificate_rectangle(page: PageBox, rotation: i64, position: CertificatePosition) -> [f64; 4] {
    let (visual_width, visual_height) = if matches!(rotation, 90 | 270) {
        (page.height, page.width)
    } else {
        (page.width, page.height)
    };
    let margin_x = (visual_width * 0.055).max(2.0).min(visual_width * 0.2);
    let margin_y = (visual_height * 0.055).max(2.0).min(visual_height * 0.2);
    let width = (visual_width * 0.34)
        .clamp(72.0, 220.0)
        .min(visual_width - margin_x * 2.0);
    let height = (visual_height * 0.1)
        .clamp(32.0, 68.0)
        .min(visual_height - margin_y * 2.0);
    let x = match position {
        CertificatePosition::Left => margin_x,
        CertificatePosition::Centre => (visual_width - width) / 2.0,
        CertificatePosition::Right => visual_width - margin_x - width,
    };
    let y = margin_y;
    match rotation {
        90 => [
            page.left + page.width - y - height,
            page.bottom + x,
            page.left + page.width - y,
            page.bottom + x + width,
        ],
        180 => [
            page.left + page.width - x - width,
            page.bottom + page.height - y - height,
            page.left + page.width - x,
            page.bottom + page.height - y,
        ],
        270 => [
            page.left + y,
            page.bottom + page.height - x - width,
            page.left + y + height,
            page.bottom + page.height - x,
        ],
        _ => [
            page.left + x,
            page.bottom + y,
            page.left + x + width,
            page.bottom + y + height,
        ],
    }
}

fn build_sign_arguments(request: SignCommandArguments<'_>) -> Vec<String> {
    let SignCommandArguments {
        input,
        output,
        pkcs12,
        passfile,
        field_spec,
        timestamp_url,
        embed_validation_info,
        trust_roots,
    } = request;
    let mut arguments = vec![
        "sign".to_string(),
        "addsig".to_string(),
        "--field".to_string(),
        field_spec.to_string(),
    ];
    if let Some(url) = timestamp_url {
        arguments.push("--timestamp-url".to_string());
        arguments.push(url.to_string());
    }
    if embed_validation_info {
        arguments.push("--with-validation-info".to_string());
        arguments.push("--use-pades".to_string());
        append_trust_arguments(&mut arguments, trust_roots);
    }
    arguments.extend([
        "pkcs12".to_string(),
        "--passfile".to_string(),
        passfile.to_string_lossy().into_owned(),
        input.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
        pkcs12.to_string_lossy().into_owned(),
    ]);
    arguments
}

fn build_validation_arguments(input: &Path, trust_roots: &[PathBuf], pretty: bool) -> Vec<String> {
    let mut arguments = vec!["sign".to_string(), "validate".to_string()];
    if pretty {
        arguments.push("--pretty-print".to_string());
    }
    append_trust_arguments(&mut arguments, trust_roots);
    arguments.push(input.to_string_lossy().into_owned());
    arguments
}

fn append_trust_arguments(arguments: &mut Vec<String>, trust_roots: &[PathBuf]) {
    for root in trust_roots {
        arguments.push("--trust".to_string());
        arguments.push(root.to_string_lossy().into_owned());
    }
}

#[cfg(test)]
fn inspect_signature_structure(document: &Document, file_size: u64) -> SignatureStructure {
    inspect_signature_structure_with_control(document, file_size, &PdfJobExecutionControl::direct())
        .expect("direct signature inspection cannot be cancelled")
}

fn inspect_signature_structure_with_control(
    document: &Document,
    file_size: u64,
    control: &PdfJobExecutionControl,
) -> Result<SignatureStructure, String> {
    let mut structure = SignatureStructure::default();
    let mut field_names = HashSet::new();
    let mut inspected_nodes = 0_usize;
    for (index, object) in document.objects.values().enumerate() {
        if index % 64 == 0 {
            control.ensure_not_cancelled()?;
        }
        visit_dictionaries_with_control(
            object,
            0,
            &mut inspected_nodes,
            control,
            &mut |dictionary| {
                if dictionary
                    .get(b"FT")
                    .and_then(Object::as_name)
                    .is_ok_and(|name| name == b"Sig")
                {
                    if structure.fields.len() >= MAX_SIGNATURE_REPORT_FIELDS {
                        structure.field_limit_reached = true;
                    } else {
                        let name = dictionary
                            .get(b"T")
                            .ok()
                            .and_then(pdf_text)
                            .unwrap_or_else(|| "Unnamed signature field".to_string());
                        if field_names.insert(name.clone()) {
                            let value = dictionary
                                .get(b"V")
                                .ok()
                                .and_then(|value| resolve_dictionary(document, value));
                            structure.fields.push(signature_field(&name, value));
                        }
                    }
                }
                if dictionary.has(b"ByteRange") && dictionary.has(b"Contents") {
                    structure.signature_count = structure.signature_count.saturating_add(1);
                    if is_document_timestamp(dictionary) {
                        structure.timestamp_count = structure.timestamp_count.saturating_add(1);
                    }
                    if byte_range_is_complete(dictionary, file_size) {
                        structure.valid_byte_ranges = structure.valid_byte_ranges.saturating_add(1);
                    }
                }
            },
        )?;
    }
    if structure.fields.is_empty() && structure.signature_count > 0 {
        structure.fields.push(CertificateSignatureField {
            name: "Embedded certificate signature".to_string(),
            signed: true,
            kind: if structure.timestamp_count == structure.signature_count {
                "document-timestamp"
            } else {
                "approval"
            },
            reason: None,
            location: None,
            signing_time: None,
        });
    }
    control.ensure_not_cancelled()?;
    Ok(structure)
}

fn visit_dictionaries_with_control(
    object: &Object,
    depth: usize,
    inspected_nodes: &mut usize,
    control: &PdfJobExecutionControl,
    visitor: &mut impl FnMut(&Dictionary),
) -> Result<(), String> {
    if depth > 64 {
        return Ok(());
    }
    *inspected_nodes = inspected_nodes.saturating_add(1);
    if *inspected_nodes > MAX_SIGNATURE_INSPECTION_NODES {
        return Err(
            "The PDF signature structure exceeds the bounded inspection limit.".to_string(),
        );
    }
    if (*inspected_nodes).is_multiple_of(256) {
        control.ensure_not_cancelled()?;
    }
    match object {
        Object::Dictionary(dictionary) => {
            visitor(dictionary);
            for (_, value) in dictionary.iter() {
                visit_dictionaries_with_control(
                    value,
                    depth + 1,
                    inspected_nodes,
                    control,
                    visitor,
                )?;
            }
        }
        Object::Stream(stream) => {
            visitor(&stream.dict);
            for (_, value) in stream.dict.iter() {
                visit_dictionaries_with_control(
                    value,
                    depth + 1,
                    inspected_nodes,
                    control,
                    visitor,
                )?;
            }
        }
        Object::Array(values) => {
            for value in values {
                visit_dictionaries_with_control(
                    value,
                    depth + 1,
                    inspected_nodes,
                    control,
                    visitor,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn resolve_dictionary<'a>(document: &'a Document, value: &'a Object) -> Option<&'a Dictionary> {
    match value {
        Object::Reference(id) => document.get_dictionary(*id).ok(),
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn signature_field(name: &str, value: Option<&Dictionary>) -> CertificateSignatureField {
    CertificateSignatureField {
        name: name.to_string(),
        signed: value
            .is_some_and(|dictionary| dictionary.has(b"ByteRange") && dictionary.has(b"Contents")),
        kind: value
            .filter(|dictionary| is_document_timestamp(dictionary))
            .map(|_| "document-timestamp")
            .unwrap_or("approval"),
        reason: value
            .and_then(|dictionary| dictionary.get(b"Reason").ok())
            .and_then(pdf_text),
        location: value
            .and_then(|dictionary| dictionary.get(b"Location").ok())
            .and_then(pdf_text),
        signing_time: value
            .and_then(|dictionary| dictionary.get(b"M").ok())
            .and_then(pdf_text),
    }
}

fn pdf_text(value: &Object) -> Option<String> {
    let decoded = match value {
        Object::String(bytes, _) if bytes.len() <= MAX_SIGNATURE_FIELD_TEXT_BYTES => {
            decode_text_string(value).ok()?
        }
        Object::Name(bytes) if bytes.len() <= MAX_SIGNATURE_FIELD_TEXT_BYTES => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        _ => return None,
    };
    let sanitised = decoded
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitised = sanitised.trim();
    if sanitised.is_empty() {
        None
    } else {
        Some(truncate_utf8_value(
            sanitised,
            MAX_SIGNATURE_FIELD_TEXT_BYTES,
        ))
    }
}

fn truncate_utf8_value(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_string();
    }
    let mut boundary = maximum_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_string()
}

fn is_document_timestamp(dictionary: &Dictionary) -> bool {
    dictionary
        .get(b"Type")
        .and_then(Object::as_name)
        .is_ok_and(|name| name == b"DocTimeStamp")
        || dictionary
            .get(b"SubFilter")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"ETSI.RFC3161")
}

fn byte_range_is_complete(dictionary: &Dictionary, file_size: u64) -> bool {
    let Ok(values) = dictionary.get(b"ByteRange").and_then(Object::as_array) else {
        return false;
    };
    if values.len() != 4 {
        return false;
    }
    let Some(numbers) = values
        .iter()
        .map(|value| {
            value
                .as_i64()
                .ok()
                .and_then(|number| u64::try_from(number).ok())
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let contents_present = dictionary
        .get(b"Contents")
        .ok()
        .is_some_and(|value| match value {
            Object::String(bytes, _) => !bytes.is_empty(),
            _ => true,
        });
    numbers[0] == 0
        && numbers[1] > 0
        && numbers[2] > numbers[1]
        && numbers[3] > 0
        && numbers[2].checked_add(numbers[3]) == Some(file_size)
        && contents_present
}

fn report_from_output(
    input: &Path,
    structure: SignatureStructure,
    engine_version: Option<String>,
    output: &CapturedOutput,
    encrypted: bool,
    secrets: &[&str],
    mut warnings: Vec<String>,
) -> CertificateValidationReport {
    let raw_details = output_text(output);
    let signals = parse_validation_signals(&raw_details, output.status.success());
    let details = redact_secrets(raw_details, secrets);
    let state = validation_state(output.status.success(), signals);
    warnings.extend(signature_field_limit_warnings(&structure));
    if output.truncated {
        warnings.push(
            "The pyHanko validation report exceeded the display limit and was truncated."
                .to_string(),
        );
    }
    if signals.intact == Some(true) && signals.trusted == Some(false) {
        warnings.push(
            "Cryptographic integrity appears intact, but the signer certificate did not chain to a configured trust root."
                .to_string(),
        );
    }
    let summary = match state {
        CertificateValidationState::Valid => {
            "pyHanko validated the certificate signature and its configured trust chain."
                .to_string()
        }
        CertificateValidationState::Invalid => {
            "pyHanko reported an invalid or non-intact certificate signature.".to_string()
        }
        CertificateValidationState::Indeterminate if signals.intact == Some(true) => {
            "The signature appears cryptographically intact, but signer trust was not established."
                .to_string()
        }
        CertificateValidationState::Indeterminate => {
            "pyHanko could not reach a conclusive validation result.".to_string()
        }
        _ => "Certificate validation did not complete.".to_string(),
    };
    CertificateValidationReport {
        input_path: input.to_string_lossy().into_owned(),
        encrypted,
        signature_count: structure.signature_count,
        timestamp_count: structure.timestamp_count,
        fields: structure.fields,
        state,
        intact: signals.intact,
        trusted: signals.trusted,
        engine_version,
        summary,
        details: truncate_text(&details, REPORT_TEXT_LIMIT),
        warnings,
    }
}

fn validation_state(success: bool, signals: ValidationSignals) -> CertificateValidationState {
    if signals.intact == Some(false) {
        CertificateValidationState::Invalid
    } else if success && signals.intact == Some(true) && signals.trusted == Some(true) {
        CertificateValidationState::Valid
    } else {
        CertificateValidationState::Indeterminate
    }
}

fn parse_validation_signals(text: &str, success: bool) -> ValidationSignals {
    let normalised = text.to_ascii_uppercase();
    let explicitly_not_intact = [
        "NOT INTACT",
        "INTACT:FALSE",
        "INTACT: FALSE",
        "INTACT = FALSE",
        "CRYPTOGRAPHIC INTEGRITY: FAILED",
        "SIGNATURE IS CRYPTOGRAPHICALLY UNSOUND",
        "SIGNATURE IS CRYPTOGRAPHICALLY INVALID",
        "DIGEST MISMATCH",
    ]
    .iter()
    .any(|marker| normalised.contains(marker));
    let intact = if explicitly_not_intact {
        Some(false)
    } else if success
        || normalised.contains("INTACT")
        || normalised.contains("CRYPTOGRAPHIC INTEGRITY: OK")
        || normalised.contains("SIGNATURE IS CRYPTOGRAPHICALLY SOUND")
    {
        Some(true)
    } else {
        None
    };
    let trusted = if normalised.contains("UNTRUSTED")
        || normalised.contains("NO PATH TO TRUST ANCHOR")
        || normalised.contains("TRUSTED:FALSE")
        || normalised.contains("TRUSTED: FALSE")
        || normalised.contains("TRUSTED = FALSE")
    {
        Some(false)
    } else if normalised.contains("CERTIFICATE IS TRUSTED")
        || normalised.contains("TRUSTED:TRUE")
        || normalised.contains("TRUSTED: TRUE")
        || normalised.contains("TRUSTED = TRUE")
        || success
    {
        Some(true)
    } else {
        None
    };
    ValidationSignals {
        intact,
        trusted,
        timestamp_present: normalised.contains("TIMESTAMP") || normalised.contains("TIME STAMP"),
    }
}

fn run_pyhanko_with_control(
    arguments: &[String],
    output_limit: usize,
    timeout: Duration,
    input_password: Option<&str>,
    control: Option<&PdfJobExecutionControl>,
) -> Result<CapturedOutput, CommandRunError> {
    let password_bridge = input_password
        .map(|_| TemporaryPyHankoPasswordBridge::new())
        .transpose()
        .map_err(CommandRunError::Setup)?;
    let mut command = Command::new("pyhanko");
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(bridge) = &password_bridge {
        command
            .env(
                "PYTHONPATH",
                bridge.python_path().map_err(CommandRunError::Setup)?,
            )
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = ManagedChild::spawn(&mut command).map_err(CommandRunError::Start)?;
    let stdout_reader = child
        .take_stdout()
        .map(|pipe| read_bounded(pipe, output_limit));
    let stderr_reader = child
        .take_stderr()
        .map(|pipe| read_bounded(pipe, output_limit));
    if let Some(password) = input_password {
        let write_result = (|| -> io::Result<()> {
            let mut stdin = child.take_stdin().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "The protected PDF password pipe was unavailable.",
                )
            })?;
            stdin.write_all(password.as_bytes())?;
            stdin.write_all(b"\n")?;
            stdin.flush()
        })();
        if let Err(error) = write_result {
            let _ = child.terminate_tree();
            let _ = child.wait();
            let _ = finish_bounded(stdout_reader);
            let _ = finish_bounded(stderr_reader);
            return Err(CommandRunError::Input(error));
        }
    }
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        if control.is_some_and(|control| control.ensure_not_cancelled().is_err()) {
            cancelled = true;
            let _ = child.terminate_tree();
            break child.wait().map_err(CommandRunError::Monitor)?;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(60)),
            Ok(None) => {
                timed_out = true;
                let _ = child.terminate_tree();
                break child.wait().map_err(CommandRunError::Monitor)?;
            }
            Err(error) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                return Err(CommandRunError::Monitor(error));
            }
        }
    };
    let stdout = finish_bounded(stdout_reader);
    let stderr = finish_bounded(stderr_reader);
    if cancelled {
        return Err(CommandRunError::Cancelled);
    }
    if timed_out {
        return Err(CommandRunError::TimedOut);
    }
    Ok(CapturedOutput {
        status,
        truncated: stdout.truncated || stderr.truncated,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

#[derive(Default)]
struct BoundedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(
    mut reader: impl Read + Send + 'static,
    limit: usize,
) -> thread::JoinHandle<BoundedBytes> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut limited = reader.by_ref().take((limit + 1) as u64);
        let _ = limited.read_to_end(&mut bytes);
        let truncated = bytes.len() > limit;
        bytes.truncate(limit);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
        BoundedBytes { bytes, truncated }
    })
}

fn finish_bounded(reader: Option<thread::JoinHandle<BoundedBytes>>) -> BoundedBytes {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

impl CommandRunError {
    fn detail(&self) -> String {
        match self {
            Self::Setup(error) => {
                format!("The private pyHanko password bridge could not be prepared: {error}")
            }
            Self::Start(error) if error.kind() == std::io::ErrorKind::NotFound => {
                "The 'pyhanko' command was not found on PATH.".to_string()
            }
            Self::Start(error) => format!("The pyHanko command could not be started: {error}"),
            Self::Input(error) => {
                format!("The PDF password could not be supplied privately to pyHanko: {error}")
            }
            Self::Monitor(error) => {
                format!("The pyHanko command could not be monitored safely: {error}")
            }
            Self::Cancelled => PDF_JOB_CANCELLED_ERROR.to_string(),
            Self::TimedOut => {
                "The pyHanko command exceeded its safety timeout and was stopped.".to_string()
            }
        }
    }
}

fn output_text(output: &CapturedOutput) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match (stdout.trim(), stderr.trim()) {
        ("", "") => "pyHanko returned no diagnostic output.".to_string(),
        (stdout, "") => stdout.to_string(),
        ("", stderr) => stderr.to_string(),
        (stdout, stderr) => format!("{stdout}\n\n{stderr}"),
    }
}

fn command_diagnostic(output: &CapturedOutput, secrets: &[&str]) -> String {
    let detail = redact_secrets(output_text(output), secrets);
    truncate_text(detail.trim(), 4096)
}

fn redact_secrets(mut detail: String, secrets: &[&str]) -> String {
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        detail = detail.replace(secret, "[redacted]");
    }
    detail
}

fn truncate_text(text: &str, maximum_bytes: usize) -> String {
    if text.len() <= maximum_bytes {
        return text.to_string();
    }
    let mut boundary = maximum_bytes;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}\n[report truncated]", &text[..boundary])
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

struct TemporaryPassfile {
    lease: TemporaryLease,
}

struct TemporaryPyHankoPasswordBridge {
    lease: TemporaryLease,
}

struct TemporaryCertificateWorkspace {
    lease: TemporaryLease,
}

impl TemporaryCertificateWorkspace {
    fn new() -> Result<Self, String> {
        let directory = std::env::temp_dir();
        for attempt in 0..16_u32 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("The system clock is invalid: {error}"))?
                .as_nanos();
            let path = directory.join(format!(
                ".paperworks-certificate-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            let mut lease = register_temporary_path(&path, TemporaryKind::CertificateWorkspace)?;
            let builder = {
                #[cfg(unix)]
                {
                    let mut builder = fs::DirBuilder::new();
                    builder.mode(0o700);
                    builder
                }
                #[cfg(not(unix))]
                {
                    fs::DirBuilder::new()
                }
            };
            match builder.create(lease.path()) {
                Ok(()) => {
                    lease.write_directory_ownership_token()?;
                    return Ok(Self { lease });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    lease.cancel_without_target_cleanup();
                }
                Err(error) => {
                    lease.cancel_without_target_cleanup();
                    return Err(format!(
                        "The private certificate workspace could not be created: {error}"
                    ));
                }
            }
        }
        Err("A unique private certificate workspace could not be allocated.".to_string())
    }

    fn snapshot_file(
        &self,
        source: &Path,
        filename: &str,
        expected: SourceFingerprint,
        control: &PdfJobExecutionControl,
    ) -> Result<PathBuf, String> {
        if filename.is_empty()
            || filename.len() > 128
            || !filename.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
        {
            return Err("The private certificate snapshot filename is invalid.".to_string());
        }
        let destination = self.lease.path().join(filename);
        let mut input = File::open(source)
            .map_err(|error| format!("A certificate input could not be snapshotted: {error}"))?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut output = options.open(&destination).map_err(|error| {
            format!("A private certificate snapshot could not be created: {error}")
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut bytes_read = 0_u64;
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|error| format!("A certificate input snapshot failed: {error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("A private certificate snapshot failed: {error}"))?;
            hasher.update(&buffer[..count]);
            bytes_read = bytes_read.saturating_add(count as u64);
            control.ensure_not_cancelled()?;
        }
        output.sync_all().map_err(|error| {
            format!("A private certificate snapshot could not be completed: {error}")
        })?;
        let digest: [u8; 32] = hasher.finalize().into();
        if bytes_read != expected.bytes || digest != expected.sha256 {
            return Err(
                "A certificate input changed before its private snapshot was completed. Choose it again."
                    .to_string(),
            );
        }
        Ok(destination)
    }
}

impl TemporaryPyHankoPasswordBridge {
    fn new() -> Result<Self, String> {
        let directory = std::env::temp_dir();
        for attempt in 0..16_u32 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("The system clock is invalid: {error}"))?
                .as_nanos();
            let path = directory.join(format!(
                ".tufekci-paperworks-pyhk-bridge-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            let mut lease = register_temporary_path(&path, TemporaryKind::PyHankoPasswordBridge)?;
            let builder = {
                #[cfg(unix)]
                {
                    let mut builder = fs::DirBuilder::new();
                    builder.mode(0o700);
                    builder
                }
                #[cfg(not(unix))]
                {
                    fs::DirBuilder::new()
                }
            };
            match builder.create(lease.path()) {
                Ok(()) => {
                    lease.write_directory_ownership_token()?;
                    let script_path = lease.path().join(PYHANKO_PASSWORD_BRIDGE_FILE);
                    let mut options = OpenOptions::new();
                    options.create_new(true).write(true);
                    #[cfg(unix)]
                    options.mode(0o600);
                    let mut script = options.open(&script_path).map_err(|error| {
                        format!("The private pyHanko password bridge could not be created: {error}")
                    })?;
                    script
                        .write_all(PYHANKO_PASSWORD_BRIDGE_SOURCE)
                        .and_then(|_| script.sync_all())
                        .map_err(|error| {
                            format!(
                                "The private pyHanko password bridge could not be completed: {error}"
                            )
                        })?;
                    return Ok(Self { lease });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    lease.cancel_without_target_cleanup();
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "The private pyHanko password bridge directory could not be created: {error}"
                    ));
                }
            }
        }
        Err("A unique private pyHanko password bridge could not be allocated.".to_string())
    }

    fn python_path(&self) -> Result<std::ffi::OsString, String> {
        let mut paths = vec![self.path().to_path_buf()];
        if let Some(existing) = std::env::var_os("PYTHONPATH") {
            paths.extend(
                std::env::split_paths(&existing).filter(|path| !path.as_os_str().is_empty()),
            );
        }
        std::env::join_paths(paths)
            .map_err(|error| format!("The Python module path could not be prepared: {error}"))
    }

    fn path(&self) -> &Path {
        self.lease.path()
    }
}

impl TemporaryPassfile {
    fn new(passphrase: &[u8]) -> Result<Self, String> {
        let directory = std::env::temp_dir();
        for attempt in 0..16_u32 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("The system clock is invalid: {error}"))?
                .as_nanos();
            let path = directory.join(format!(
                ".tufekci-paperworks-pyhk-{}-{nonce}-{attempt}.secret",
                std::process::id()
            ));
            let mut lease = register_temporary_path(&path, TemporaryKind::CertificatePassfile)?;
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(lease.path()) {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(passphrase).and_then(|_| file.sync_all()) {
                        drop(file);
                        return Err(format!(
                            "The temporary certificate passfile could not be written: {error}"
                        ));
                    }
                    return Ok(Self { lease });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    lease.cancel_without_target_cleanup();
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "The temporary certificate passfile could not be created: {error}"
                    ))
                }
            }
        }
        Err("A unique temporary certificate passfile could not be allocated.".to_string())
    }

    fn path(&self) -> &Path {
        self.lease.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object, Stream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn validation_parser_distinguishes_trust_from_integrity() {
        assert_eq!(
            parse_validation_signals("Sig1:INTACT:TRUSTED,UNTOUCHED", true),
            ValidationSignals {
                intact: Some(true),
                trusted: Some(true),
                timestamp_present: false,
            }
        );
        assert_eq!(
            parse_validation_signals("Sig1:INTACT:UNTRUSTED,UNTOUCHED", false),
            ValidationSignals {
                intact: Some(true),
                trusted: Some(false),
                timestamp_present: false,
            }
        );
        assert_eq!(
            parse_validation_signals("Intact: False\nTrusted: False", false).intact,
            Some(false)
        );
        assert_eq!(
            parse_validation_signals(
                "The signer's certificate is untrusted.\nThe signature is cryptographically sound.\nThe signature is judged INVALID.",
                false,
            ),
            ValidationSignals {
                intact: Some(true),
                trusted: Some(false),
                timestamp_present: false,
            }
        );
        assert_eq!(
            parse_validation_signals(
                "No path to trust anchor found.\nThe signature is cryptographically unsound.",
                false,
            ),
            ValidationSignals {
                intact: Some(false),
                trusted: Some(false),
                timestamp_present: false,
            }
        );
    }

    #[test]
    fn source_fingerprint_rejects_same_size_changes_before_signed_publication() {
        let path = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-fingerprint-{}-{}.pdf",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"%PDF-1.7\noriginal").unwrap();
        let fingerprint = source_fingerprint(&path, None).unwrap();
        fs::write(&path, b"%PDF-1.7\nmutation").unwrap();
        let changed = source_fingerprint(&path, None).unwrap();

        assert_eq!(fingerprint.bytes, changed.bytes);
        assert_ne!(fingerprint.sha256, changed.sha256);
        let error =
            verify_source_fingerprint(&path, fingerprint, &PdfJobExecutionControl::direct())
                .unwrap_err();

        assert!(error.contains("changed on disk"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn scheduler_certificate_errors_are_content_free() {
        let error = safe_certificate_job_error(
            "PKCS#12 C:\\Private\\Client.p12 rejected passphrase super-secret",
        );
        assert_eq!(
            error,
            "The signing certificate or its passphrase could not be accepted."
        );
        assert!(!error.contains("Client"));
        assert!(!error.contains("super-secret"));
        assert_eq!(
            safe_certificate_job_error(
                "The signing certificate changed on disk: C:\\Private\\Client.p12"
            ),
            "The signing certificate changed during certificate signing. Choose it again before signing."
        );
    }

    #[test]
    fn certificate_requests_have_redacted_debug_output() {
        let signing = CertificateSignRequest {
            input_path: "private-source.pdf".to_string(),
            input_password: Some("private-pdf-password".to_string()),
            output_path: "private-output.pdf".to_string(),
            pkcs12_path: "private-certificate.p12".to_string(),
            pkcs12_passphrase: "private-certificate-password".to_string(),
            pkcs12_passphrase_confirmation: "private-certificate-password".to_string(),
            visible: false,
            page_number: None,
            position: None,
            field_name: "PrivateField".to_string(),
            timestamp_url: None,
            embed_validation_info: false,
            trust_roots: vec!["private-root.pem".to_string()],
        };
        let validation = InspectCertificateRequest {
            input_path: "private-source.pdf".to_string(),
            input_password: Some("private-pdf-password".to_string()),
            trust_roots: vec!["private-root.pem".to_string()],
        };

        assert_eq!(
            format!("{signing:?}"),
            "CertificateSignRequest { redacted: true }"
        );
        assert_eq!(
            format!("{validation:?}"),
            "InspectCertificateRequest { redacted: true }"
        );
    }

    #[test]
    fn controlled_validation_reports_progress_and_honours_cancellation() {
        let path = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-validation-{}-{}.pdf",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _cleanup = TestOutputCleanup(vec![path.clone()]);
        sample_document().save(&path).unwrap();
        let progress = Arc::new(Mutex::new(Vec::<(u8, String)>::new()));
        let observed = Arc::clone(&progress);
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |value, stage| observed.lock().unwrap().push((value, stage))),
        );

        let report = run_certificate_validation_job_with_control(
            InspectCertificateRequest {
                input_path: path.to_string_lossy().into_owned(),
                input_password: None,
                trust_roots: Vec::new(),
            },
            &control,
        )
        .unwrap();

        assert_eq!(report.state, CertificateValidationState::Unsigned);
        assert_eq!(report.input_path, "PDF");
        let observed = progress.lock().unwrap();
        assert!(observed.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(observed
            .iter()
            .any(|(value, stage)| *value == 24 && stage.contains("signature fields")));
        assert_eq!(observed.last().map(|entry| entry.0), Some(98));

        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation_flag = Arc::clone(&cancelled);
        let cancelling_control = PdfJobExecutionControl::new(
            cancelled,
            Arc::new(move |value, _| {
                if value >= 24 {
                    cancellation_flag.store(true, Ordering::Release);
                }
            }),
        );
        let error = inspect_certificate_signatures_with_control(
            InspectCertificateRequest {
                input_path: path.to_string_lossy().into_owned(),
                input_password: None,
                trust_roots: Vec::new(),
            },
            &cancelling_control,
        )
        .unwrap_err();
        assert_eq!(error, PDF_JOB_CANCELLED_ERROR);
    }

    #[test]
    fn validation_rejects_a_source_changed_at_the_final_gate() {
        let path = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-validation-mutation-{}-{}.pdf",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _cleanup = TestOutputCleanup(vec![path.clone()]);
        sample_document().save(&path).unwrap();
        let path_to_change = path.clone();
        let control = PdfJobExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |value, _| {
                if value == 98 {
                    let mut file = std::fs::OpenOptions::new()
                        .append(true)
                        .open(&path_to_change)
                        .unwrap();
                    file.write_all(b"\n% changed during validation").unwrap();
                }
            }),
        );

        let error = inspect_certificate_signatures_with_control(
            InspectCertificateRequest {
                input_path: path.to_string_lossy().into_owned(),
                input_password: None,
                trust_roots: Vec::new(),
            },
            &control,
        )
        .unwrap_err();

        assert!(error.contains("changed on disk during certificate validation"));
    }

    #[test]
    fn validation_job_reports_scrub_local_paths() {
        let private_pdf = "C:\\Private\\Client Agreement.pdf";
        let private_root = "C:\\Private\\Client Root.pem";
        let report = CertificateValidationReport {
            input_path: private_pdf.to_string(),
            encrypted: false,
            signature_count: 1,
            timestamp_count: 0,
            fields: vec![CertificateSignatureField {
                name: format!("Approval from {private_pdf}"),
                signed: true,
                kind: "approval",
                reason: Some(private_root.to_string()),
                location: None,
                signing_time: None,
            }],
            state: CertificateValidationState::Indeterminate,
            intact: Some(true),
            trusted: Some(false),
            engine_version: Some("test".to_string()),
            summary: format!("Reviewed {private_pdf}"),
            details: format!("Trust source: {private_root}"),
            warnings: vec![format!("Check {private_root}")],
        };
        let safe = job_safe_certificate_validation_report(
            report,
            &[private_pdf.to_string(), private_root.to_string()],
        );
        let serialised = serde_json::to_string(&safe).unwrap();

        assert_eq!(safe.input_path, "PDF");
        assert!(safe.details.is_empty());
        assert!(!serialised.contains("Client Agreement"));
        assert!(!serialised.contains("Client Root"));
        assert!(serialised.contains("[local file]"));
    }

    #[test]
    fn signature_metadata_is_decoded_sanitised_and_bounded() {
        let unicode = lopdf::text_string("İmzalama nedeni");
        assert_eq!(pdf_text(&unicode).as_deref(), Some("İmzalama nedeni"));

        let controls = Object::String(
            b"Approved\n\0in London".to_vec(),
            lopdf::StringFormat::Literal,
        );
        assert_eq!(pdf_text(&controls).as_deref(), Some("Approvedin London"));

        let oversized = Object::String(
            vec![b'x'; MAX_SIGNATURE_FIELD_TEXT_BYTES + 1],
            lopdf::StringFormat::Literal,
        );
        assert!(pdf_text(&oversized).is_none());
    }

    #[test]
    fn signature_field_reports_have_a_hard_cardinality_limit() {
        let mut document = sample_document();
        for index in 0..=MAX_SIGNATURE_REPORT_FIELDS {
            document.add_object(dictionary! {
                "FT" => "Sig",
                "T" => Object::string_literal(format!("Signature_{index}")),
            });
        }

        let structure = inspect_signature_structure(&document, 1);

        assert_eq!(structure.fields.len(), MAX_SIGNATURE_REPORT_FIELDS);
        assert!(structure.field_limit_reached);
        assert_eq!(signature_field_limit_warnings(&structure).len(), 1);
    }

    #[test]
    fn signing_arguments_keep_passphrase_out_of_process_arguments() {
        let pdf_password = "private-pdf-password";
        let trust_roots = [PathBuf::from("root.pem")];
        let arguments = build_sign_arguments(SignCommandArguments {
            input: Path::new("input.pdf"),
            output: Path::new("temporary.pdf"),
            pkcs12: Path::new("certificate.p12"),
            passfile: Path::new("passfile.secret"),
            field_spec: "1/20,20,200,80/Approval_1",
            timestamp_url: Some("https://tsa.example.test"),
            embed_validation_info: true,
            trust_roots: &trust_roots,
        });
        assert!(arguments.contains(&"--passfile".to_string()));
        assert!(arguments.contains(&"--timestamp-url".to_string()));
        assert!(arguments.contains(&"--with-validation-info".to_string()));
        assert!(arguments.contains(&"--use-pades".to_string()));
        assert!(arguments.contains(&"--trust".to_string()));
        assert!(!arguments
            .iter()
            .any(|argument| argument.contains("certificate-password")));
        assert!(!arguments.iter().any(|argument| argument == pdf_password));
    }

    #[test]
    fn pdf_password_validation_is_bounded_and_rejects_control_characters() {
        assert!(validate_optional_pdf_password(None).is_ok());
        assert!(validate_optional_pdf_password(Some("")).is_ok());
        assert!(validate_optional_pdf_password(Some("protected document")).is_ok());
        assert!(validate_optional_pdf_password(Some("line\nbreak")).is_err());
        assert!(validate_optional_pdf_password(Some(&"x".repeat(1025))).is_err());
    }

    #[test]
    fn encrypted_pdf_loader_requires_the_password_and_remembers_protection() {
        let path = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-encrypted-{}-{}.pdf",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _cleanup = TestOutputCleanup(vec![path.clone()]);
        let password = "correct horse battery staple";
        let mut document = sample_document();
        document.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(vec![1; 16], lopdf::StringFormat::Literal),
                Object::String(vec![2; 16], lopdf::StringFormat::Literal),
            ]),
        );
        let encryption = lopdf::EncryptionState::try_from(lopdf::EncryptionVersion::V2 {
            document: &document,
            owner_password: "owner password",
            user_password: password,
            key_length: 128,
            permissions: lopdf::Permissions::all(),
        })
        .unwrap();
        document.encrypt(&encryption).unwrap();
        document.save(&path).unwrap();

        let wrong = load_certificate_pdf(&path, Some("wrong password"))
            .err()
            .expect("reject the wrong PDF password");
        assert_eq!(wrong, "The source PDF password was not accepted.");
        assert!(!wrong.contains(password));

        let loaded = load_certificate_pdf(&path, Some(password)).unwrap();
        assert!(loaded.encrypted);
        assert!(!loaded.document.is_encrypted());
        assert!(loaded.document.was_encrypted());
        assert_eq!(loaded.document.get_pages().len(), 1);
    }

    #[test]
    fn private_password_bridge_contains_no_secret_and_is_removed_on_drop() {
        let secret = "never-write-this-pdf-password";
        let bridge = TemporaryPyHankoPasswordBridge::new().unwrap();
        let path = bridge.path().to_path_buf();
        let script = fs::read(path.join(PYHANKO_PASSWORD_BRIDGE_FILE)).unwrap();
        assert_eq!(script, PYHANKO_PASSWORD_BRIDGE_SOURCE);
        assert!(!String::from_utf8_lossy(&script).contains(secret));
        assert!(!String::from_utf8_lossy(&script).contains("sys.argv"));
        assert_eq!(
            std::env::split_paths(&bridge.python_path().unwrap()).next(),
            Some(path.clone())
        );

        drop(bridge);
        assert!(!path.exists());
    }

    #[test]
    fn private_certificate_workspace_snapshots_exact_bytes_and_cleans_up() {
        let source = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-snapshot-source-{}-{}.pdf",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&source, b"%PDF-1.7\nprivate snapshot source").unwrap();
        let expected = source_fingerprint(&source, None).unwrap();
        let workspace = TemporaryCertificateWorkspace::new().unwrap();
        let workspace_path = workspace.lease.path().to_path_buf();
        let snapshot = workspace
            .snapshot_file(
                &source,
                "source.pdf",
                expected,
                &PdfJobExecutionControl::direct(),
            )
            .unwrap();

        assert_eq!(fs::read(snapshot).unwrap(), fs::read(&source).unwrap());
        drop(workspace);
        assert!(!workspace_path.exists());
        let _ = fs::remove_file(source);
    }

    #[test]
    fn timestamp_urls_require_https_except_for_loopback_testing() {
        let credential_url = ["https://user", "secret@tsa.example.test"].join(":");
        assert!(validate_timestamp_url("https://tsa.example.test/timestamp").is_ok());
        assert!(validate_timestamp_url("http://127.0.0.1:8080/timestamp").is_ok());
        assert!(validate_timestamp_url("http://tsa.example.test/timestamp").is_err());
        assert!(validate_timestamp_url(&credential_url).is_err());
        assert!(validate_timestamp_url("https://tsa.example.test?token=secret").is_err());
    }

    #[test]
    fn untrusted_signatures_are_never_labelled_valid() {
        assert_eq!(
            validation_state(
                true,
                ValidationSignals {
                    intact: Some(true),
                    trusted: Some(false),
                    timestamp_present: false,
                },
            ),
            CertificateValidationState::Indeterminate
        );
        assert_eq!(
            validation_state(
                true,
                ValidationSignals {
                    intact: Some(true),
                    trusted: Some(true),
                    timestamp_present: false,
                },
            ),
            CertificateValidationState::Valid
        );
    }

    #[test]
    fn visible_field_rectangles_stay_inside_rotated_page_bounds() {
        let page = PageBox {
            left: 0.0,
            bottom: 0.0,
            width: 595.0,
            height: 842.0,
        };
        for rotation in [0, 90, 180, 270] {
            for position in [
                CertificatePosition::Left,
                CertificatePosition::Centre,
                CertificatePosition::Right,
            ] {
                let rectangle = certificate_rectangle(page, rotation, position);
                assert!(rectangle[0] >= page.left);
                assert!(rectangle[1] >= page.bottom);
                assert!(rectangle[2] <= page.left + page.width);
                assert!(rectangle[3] <= page.bottom + page.height);
                assert!(rectangle[2] > rectangle[0]);
                assert!(rectangle[3] > rectangle[1]);
            }
        }

        let tiny_page = PageBox {
            left: -2.0,
            bottom: 3.0,
            width: 10.0,
            height: 14.0,
        };
        let tiny_rectangle = certificate_rectangle(tiny_page, 270, CertificatePosition::Centre);
        assert!(tiny_rectangle[0] >= tiny_page.left);
        assert!(tiny_rectangle[1] >= tiny_page.bottom);
        assert!(tiny_rectangle[2] <= tiny_page.left + tiny_page.width);
        assert!(tiny_rectangle[3] <= tiny_page.bottom + tiny_page.height);
        assert!(tiny_rectangle[2] > tiny_rectangle[0]);
        assert!(tiny_rectangle[3] > tiny_rectangle[1]);
    }

    #[test]
    fn visible_field_specs_use_inward_rounded_integer_coordinates() {
        let specification = visible_field_spec(
            &sample_document(),
            1,
            CertificatePosition::Right,
            "Approval_1",
        )
        .unwrap();
        let (geometry, field_name) = specification.rsplit_once('/').unwrap();
        let (page, coordinates) = geometry.split_once('/').unwrap();
        let coordinates = coordinates
            .split(',')
            .map(str::parse::<i64>)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(page, "1");
        assert_eq!(field_name, "Approval_1");
        assert_eq!(coordinates.len(), 4);
        assert!(coordinates[2] > coordinates[0]);
        assert!(coordinates[3] > coordinates[1]);
    }

    #[test]
    fn structural_inspection_finds_signed_and_empty_fields() {
        let file_size = 120_u64;
        let mut document = sample_document();
        let signature = document.add_object(dictionary! {
            "Type" => "Sig",
            "ByteRange" => vec![0.into(), 20.into(), 40.into(), 80.into()],
            "Contents" => Object::String(vec![1, 2, 3, 4], lopdf::StringFormat::Hexadecimal),
            "Reason" => Object::string_literal("Approved"),
        });
        let signed_field = document.add_object(dictionary! {
            "FT" => "Sig",
            "T" => Object::string_literal("Approval_1"),
            "V" => signature,
        });
        let empty_field = document.add_object(dictionary! {
            "FT" => "Sig",
            "T" => Object::string_literal("Approval_2"),
        });
        document.catalog_mut().unwrap().set(
            "AcroForm",
            dictionary! { "Fields" => vec![signed_field.into(), empty_field.into()] },
        );

        let structure = inspect_signature_structure(&document, file_size);
        assert_eq!(structure.signature_count, 1);
        assert_eq!(structure.valid_byte_ranges, 1);
        assert_eq!(structure.fields.len(), 2);
        assert!(structure
            .fields
            .iter()
            .any(|field| field.name == "Approval_1" && field.signed));
        assert!(structure
            .fields
            .iter()
            .any(|field| field.name == "Approval_2" && !field.signed));
    }

    #[test]
    fn incremental_signatures_only_require_the_latest_range_to_cover_the_file() {
        let mut document = sample_document();
        let previous_signature = document.add_object(dictionary! {
            "Type" => "Sig",
            "ByteRange" => vec![0.into(), 20.into(), 40.into(), 80.into()],
            "Contents" => Object::String(vec![1], lopdf::StringFormat::Hexadecimal),
        });
        let latest_signature = document.add_object(dictionary! {
            "Type" => "Sig",
            "ByteRange" => vec![0.into(), 80.into(), 120.into(), 80.into()],
            "Contents" => Object::String(vec![2], lopdf::StringFormat::Hexadecimal),
        });
        document.catalog_mut().unwrap().set(
            "AcroForm",
            dictionary! {
                "Fields" => vec![previous_signature.into(), latest_signature.into()]
            },
        );

        let structure = inspect_signature_structure(&document, 200);

        assert_eq!(structure.signature_count, 2);
        assert_eq!(structure.valid_byte_ranges, 1);
    }

    #[test]
    #[ignore = "requires a generated local certificate corpus and pyHanko"]
    fn live_certificate_corpus() {
        let corpus = PathBuf::from(
            std::env::var("PAPERWORKS_CERTIFICATE_CORPUS")
                .expect("set PAPERWORKS_CERTIFICATE_CORPUS to the private fixture directory"),
        );
        let source = corpus.join("unsigned.pdf");
        let encrypted_source = corpus.join("encrypted.pdf");
        let certificate = corpus.join("signer.p12");
        let trust_root = corpus.join("trust-root.pem");
        let passphrase_path = corpus.join("passphrase.txt");
        let pdf_password_path = corpus.join("pdf-password.txt");
        let timestamp_url = std::env::var("PAPERWORKS_TEST_TSA_URL").ok();
        let passphrase = fs::read_to_string(&passphrase_path)
            .expect("read the private certificate passphrase fixture")
            .trim_end_matches(['\r', '\n'])
            .to_string();
        let pdf_password = fs::read_to_string(&pdf_password_path)
            .expect("read the private PDF password fixture")
            .trim_end_matches(['\r', '\n'])
            .to_string();

        for path in [
            &source,
            &encrypted_source,
            &certificate,
            &trust_root,
            &passphrase_path,
            &pdf_password_path,
        ] {
            assert!(path.is_file(), "missing certificate corpus file: {path:?}");
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        let visible_output = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-visible-{}-{nonce}.pdf",
            std::process::id()
        ));
        let invisible_output = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-invisible-{}-{nonce}.pdf",
            std::process::id()
        ));
        let encrypted_output = std::env::temp_dir().join(format!(
            "tufekci-paperworks-certificate-encrypted-{}-{nonce}.pdf",
            std::process::id()
        ));
        let _outputs = TestOutputCleanup(vec![
            visible_output.clone(),
            invisible_output.clone(),
            encrypted_output.clone(),
        ]);

        let visible = certificate_sign_pdf(CertificateSignRequest {
            input_path: source.to_string_lossy().into_owned(),
            input_password: None,
            output_path: visible_output.to_string_lossy().into_owned(),
            pkcs12_path: certificate.to_string_lossy().into_owned(),
            pkcs12_passphrase: passphrase.clone(),
            pkcs12_passphrase_confirmation: passphrase.clone(),
            visible: true,
            page_number: Some(1),
            position: Some(CertificatePosition::Right),
            field_name: "ReleaseGateVisible".to_string(),
            timestamp_url: timestamp_url.clone(),
            embed_validation_info: timestamp_url.is_some(),
            trust_roots: vec![trust_root.to_string_lossy().into_owned()],
        })
        .expect("create and verify the visible certificate signature");
        assert!(visible.visible);
        assert_eq!(visible.timestamped, timestamp_url.is_some());
        assert_eq!(visible.validation.intact, Some(true));
        assert_eq!(visible.validation.trusted, Some(true));
        assert_eq!(visible.validation.state, CertificateValidationState::Valid);

        let invisible = certificate_sign_pdf(CertificateSignRequest {
            input_path: visible_output.to_string_lossy().into_owned(),
            input_password: None,
            output_path: invisible_output.to_string_lossy().into_owned(),
            pkcs12_path: certificate.to_string_lossy().into_owned(),
            pkcs12_passphrase: passphrase.clone(),
            pkcs12_passphrase_confirmation: passphrase.clone(),
            visible: false,
            page_number: None,
            position: None,
            field_name: "ReleaseGateInvisible".to_string(),
            timestamp_url: timestamp_url.clone(),
            embed_validation_info: timestamp_url.is_some(),
            trust_roots: vec![trust_root.to_string_lossy().into_owned()],
        })
        .expect("append and verify the invisible certificate signature");
        assert!(!invisible.visible);
        assert_eq!(invisible.timestamped, timestamp_url.is_some());

        let inspection = inspect_certificate_signatures(InspectCertificateRequest {
            input_path: invisible_output.to_string_lossy().into_owned(),
            input_password: None,
            trust_roots: vec![trust_root.to_string_lossy().into_owned()],
        })
        .expect("validate the twice-signed output");
        assert_eq!(inspection.signature_count, 2);
        assert_eq!(inspection.intact, Some(true));
        assert_eq!(inspection.trusted, Some(true));
        assert_eq!(inspection.state, CertificateValidationState::Valid);

        let untrusted_inspection = inspect_certificate_signatures(InspectCertificateRequest {
            input_path: invisible_output.to_string_lossy().into_owned(),
            input_password: None,
            trust_roots: Vec::new(),
        })
        .expect("separate integrity from an unconfigured trust chain");
        assert_eq!(untrusted_inspection.intact, Some(true));
        assert_eq!(untrusted_inspection.trusted, Some(false));
        assert_eq!(
            untrusted_inspection.state,
            CertificateValidationState::Indeterminate
        );

        let protected = certificate_sign_pdf(CertificateSignRequest {
            input_path: encrypted_source.to_string_lossy().into_owned(),
            input_password: Some(pdf_password.clone()),
            output_path: encrypted_output.to_string_lossy().into_owned(),
            pkcs12_path: certificate.to_string_lossy().into_owned(),
            pkcs12_passphrase: passphrase.clone(),
            pkcs12_passphrase_confirmation: passphrase,
            visible: false,
            page_number: None,
            position: None,
            field_name: "ReleaseGateEncrypted".to_string(),
            timestamp_url: None,
            embed_validation_info: false,
            trust_roots: vec![trust_root.to_string_lossy().into_owned()],
        })
        .expect("sign and verify the password-protected PDF");
        assert!(protected.encrypted);
        assert!(protected.validation.encrypted);
        assert_eq!(protected.validation.intact, Some(true));

        let protected_inspection = inspect_certificate_signatures(InspectCertificateRequest {
            input_path: encrypted_output.to_string_lossy().into_owned(),
            input_password: Some(pdf_password),
            trust_roots: vec![trust_root.to_string_lossy().into_owned()],
        })
        .expect("validate the password-protected signed output");
        assert!(protected_inspection.encrypted);
        assert_eq!(protected_inspection.intact, Some(true));
        assert_eq!(protected_inspection.trusted, Some(true));

        let engine_version = inspect_capabilities()
            .version
            .expect("retain the pyHanko engine version");
        assert!(!engine_version.contains(['\r', '\n', '\t']));
        println!(
            "PAPERWORKS_CERTIFICATE_V1\t1\t{}\t1\t1\t1\t{}\t{}",
            inspection.signature_count,
            u8::from(timestamp_url.is_some()),
            engine_version
        );
    }

    struct TestOutputCleanup(Vec<PathBuf>);

    impl Drop for TestOutputCleanup {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn sample_document() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => dictionary! {},
                "Contents" => content_id,
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id =
            document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog_id);
        document
    }
}
