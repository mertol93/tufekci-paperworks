use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::PublicKey;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::sync::Mutex;
use tauri::{ipc::Channel, AppHandle, State};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri_plugin_updater::{Update, UpdaterExt};
use url::Url;

const MAXIMUM_PUBLIC_KEY_BYTES: usize = 4_096;

#[derive(Clone)]
struct UpdateConfiguration {
    channel: String,
    endpoint: Url,
    public_key: String,
}

pub struct AppUpdateState {
    busy: AtomicBool,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    configuration: Option<UpdateConfiguration>,
    installed: AtomicBool,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pending: Mutex<Option<Update>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReadiness {
    configured: bool,
    channel: Option<String>,
    current_version: String,
    managed_by_store: bool,
    restart_required: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    channel: String,
    current_version: String,
    version: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum UpdateDownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn acquire_busy(state: &AppUpdateState) -> Result<BusyGuard<'_>, String> {
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("Another update operation is already in progress.".to_string());
    }
    Ok(BusyGuard(&state.busy))
}

fn public_key_is_bounded(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAXIMUM_PUBLIC_KEY_BYTES
        || value.contains(['\0', '\r', '\n'])
    {
        return false;
    }
    let Ok(decoded) = STANDARD.decode(value) else {
        return false;
    };
    if STANDARD.encode(&decoded) != value {
        return false;
    }
    let Ok(document) = std::str::from_utf8(&decoded) else {
        return false;
    };
    if document.contains(['\0', '\r']) {
        return false;
    }
    let lines = document.lines().collect::<Vec<_>>();
    lines.len() == 2
        && lines[0].starts_with("untrusted comment: ")
        && lines[0].len() <= 512
        && PublicKey::decode(document).is_ok()
}

fn configuration_from_values(
    channel: Option<&str>,
    endpoint: Option<&str>,
    public_key: Option<&str>,
) -> Result<Option<UpdateConfiguration>, String> {
    if channel.is_none() && endpoint.is_none() && public_key.is_none() {
        return Ok(None);
    }
    let channel = channel.ok_or_else(|| "The embedded updater channel is missing.".to_string())?;
    let endpoint =
        endpoint.ok_or_else(|| "The embedded updater endpoint is missing.".to_string())?;
    let public_key =
        public_key.ok_or_else(|| "The embedded updater public key is missing.".to_string())?;
    if !matches!(channel, "alpha" | "beta" | "stable") {
        return Err("The embedded updater channel is invalid.".to_string());
    }
    if !public_key_is_bounded(public_key) {
        return Err("The embedded updater public key is invalid.".to_string());
    }
    let endpoint = Url::parse(endpoint)
        .map_err(|_| "The embedded updater endpoint is invalid.".to_string())?;
    let expected_segment = format!("updates-{channel}");
    if endpoint.scheme() != "https"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
        || endpoint.host_str().is_none()
        || !endpoint.path_segments().is_some_and(|segments| {
            segments
                .into_iter()
                .any(|segment| segment == expected_segment)
        })
    {
        return Err("The embedded updater endpoint is not a safe channel URL.".to_string());
    }
    Ok(Some(UpdateConfiguration {
        channel: channel.to_string(),
        endpoint,
        public_key: public_key.to_string(),
    }))
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn initialise(app: &AppHandle) -> Result<AppUpdateState, String> {
    let configuration = configuration_from_values(
        option_env!("PAPERWORKS_UPDATE_CHANNEL"),
        option_env!("PAPERWORKS_UPDATE_ENDPOINT"),
        option_env!("PAPERWORKS_UPDATE_PUBLIC_KEY"),
    )?;
    if let Some(configuration) = &configuration {
        app.plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(configuration.public_key.clone())
                .build(),
        )
        .map_err(|_| "The signed updater could not be initialised.".to_string())?;
    }
    Ok(AppUpdateState {
        busy: AtomicBool::new(false),
        configuration,
        installed: AtomicBool::new(false),
        pending: Mutex::new(None),
    })
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub fn initialise(_app: &AppHandle) -> Result<AppUpdateState, String> {
    Ok(AppUpdateState {
        busy: AtomicBool::new(false),
        installed: AtomicBool::new(false),
    })
}

#[tauri::command]
pub fn update_readiness(app: AppHandle, state: State<'_, AppUpdateState>) -> UpdateReadiness {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let (configured, channel) = (
        state.configuration.is_some(),
        state
            .configuration
            .as_ref()
            .map(|configuration| configuration.channel.clone()),
    );
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let (configured, channel) = (false, None);

    UpdateReadiness {
        configured,
        channel,
        current_version: app.package_info().version.to_string(),
        managed_by_store: cfg!(target_os = "ios"),
        restart_required: state.installed.load(Ordering::SeqCst),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppUpdateState>,
) -> Result<Option<UpdateMetadata>, String> {
    let _busy = acquire_busy(&state)?;
    if state.installed.load(Ordering::SeqCst) {
        return Err("Restart the application to finish the installed update.".to_string());
    }
    let configuration = state.configuration.as_ref().ok_or_else(|| {
        "Signed updates are not configured in this application build.".to_string()
    })?;
    state
        .pending
        .lock()
        .map_err(|_| "The pending update state is unavailable.".to_string())?
        .take();

    let updater = app
        .updater_builder()
        .endpoints(vec![configuration.endpoint.clone()])
        .map_err(|_| "The update service configuration could not be applied.".to_string())?
        .build()
        .map_err(|_| "The update service could not be prepared.".to_string())?;
    let update = updater.check().await.map_err(|_| {
        "The update service could not be reached or its response was invalid.".to_string()
    })?;
    let metadata = update.as_ref().map(|update| UpdateMetadata {
        channel: configuration.channel.clone(),
        current_version: update.current_version.clone(),
        version: update.version.clone(),
    });
    *state
        .pending
        .lock()
        .map_err(|_| "The pending update state is unavailable.".to_string())? = update;
    Ok(metadata)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[tauri::command]
pub async fn check_for_update(
    _app: AppHandle,
    state: State<'_, AppUpdateState>,
) -> Result<Option<UpdateMetadata>, String> {
    let _busy = acquire_busy(&state)?;
    Err("Application updates are managed by the platform store on this device.".to_string())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[tauri::command]
pub async fn install_update(
    state: State<'_, AppUpdateState>,
    on_event: Channel<UpdateDownloadEvent>,
) -> Result<(), String> {
    let _busy = acquire_busy(&state)?;
    let update = state
        .pending
        .lock()
        .map_err(|_| "The pending update state is unavailable.".to_string())?
        .take()
        .ok_or_else(|| "Check for a signed update before installing it.".to_string())?;
    let mut started = false;
    update
        .download_and_install(
            |chunk_length, content_length| {
                if !started {
                    let _ = on_event.send(UpdateDownloadEvent::Started { content_length });
                    started = true;
                }
                let _ = on_event.send(UpdateDownloadEvent::Progress { chunk_length });
            },
            || {
                let _ = on_event.send(UpdateDownloadEvent::Finished);
            },
        )
        .await
        .map_err(|_| {
            "The signed update could not be downloaded, verified, or installed.".to_string()
        })?;
    state.installed.store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[tauri::command]
pub async fn install_update(
    state: State<'_, AppUpdateState>,
    _on_event: Channel<UpdateDownloadEvent>,
) -> Result<(), String> {
    let _busy = acquire_busy(&state)?;
    Err("Application updates are managed by the platform store on this device.".to_string())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[tauri::command]
pub fn restart_after_update(
    app: AppHandle,
    state: State<'_, AppUpdateState>,
) -> Result<(), String> {
    if !state.installed.load(Ordering::SeqCst) {
        return Err("No installed update is waiting for a restart.".to_string());
    }
    app.restart();
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[tauri::command]
pub fn restart_after_update(
    _app: AppHandle,
    _state: State<'_, AppUpdateState>,
) -> Result<(), String> {
    Err("Application updates are managed by the platform store on this device.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public_key() -> String {
        let mut key = [0_u8; 42];
        key[0] = 0x45;
        key[1] = 0x64;
        let document = format!(
            "untrusted comment: minisign public key\n{}",
            STANDARD.encode(key)
        );
        STANDARD.encode(document)
    }

    #[test]
    fn ordinary_development_builds_can_leave_updates_unconfigured() {
        assert!(configuration_from_values(None, None, None)
            .expect("unconfigured updater")
            .is_none());
    }

    #[test]
    fn updater_configuration_is_all_or_nothing() {
        let public_key = public_key();
        let error = configuration_from_values(Some("alpha"), None, Some(&public_key))
            .err()
            .expect("partial updater configuration must fail");
        assert!(error.contains("endpoint is missing"));
    }

    #[test]
    fn updater_configuration_requires_https_and_its_exact_channel() {
        let public_key = public_key();
        for endpoint in [
            "http://example.test/releases/updates-alpha/latest.json",
            "https://example.test/releases/updates-stable/latest.json",
            "https://user@example.test/releases/updates-alpha/latest.json",
        ] {
            assert!(
                configuration_from_values(Some("alpha"), Some(endpoint), Some(&public_key))
                    .is_err()
            );
        }
        let configuration = configuration_from_values(
            Some("alpha"),
            Some("https://example.test/releases/updates-alpha/latest.json"),
            Some(&public_key),
        )
        .expect("valid updater configuration")
        .expect("configured updater");
        assert_eq!(configuration.channel, "alpha");
        assert_eq!(configuration.endpoint.scheme(), "https");
    }

    #[test]
    fn updater_public_keys_are_bounded_and_never_private_configuration() {
        let public_key = public_key();
        assert!(public_key_is_bounded(&public_key));
        assert!(!public_key_is_bounded("RWshort"));
        assert!(!public_key_is_bounded(
            "untrusted comment: minisign public key\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        ));
        assert!(!public_key_is_bounded(
            &"A".repeat(MAXIMUM_PUBLIC_KEY_BYTES + 1)
        ));
        assert!(!public_key_is_bounded(&format!("{public_key}\r")));
    }
}
