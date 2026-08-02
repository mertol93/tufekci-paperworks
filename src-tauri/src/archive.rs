use crate::child_process::ManagedChild;
use crate::file_safety::{
    canonical_pdf_input, paths_are_equal, publish_prepared_file, reject_control_characters,
    validated_new_pdf_output,
};
use crate::health::ensure_pdf_rewrite_acknowledged;
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::ocr::{ensure_ocr_ready, validate_ocr_language};
use crate::pdfx::{run_pdfx_preflight, PdfXPreflightResult, PdfXProfile};
use crate::protection::decrypt_pdf_to_path_with_control;
use crate::runtime_capabilities::current_capabilities;
use crate::scan_export::{inspect_searchable_text_pages, run_ocrmypdf_pdfa, OcrPdfOutputKind};
use crate::temporary_cleanup::{register_temporary_path, TemporaryKind, TemporaryLease};
use lopdf::{Document, LoadOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::env;
#[cfg(windows)]
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_ARCHIVE_SOURCE_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_PAGES: usize = 100_000;
const MAX_OBJECT_STREAM_DECOMPRESSION: usize = 64 * 1024 * 1024;
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_VALIDATOR_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_VALIDATOR_RULES: usize = 50;
const MAX_VALIDATOR_TREE_NODES: usize = 200_000;
const MAX_RULE_TEXT_BYTES: usize = 320;
const ENGINE_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const VERAPDF_PROBE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PdfArchiveMode {
    #[serde(rename = "convert")]
    Convert,
    #[serde(rename = "validate")]
    Validate,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PdfAProfile {
    #[serde(rename = "pdfa-1b")]
    PdfA1b,
    #[serde(rename = "pdfa-2b")]
    PdfA2b,
    #[serde(rename = "pdfa-3b")]
    PdfA3b,
}

impl PdfAProfile {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PdfA1b => "PDF/A-1b",
            Self::PdfA2b => "PDF/A-2b",
            Self::PdfA3b => "PDF/A-3b",
        }
    }

    fn ocr_output_kind(self) -> OcrPdfOutputKind {
        match self {
            Self::PdfA1b => OcrPdfOutputKind::PdfA1,
            Self::PdfA2b => OcrPdfOutputKind::PdfA2,
            Self::PdfA3b => OcrPdfOutputKind::PdfA3,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PdfConformanceProfile {
    #[serde(rename = "pdfa-1b")]
    PdfA1b,
    #[serde(rename = "pdfa-2b")]
    PdfA2b,
    #[serde(rename = "pdfa-3b")]
    PdfA3b,
    #[serde(rename = "pdfua-1")]
    PdfUa1,
    #[serde(rename = "pdfua-2")]
    PdfUa2,
    #[serde(rename = "pdfx-1a-2001")]
    PdfX1a2001,
    #[serde(rename = "pdfx-3-2002")]
    PdfX3_2002,
    #[serde(rename = "pdfx-4")]
    PdfX4,
}

impl PdfConformanceProfile {
    fn label(self) -> &'static str {
        match self {
            Self::PdfA1b => "PDF/A-1b",
            Self::PdfA2b => "PDF/A-2b",
            Self::PdfA3b => "PDF/A-3b",
            Self::PdfUa1 => "PDF/UA-1",
            Self::PdfUa2 => "PDF/UA-2",
            Self::PdfX1a2001 => "PDF/X-1a:2001",
            Self::PdfX3_2002 => "PDF/X-3:2002",
            Self::PdfX4 => "PDF/X-4",
        }
    }

    fn vera_flavour(self) -> Option<&'static str> {
        match self {
            Self::PdfA1b => Some("1b"),
            Self::PdfA2b => Some("2b"),
            Self::PdfA3b => Some("3b"),
            Self::PdfUa1 => Some("ua1"),
            Self::PdfUa2 => Some("ua2"),
            Self::PdfX1a2001 | Self::PdfX3_2002 | Self::PdfX4 => None,
        }
    }

    fn pdfa_profile(self) -> Option<PdfAProfile> {
        match self {
            Self::PdfA1b => Some(PdfAProfile::PdfA1b),
            Self::PdfA2b => Some(PdfAProfile::PdfA2b),
            Self::PdfA3b => Some(PdfAProfile::PdfA3b),
            Self::PdfUa1 | Self::PdfUa2 | Self::PdfX1a2001 | Self::PdfX3_2002 | Self::PdfX4 => None,
        }
    }

    fn pdfx_profile(self) -> Option<PdfXProfile> {
        match self {
            Self::PdfX1a2001 => Some(PdfXProfile::PdfX1a2001),
            Self::PdfX3_2002 => Some(PdfXProfile::PdfX3_2002),
            Self::PdfX4 => Some(PdfXProfile::PdfX4),
            Self::PdfA1b | Self::PdfA2b | Self::PdfA3b | Self::PdfUa1 | Self::PdfUa2 => None,
        }
    }

    fn is_pdfa(self) -> bool {
        self.pdfa_profile().is_some()
    }

    fn is_pdfua(self) -> bool {
        matches!(self, Self::PdfUa1 | Self::PdfUa2)
    }
}

impl From<PdfAProfile> for PdfConformanceProfile {
    fn from(profile: PdfAProfile) -> Self {
        match profile {
            PdfAProfile::PdfA1b => Self::PdfA1b,
            PdfAProfile::PdfA2b => Self::PdfA2b,
            PdfAProfile::PdfA3b => Self::PdfA3b,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfArchiveRequest {
    pub(crate) mode: PdfArchiveMode,
    pub(crate) profile: PdfConformanceProfile,
    pub(crate) input_path: String,
    pub(crate) input_password: Option<String>,
    pub(crate) output_path: Option<String>,
    pub(crate) recognise_text: bool,
    pub(crate) ocr_language: String,
    pub(crate) straighten: bool,
    pub(crate) acknowledge_certificate_signatures: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfArchiveRuleFailure {
    specification: Option<String>,
    clause: Option<String>,
    test_number: Option<String>,
    description: Option<String>,
    failed_checks: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfArchiveValidationReport {
    profile: PdfConformanceProfile,
    profile_name: String,
    assessment: PdfConformanceAssessment,
    outcome: PdfConformanceOutcome,
    passed: bool,
    passed_rules: u64,
    failed_rules: u64,
    passed_checks: u64,
    failed_checks: u64,
    failed_rule_summaries: Vec<PdfArchiveRuleFailure>,
    rules_truncated: bool,
    scope_note: String,
    validator_name: String,
    validator_version: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfConformanceAssessment {
    IndependentValidation,
    StructuralPreflight,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfConformanceOutcome {
    Conforms,
    DoesNotConform,
    PreflightPassed,
    PreflightFailed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfArchiveResult {
    mode: PdfArchiveMode,
    profile: PdfConformanceProfile,
    report: PdfArchiveValidationReport,
    output_path: Option<String>,
    bytes_written: u64,
    page_count: usize,
    source_size: u64,
    searchable_text_pages: usize,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfArchiveEngineStatus {
    name: &'static str,
    command: &'static str,
    available: bool,
    version: Option<String>,
    detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfArchiveReadiness {
    ready: bool,
    conversion_ready: bool,
    formal_validation_ready: bool,
    ocr_my_pdf: PdfArchiveEngineStatus,
    ghostscript: PdfArchiveEngineStatus,
    vera_pdf: PdfArchiveEngineStatus,
    detail: String,
}

#[derive(Clone, Copy)]
struct SourceFingerprint {
    size: u64,
    modified_at_ms: Option<u64>,
}

struct PdfInspection {
    encrypted: bool,
    page_count: usize,
}

#[tauri::command]
pub fn pdf_archive_readiness() -> PdfArchiveReadiness {
    if !current_capabilities().archival_pdf() {
        return unavailable_archive_readiness();
    }
    inspect_archive_readiness(&PdfJobExecutionControl::direct())
}

fn unavailable_archive_readiness() -> PdfArchiveReadiness {
    let unavailable = |name, command| PdfArchiveEngineStatus {
        name,
        command,
        available: false,
        version: None,
        detail: Some("Desktop archival engines are unavailable on this platform.".to_string()),
    };
    PdfArchiveReadiness {
        ready: false,
        conversion_ready: false,
        formal_validation_ready: false,
        ocr_my_pdf: unavailable("OCRmyPDF", "ocrmypdf"),
        ghostscript: unavailable("Ghostscript", "ghostscript"),
        vera_pdf: unavailable("veraPDF", "verapdf"),
        detail:
            "PDF archival workflows require desktop engines and are unavailable on this platform."
                .to_string(),
    }
}

pub(crate) fn validate_pdf_archive_request(request: &PdfArchiveRequest) -> Result<(), String> {
    reject_control_characters("Archive source path", &request.input_path)?;
    if let Some(password) = request.input_password.as_deref() {
        reject_control_characters("Archive source password", password)?;
        if password.len() > MAX_PASSWORD_BYTES {
            return Err(format!(
                "An archive source password may contain no more than {MAX_PASSWORD_BYTES} UTF-8 bytes."
            ));
        }
    }
    match request.mode {
        PdfArchiveMode::Validate => {
            if request.output_path.is_some() {
                return Err("Validation does not create an output PDF.".to_string());
            }
            if request.recognise_text || request.straighten {
                return Err("Text recognition and deskew are conversion options.".to_string());
            }
        }
        PdfArchiveMode::Convert => {
            if !request.profile.is_pdfa() {
                return Err(
                    "PDF/UA and PDF/X profiles are validation-only. Choose a PDF/A profile for conversion."
                        .to_string(),
                );
            }
            let output = request
                .output_path
                .as_deref()
                .ok_or_else(|| "Choose a new PDF/A destination.".to_string())?;
            reject_control_characters("Archive output path", output)?;
            if request.straighten && !request.recognise_text {
                return Err("Enable searchable OCR before deskewing archival pages.".to_string());
            }
            if request.recognise_text {
                validate_ocr_language(&request.ocr_language)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn run_pdf_archive_job_with_control(
    request: PdfArchiveRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfArchiveResult, String> {
    run_pdf_archive_with_control(request, control).map_err(|error| {
        if error == PDF_JOB_CANCELLED_ERROR {
            error
        } else {
            safe_archive_job_error(&error)
        }
    })
}

fn run_pdf_archive_with_control(
    request: PdfArchiveRequest,
    control: &PdfJobExecutionControl,
) -> Result<PdfArchiveResult, String> {
    control.checkpoint(2, "Validating archival request")?;
    validate_pdf_archive_request(&request)?;
    let input = canonical_pdf_input(&request.input_path)?;
    let opening_fingerprint = source_fingerprint(&input)?;
    if opening_fingerprint.size == 0 || opening_fingerprint.size > MAX_ARCHIVE_SOURCE_BYTES {
        return Err(format!(
            "An archive source must contain between 1 byte and {} GiB.",
            MAX_ARCHIVE_SOURCE_BYTES / (1024 * 1024 * 1024)
        ));
    }
    let inspection = inspect_pdf(&input, request.input_password.as_deref())?;

    match request.mode {
        PdfArchiveMode::Validate => validate_existing_pdf(
            &input,
            request.input_password.as_deref(),
            opening_fingerprint,
            inspection,
            request.profile,
            control,
        ),
        PdfArchiveMode::Convert => {
            convert_pdf_archive(request, input, opening_fingerprint, inspection, control)
        }
    }
}

fn validate_existing_pdf(
    input: &Path,
    input_password: Option<&str>,
    opening_fingerprint: SourceFingerprint,
    inspection: PdfInspection,
    profile: PdfConformanceProfile,
    control: &PdfJobExecutionControl,
) -> Result<PdfArchiveResult, String> {
    if inspection.encrypted && profile.is_pdfua() {
        return Err(
            "PDF/UA validation must inspect the exact unprotected source. Remove password protection from a copy before validating it."
                .to_string(),
        );
    }
    let launcher = if profile.pdfx_profile().is_some() {
        None
    } else {
        control.checkpoint(8, "Checking veraPDF readiness")?;
        Some(ensure_verapdf_ready(control)?)
    };
    let preferred_parent = input.parent().unwrap_or_else(|| Path::new("."));
    let workspace = ArchiveWorkspace::new(preferred_parent).or_else(|_| {
        let fallback = fs::canonicalize(env::temp_dir())
            .map_err(|error| format!("The temporary folder could not be opened: {error}"))?;
        ArchiveWorkspace::new(&fallback)
    })?;
    let candidate = workspace.path.join("candidate.pdf");

    if inspection.encrypted {
        control.checkpoint(14, "Preparing protected PDF for validation")?;
        decrypt_pdf_to_path_with_control(
            input,
            &candidate,
            input_password.unwrap_or_default(),
            &control.subrange(14, 36, "Preparing protected source".to_string()),
        )?;
    } else {
        copy_or_link_with_control(input, &candidate, &control.subrange(12, 36, String::new()))?;
    }
    verify_validation_candidate(&candidate, inspection.page_count)?;
    verify_source_fingerprint(input, opening_fingerprint)?;
    let mut report = if let Some(pdfx_profile) = profile.pdfx_profile() {
        control.checkpoint(
            40,
            format!("Running {} structural preflight", profile.label()),
        )?;
        pdfx_validation_report(
            profile,
            run_pdfx_preflight(
                &candidate,
                pdfx_profile,
                inspection.encrypted,
                &control.subrange(40, 92, "PDF/X structural preflight".to_string()),
            )?,
        )
    } else {
        control.checkpoint(40, format!("Validating {} conformance", profile.label()))?;
        run_verapdf(
            &candidate,
            profile,
            &workspace,
            launcher
                .as_ref()
                .ok_or_else(|| "The veraPDF validation launcher is unavailable.".to_string())?,
            &control.subrange(40, 92, "veraPDF".to_string()),
        )?
    };
    let mut warnings = Vec::new();
    if inspection.encrypted && profile.is_pdfa() {
        apply_encryption_failure(&mut report);
        warnings.push(
            "The protected source was validated through a private decrypted copy, but the original cannot conform because PDF/A forbids encryption."
                .to_string(),
        );
    }
    let pages_without_text = inspect_searchable_text_pages(&candidate)?;
    verify_source_fingerprint(input, opening_fingerprint)?;
    control.checkpoint(99, "Finalising conformance report")?;
    Ok(PdfArchiveResult {
        mode: PdfArchiveMode::Validate,
        profile,
        report,
        output_path: None,
        bytes_written: 0,
        page_count: inspection.page_count,
        source_size: opening_fingerprint.size,
        searchable_text_pages: inspection
            .page_count
            .saturating_sub(pages_without_text.len()),
        warnings,
    })
}

fn convert_pdf_archive(
    request: PdfArchiveRequest,
    input: PathBuf,
    opening_fingerprint: SourceFingerprint,
    inspection: PdfInspection,
    control: &PdfJobExecutionControl,
) -> Result<PdfArchiveResult, String> {
    let pdfa_profile = request.profile.pdfa_profile().ok_or_else(|| {
        "Choose a PDF/A profile for archival conversion; PDF/UA and PDF/X are validation-only."
            .to_string()
    })?;
    let output = validated_new_pdf_output(
        request
            .output_path
            .as_deref()
            .ok_or_else(|| "Choose a new PDF/A destination.".to_string())?,
    )?;
    if paths_are_equal(&input, &output) {
        return Err("The source PDF cannot be overwritten. Choose a new filename.".to_string());
    }
    control.checkpoint(6, "Checking certificate-signature risk")?;
    ensure_pdf_rewrite_acknowledged(
        &request.input_path,
        request.input_password.as_deref(),
        request.acknowledge_certificate_signatures,
    )?;
    verify_source_fingerprint(&input, opening_fingerprint)?;

    control.checkpoint(10, "Checking local archival engines")?;
    ensure_pdf_archive_ready(
        request
            .recognise_text
            .then_some(request.ocr_language.as_str()),
        control,
    )?;
    let launcher = VeraPdfLauncher::resolve()?;
    let workspace_parent = output
        .parent()
        .ok_or_else(|| "The PDF/A destination folder is invalid.".to_string())?;
    let workspace = ArchiveWorkspace::new(workspace_parent)?;
    let source = if inspection.encrypted {
        let unlocked = workspace.path.join("unlocked.pdf");
        decrypt_pdf_to_path_with_control(
            &input,
            &unlocked,
            request.input_password.as_deref().unwrap_or_default(),
            &control.subrange(12, 24, "Preparing protected source".to_string()),
        )?;
        unlocked
    } else {
        input.clone()
    };
    let candidate = workspace.path.join("candidate.pdf");
    let conversion_control =
        control.subrange(24, 76, format!("Creating {}", request.profile.label()));
    run_ocrmypdf_pdfa(
        &source,
        &candidate,
        pdfa_profile.ocr_output_kind(),
        request
            .recognise_text
            .then_some(request.ocr_language.as_str()),
        request.straighten,
        &conversion_control,
    )?;
    verify_validation_candidate(&candidate, inspection.page_count)?;
    let pages_without_text = inspect_searchable_text_pages(&candidate)?;
    control.checkpoint(78, "Running independent veraPDF validation")?;
    let report = run_verapdf(
        &candidate,
        request.profile,
        &workspace,
        &launcher,
        &control.subrange(78, 93, "veraPDF".to_string()),
    )?;
    if !report.passed {
        return Err(format!(
            "The converted candidate did not pass {} validation ({} failed rules and {} failed checks).",
            request.profile.label(),
            report.failed_rules,
            report.failed_checks
        ));
    }
    verify_source_fingerprint(&input, opening_fingerprint)?;
    control.checkpoint(96, "Publishing verified PDF/A copy")?;
    let bytes_written = publish_prepared_file(&candidate, &output)?;
    Ok(PdfArchiveResult {
        mode: PdfArchiveMode::Convert,
        profile: request.profile,
        report,
        output_path: Some(output.to_string_lossy().into_owned()),
        bytes_written,
        page_count: inspection.page_count,
        source_size: opening_fingerprint.size,
        searchable_text_pages: inspection.page_count.saturating_sub(pages_without_text.len()),
        warnings: vec![
            "PDF/A conversion is a structural rewrite and invalidates existing certificate signatures."
                .to_string(),
        ],
    })
}

pub(crate) fn ensure_pdf_archive_ready(
    ocr_language: Option<&str>,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    if let Some(language) = ocr_language {
        ensure_ocr_ready(language)?;
    }
    let readiness = inspect_archive_readiness(control);
    if readiness.ready {
        Ok(())
    } else {
        Err(readiness.detail)
    }
}

pub(crate) fn convert_pdfa_candidate_with_control(
    input: &Path,
    output: &Path,
    profile: PdfAProfile,
    expected_page_count: usize,
    control: &PdfJobExecutionControl,
) -> Result<PdfArchiveValidationReport, String> {
    let parent = output
        .parent()
        .ok_or_else(|| "The archival workspace destination is invalid.".to_string())?;
    let workspace = ArchiveWorkspace::new(parent)?;
    let candidate = workspace.path.join("candidate.pdf");
    run_ocrmypdf_pdfa(
        input,
        &candidate,
        profile.ocr_output_kind(),
        None,
        false,
        &control.subrange(0, 72, format!("Creating {}", profile.label())),
    )?;
    verify_validation_candidate(&candidate, expected_page_count)?;
    let launcher = VeraPdfLauncher::resolve()?;
    let report = run_verapdf(
        &candidate,
        profile.into(),
        &workspace,
        &launcher,
        &control.subrange(72, 94, "veraPDF".to_string()),
    )?;
    if !report.passed {
        return Err(format!(
            "The archival candidate did not pass {} validation.",
            profile.label()
        ));
    }
    publish_prepared_file(&candidate, output)?;
    Ok(report)
}

fn inspect_archive_readiness(control: &PdfJobExecutionControl) -> PdfArchiveReadiness {
    let ocr_my_pdf = require_ocrmypdf_17(probe_direct_engine(
        "OCRmyPDF",
        "ocrmypdf",
        &["--version"],
        control,
    ));
    let ghostscript = probe_first_direct_engine(
        "Ghostscript",
        "ghostscript",
        &["gs", "gswin64c", "gswin32c"],
        &["--version"],
        control,
    );
    let vera_pdf = match VeraPdfLauncher::resolve() {
        Ok(launcher) => probe_verapdf(&launcher, control),
        Err(error) => PdfArchiveEngineStatus {
            name: "veraPDF",
            command: "verapdf",
            available: false,
            version: None,
            detail: Some(error),
        },
    };
    let conversion_ready = ocr_my_pdf.available && ghostscript.available && vera_pdf.available;
    let formal_validation_ready = vera_pdf.available;
    let ready = conversion_ready;
    let detail = if !ocr_my_pdf.available {
        "Install OCRmyPDF 17 or later for local PDF/A conversion.".to_string()
    } else if !ghostscript.available {
        "Install Ghostscript and add it to PATH for explicit PDF/A conversion.".to_string()
    } else if !vera_pdf.available {
        "Install veraPDF and add its CLI to PATH for independent conformance validation."
            .to_string()
    } else {
        "OCRmyPDF, Ghostscript and veraPDF are ready for local archival conversion.".to_string()
    };
    PdfArchiveReadiness {
        ready,
        conversion_ready,
        formal_validation_ready,
        ocr_my_pdf,
        ghostscript,
        vera_pdf,
        detail,
    }
}

fn require_ocrmypdf_17(mut status: PdfArchiveEngineStatus) -> PdfArchiveEngineStatus {
    if status.available
        && status
            .version
            .as_deref()
            .and_then(first_version_major)
            .is_none_or(|major| major < 17)
    {
        status.available = false;
        status.detail = Some(
            "PDF/A conversion requires OCRmyPDF 17 or later for processing without forced OCR."
                .to_string(),
        );
    }
    status
}

fn first_version_major(value: &str) -> Option<u32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())?
        .parse()
        .ok()
}

fn ensure_verapdf_ready(control: &PdfJobExecutionControl) -> Result<VeraPdfLauncher, String> {
    let launcher = VeraPdfLauncher::resolve()?;
    let status = probe_verapdf(&launcher, control);
    if status.available {
        Ok(launcher)
    } else {
        Err(status.detail.unwrap_or_else(|| {
            "veraPDF is unavailable. Install its local CLI and add it to PATH.".to_string()
        }))
    }
}

fn probe_direct_engine(
    name: &'static str,
    logical_command: &'static str,
    arguments: &[&str],
    control: &PdfJobExecutionControl,
) -> PdfArchiveEngineStatus {
    probe_first_direct_engine(
        name,
        logical_command,
        &[logical_command],
        arguments,
        control,
    )
}

fn probe_first_direct_engine(
    name: &'static str,
    logical_command: &'static str,
    candidates: &[&str],
    arguments: &[&str],
    control: &PdfJobExecutionControl,
) -> PdfArchiveEngineStatus {
    let mut last_detail = None;
    for candidate in candidates {
        let mut command = Command::new(candidate);
        command.args(arguments);
        match run_external_command(
            &mut command,
            control,
            MAX_VALIDATOR_OUTPUT_BYTES.min(64 * 1024),
            Some(ENGINE_PROBE_TIMEOUT),
        ) {
            Ok(output) if output.status.success() => {
                return PdfArchiveEngineStatus {
                    name,
                    command: logical_command,
                    available: true,
                    version: first_output_line(&output.stdout)
                        .or_else(|| first_output_line(&output.stderr)),
                    detail: None,
                };
            }
            Ok(output) => {
                last_detail =
                    first_output_line(&output.stderr).or_else(|| first_output_line(&output.stdout));
            }
            Err(error) if error == PDF_JOB_CANCELLED_ERROR => {
                last_detail = Some(error);
                break;
            }
            Err(error) => last_detail = Some(error),
        }
    }
    PdfArchiveEngineStatus {
        name,
        command: logical_command,
        available: false,
        version: None,
        detail: last_detail,
    }
}

fn probe_verapdf(
    launcher: &VeraPdfLauncher,
    control: &PdfJobExecutionControl,
) -> PdfArchiveEngineStatus {
    let workspace_parent = fs::canonicalize(env::temp_dir()).ok();
    let workspace = workspace_parent
        .as_deref()
        .and_then(|parent| ArchiveWorkspace::new(parent).ok());
    let result = workspace.as_ref().ok_or_else(|| {
        "A private workspace for the veraPDF readiness check could not be created.".to_string()
    });
    let output = result.and_then(|workspace| {
        let mut command = launcher.command(workspace, &["--version"])?;
        run_external_command(
            &mut command,
            control,
            64 * 1024,
            Some(VERAPDF_PROBE_TIMEOUT),
        )
    });
    match output {
        Ok(output) if output.status.success() => PdfArchiveEngineStatus {
            name: "veraPDF",
            command: "verapdf",
            available: true,
            version: first_output_line(&output.stdout)
                .or_else(|| first_output_line(&output.stderr)),
            detail: None,
        },
        Ok(output) => PdfArchiveEngineStatus {
            name: "veraPDF",
            command: "verapdf",
            available: false,
            version: None,
            detail: first_output_line(&output.stderr).or_else(|| first_output_line(&output.stdout)),
        },
        Err(error) => PdfArchiveEngineStatus {
            name: "veraPDF",
            command: "verapdf",
            available: false,
            version: None,
            detail: Some(error),
        },
    }
}

fn run_verapdf(
    candidate: &Path,
    profile: PdfConformanceProfile,
    workspace: &ArchiveWorkspace,
    launcher: &VeraPdfLauncher,
    control: &PdfJobExecutionControl,
) -> Result<PdfArchiveValidationReport, String> {
    if candidate.file_name().and_then(|value| value.to_str()) != Some("candidate.pdf") {
        return Err("The archival validator received an unsafe candidate path.".to_string());
    }
    let flavour = profile.vera_flavour().ok_or_else(|| {
        "The selected profile is not supported by the veraPDF validation boundary.".to_string()
    })?;
    let arguments = [
        "--format",
        "json",
        "--flavour",
        flavour,
        "--maxfailuresdisplayed",
        "3",
        "--disableerrormessages",
        "--loglevel",
        "0",
        "candidate.pdf",
    ];
    let mut command = launcher.command(workspace, &arguments)?;
    let output = run_external_command(&mut command, control, MAX_VALIDATOR_OUTPUT_BYTES, None)?;
    if let Ok(report) = parse_verapdf_report(&output.stdout, profile) {
        return Ok(report);
    }
    let detail = first_output_line(&output.stderr)
        .or_else(|| first_output_line(&output.stdout))
        .unwrap_or_else(|| "veraPDF returned no bounded validation report.".to_string());
    Err(format!("veraPDF validation could not complete: {detail}"))
}

fn parse_verapdf_report(
    bytes: &[u8],
    expected_profile: PdfConformanceProfile,
) -> Result<PdfArchiveValidationReport, String> {
    let json = bounded_json_slice(bytes)
        .ok_or_else(|| "The veraPDF JSON report is missing.".to_string())?;
    let root: Value = serde_json::from_slice(json)
        .map_err(|_| "The veraPDF JSON report is malformed.".to_string())?;
    let report = find_object_with_key(&root, "compliant", &mut 0)
        .ok_or_else(|| "The veraPDF validation result is missing.".to_string())?;
    let compliant = object_bool(report, "compliant")
        .ok_or_else(|| "The veraPDF compliance verdict is missing.".to_string())?;
    let profile_name = object_string(report, "profileName")
        .ok_or_else(|| "The veraPDF profile name is missing.".to_string())?;
    if !profile_matches(&profile_name, expected_profile) {
        return Err(format!(
            "veraPDF validated an unexpected profile instead of {}.",
            expected_profile.label()
        ));
    }
    if object_string(report, "jobEndStatus")
        .is_some_and(|status| !status.eq_ignore_ascii_case("normal"))
    {
        return Err("veraPDF did not finish the validation job normally.".to_string());
    }
    let details = object_value(report, "details").and_then(Value::as_object);
    let passed_rules = details
        .and_then(|value| object_u64(value, "passedRules"))
        .unwrap_or(0);
    let failed_rules = details
        .and_then(|value| object_u64(value, "failedRules"))
        .unwrap_or(0);
    let passed_checks = details
        .and_then(|value| object_u64(value, "passedChecks"))
        .unwrap_or(0);
    let failed_checks = details
        .and_then(|value| object_u64(value, "failedChecks"))
        .unwrap_or(0);
    if compliant && (failed_rules != 0 || failed_checks != 0) {
        return Err("The veraPDF verdict contradicts its failed-check totals.".to_string());
    }
    if !compliant && failed_rules == 0 && failed_checks == 0 {
        return Err(
            "The veraPDF non-conformance verdict has no failed-check evidence.".to_string(),
        );
    }

    let mut summaries = Vec::new();
    let mut seen = HashSet::new();
    let mut total_failed_rule_objects = 0_usize;
    let mut visited = 0_usize;
    collect_failed_rules(
        &root,
        &mut summaries,
        &mut seen,
        &mut total_failed_rule_objects,
        &mut visited,
    );
    let validator_version = find_release_version(&root, &mut 0);
    let outcome = if compliant {
        PdfConformanceOutcome::Conforms
    } else {
        PdfConformanceOutcome::DoesNotConform
    };
    Ok(PdfArchiveValidationReport {
        profile: expected_profile,
        profile_name: bounded_text(&profile_name),
        assessment: PdfConformanceAssessment::IndependentValidation,
        outcome,
        passed: compliant,
        passed_rules,
        failed_rules,
        passed_checks,
        failed_checks,
        failed_rule_summaries: summaries,
        rules_truncated: total_failed_rule_objects > MAX_VALIDATOR_RULES,
        scope_note:
            "Independent veraPDF validation against the selected built-in conformance profile."
                .to_string(),
        validator_name: "veraPDF".to_string(),
        validator_version,
    })
}

fn pdfx_validation_report(
    profile: PdfConformanceProfile,
    preflight: PdfXPreflightResult,
) -> PdfArchiveValidationReport {
    let passed = preflight.failed_checks == 0;
    let failed_rule_summaries = preflight
        .failures
        .into_iter()
        .take(MAX_VALIDATOR_RULES)
        .map(|failure| PdfArchiveRuleFailure {
            specification: Some("ISO 15930 PDF/X structural preflight".to_string()),
            clause: Some(failure.title.to_string()),
            test_number: Some(failure.code.to_string()),
            description: Some(bounded_text(&failure.description)),
            failed_checks: 1,
        })
        .collect();
    PdfArchiveValidationReport {
        profile,
        profile_name: format!("{} structural preflight", profile.label()),
        assessment: PdfConformanceAssessment::StructuralPreflight,
        outcome: if passed {
            PdfConformanceOutcome::PreflightPassed
        } else {
            PdfConformanceOutcome::PreflightFailed
        },
        passed,
        passed_rules: preflight.passed_checks,
        failed_rules: preflight.failed_checks,
        passed_checks: preflight.passed_checks,
        failed_checks: preflight.failed_checks,
        failed_rule_summaries,
        rules_truncated: false,
        scope_note: "This bounded structural preflight checks profile identifiers, trapping, encryption, output intent and ICC structure, embedded fonts, object integrity, page boxes, JavaScript, forms, attachments, external content, non-printing media and transfer curves. It is not ISO 15930 certification, colourimetric proofing, or print-service approval."
            .to_string(),
        validator_name: "Paperworks built-in preflight".to_string(),
        validator_version: Some(env!("CARGO_PKG_VERSION").to_string()),
    }
}

fn apply_encryption_failure(report: &mut PdfArchiveValidationReport) {
    report.passed = false;
    report.outcome = PdfConformanceOutcome::DoesNotConform;
    report.failed_rules = report.failed_rules.saturating_add(1);
    report.failed_checks = report.failed_checks.saturating_add(1);
    report.failed_rule_summaries.insert(
        0,
        PdfArchiveRuleFailure {
            specification: Some("ISO 19005 PDF/A".to_string()),
            clause: Some("Document encryption".to_string()),
            test_number: None,
            description: Some("A PDF/A document shall not be encrypted.".to_string()),
            failed_checks: 1,
        },
    );
    if report.failed_rule_summaries.len() > MAX_VALIDATOR_RULES {
        report.failed_rule_summaries.truncate(MAX_VALIDATOR_RULES);
        report.rules_truncated = true;
    }
}

fn collect_failed_rules(
    value: &Value,
    summaries: &mut Vec<PdfArchiveRuleFailure>,
    seen: &mut HashSet<String>,
    total: &mut usize,
    visited: &mut usize,
) {
    if *visited >= MAX_VALIDATOR_TREE_NODES {
        return;
    }
    *visited += 1;
    match value {
        Value::Object(object) => {
            if object_string(object, "ruleStatus")
                .is_some_and(|status| status.eq_ignore_ascii_case("failed"))
            {
                *total = total.saturating_add(1);
                let specification =
                    object_string(object, "specification").map(|v| bounded_text(&v));
                let clause = object_string(object, "clause").map(|v| bounded_text(&v));
                let test_number = object_string(object, "testNumber").map(|v| bounded_text(&v));
                let description = object_string(object, "description").map(|v| bounded_text(&v));
                let key = format!(
                    "{}|{}|{}",
                    specification.as_deref().unwrap_or_default(),
                    clause.as_deref().unwrap_or_default(),
                    test_number.as_deref().unwrap_or_default()
                );
                if summaries.len() < MAX_VALIDATOR_RULES && seen.insert(key) {
                    summaries.push(PdfArchiveRuleFailure {
                        specification,
                        clause,
                        test_number,
                        description,
                        failed_checks: object_u64(object, "failedChecks").unwrap_or(0),
                    });
                }
            }
            for child in object.values() {
                collect_failed_rules(child, summaries, seen, total, visited);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_failed_rules(child, summaries, seen, total, visited);
            }
        }
        _ => {}
    }
}

fn find_release_version(value: &Value, visited: &mut usize) -> Option<String> {
    if *visited >= MAX_VALIDATOR_TREE_NODES {
        return None;
    }
    *visited += 1;
    match value {
        Value::Object(object) => {
            if object_string(object, "id").is_some_and(|id| id.eq_ignore_ascii_case("core")) {
                if let Some(version) = object_string(object, "version") {
                    return Some(bounded_text(&version));
                }
            }
            object
                .values()
                .find_map(|child| find_release_version(child, visited))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|child| find_release_version(child, visited)),
        _ => None,
    }
}

fn find_object_with_key<'a>(
    value: &'a Value,
    key: &str,
    visited: &mut usize,
) -> Option<&'a Map<String, Value>> {
    if *visited >= MAX_VALIDATOR_TREE_NODES {
        return None;
    }
    *visited += 1;
    match value {
        Value::Object(object) => {
            if find_key(object, key).is_some() {
                return Some(object);
            }
            object
                .values()
                .find_map(|child| find_object_with_key(child, key, visited))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|child| find_object_with_key(child, key, visited)),
        _ => None,
    }
}

fn object_value<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    find_key(object, key).and_then(|found| object.get(found))
}

fn find_key<'a>(object: &'a Map<String, Value>, expected: &str) -> Option<&'a String> {
    object
        .keys()
        .find(|candidate| candidate.eq_ignore_ascii_case(expected))
}

fn object_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object_value(object, key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn object_bool(object: &Map<String, Value>, key: &str) -> Option<bool> {
    object_value(object, key).and_then(|value| match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) if value.eq_ignore_ascii_case("true") => Some(true),
        Value::String(value) if value.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    })
}

fn object_u64(object: &Map<String, Value>, key: &str) -> Option<u64> {
    object_value(object, key).and_then(|value| match value {
        Value::Number(value) => value.as_u64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    })
}

fn profile_matches(profile_name: &str, expected: PdfConformanceProfile) -> bool {
    let compact = profile_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    compact.contains(
        &expected
            .label()
            .replace(['/', '-'], "")
            .to_ascii_uppercase(),
    )
}

fn bounded_text(value: &str) -> String {
    let normalised = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalised.len() <= MAX_RULE_TEXT_BYTES {
        return normalised;
    }
    let mut end = MAX_RULE_TEXT_BYTES;
    while !normalised.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &normalised[..end])
}

fn bounded_json_slice(bytes: &[u8]) -> Option<&[u8]> {
    let start = bytes.iter().position(|byte| *byte == b'{')?;
    let end = bytes.iter().rposition(|byte| *byte == b'}')?;
    (start <= end).then_some(&bytes[start..=end])
}

fn verify_validation_candidate(path: &Path, expected_page_count: usize) -> Result<(), String> {
    let inspection = inspect_pdf(path, None)
        .map_err(|_| "The standards-check candidate could not be reopened safely.".to_string())?;
    if inspection.encrypted {
        return Err(
            "The private standards-check candidate must not contain encryption.".to_string(),
        );
    }
    if inspection.page_count != expected_page_count {
        return Err(
            "The standards-check candidate did not preserve the reviewed page count.".to_string(),
        );
    }
    Ok(())
}

fn inspect_pdf(path: &Path, password: Option<&str>) -> Result<PdfInspection, String> {
    let mut document = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_OBJECT_STREAM_DECOMPRESSION),
    )
    .map_err(|_| "The archive source is not a readable PDF.".to_string())?;
    let encrypted = document.is_encrypted();
    if encrypted {
        document
            .decrypt(password.unwrap_or_default())
            .map_err(|_| {
                "The archive source could not be opened with its supplied password.".to_string()
            })?;
    }
    let page_count = document.get_pages().len();
    if page_count == 0 || page_count > MAX_ARCHIVE_PAGES {
        return Err(format!(
            "An archive source must contain between 1 and {MAX_ARCHIVE_PAGES} pages."
        ));
    }
    Ok(PdfInspection {
        encrypted,
        page_count,
    })
}

fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("The archive source could not be inspected: {error}"))?;
    Ok(SourceFingerprint {
        size: metadata.len(),
        modified_at_ms: modified_at_ms(&metadata),
    })
}

fn verify_source_fingerprint(path: &Path, expected: SourceFingerprint) -> Result<(), String> {
    let observed = source_fingerprint(path)?;
    if observed.size != expected.size || observed.modified_at_ms != expected.modified_at_ms {
        return Err(
            "The archive source changed during processing. No archival copy was published."
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
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

fn copy_or_link_with_control(
    source: &Path,
    destination: &Path,
    control: &PdfJobExecutionControl,
) -> Result<(), String> {
    control.ensure_not_cancelled()?;
    if fs::hard_link(source, destination).is_ok() {
        return control.checkpoint(100, "Prepared PDF for local validation");
    }
    let metadata = fs::metadata(source)
        .map_err(|error| format!("The validation source could not be inspected: {error}"))?;
    let mut input = File::open(source)
        .map_err(|error| format!("The validation source could not be opened: {error}"))?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("The private validation copy could not be created: {error}"))?;
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        control.ensure_not_cancelled()?;
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("The validation source could not be read: {error}"))?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(|error| {
            format!("The private validation copy could not be written: {error}")
        })?;
        copied = copied.saturating_add(read as u64);
        let progress = if metadata.len() == 0 {
            100
        } else {
            ((copied.saturating_mul(100) / metadata.len()).min(100)) as u8
        };
        control.checkpoint(progress, "Preparing PDF for local validation")?;
    }
    output
        .sync_all()
        .map_err(|error| format!("The private validation copy could not be completed: {error}"))?;
    Ok(())
}

fn run_external_command(
    command: &mut Command,
    control: &PdfJobExecutionControl,
    output_limit: usize,
    timeout: Option<Duration>,
) -> Result<Output, String> {
    control.ensure_not_cancelled()?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = ManagedChild::spawn(command)
        .map_err(|error| format!("The local archival engine could not be started: {error}"))?;
    let stdout_reader = child
        .take_stdout()
        .map(|pipe| read_bounded_output(pipe, output_limit));
    let stderr_reader = child
        .take_stderr()
        .map(|pipe| read_bounded_output(pipe, output_limit));
    loop {
        if control.is_cancelled() {
            let _ = child.terminate_tree();
            let _ = child.wait();
            drop(stdout_reader);
            drop(stderr_reader);
            return Err(PDF_JOB_CANCELLED_ERROR.to_string());
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            let _ = child.terminate_tree();
            let _ = child.wait();
            drop(stdout_reader);
            drop(stderr_reader);
            return Err("The local archival engine readiness check timed out safely.".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Output {
                    status,
                    stdout: finish_output_reader(stdout_reader),
                    stderr: finish_output_reader(stderr_reader),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(75)),
            Err(error) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                drop(stdout_reader);
                drop(stderr_reader);
                return Err(format!(
                    "The local archival engine could not be monitored safely: {error}"
                ));
            }
        }
    }
}

fn read_bounded_output<R: Read + Send + 'static>(
    mut pipe: R,
    limit: usize,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 8 * 1024];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let remaining = limit.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..read.min(remaining)]);
        }
        bytes
    })
}

fn finish_output_reader(reader: Option<thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(bounded_text)
}

fn safe_archive_job_error(error: &str) -> String {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("cancelled") {
        return PDF_JOB_CANCELLED_ERROR.to_string();
    }
    if normalised.contains("changed during processing") {
        return "The archive source changed during processing. Reopen it before trying again."
            .to_string();
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return "The archive source could not be opened with its supplied password.".to_string();
    }
    if normalised.contains("pdf/ua validation must inspect") {
        return "PDF/UA validation must inspect an exact unprotected PDF. Remove protection from a copy before validating it."
            .to_string();
    }
    if normalised.contains("verapdf") || normalised.contains("validation") {
        return "Formal PDF/A or PDF/UA validation could not complete. Check the local veraPDF installation and the selected profile."
            .to_string();
    }
    if normalised.contains("pdf/x") || normalised.contains("preflight") {
        return "The bounded PDF/X structural preflight could not complete. Review the source PDF and try again."
            .to_string();
    }
    if normalised.contains("ghostscript")
        || normalised.contains("ocrmypdf")
        || normalised.contains("pdf/a conversion")
    {
        return "PDF/A conversion could not complete. Check OCRmyPDF and Ghostscript, then review the source PDF."
            .to_string();
    }
    "The archival workflow failed a structural safety check. Review the source and try again."
        .to_string()
}

#[derive(Clone)]
enum VeraPdfLauncher {
    Direct(PathBuf),
    #[cfg(windows)]
    WindowsBatch(PathBuf),
}

#[cfg(windows)]
fn windows_cmd_compatible_path(path: &Path) -> PathBuf {
    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const BACKSLASH: u16 = b'\\' as u16;

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if !units.starts_with(VERBATIM_PREFIX) {
        return path.to_path_buf();
    }

    let is_unc = units.len() >= 8
        && matches!(units[4], 85 | 117)
        && matches!(units[5], 78 | 110)
        && matches!(units[6], 67 | 99)
        && units[7] == BACKSLASH;
    let normalised = if is_unc {
        let mut normalised = Vec::with_capacity(units.len().saturating_sub(6));
        normalised.extend_from_slice(&[BACKSLASH, BACKSLASH]);
        normalised.extend_from_slice(&units[8..]);
        normalised
    } else {
        let has_rooted_drive = units.len() >= 7
            && ((units[4] >= 65 && units[4] <= 90) || (units[4] >= 97 && units[4] <= 122))
            && units[5] == b':' as u16
            && units[6] == BACKSLASH;
        if !has_rooted_drive {
            return path.to_path_buf();
        }
        units[4..].to_vec()
    };

    PathBuf::from(OsString::from_wide(&normalised))
}

impl VeraPdfLauncher {
    fn resolve() -> Result<Self, String> {
        if let Some(configured) = env::var_os("PAPERWORKS_VERAPDF") {
            let path = fs::canonicalize(PathBuf::from(configured)).map_err(|_| {
                "PAPERWORKS_VERAPDF does not point to an installed veraPDF launcher.".to_string()
            })?;
            return Self::from_path(path);
        }
        #[cfg(windows)]
        let names = ["verapdf.exe", "verapdf.bat"];
        #[cfg(not(windows))]
        let names = ["verapdf"];
        for directory in env::var_os("PATH")
            .map(|path| env::split_paths(&path).collect::<Vec<_>>())
            .unwrap_or_default()
        {
            for name in names {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    if let Ok(path) = fs::canonicalize(candidate) {
                        return Self::from_path(path);
                    }
                }
            }
        }
        Err(
            "veraPDF was not found on PATH. Install its local CLI before validating PDF/A or PDF/UA."
                .to_string(),
        )
    }

    fn from_path(path: PathBuf) -> Result<Self, String> {
        #[cfg(windows)]
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("bat"))
        {
            return Ok(Self::WindowsBatch(windows_cmd_compatible_path(&path)));
        }
        Ok(Self::Direct(path))
    }

    fn command(&self, workspace: &ArchiveWorkspace, arguments: &[&str]) -> Result<Command, String> {
        match self {
            Self::Direct(path) => {
                let mut command = Command::new(path);
                command.current_dir(&workspace.path).args(arguments);
                Ok(command)
            }
            #[cfg(windows)]
            Self::WindowsBatch(path) => {
                let launcher_name = "paperworks-verapdf.cmd";
                let launcher_path = workspace.path.join(launcher_name);
                if !launcher_path.exists() {
                    let escaped = path.to_string_lossy().replace('^', "^^").replace('%', "%%");
                    let source = format!(
                        "@echo off\r\nsetlocal DisableDelayedExpansion\r\ncall \"{escaped}\" %*\r\nexit /b %errorlevel%\r\n"
                    );
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&launcher_path)
                        .and_then(|mut file| {
                            file.write_all(source.as_bytes())?;
                            file.sync_all()
                        })
                        .map_err(|error| {
                            format!("The private veraPDF launcher could not be created: {error}")
                        })?;
                }
                let mut command = Command::new("cmd.exe");
                command
                    .current_dir(&workspace.path)
                    .args(["/D", "/V:OFF", "/S", "/C", launcher_name])
                    .args(arguments);
                Ok(command)
            }
        }
    }
}

struct ArchiveWorkspace {
    path: PathBuf,
    _lease: TemporaryLease,
}

impl ArchiveWorkspace {
    fn new(parent: &Path) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("The system clock is invalid: {error}"))?
            .as_nanos();
        for attempt in 0..16_u8 {
            let path = parent.join(format!(
                ".paperworks-batch-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            if fs::symlink_metadata(&path).is_ok() {
                continue;
            }
            let mut lease = register_temporary_path(&path, TemporaryKind::BatchDirectory)?;
            match fs::create_dir(lease.path()) {
                Ok(()) => {
                    lease.write_directory_ownership_token()?;
                    return Ok(Self {
                        path: lease.path().to_path_buf(),
                        _lease: lease,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    lease.cancel_without_target_cleanup();
                }
                Err(error) => {
                    lease.cancel_without_target_cleanup();
                    return Err(format!(
                        "The isolated archival workspace could not be created: {error}"
                    ));
                }
            }
        }
        Err("A unique isolated archival workspace could not be created.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan_export::{create_scan_pdf, test_scan_pdf_request};

    fn report_json_for_profile(compliant: bool, profile_name: &str) -> Vec<u8> {
        let (failed_rules, failed_checks, status) = if compliant {
            (0, 0, "passed")
        } else {
            (1, 2, "failed")
        };
        format!(
            r#"JVM warning before JSON
{{
  "report": {{
    "buildInformation": {{"releaseDetails": [{{"id": "core", "version": "1.30.1"}}]}},
    "jobs": [{{
      "validationResult": [{{
        "jobEndStatus": "normal",
        "profileName": "{profile_name}",
        "statement": "Bounded validation fixture.",
        "compliant": {compliant},
        "details": {{
          "passedRules": 143,
          "failedRules": {failed_rules},
          "passedChecks": 900,
          "failedChecks": {failed_checks},
          "ruleSummaries": [{{
            "ruleStatus": "{status}",
            "specification": "ISO 19005-2:2011",
            "clause": "6.2.11.4",
            "testNumber": 1,
            "status": "{status}",
            "description": "Fonts shall be embedded.",
            "failedChecks": {failed_checks},
            "checks": [{{"status": "{status}", "context": "private fixture path"}}]
          }}]
        }}
      }}]
    }}]
  }}
}}
trailing message"#
        )
        .into_bytes()
    }

    fn report_json(compliant: bool) -> Vec<u8> {
        report_json_for_profile(compliant, "PDF/A-2B validation profile")
    }

    #[cfg(windows)]
    #[test]
    fn canonical_windows_batch_launcher_runs_without_verbatim_prefix() {
        let directory = LiveTestDirectory::new();
        let validator_directory = directory.path.join("validator space");
        fs::create_dir(&validator_directory).unwrap();
        let batch_path = validator_directory.join("verapdf.bat");
        fs::write(
            &batch_path,
            b"@echo off\r\necho veraPDF 1.30.2\r\nexit /b 0\r\n",
        )
        .unwrap();

        let canonical = fs::canonicalize(&batch_path).unwrap();
        assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
        let launcher = VeraPdfLauncher::from_path(canonical.clone()).unwrap();
        let VeraPdfLauncher::WindowsBatch(cmd_path) = &launcher else {
            panic!("The test launcher was not recognised as a Windows batch file");
        };
        assert!(!cmd_path.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(fs::canonicalize(cmd_path).unwrap(), canonical);

        let workspace = ArchiveWorkspace::new(&directory.path).unwrap();
        let output = launcher
            .command(&workspace, &["--version"])
            .unwrap()
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            first_output_line(&output.stdout).as_deref(),
            Some("veraPDF 1.30.2")
        );
    }

    #[test]
    fn parses_bounded_compliant_verapdf_json() {
        let report =
            parse_verapdf_report(&report_json(true), PdfConformanceProfile::PdfA2b).unwrap();
        assert!(report.passed);
        assert_eq!(report.profile_name, "PDF/A-2B validation profile");
        assert_eq!(report.validator_version.as_deref(), Some("1.30.1"));
        assert!(report.failed_rule_summaries.is_empty());
    }

    #[test]
    fn retains_only_generic_failed_rule_evidence() {
        let report =
            parse_verapdf_report(&report_json(false), PdfConformanceProfile::PdfA2b).unwrap();
        assert!(!report.passed);
        assert_eq!(report.failed_rules, 1);
        assert_eq!(report.failed_checks, 2);
        assert_eq!(report.failed_rule_summaries.len(), 1);
        assert_eq!(
            report.failed_rule_summaries[0].description.as_deref(),
            Some("Fonts shall be embedded.")
        );
    }

    #[test]
    fn protected_sources_remain_nonconforming_after_private_decrypted_validation() {
        let mut report =
            parse_verapdf_report(&report_json(true), PdfConformanceProfile::PdfA2b).unwrap();
        apply_encryption_failure(&mut report);
        assert!(!report.passed);
        assert_eq!(report.failed_rules, 1);
        assert_eq!(report.failed_checks, 1);
        assert!(report.failed_rule_summaries[0]
            .description
            .as_deref()
            .unwrap()
            .contains("shall not be encrypted"));
    }

    #[test]
    fn rejects_profile_and_verdict_contradictions() {
        assert!(
            parse_verapdf_report(&report_json(true), PdfConformanceProfile::PdfA1b)
                .unwrap_err()
                .contains("unexpected profile")
        );
        let contradictory = String::from_utf8(report_json(true))
            .unwrap()
            .replace("\"failedRules\": 0", "\"failedRules\": 1");
        assert!(
            parse_verapdf_report(contradictory.as_bytes(), PdfConformanceProfile::PdfA2b)
                .unwrap_err()
                .contains("contradicts")
        );
    }

    #[test]
    fn validates_mode_specific_archive_requests() {
        let mut request = PdfArchiveRequest {
            mode: PdfArchiveMode::Validate,
            profile: PdfConformanceProfile::PdfA2b,
            input_path: "source.pdf".to_string(),
            input_password: None,
            output_path: None,
            recognise_text: false,
            ocr_language: "eng".to_string(),
            straighten: false,
            acknowledge_certificate_signatures: false,
        };
        assert!(validate_pdf_archive_request(&request).is_ok());
        request.mode = PdfArchiveMode::Convert;
        assert!(validate_pdf_archive_request(&request)
            .unwrap_err()
            .contains("destination"));
        request.output_path = Some("archive.pdf".to_string());
        request.straighten = true;
        assert!(validate_pdf_archive_request(&request)
            .unwrap_err()
            .contains("before deskewing"));
        request.straighten = false;
        request.profile = PdfConformanceProfile::PdfUa1;
        assert!(validate_pdf_archive_request(&request)
            .unwrap_err()
            .contains("validation-only"));
    }

    #[test]
    fn profile_contract_maps_pdfa_conversion_and_formal_pdfua_validation() {
        assert_eq!(
            PdfAProfile::PdfA2b.ocr_output_kind(),
            OcrPdfOutputKind::PdfA2
        );
        assert_eq!(PdfAProfile::PdfA3b.label(), "PDF/A-3b");
        assert_eq!(PdfConformanceProfile::PdfUa1.vera_flavour(), Some("ua1"));
        assert_eq!(PdfConformanceProfile::PdfUa2.vera_flavour(), Some("ua2"));
        assert!(PdfConformanceProfile::PdfX4.vera_flavour().is_none());
        let report = parse_verapdf_report(
            &report_json_for_profile(true, "PDF/UA-2 validation profile"),
            PdfConformanceProfile::PdfUa2,
        )
        .unwrap();
        assert!(report.passed);
        assert_eq!(report.profile_name, "PDF/UA-2 validation profile");
    }

    #[test]
    fn archival_readiness_requires_ocrmypdf_seventeen() {
        let status = |version: Option<&str>| PdfArchiveEngineStatus {
            name: "OCRmyPDF",
            command: "ocrmypdf",
            available: true,
            version: version.map(str::to_string),
            detail: None,
        };
        assert!(require_ocrmypdf_17(status(Some("17.8.1"))).available);
        assert!(require_ocrmypdf_17(status(Some("ocrmypdf 18.0"))).available);
        assert!(!require_ocrmypdf_17(status(Some("15.2.0"))).available);
        assert!(!require_ocrmypdf_17(status(None)).available);
    }

    #[test]
    #[ignore = "requires OCRmyPDF, Ghostscript, veraPDF, Tesseract eng data, and PAPERWORKS_OCR_CORPUS"]
    fn live_pdfa_profiles_convert_ocr_and_validate() {
        let corpus = env::var_os("PAPERWORKS_OCR_CORPUS")
            .map(PathBuf::from)
            .expect("PAPERWORKS_OCR_CORPUS is required");
        let source_image = corpus.join("english.png");
        assert!(
            source_image.is_file(),
            "the public English OCR fixture is missing"
        );
        let directory = LiveTestDirectory::new();
        let image_pdf = directory.path.join("image-only.pdf");
        create_scan_pdf(test_scan_pdf_request(
            vec![source_image.to_string_lossy().into_owned()],
            image_pdf.to_string_lossy().into_owned(),
        ))
        .unwrap();

        let source_report = run_pdf_archive_with_control(
            PdfArchiveRequest {
                mode: PdfArchiveMode::Validate,
                profile: PdfConformanceProfile::PdfA2b,
                input_path: image_pdf.to_string_lossy().into_owned(),
                input_password: None,
                output_path: None,
                recognise_text: false,
                ocr_language: "eng".to_string(),
                straighten: false,
                acknowledge_certificate_signatures: false,
            },
            &PdfJobExecutionControl::direct(),
        )
        .unwrap();
        assert!(!source_report.report.passed);
        assert!(source_report.report.failed_rules > 0);
        assert!(source_report.report.failed_checks > 0);
        assert!(!source_report.report.failed_rule_summaries.is_empty());

        for (profile, profile_id) in [
            (PdfAProfile::PdfA1b, "pdfa-1b"),
            (PdfAProfile::PdfA2b, "pdfa-2b"),
            (PdfAProfile::PdfA3b, "pdfa-3b"),
        ] {
            let output = directory.path.join(format!("{profile_id}.pdf"));
            let result = run_pdf_archive_with_control(
                PdfArchiveRequest {
                    mode: PdfArchiveMode::Convert,
                    profile: profile.into(),
                    input_path: image_pdf.to_string_lossy().into_owned(),
                    input_password: None,
                    output_path: Some(output.to_string_lossy().into_owned()),
                    recognise_text: true,
                    ocr_language: "eng".to_string(),
                    straighten: true,
                    acknowledge_certificate_signatures: false,
                },
                &PdfJobExecutionControl::direct(),
            )
            .unwrap();

            assert!(result.report.passed);
            assert_eq!(result.report.failed_rules, 0);
            assert_eq!(result.report.failed_checks, 0);
            assert_eq!(result.page_count, 1);
            assert_eq!(result.searchable_text_pages, 1);
            assert!(output.is_file());
            let validator_version = result
                .report
                .validator_version
                .as_deref()
                .expect("veraPDF version evidence is required");
            assert!(
                validator_version.chars().all(
                    |character| character.is_ascii_alphanumeric() || ".+-_".contains(character)
                ),
                "veraPDF returned an unsafe version marker"
            );
            println!("PAPERWORKS_PDFA_PROFILE_V1\t{profile_id}\t1\t1\t0\t0\t{validator_version}");
        }
    }

    struct LiveTestDirectory {
        path: PathBuf,
    }

    impl LiveTestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "paperworks-pdfa-live-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for LiveTestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
