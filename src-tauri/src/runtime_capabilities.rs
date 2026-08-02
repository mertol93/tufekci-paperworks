use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RuntimePlatform {
    Windows,
    Macos,
    Linux,
    Ios,
    Android,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeCapabilities {
    platform: RuntimePlatform,
    mobile: bool,
    native_file_dialogs: bool,
    local_pdf_editing: bool,
    local_visual_marks: bool,
    image_to_pdf: bool,
    external_processes: bool,
    connected_scanning: bool,
    camera_capture: bool,
    searchable_ocr: bool,
    certificate_signing: bool,
    archival_pdf: bool,
    password_protection: bool,
    direct_updates: bool,
    app_store_updates: bool,
}

impl RuntimeCapabilities {
    pub(crate) fn external_processes(self) -> bool {
        self.external_processes
    }

    pub(crate) fn connected_scanning(self) -> bool {
        self.connected_scanning
    }

    pub(crate) fn searchable_ocr(self) -> bool {
        self.searchable_ocr
    }

    pub(crate) fn certificate_signing(self) -> bool {
        self.certificate_signing
    }

    pub(crate) fn archival_pdf(self) -> bool {
        self.archival_pdf
    }

    pub(crate) fn password_protection(self) -> bool {
        self.password_protection
    }
}

pub(crate) const fn capabilities_for(platform: RuntimePlatform) -> RuntimeCapabilities {
    let mobile = matches!(platform, RuntimePlatform::Ios | RuntimePlatform::Android);
    let desktop = matches!(
        platform,
        RuntimePlatform::Windows | RuntimePlatform::Macos | RuntimePlatform::Linux
    );
    RuntimeCapabilities {
        platform,
        mobile,
        native_file_dialogs: !matches!(platform, RuntimePlatform::Other),
        local_pdf_editing: !matches!(platform, RuntimePlatform::Other),
        local_visual_marks: !matches!(platform, RuntimePlatform::Other),
        image_to_pdf: !matches!(platform, RuntimePlatform::Other),
        external_processes: desktop,
        connected_scanning: desktop,
        camera_capture: false,
        searchable_ocr: desktop,
        certificate_signing: desktop,
        archival_pdf: desktop,
        password_protection: desktop,
        direct_updates: desktop,
        app_store_updates: matches!(platform, RuntimePlatform::Ios),
    }
}

pub(crate) const fn current_platform() -> RuntimePlatform {
    if cfg!(target_os = "windows") {
        RuntimePlatform::Windows
    } else if cfg!(target_os = "macos") {
        RuntimePlatform::Macos
    } else if cfg!(target_os = "linux") {
        RuntimePlatform::Linux
    } else if cfg!(target_os = "ios") {
        RuntimePlatform::Ios
    } else if cfg!(target_os = "android") {
        RuntimePlatform::Android
    } else {
        RuntimePlatform::Other
    }
}

pub(crate) const fn current_capabilities() -> RuntimeCapabilities {
    capabilities_for(current_platform())
}

#[tauri::command]
pub(crate) fn runtime_capabilities() -> RuntimeCapabilities {
    current_capabilities()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_targets_expose_subprocess_and_scanner_capabilities() {
        for platform in [
            RuntimePlatform::Windows,
            RuntimePlatform::Macos,
            RuntimePlatform::Linux,
        ] {
            let capabilities = capabilities_for(platform);
            assert!(!capabilities.mobile);
            assert!(capabilities.local_pdf_editing);
            assert!(capabilities.external_processes);
            assert!(capabilities.connected_scanning);
            assert!(capabilities.searchable_ocr);
            assert!(capabilities.certificate_signing);
            assert!(capabilities.direct_updates);
            assert!(!capabilities.app_store_updates);
        }
    }

    #[test]
    fn ios_keeps_the_local_pdf_core_without_desktop_process_claims() {
        let capabilities = capabilities_for(RuntimePlatform::Ios);
        assert!(capabilities.mobile);
        assert!(capabilities.native_file_dialogs);
        assert!(capabilities.local_pdf_editing);
        assert!(capabilities.local_visual_marks);
        assert!(capabilities.image_to_pdf);
        assert!(!capabilities.external_processes);
        assert!(!capabilities.connected_scanning);
        assert!(!capabilities.camera_capture);
        assert!(!capabilities.searchable_ocr);
        assert!(!capabilities.certificate_signing);
        assert!(!capabilities.archival_pdf);
        assert!(!capabilities.password_protection);
        assert!(!capabilities.direct_updates);
        assert!(capabilities.app_store_updates);
    }

    #[test]
    fn android_is_not_accidentally_reported_as_an_apple_store_build() {
        let capabilities = capabilities_for(RuntimePlatform::Android);
        assert!(capabilities.mobile);
        assert!(capabilities.local_pdf_editing);
        assert!(!capabilities.external_processes);
        assert!(!capabilities.connected_scanning);
        assert!(!capabilities.app_store_updates);
    }
}
