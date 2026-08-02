use crate::file_safety::reject_control_characters;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::ipc::Response;

const INITIAL_PDF_BYTES: usize = 64 * 1024;
const MAX_FULL_DOCUMENT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PDF_RANGE_BYTES: u64 = 1024 * 1024;
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "pdf", "avif", "bmp", "gif", "heic", "heif", "jpeg", "jpg", "pbm", "pgm", "png", "pnm", "ppm",
    "tif", "tiff", "webp",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPdfInfo {
    path: String,
    name: String,
    size: u64,
    modified_at_ms: Option<u64>,
    initial_data: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadLocalPdfRangeRequest {
    path: String,
    expected_size: u64,
    expected_modified_at_ms: Option<u64>,
    begin: u64,
    end: u64,
}

#[tauri::command]
pub fn read_local_document(path: String) -> Result<Response, String> {
    let path = validate_document_path(&path)?;
    let size = fs::metadata(&path)
        .map_err(|error| format!("The selected file could not be inspected: {error}"))?
        .len();
    if size > MAX_FULL_DOCUMENT_BYTES {
        return Err(format!(
            "The selected file is larger than {} MB and cannot be loaded into memory safely.",
            MAX_FULL_DOCUMENT_BYTES / (1024 * 1024)
        ));
    }
    let data =
        fs::read(&path).map_err(|error| format!("The selected file could not be read: {error}"))?;

    Ok(Response::new(data))
}

#[tauri::command]
pub fn open_local_pdf(path: String) -> Result<LocalPdfInfo, String> {
    let path = validate_pdf_path(&path)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("The selected PDF could not be inspected: {error}"))?;
    let size = metadata.len();
    if size == 0 {
        return Err("The selected PDF is empty.".to_string());
    }
    if size > MAX_PDF_BYTES {
        return Err(format!(
            "The selected PDF is larger than the supported {} GB limit.",
            MAX_PDF_BYTES / (1024 * 1024 * 1024)
        ));
    }
    let modified_at_ms = modified_at_ms(&metadata);
    let mut file = File::open(&path)
        .map_err(|error| format!("The selected PDF could not be opened: {error}"))?;
    let mut initial_data = vec![0_u8; usize::try_from(size.min(INITIAL_PDF_BYTES as u64)).unwrap()];
    file.read_exact(&mut initial_data)
        .map_err(|error| format!("The selected PDF header could not be read: {error}"))?;

    Ok(LocalPdfInfo {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document.pdf".to_string()),
        path: path.to_string_lossy().into_owned(),
        size,
        modified_at_ms,
        initial_data,
    })
}

#[tauri::command]
pub fn read_local_pdf_range(request: ReadLocalPdfRangeRequest) -> Result<Response, String> {
    let path = validate_pdf_path(&request.path)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("The PDF range could not be inspected: {error}"))?;
    if metadata.len() != request.expected_size
        || modified_at_ms(&metadata) != request.expected_modified_at_ms
    {
        return Err(
            "The PDF changed on disk while it was open. Reopen it before continuing.".to_string(),
        );
    }
    if request.begin >= request.end || request.end > request.expected_size {
        return Err("The requested PDF byte range is invalid.".to_string());
    }
    let length = request.end - request.begin;
    if length > MAX_PDF_RANGE_BYTES {
        return Err(format!(
            "A PDF range request may contain no more than {} MB.",
            MAX_PDF_RANGE_BYTES / (1024 * 1024)
        ));
    }

    let mut file =
        File::open(&path).map_err(|error| format!("The PDF range could not be opened: {error}"))?;
    file.seek(SeekFrom::Start(request.begin))
        .map_err(|error| format!("The PDF range could not be located: {error}"))?;
    let mut data = vec![0_u8; usize::try_from(length).unwrap()];
    file.read_exact(&mut data)
        .map_err(|error| format!("The PDF range could not be read: {error}"))?;
    Ok(Response::new(data))
}

fn validate_pdf_path(path: &str) -> Result<PathBuf, String> {
    let path = validate_document_path(path)?;
    if !has_extension(&path, "pdf") {
        return Err("Choose a PDF document.".to_string());
    }
    Ok(path)
}

fn validate_document_path(path: &str) -> Result<PathBuf, String> {
    reject_control_characters("Document path", path)?;
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("The selected file could not be opened: {error}"))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("The selected file could not be inspected: {error}"))?;

    if !metadata.is_file() || !has_supported_extension(&canonical) {
        return Err("Choose a supported PDF or image file.".to_string());
    }

    Ok(canonical)
}

fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::ipc::{InvokeResponseBody, IpcResponse};

    #[test]
    fn recognises_supported_extensions_without_case_sensitivity() {
        assert!(has_supported_extension(Path::new("scan.TIFF")));
        assert!(has_supported_extension(Path::new("scanner-output.PNM")));
        assert!(has_supported_extension(Path::new("document.PDF")));
        assert!(!has_supported_extension(Path::new("script.exe")));
    }

    #[test]
    fn opens_pdf_metadata_and_reads_bounded_ranges() {
        let directory = TestDirectory::new();
        let path = directory.path.join("large.PDF");
        let bytes = (0..200_000)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&path, &bytes).unwrap();

        let info = open_local_pdf(path.to_string_lossy().into_owned()).unwrap();
        assert_eq!(info.name, "large.PDF");
        assert_eq!(info.size, bytes.len() as u64);
        assert_eq!(info.initial_data, bytes[..INITIAL_PDF_BYTES]);

        let response = read_local_pdf_range(ReadLocalPdfRangeRequest {
            path: info.path,
            expected_size: info.size,
            expected_modified_at_ms: info.modified_at_ms,
            begin: 70_000,
            end: 90_000,
        })
        .unwrap();
        let InvokeResponseBody::Raw(range) = response.body().unwrap() else {
            panic!("range response was not binary");
        };
        assert_eq!(range, bytes[70_000..90_000]);
    }

    #[test]
    fn range_reads_reject_a_pdf_that_changed_on_disk() {
        let directory = TestDirectory::new();
        let path = directory.path.join("changing.pdf");
        fs::write(&path, vec![7_u8; 100_000]).unwrap();
        let info = open_local_pdf(path.to_string_lossy().into_owned()).unwrap();
        fs::write(&path, vec![7_u8; 100_001]).unwrap();

        let error = match read_local_pdf_range(ReadLocalPdfRangeRequest {
            path: info.path,
            expected_size: info.size,
            expected_modified_at_ms: info.modified_at_ms,
            begin: 0,
            end: 64,
        }) {
            Ok(_) => panic!("changed PDF range was accepted"),
            Err(error) => error,
        };
        assert!(error.contains("changed on disk"));
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-document-io-test",
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
