use crate::runtime_capabilities::current_capabilities;
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Serialize)]
pub struct ToolStatus {
    name: &'static str,
    command: &'static str,
    available: bool,
    version: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPreset {
    id: &'static str,
    name: &'static str,
    width_mm: f32,
    height_mm: f32,
    description: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureCapabilities {
    accepted_image_formats: Vec<&'static str>,
    background_removal: bool,
    certificate_provider: &'static str,
    document_permission_lock: bool,
    editor_placement_lock: bool,
    flatten_on_export: bool,
}

#[tauri::command]
pub fn probe_tools() -> Vec<ToolStatus> {
    let external_processes = current_capabilities().external_processes();
    [
        ("QPDF", "qpdf", "--version"),
        ("ImageMagick", "magick", "--version"),
        ("img2pdf", "img2pdf", "--version"),
        ("OCRmyPDF", "ocrmypdf", "--version"),
        ("Tesseract", "tesseract", "--version"),
        ("pyHanko", "pyhanko", "--version"),
    ]
    .into_iter()
    .map(|(name, command, version_arg)| {
        if external_processes {
            probe_tool(name, command, version_arg)
        } else {
            ToolStatus {
                name,
                command,
                available: false,
                version: None,
                detail: Some(
                    "Desktop command-line engines are unavailable on this platform.".to_string(),
                ),
            }
        }
    })
    .collect()
}

#[tauri::command]
pub fn scan_presets() -> Vec<ScanPreset> {
    vec![
        ScanPreset {
            id: "a4",
            name: "A4",
            width_mm: 210.0,
            height_mm: 297.0,
            description: "Standard UK document page",
        },
        ScanPreset {
            id: "letter",
            name: "US Letter",
            width_mm: 216.0,
            height_mm: 279.0,
            description: "Common North American document page",
        },
        ScanPreset {
            id: "business-card",
            name: "Business card",
            width_mm: 85.0,
            height_mm: 55.0,
            description: "Compact card layout",
        },
        ScanPreset {
            id: "id-card",
            name: "ID card",
            width_mm: 85.6,
            height_mm: 54.0,
            description: "Credit-card sized identity document",
        },
        ScanPreset {
            id: "driving-licence",
            name: "Driving licence",
            width_mm: 85.6,
            height_mm: 54.0,
            description: "UK photocard driving licence size",
        },
    ]
}

#[tauri::command]
pub fn signature_capabilities() -> SignatureCapabilities {
    let capabilities = current_capabilities();
    SignatureCapabilities {
        accepted_image_formats: vec!["PNG", "JPEG", "WebP", "BMP", "TIFF"],
        background_removal: true,
        certificate_provider: if capabilities.certificate_signing() {
            "pyHanko"
        } else {
            "Unavailable"
        },
        document_permission_lock: capabilities.password_protection(),
        editor_placement_lock: true,
        flatten_on_export: true,
    }
}

fn probe_tool(name: &'static str, command: &'static str, version_arg: &'static str) -> ToolStatus {
    match Command::new(command).arg(version_arg).output() {
        Ok(output) if output.status.success() => ToolStatus {
            name,
            command,
            available: true,
            version: first_output_line(&output.stdout)
                .or_else(|| first_output_line(&output.stderr)),
            detail: None,
        },
        Ok(output) => ToolStatus {
            name,
            command,
            available: false,
            version: None,
            detail: first_output_line(&output.stderr)
                .or_else(|| first_output_line(&output.stdout))
                .or_else(|| Some("Installed command returned an error".to_string())),
        },
        Err(error) => ToolStatus {
            name,
            command,
            available: false,
            version: None,
            detail: Some(error.to_string()),
        },
    }
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);

    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}
