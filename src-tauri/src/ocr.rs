use crate::child_process::ManagedChild;
use crate::file_safety::reject_control_characters;
use crate::runtime_capabilities::current_capabilities;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const ENGINE_OUTPUT_LIMIT: usize = 1024 * 1024;
const TSV_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);
const REVIEW_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_LANGUAGE_CODES: usize = 8;
const MAX_LANGUAGE_LENGTH: usize = 160;
const MAX_OCR_WORDS: usize = 20_000;
const MAX_RETURNED_LOW_CONFIDENCE_WORDS: usize = 250;
const MAX_WORD_BYTES: usize = 512;
const LOW_CONFIDENCE_THRESHOLD: f32 = 80.0;
pub(crate) const OCR_REVIEW_CANCELLED_ERROR: &str = "The OCR review was cancelled.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLanguage {
    code: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OcrReadinessRequest {
    language: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrEngineStatus {
    name: &'static str,
    command: &'static str,
    available: bool,
    version: Option<String>,
    detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrReadinessReport {
    ready: bool,
    selected_language: String,
    language_available: bool,
    languages: Vec<OcrLanguage>,
    ocr_my_pdf: OcrEngineStatus,
    tesseract: OcrEngineStatus,
    detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrConfidenceWord {
    pub(crate) word_number: usize,
    pub(crate) text: String,
    pub(crate) confidence: f32,
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrConfidenceResult {
    pub(crate) language: String,
    pub(crate) image_width: u32,
    pub(crate) image_height: u32,
    pub(crate) word_count: usize,
    pub(crate) average_confidence: Option<f32>,
    pub(crate) minimum_confidence: Option<f32>,
    pub(crate) low_confidence_threshold: f32,
    pub(crate) low_confidence_count: usize,
    pub(crate) low_confidence_words: Vec<OcrConfidenceWord>,
    pub(crate) malformed_rows: usize,
    pub(crate) warnings: Vec<String>,
}

#[tauri::command]
pub fn ocr_readiness(request: OcrReadinessRequest) -> Result<OcrReadinessReport, String> {
    if !current_capabilities().searchable_ocr() {
        return Err(
            "Searchable OCR requires a desktop engine and is unavailable on this platform."
                .to_string(),
        );
    }
    validate_ocr_language(&request.language)?;
    Ok(inspect_ocr_readiness(&request.language))
}

pub(crate) fn ensure_ocr_ready(language: &str) -> Result<(), String> {
    validate_ocr_language(language)?;
    let report = inspect_ocr_readiness(language);
    if report.ready {
        Ok(())
    } else {
        Err(report.detail)
    }
}

pub(crate) fn validate_ocr_language(language: &str) -> Result<(), String> {
    reject_control_characters("OCR language", language)?;
    if language.is_empty() || language.len() > MAX_LANGUAGE_LENGTH {
        return Err("Choose a valid installed OCR language code.".to_string());
    }

    let codes = language.split('+').collect::<Vec<_>>();
    if codes.is_empty()
        || codes.len() > MAX_LANGUAGE_CODES
        || codes.iter().any(|code| {
            code.is_empty()
                || code.len() > 32
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
    {
        return Err("Choose a valid installed OCR language code.".to_string());
    }
    Ok(())
}

pub(crate) fn analyse_raster_with_tesseract_with_cancellation(
    raster: &Path,
    language: &str,
    image_width: u32,
    image_height: u32,
    dpi: u32,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<OcrConfidenceResult, String> {
    validate_ocr_language(language)?;
    ensure_tesseract_ready_with_cancellation(language, is_cancelled)?;
    if image_width == 0 || image_height == 0 {
        return Err("The OCR review image has no pixels.".to_string());
    }

    let mut command = Command::new("tesseract");
    command
        .arg(raster)
        .arg("stdout")
        .arg("-l")
        .arg(language)
        .arg("--dpi")
        .arg(dpi.to_string())
        .arg("--psm")
        .arg("6")
        .arg("tsv");
    let output = match run_bounded_command_with_cancellation(
        &mut command,
        TSV_OUTPUT_LIMIT,
        REVIEW_TIMEOUT,
        is_cancelled,
    ) {
        Ok(output) => output,
        Err(CommandRunError::Cancelled) => return Err(OCR_REVIEW_CANCELLED_ERROR.to_string()),
        Err(error) => {
            return Err(format!(
                "Tesseract confidence review could not be started: {}",
                error.detail()
            ))
        }
    };
    if !output.success {
        let detail = first_output_line(&output.stderr)
            .or_else(|| first_output_line(&output.stdout))
            .unwrap_or_else(|| "Tesseract returned an unknown error.".to_string());
        return Err(format!(
            "Tesseract confidence review did not complete: {detail}"
        ));
    }
    if output.stdout_truncated {
        return Err(format!(
            "Tesseract confidence output exceeded the {} MB safety limit.",
            TSV_OUTPUT_LIMIT / (1024 * 1024)
        ));
    }

    parse_tesseract_tsv(&output.stdout, language, image_width, image_height)
}

fn inspect_ocr_readiness(language: &str) -> OcrReadinessReport {
    let ocr_my_pdf = probe_engine("OCRmyPDF", "ocrmypdf");
    let tesseract = probe_engine("Tesseract", "tesseract");
    let mut language_diagnostic = None;
    let languages = if tesseract.available {
        match list_tesseract_languages() {
            Ok(languages) if !languages.is_empty() => languages,
            Ok(_) => {
                language_diagnostic = Some(
                    "Tesseract did not report any installed recognition language packs."
                        .to_string(),
                );
                Vec::new()
            }
            Err(error) => {
                language_diagnostic = Some(error);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let installed = languages
        .iter()
        .map(|language| language.code.as_str())
        .collect::<HashSet<_>>();
    let language_available = language.split('+').all(|code| installed.contains(code));
    let ready = ocr_my_pdf.available && tesseract.available && language_available;
    let detail = if !ocr_my_pdf.available {
        unavailable_detail(&ocr_my_pdf)
    } else if !tesseract.available {
        unavailable_detail(&tesseract)
    } else if let Some(detail) = language_diagnostic {
        detail
    } else if !language_available {
        format!(
            "The selected Tesseract language pack ({language}) is not installed. Install it locally, then refresh OCR readiness."
        )
    } else {
        format!("OCRmyPDF, Tesseract and the {language} language pack are ready.")
    };

    OcrReadinessReport {
        ready,
        selected_language: language.to_string(),
        language_available,
        languages,
        ocr_my_pdf,
        tesseract,
        detail,
    }
}

fn ensure_tesseract_ready_with_cancellation(
    language: &str,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let tesseract = match probe_engine_with_cancellation("Tesseract", "tesseract", is_cancelled) {
        Ok(status) => status,
        Err(CommandRunError::Cancelled) => return Err(OCR_REVIEW_CANCELLED_ERROR.to_string()),
        Err(error) => return Err(format!("Tesseract readiness failed: {}", error.detail())),
    };
    if !tesseract.available {
        return Err(unavailable_detail(&tesseract));
    }
    let languages = list_tesseract_languages_with_cancellation(is_cancelled)?;
    let installed = languages
        .iter()
        .map(|entry| entry.code.as_str())
        .collect::<HashSet<_>>();
    let missing = language
        .split('+')
        .filter(|code| !installed.contains(code))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Install the missing Tesseract language pack{}: {}.",
            if missing.len() == 1 { "" } else { "s" },
            missing.join(", ")
        ))
    }
}

fn probe_engine(name: &'static str, command_name: &'static str) -> OcrEngineStatus {
    probe_engine_with_cancellation(name, command_name, &|| false).unwrap_or_else(|error| {
        OcrEngineStatus {
            name,
            command: command_name,
            available: false,
            version: None,
            detail: Some(error.detail()),
        }
    })
}

fn probe_engine_with_cancellation(
    name: &'static str,
    command_name: &'static str,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<OcrEngineStatus, CommandRunError> {
    let mut command = Command::new(command_name);
    command.arg("--version");
    match run_bounded_command_with_cancellation(
        &mut command,
        ENGINE_OUTPUT_LIMIT,
        ENGINE_TIMEOUT,
        is_cancelled,
    ) {
        Ok(output) if output.success => Ok(OcrEngineStatus {
            name,
            command: command_name,
            available: true,
            version: first_output_line(&output.stdout)
                .or_else(|| first_output_line(&output.stderr)),
            detail: output
                .stdout_truncated
                .then(|| "Version output was truncated safely.".to_string()),
        }),
        Ok(output) => Ok(OcrEngineStatus {
            name,
            command: command_name,
            available: false,
            version: None,
            detail: first_output_line(&output.stderr)
                .or_else(|| first_output_line(&output.stdout))
                .or_else(|| Some("The installed command returned an error.".to_string())),
        }),
        Err(CommandRunError::Cancelled) => Err(CommandRunError::Cancelled),
        Err(error) => Ok(OcrEngineStatus {
            name,
            command: command_name,
            available: false,
            version: None,
            detail: Some(error.detail()),
        }),
    }
}

fn list_tesseract_languages() -> Result<Vec<OcrLanguage>, String> {
    list_tesseract_languages_with_cancellation(&|| false)
}

fn list_tesseract_languages_with_cancellation(
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<OcrLanguage>, String> {
    let mut command = Command::new("tesseract");
    command.arg("--list-langs");
    let output = match run_bounded_command_with_cancellation(
        &mut command,
        ENGINE_OUTPUT_LIMIT,
        ENGINE_TIMEOUT,
        is_cancelled,
    ) {
        Ok(output) => output,
        Err(CommandRunError::Cancelled) => return Err(OCR_REVIEW_CANCELLED_ERROR.to_string()),
        Err(error) => {
            return Err(format!(
                "Tesseract language discovery failed: {}",
                error.detail()
            ))
        }
    };
    if !output.success {
        let detail = first_output_line(&output.stderr)
            .or_else(|| first_output_line(&output.stdout))
            .unwrap_or_else(|| "Tesseract returned an unknown error.".to_string());
        return Err(format!("Tesseract language discovery failed: {detail}"));
    }
    if output.stdout_truncated || output.stderr_truncated {
        return Err("Tesseract language discovery exceeded its safety limit.".to_string());
    }
    let mut bytes = output.stdout;
    bytes.push(b'\n');
    bytes.extend_from_slice(&output.stderr);
    Ok(parse_tesseract_languages(&bytes))
}

fn parse_tesseract_languages(bytes: &[u8]) -> Vec<OcrLanguage> {
    let mut languages = String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("List of available languages")
                && *line != "osd"
                && line
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
        .map(|code| OcrLanguage {
            code: code.to_string(),
            name: language_name(code).to_string(),
        })
        .collect::<Vec<_>>();
    languages.sort_by(|left, right| left.name.cmp(&right.name).then(left.code.cmp(&right.code)));
    languages.dedup_by(|left, right| left.code == right.code);
    languages
}

fn parse_tesseract_tsv(
    bytes: &[u8],
    language: &str,
    image_width: u32,
    image_height: u32,
) -> Result<OcrConfidenceResult, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Tesseract returned confidence data that was not valid UTF-8.".to_string())?;
    let mut word_count = 0_usize;
    let mut confidence_sum = 0.0_f64;
    let mut minimum_confidence = None::<f32>;
    let mut low_confidence_count = 0_usize;
    let mut low_confidence_words = Vec::new();
    let mut malformed_rows = 0_usize;

    for line in text.lines() {
        if line.is_empty() || line.starts_with("level\t") || line.starts_with('\u{feff}') {
            continue;
        }
        let fields = line.splitn(12, '\t').collect::<Vec<_>>();
        if fields.len() != 12 {
            malformed_rows = malformed_rows.saturating_add(1);
            continue;
        }
        let Ok(level) = fields[0].parse::<u8>() else {
            malformed_rows = malformed_rows.saturating_add(1);
            continue;
        };
        if level != 5 {
            continue;
        }

        let parsed = (
            fields[6].parse::<u32>(),
            fields[7].parse::<u32>(),
            fields[8].parse::<u32>(),
            fields[9].parse::<u32>(),
            fields[10].parse::<f32>(),
        );
        let (Ok(left), Ok(top), Ok(width), Ok(height), Ok(confidence)) = parsed else {
            malformed_rows = malformed_rows.saturating_add(1);
            continue;
        };
        let raw_word = fields[11].trim();
        if raw_word.is_empty() {
            continue;
        }
        if raw_word.len() > MAX_WORD_BYTES
            || width == 0
            || height == 0
            || left
                .checked_add(width)
                .is_none_or(|right| right > image_width)
            || top
                .checked_add(height)
                .is_none_or(|bottom| bottom > image_height)
            || !confidence.is_finite()
            || !(0.0..=100.0).contains(&confidence)
        {
            malformed_rows = malformed_rows.saturating_add(1);
            continue;
        }

        word_count = word_count.saturating_add(1);
        if word_count > MAX_OCR_WORDS {
            return Err(format!(
                "Tesseract returned more than {MAX_OCR_WORDS} words for one review page."
            ));
        }
        confidence_sum += f64::from(confidence);
        minimum_confidence =
            Some(minimum_confidence.map_or(confidence, |current| current.min(confidence)));
        if confidence < LOW_CONFIDENCE_THRESHOLD {
            low_confidence_count = low_confidence_count.saturating_add(1);
            if low_confidence_words.len() < MAX_RETURNED_LOW_CONFIDENCE_WORDS {
                low_confidence_words.push(OcrConfidenceWord {
                    word_number: word_count,
                    text: sanitise_ocr_word(raw_word),
                    confidence: round_tenth(confidence),
                    left,
                    top,
                    width,
                    height,
                });
            }
        }
    }

    let mut warnings = Vec::new();
    if word_count == 0 {
        warnings.push("No words were recognised on this page.".to_string());
    }
    if malformed_rows > 0 {
        warnings.push(format!(
            "{malformed_rows} malformed confidence row{} were ignored safely.",
            if malformed_rows == 1 { "" } else { "s" }
        ));
    }
    if low_confidence_count > low_confidence_words.len() {
        warnings.push(format!(
            "Only the first {} low-confidence words are shown.",
            low_confidence_words.len()
        ));
    }

    Ok(OcrConfidenceResult {
        language: language.to_string(),
        image_width,
        image_height,
        word_count,
        average_confidence: (word_count > 0)
            .then(|| round_tenth((confidence_sum / word_count as f64) as f32)),
        minimum_confidence: minimum_confidence.map(round_tenth),
        low_confidence_threshold: LOW_CONFIDENCE_THRESHOLD,
        low_confidence_count,
        low_confidence_words,
        malformed_rows,
        warnings,
    })
}

fn sanitise_ocr_word(word: &str) -> String {
    word.chars()
        .map(|character| {
            if matches!(character, '\t' | '\r' | '\n') {
                ' '
            } else {
                character
            }
        })
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn language_name(code: &str) -> &str {
    match code {
        "ara" => "Arabic",
        "chi_sim" => "Chinese (Simplified)",
        "chi_tra" => "Chinese (Traditional)",
        "deu" => "German",
        "eng" => "English",
        "fra" => "French",
        "ita" => "Italian",
        "jpn" => "Japanese",
        "kor" => "Korean",
        "nld" => "Dutch",
        "osd" => "Orientation and script detection",
        "pol" => "Polish",
        "por" => "Portuguese",
        "rus" => "Russian",
        "spa" => "Spanish",
        "tur" => "Turkish",
        _ => code,
    }
}

fn unavailable_detail(status: &OcrEngineStatus) -> String {
    let detail = status
        .detail
        .as_deref()
        .unwrap_or("No diagnostic detail was returned.");
    format!(
        "{} is unavailable. Install {} and make sure '{}' is on PATH. {detail}",
        status.name, status.name, status.command
    )
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.chars()
                .filter(|character| !character.is_control())
                .take(320)
                .collect::<String>()
        })
}

fn round_tenth(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

struct CapturedOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

enum CommandRunError {
    Cancelled,
    Start(std::io::Error),
    Monitor(std::io::Error),
    TimedOut,
}

impl CommandRunError {
    fn detail(&self) -> String {
        match self {
            Self::Cancelled => "The command was cancelled.".to_string(),
            Self::Start(error) if error.kind() == std::io::ErrorKind::NotFound => {
                "The command was not found on PATH.".to_string()
            }
            Self::Start(error) => format!("The command could not be started: {error}"),
            Self::Monitor(error) => format!("The command could not be monitored safely: {error}"),
            Self::TimedOut => "The command did not respond before the safety timeout.".to_string(),
        }
    }
}

#[derive(Default)]
struct BoundedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

fn run_bounded_command_with_cancellation(
    command: &mut Command,
    output_limit: usize,
    timeout: Duration,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<CapturedOutput, CommandRunError> {
    if is_cancelled() {
        return Err(CommandRunError::Cancelled);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(command).map_err(CommandRunError::Start)?;
    let stdout_reader = child
        .take_stdout()
        .map(|pipe| read_bounded_output(pipe, output_limit));
    let stderr_reader = child
        .take_stderr()
        .map(|pipe| read_bounded_output(pipe, output_limit));
    let deadline = Instant::now() + timeout;

    let status = loop {
        if is_cancelled() {
            let _ = child.terminate_tree();
            let _ = child.wait();
            drop(stdout_reader);
            drop(stderr_reader);
            return Err(CommandRunError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(40)),
            Ok(None) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                drop(stdout_reader);
                drop(stderr_reader);
                return Err(CommandRunError::TimedOut);
            }
            Err(error) => {
                let _ = child.terminate_tree();
                let _ = child.wait();
                drop(stdout_reader);
                drop(stderr_reader);
                return Err(CommandRunError::Monitor(error));
            }
        }
    };
    let stdout = finish_bounded_output(stdout_reader);
    let stderr = finish_bounded_output(stderr_reader);
    Ok(CapturedOutput {
        success: status.success(),
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
    })
}

fn read_bounded_output<R>(mut pipe: R, limit: usize) -> thread::JoinHandle<BoundedBytes>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut captured = BoundedBytes::default();
        let mut chunk = [0_u8; 8 * 1024];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let remaining = limit.saturating_sub(captured.bytes.len());
            captured
                .bytes
                .extend_from_slice(&chunk[..read.min(remaining)]);
            captured.truncated |= read > remaining;
        }
        captured
    })
}

fn finish_bounded_output(reader: Option<thread::JoinHandle<BoundedBytes>>) -> BoundedBytes {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_single_and_combined_language_codes() {
        assert!(validate_ocr_language("eng").is_ok());
        assert!(validate_ocr_language("eng+tur").is_ok());
        assert!(validate_ocr_language("eng++tur").is_err());
        assert!(validate_ocr_language("eng;tur").is_err());
    }

    #[test]
    fn confidence_review_cancellation_stops_before_starting_tesseract() {
        let error = analyse_raster_with_tesseract_with_cancellation(
            Path::new("unused-review-raster.png"),
            "eng",
            100,
            100,
            150,
            &|| true,
        )
        .unwrap_err();
        assert_eq!(error, OCR_REVIEW_CANCELLED_ERROR);
    }

    #[test]
    fn parses_and_sorts_installed_languages() {
        let languages = parse_tesseract_languages(
            b"List of available languages in C:\\tessdata (3):\ntur\neng\nosd\n",
        );
        assert_eq!(languages.len(), 2);
        assert_eq!(languages[0].code, "eng");
        assert_eq!(languages[0].name, "English");
    }

    #[test]
    fn parses_bounded_word_confidence_and_boxes() {
        let tsv = concat!(
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
            "1\t1\t0\t0\t0\t0\t0\t0\t800\t600\t-1\t\n",
            "5\t1\t1\t1\t1\t1\t40\t50\t120\t30\t96.4\tMerhaba\n",
            "5\t1\t1\t1\t1\t2\t180\t50\t100\t30\t62.5\tdunya\n",
            "5\t1\t1\t1\t1\t3\t300\t50\t80\t30\t79.9\ttwo\twords\n",
            "5\t1\t1\t1\t1\t4\t790\t50\t20\t30\t45\toutside\n",
        );
        let report = parse_tesseract_tsv(tsv.as_bytes(), "tur", 800, 600).unwrap();
        assert_eq!(report.word_count, 3);
        assert_eq!(report.low_confidence_count, 2);
        assert_eq!(report.low_confidence_words[0].text, "dunya");
        assert_eq!(report.low_confidence_words[1].text, "two words");
        assert_eq!(report.average_confidence, Some(79.6));
        assert_eq!(report.minimum_confidence, Some(62.5));
        assert_eq!(report.malformed_rows, 1);
    }

    #[test]
    fn reports_empty_confidence_output_without_panicking() {
        let report = parse_tesseract_tsv(
            b"level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
            "eng",
            640,
            480,
        )
        .unwrap();
        assert_eq!(report.word_count, 0);
        assert_eq!(report.average_confidence, None);
        assert_eq!(
            report.warnings,
            vec!["No words were recognised on this page."]
        );
    }
}
