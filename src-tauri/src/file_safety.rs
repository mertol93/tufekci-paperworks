use crate::temporary_cleanup::{register_temporary_path, TemporaryKind, TemporaryLease};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct ValidatedPdfPaths {
    pub(crate) input: PathBuf,
    pub(crate) output: PathBuf,
}

impl ValidatedPdfPaths {
    pub(crate) fn new(input: &str, output: &str) -> Result<Self, String> {
        reject_control_characters("Input path", input)?;
        let input = canonical_pdf_input(input)?;
        let output = validated_new_pdf_output(output)?;

        if paths_are_equal(&input, &output) {
            return Err("The source PDF cannot be overwritten. Choose a new filename.".to_string());
        }

        Ok(Self { input, output })
    }
}

pub(crate) struct TemporaryOutput {
    lease: TemporaryLease,
}

impl TemporaryOutput {
    pub(crate) fn new(destination: &Path) -> Result<Self, String> {
        let parent = destination
            .parent()
            .ok_or_else(|| "The destination folder is invalid.".to_string())?;
        let file_name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("output.pdf");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("The system clock is invalid: {error}"))?
            .as_nanos();
        let path = parent.join(format!(
            ".{file_name}.{}.{}.paperworks.tmp",
            std::process::id(),
            nonce
        ));

        let lease = register_temporary_path(&path, TemporaryKind::OutputFile)?;
        Ok(Self { lease })
    }

    pub(crate) fn path(&self) -> &Path {
        self.lease.path()
    }

    pub(crate) fn persist(&self, destination: &Path) -> Result<u64, String> {
        let metadata = fs::metadata(self.path())
            .map_err(|error| format!("The output PDF was not created: {error}"))?;
        if metadata.len() == 0 {
            return Err("The output PDF was empty and has not been saved.".to_string());
        }

        match fs::hard_link(self.path(), destination) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err("The destination already exists. Choose a new filename.".to_string());
            }
            Err(_) => copy_without_overwriting(self.path(), destination)?,
        }

        sync_parent_directory(destination);
        Ok(metadata.len())
    }
}

pub(crate) fn canonical_pdf_input(path: &str) -> Result<PathBuf, String> {
    reject_control_characters("Input path", path)?;
    let input = fs::canonicalize(path)
        .map_err(|error| format!("The source PDF could not be opened: {error}"))?;
    let metadata = fs::metadata(&input)
        .map_err(|error| format!("The source PDF could not be inspected: {error}"))?;

    if !metadata.is_file() || !has_pdf_extension(&input) {
        return Err("Choose an existing PDF file as the source.".to_string());
    }

    Ok(input)
}

pub(crate) fn validated_new_pdf_output(output: &str) -> Result<PathBuf, String> {
    reject_control_characters("Output path", output)?;
    let requested_output = PathBuf::from(output);
    if !has_pdf_extension(&requested_output) {
        return Err("The destination filename must end in .pdf.".to_string());
    }
    if requested_output.exists() {
        return Err("The destination already exists. Choose a new filename.".to_string());
    }

    let file_name = requested_output
        .file_name()
        .ok_or_else(|| "Choose a destination filename.".to_string())?;
    let parent = requested_output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| format!("The destination folder could not be opened: {error}"))?;
    Ok(canonical_parent.join(file_name))
}

pub(crate) fn reject_control_characters(label: &str, value: &str) -> Result<(), String> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(format!(
            "{label} cannot contain line breaks or null characters."
        ));
    }
    Ok(())
}

fn copy_without_overwriting(source: &Path, destination: &Path) -> Result<(), String> {
    let mut source_file = File::open(source)
        .map_err(|error| format!("The temporary PDF could not be read: {error}"))?;
    let mut destination_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                "The destination already exists. Choose a new filename.".to_string()
            } else {
                format!("The destination PDF could not be created: {error}")
            }
        })?;

    if let Err(error) =
        io::copy(&mut source_file, &mut destination_file).and_then(|_| destination_file.sync_all())
    {
        drop(destination_file);
        let _ = fs::remove_file(destination);
        return Err(format!(
            "The destination PDF could not be completed: {error}"
        ));
    }

    Ok(())
}

pub(crate) fn publish_prepared_file(source: &Path, destination: &Path) -> Result<u64, String> {
    let metadata = fs::metadata(source)
        .map_err(|error| format!("The prepared PDF could not be inspected: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("The prepared PDF was empty and has not been published.".to_string());
    }
    match fs::hard_link(source, destination) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err("The destination already exists. Choose a new filename.".to_string());
        }
        Err(_) => copy_without_overwriting(source, destination)?,
    }
    sync_parent_directory(destination);
    Ok(metadata.len())
}

#[cfg(unix)]
fn sync_parent_directory(destination: &Path) {
    if let Some(parent) = destination.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_parent_directory(_destination: &Path) {}

fn has_pdf_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

pub(crate) fn paths_are_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_control_characters_in_paths() {
        let error = reject_control_characters("Path", "one.pdf\ntwo.pdf").unwrap_err();
        assert!(error.contains("line breaks"));
    }

    #[test]
    fn compares_windows_paths_without_case() {
        if cfg!(windows) {
            assert!(paths_are_equal(
                Path::new(r"C:\Documents\SOURCE.PDF"),
                Path::new(r"c:\documents\source.pdf")
            ));
        }
    }
}
