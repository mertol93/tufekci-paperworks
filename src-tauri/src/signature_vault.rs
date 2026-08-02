use crate::file_safety::reject_control_characters;
use aes_gcm::aead::{rand_core::RngCore, Aead, OsRng, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;
use zeroize::{Zeroize, Zeroizing};

const VAULT_MAGIC: &[u8; 8] = b"TPWSIG01";
const ENVELOPE_VERSION: u8 = 1;
const LEGACY_PAYLOAD_VERSION: u8 = 1;
const PAYLOAD_VERSION: u8 = 2;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const KEY_BYTES: usize = 32;
const GCM_TAG_BYTES: usize = 16;
const ENVELOPE_HEADER_BYTES: usize = 8 + 1 + 4 + 4 + 4 + SALT_BYTES + NONCE_BYTES + 4;
const KDF_MEMORY_KIB: u32 = 19 * 1024;
const KDF_ITERATIONS: u32 = 2;
const KDF_PARALLELISM: u32 = 1;
const MIN_KDF_MEMORY_KIB: u32 = 8 * 1024;
const MAX_KDF_MEMORY_KIB: u32 = 128 * 1024;
const MAX_KDF_ITERATIONS: u32 = 10;
const MAX_KDF_PARALLELISM: u32 = 4;
const MIN_PASSPHRASE_CHARS: usize = 12;
const MAX_PASSPHRASE_BYTES: usize = 1024;
const MAX_SIGNATURES: usize = 50;
const MAX_SIGNATURE_DATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_VAULT_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_SIGNATURE_DIMENSION: u32 = 8_192;
const MAX_SIGNATURE_ALLOCATION: u64 = 128 * 1024 * 1024;
const MAX_LABEL_BYTES: usize = 256;
const MAX_SOURCE_NAME_BYTES: usize = 1024;
const PASSPHRASE_REJECTED_MESSAGE: &str =
    "The passphrase is incorrect or the stored signature was altered.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureVaultErrorCode {
    CapacityReached,
    DeleteFailed,
    EntryInvalid,
    EntryUnavailable,
    LibraryUnavailable,
    PassphraseRejected,
    SaveFailed,
    UnlockFailed,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StoreSignatureRequest {
    label: String,
    kind: VisualMarkKind,
    method: VisualMarkMethod,
    source_name: String,
    width: u32,
    height: u32,
    png_data_url: String,
    passphrase: String,
    passphrase_confirmation: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UnlockSignatureRequest {
    id: String,
    passphrase: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeleteSignatureRequest {
    id: String,
    confirm: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureVaultEntry {
    id: String,
    stored_at_ms: u64,
    bytes_on_disk: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockedSignature {
    id: String,
    label: String,
    kind: VisualMarkKind,
    method: VisualMarkMethod,
    source_name: String,
    width: u32,
    height: u32,
    png_data_url: String,
    stored_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualMarkKind {
    Signature,
    Initials,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualMarkMethod {
    Draw,
    Image,
    Type,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSignatureResult {
    id: String,
    deleted: bool,
}

#[tauri::command]
pub fn list_signature_vault(
    app: tauri::AppHandle,
) -> Result<Vec<SignatureVaultEntry>, SignatureVaultErrorCode> {
    let root =
        signature_vault_root(&app).map_err(|_| SignatureVaultErrorCode::LibraryUnavailable)?;
    list_at_root(&root).map_err(|_| SignatureVaultErrorCode::LibraryUnavailable)
}

#[tauri::command]
pub async fn store_signature_vault(
    app: tauri::AppHandle,
    request: StoreSignatureRequest,
) -> Result<SignatureVaultEntry, SignatureVaultErrorCode> {
    let root =
        signature_vault_root(&app).map_err(|_| SignatureVaultErrorCode::LibraryUnavailable)?;
    tauri::async_runtime::spawn_blocking(move || store_at_root(&root, request))
        .await
        .map_err(|_| SignatureVaultErrorCode::SaveFailed)?
        .map_err(store_error_code)
}

#[tauri::command]
pub async fn unlock_signature_vault(
    app: tauri::AppHandle,
    request: UnlockSignatureRequest,
) -> Result<UnlockedSignature, SignatureVaultErrorCode> {
    let root =
        signature_vault_root(&app).map_err(|_| SignatureVaultErrorCode::LibraryUnavailable)?;
    tauri::async_runtime::spawn_blocking(move || unlock_at_root(&root, request))
        .await
        .map_err(|_| SignatureVaultErrorCode::UnlockFailed)?
        .map_err(unlock_error_code)
}

#[tauri::command]
pub fn delete_signature_vault(
    app: tauri::AppHandle,
    request: DeleteSignatureRequest,
) -> Result<DeleteSignatureResult, SignatureVaultErrorCode> {
    if !request.confirm {
        return Err(SignatureVaultErrorCode::DeleteFailed);
    }
    validate_id(&request.id).map_err(|_| SignatureVaultErrorCode::DeleteFailed)?;
    let root =
        signature_vault_root(&app).map_err(|_| SignatureVaultErrorCode::LibraryUnavailable)?;
    let path = vault_path(&root, &request.id);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SignatureVaultErrorCode::EntryUnavailable
        } else {
            SignatureVaultErrorCode::DeleteFailed
        }
    })?;
    if !metadata.file_type().is_file() {
        return Err(SignatureVaultErrorCode::EntryInvalid);
    }
    fs::remove_file(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SignatureVaultErrorCode::EntryUnavailable
        } else {
            SignatureVaultErrorCode::DeleteFailed
        }
    })?;
    sync_directory(&root);
    Ok(DeleteSignatureResult {
        id: request.id,
        deleted: true,
    })
}

fn store_error_code(error: String) -> SignatureVaultErrorCode {
    if error
        == format!("The local signature library may contain no more than {MAX_SIGNATURES} entries.")
    {
        SignatureVaultErrorCode::CapacityReached
    } else {
        SignatureVaultErrorCode::SaveFailed
    }
}

fn unlock_error_code(error: String) -> SignatureVaultErrorCode {
    if error == PASSPHRASE_REJECTED_MESSAGE {
        SignatureVaultErrorCode::PassphraseRejected
    } else if error.starts_with("The encrypted signature could not be found:") {
        SignatureVaultErrorCode::EntryUnavailable
    } else if error.starts_with("The encrypted signature could not be read:")
        || error == "The signature decryption key could not be prepared."
    {
        SignatureVaultErrorCode::UnlockFailed
    } else {
        SignatureVaultErrorCode::EntryInvalid
    }
}

fn signature_vault_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("The local signature library is unavailable: {error}"))?
        .join("signature-vault");
    ensure_private_directory(&root)?;
    Ok(root)
}

fn ensure_private_directory(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("The local signature library could not be created: {error}"))?;
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("The local signature library could not be inspected: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("The local signature library is not a private directory.".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!("The local signature library permissions could not be secured: {error}")
        })?;
    }
    Ok(())
}

fn list_at_root(root: &Path) -> Result<Vec<SignatureVaultEntry>, String> {
    ensure_private_directory(root)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|error| format!("The local signature library could not be read: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("A local signature entry could not be read: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("A local signature entry could not be inspected: {error}"))?;
        if !file_type.is_file() {
            continue;
        }
        let Some(id) = id_from_path(&entry.path()) else {
            continue;
        };
        let metadata = entry
            .metadata()
            .map_err(|error| format!("A local signature entry could not be inspected: {error}"))?;
        entries.push(SignatureVaultEntry {
            id,
            stored_at_ms: system_time_ms(metadata.modified().unwrap_or(UNIX_EPOCH)),
            bytes_on_disk: metadata.len(),
        });
        if entries.len() > MAX_SIGNATURES {
            return Err(format!(
                "The local signature library contains more than {MAX_SIGNATURES} entries."
            ));
        }
    }
    entries.sort_by(|left, right| {
        right
            .stored_at_ms
            .cmp(&left.stored_at_ms)
            .then(left.id.cmp(&right.id))
    });
    Ok(entries)
}

fn store_at_root(
    root: &Path,
    request: StoreSignatureRequest,
) -> Result<SignatureVaultEntry, String> {
    let StoreSignatureRequest {
        label,
        kind,
        method,
        source_name,
        width,
        height,
        png_data_url,
        passphrase,
        passphrase_confirmation,
    } = request;
    let passphrase = Zeroizing::new(passphrase);
    let passphrase_confirmation = Zeroizing::new(passphrase_confirmation);
    validate_passphrase(&passphrase)?;
    if passphrase.as_str() != passphrase_confirmation.as_str() {
        return Err("The signature-library passphrases do not match.".to_string());
    }
    validate_text("Signature name", &label, MAX_LABEL_BYTES)?;
    validate_text("Signature source name", &source_name, MAX_SOURCE_NAME_BYTES)?;
    let png_data_url = Zeroizing::new(png_data_url);
    let png = Zeroizing::new(decode_and_validate_png(&png_data_url, width, height)?);
    ensure_private_directory(root)?;
    if list_at_root(root)?.len() >= MAX_SIGNATURES {
        return Err(format!(
            "The local signature library may contain no more than {MAX_SIGNATURES} entries."
        ));
    }

    let stored_at_ms = timestamp_ms();
    let mut plaintext = Zeroizing::new(encode_payload(&SignaturePayload {
        label: &label,
        kind,
        method,
        source_name: &source_name,
        width,
        height,
        stored_at_ms,
        png: png.as_slice(),
    })?);
    let mut salt = [0_u8; SALT_BYTES];
    let mut nonce = [0_u8; NONCE_BYTES];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let mut key = Zeroizing::new([0_u8; KEY_BYTES]);
    derive_key(
        &passphrase,
        &salt,
        KDF_MEMORY_KIB,
        KDF_ITERATIONS,
        KDF_PARALLELISM,
        key.as_mut(),
    )?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| "The signature encryption key could not be prepared.".to_string())?;
    let ciphertext_length = plaintext
        .len()
        .checked_add(GCM_TAG_BYTES)
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| "The encrypted signature payload is too large.".to_string())?;
    let header = encode_envelope_header(
        KDF_MEMORY_KIB,
        KDF_ITERATIONS,
        KDF_PARALLELISM,
        &salt,
        &nonce,
        ciphertext_length,
    );
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_ref(),
                aad: &header,
            },
        )
        .map_err(|_| "The signature could not be encrypted.".to_string())?;
    plaintext.zeroize();
    if ciphertext.len() != ciphertext_length as usize {
        return Err("The encrypted signature payload has an unexpected size.".to_string());
    }

    let (id, path) = allocate_entry(root)?;
    let mut envelope = header;
    envelope.extend_from_slice(&ciphertext);
    write_private_file(&path, &envelope)?;
    sync_directory(root);
    Ok(SignatureVaultEntry {
        id,
        stored_at_ms,
        bytes_on_disk: envelope.len() as u64,
    })
}

fn unlock_at_root(
    root: &Path,
    request: UnlockSignatureRequest,
) -> Result<UnlockedSignature, String> {
    let UnlockSignatureRequest { id, passphrase } = request;
    validate_id(&id)?;
    let passphrase = Zeroizing::new(passphrase);
    validate_passphrase(&passphrase)?;
    let path = vault_path(root, &id);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("The encrypted signature could not be found: {error}"))?;
    if !metadata.file_type().is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_VAULT_FILE_BYTES
    {
        return Err("The encrypted signature entry is not a valid bounded file.".to_string());
    }
    let envelope = fs::read(&path)
        .map_err(|error| format!("The encrypted signature could not be read: {error}"))?;
    let parsed = parse_envelope(&envelope)?;
    let mut key = Zeroizing::new([0_u8; KEY_BYTES]);
    derive_key(
        &passphrase,
        &parsed.salt,
        parsed.memory_kib,
        parsed.iterations,
        parsed.parallelism,
        key.as_mut(),
    )?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| "The signature decryption key could not be prepared.".to_string())?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&parsed.nonce),
            Payload {
                msg: parsed.ciphertext,
                aad: parsed.header,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| PASSPHRASE_REJECTED_MESSAGE.to_string())?;
    let payload = parse_payload(&plaintext)?;
    validate_png_bytes(payload.png, payload.width, payload.height)?;
    let png_data_url = format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(payload.png)
    );
    Ok(UnlockedSignature {
        id,
        label: payload.label.to_string(),
        kind: payload.kind,
        method: payload.method,
        source_name: payload.source_name.to_string(),
        width: payload.width,
        height: payload.height,
        png_data_url,
        stored_at_ms: payload.stored_at_ms,
    })
}

fn validate_passphrase(passphrase: &str) -> Result<(), String> {
    reject_control_characters("Signature-library passphrase", passphrase)?;
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS || passphrase.len() > MAX_PASSPHRASE_BYTES
    {
        return Err(format!(
            "The signature-library passphrase must contain at least {MIN_PASSPHRASE_CHARS} characters and no more than {MAX_PASSPHRASE_BYTES} UTF-8 bytes."
        ));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, maximum_bytes: usize) -> Result<(), String> {
    reject_control_characters(label, value)?;
    if value.trim().is_empty() || value.len() > maximum_bytes {
        return Err(format!(
            "{label} must contain between 1 and {maximum_bytes} UTF-8 bytes."
        ));
    }
    Ok(())
}

fn decode_and_validate_png(data_url: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "Only a prepared transparent PNG signature can be stored.".to_string())?;
    if encoded.len() > MAX_SIGNATURE_DATA_BYTES * 2 {
        return Err("The prepared signature image is too large to store safely.".to_string());
    }
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| "The prepared signature image is not valid base64 data.".to_string())?;
    validate_png_bytes(&bytes, width, height)?;
    Ok(bytes)
}

fn validate_png_bytes(bytes: &[u8], width: u32, height: u32) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_SIGNATURE_DATA_BYTES {
        return Err(
            "The prepared signature image is empty or too large to store safely.".to_string(),
        );
    }
    let image = match catch_unwind(AssertUnwindSafe(|| {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_SIGNATURE_DIMENSION);
        limits.max_image_height = Some(MAX_SIGNATURE_DIMENSION);
        limits.max_alloc = Some(MAX_SIGNATURE_ALLOCATION);
        reader.limits(limits);
        reader.decode()
    })) {
        Ok(Ok(image)) => image,
        Ok(Err(error)) => {
            return Err(format!(
                "The prepared signature PNG could not be decoded: {error}"
            ))
        }
        Err(_) => return Err("The prepared signature PNG was rejected safely.".to_string()),
    };
    if image.dimensions() != (width, height) || width == 0 || height == 0 {
        return Err("The prepared signature dimensions do not match its PNG data.".to_string());
    }
    let rgba = image.to_rgba8();
    let has_ink = rgba.pixels().any(|pixel| pixel.0[3] > 0);
    let has_transparency = rgba.pixels().any(|pixel| pixel.0[3] < 255);
    if !has_ink || !has_transparency {
        return Err(
            "Store a prepared signature with visible ink and a transparent background.".to_string(),
        );
    }
    Ok(())
}

fn derive_key(
    passphrase: &str,
    salt: &[u8; SALT_BYTES],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    output: &mut [u8],
) -> Result<(), String> {
    validate_kdf_parameters(memory_kib, iterations, parallelism)?;
    let params = Params::new(memory_kib, iterations, parallelism, Some(KEY_BYTES))
        .map_err(|_| "The signature-library key settings are invalid.".to_string())?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, output)
        .map_err(|_| "The signature-library key could not be derived.".to_string())
}

fn validate_kdf_parameters(
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<(), String> {
    if !(MIN_KDF_MEMORY_KIB..=MAX_KDF_MEMORY_KIB).contains(&memory_kib)
        || !(1..=MAX_KDF_ITERATIONS).contains(&iterations)
        || !(1..=MAX_KDF_PARALLELISM).contains(&parallelism)
    {
        return Err("The encrypted signature uses unsafe key settings.".to_string());
    }
    Ok(())
}

fn encode_payload(payload: &SignaturePayload<'_>) -> Result<Vec<u8>, String> {
    let label_length = u16::try_from(payload.label.len())
        .map_err(|_| "The signature name is too long.".to_string())?;
    let source_length = u16::try_from(payload.source_name.len())
        .map_err(|_| "The signature source name is too long.".to_string())?;
    let png_length = u32::try_from(payload.png.len())
        .map_err(|_| "The signature PNG is too large.".to_string())?;
    let mut bytes = Vec::with_capacity(
        27 + payload.label.len() + payload.source_name.len() + payload.png.len(),
    );
    bytes.push(PAYLOAD_VERSION);
    bytes.push(visual_mark_kind_code(payload.kind));
    bytes.push(visual_mark_method_code(payload.method));
    bytes.extend_from_slice(&payload.stored_at_ms.to_be_bytes());
    bytes.extend_from_slice(&payload.width.to_be_bytes());
    bytes.extend_from_slice(&payload.height.to_be_bytes());
    bytes.extend_from_slice(&label_length.to_be_bytes());
    bytes.extend_from_slice(&source_length.to_be_bytes());
    bytes.extend_from_slice(&png_length.to_be_bytes());
    bytes.extend_from_slice(payload.label.as_bytes());
    bytes.extend_from_slice(payload.source_name.as_bytes());
    bytes.extend_from_slice(payload.png);
    Ok(bytes)
}

fn encode_envelope_header(
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: &[u8; SALT_BYTES],
    nonce: &[u8; NONCE_BYTES],
    ciphertext_length: u32,
) -> Vec<u8> {
    let mut header = Vec::with_capacity(ENVELOPE_HEADER_BYTES);
    header.extend_from_slice(VAULT_MAGIC);
    header.push(ENVELOPE_VERSION);
    header.extend_from_slice(&memory_kib.to_be_bytes());
    header.extend_from_slice(&iterations.to_be_bytes());
    header.extend_from_slice(&parallelism.to_be_bytes());
    header.extend_from_slice(salt);
    header.extend_from_slice(nonce);
    header.extend_from_slice(&ciphertext_length.to_be_bytes());
    header
}

struct ParsedEnvelope<'a> {
    header: &'a [u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: [u8; SALT_BYTES],
    nonce: [u8; NONCE_BYTES],
    ciphertext: &'a [u8],
}

fn parse_envelope(bytes: &[u8]) -> Result<ParsedEnvelope<'_>, String> {
    if bytes.len() < ENVELOPE_HEADER_BYTES + GCM_TAG_BYTES
        || bytes.len() as u64 > MAX_VAULT_FILE_BYTES
        || &bytes[..VAULT_MAGIC.len()] != VAULT_MAGIC
        || bytes[VAULT_MAGIC.len()] != ENVELOPE_VERSION
    {
        return Err("The encrypted signature file format is invalid.".to_string());
    }
    let mut offset = VAULT_MAGIC.len() + 1;
    let memory_kib = read_u32(bytes, &mut offset)?;
    let iterations = read_u32(bytes, &mut offset)?;
    let parallelism = read_u32(bytes, &mut offset)?;
    validate_kdf_parameters(memory_kib, iterations, parallelism)?;
    let salt = read_array::<SALT_BYTES>(bytes, &mut offset)?;
    let nonce = read_array::<NONCE_BYTES>(bytes, &mut offset)?;
    let ciphertext_length = read_u32(bytes, &mut offset)? as usize;
    if offset != ENVELOPE_HEADER_BYTES
        || ciphertext_length < GCM_TAG_BYTES
        || bytes.len() != offset.saturating_add(ciphertext_length)
    {
        return Err("The encrypted signature file length is invalid.".to_string());
    }
    Ok(ParsedEnvelope {
        header: &bytes[..offset],
        memory_kib,
        iterations,
        parallelism,
        salt,
        nonce,
        ciphertext: &bytes[offset..],
    })
}

#[derive(Debug)]
struct SignaturePayload<'a> {
    label: &'a str,
    kind: VisualMarkKind,
    method: VisualMarkMethod,
    source_name: &'a str,
    width: u32,
    height: u32,
    stored_at_ms: u64,
    png: &'a [u8],
}

fn parse_payload(bytes: &[u8]) -> Result<SignaturePayload<'_>, String> {
    let version = bytes
        .first()
        .copied()
        .ok_or_else(|| "The decrypted signature payload version is unsupported.".to_string())?;
    let mut offset = 1;
    let (kind, method) = match version {
        LEGACY_PAYLOAD_VERSION => (VisualMarkKind::Signature, VisualMarkMethod::Image),
        PAYLOAD_VERSION => (
            visual_mark_kind_from_code(read_u8(bytes, &mut offset)?)?,
            visual_mark_method_from_code(read_u8(bytes, &mut offset)?)?,
        ),
        _ => return Err("The decrypted signature payload version is unsupported.".to_string()),
    };
    let stored_at_ms = read_u64(bytes, &mut offset)?;
    let width = read_u32(bytes, &mut offset)?;
    let height = read_u32(bytes, &mut offset)?;
    let label_length = read_u16(bytes, &mut offset)? as usize;
    let source_length = read_u16(bytes, &mut offset)? as usize;
    let png_length = read_u32(bytes, &mut offset)? as usize;
    if label_length == 0
        || label_length > MAX_LABEL_BYTES
        || source_length == 0
        || source_length > MAX_SOURCE_NAME_BYTES
        || png_length == 0
        || png_length > MAX_SIGNATURE_DATA_BYTES
    {
        return Err("The decrypted signature payload has invalid lengths.".to_string());
    }
    let label_bytes = take(bytes, &mut offset, label_length)?;
    let source_bytes = take(bytes, &mut offset, source_length)?;
    let png = take(bytes, &mut offset, png_length)?;
    if offset != bytes.len() {
        return Err("The decrypted signature payload contains trailing data.".to_string());
    }
    let label = std::str::from_utf8(label_bytes)
        .map_err(|_| "The decrypted signature name is not valid UTF-8.".to_string())?;
    let source_name = std::str::from_utf8(source_bytes)
        .map_err(|_| "The decrypted signature source name is not valid UTF-8.".to_string())?;
    validate_text("Signature name", label, MAX_LABEL_BYTES)?;
    validate_text("Signature source name", source_name, MAX_SOURCE_NAME_BYTES)?;
    Ok(SignaturePayload {
        label,
        kind,
        method,
        source_name,
        width,
        height,
        stored_at_ms,
        png,
    })
}

fn read_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, String> {
    Ok(read_array::<1>(bytes, offset)?[0])
}

fn visual_mark_kind_code(kind: VisualMarkKind) -> u8 {
    match kind {
        VisualMarkKind::Signature => 1,
        VisualMarkKind::Initials => 2,
    }
}

fn visual_mark_kind_from_code(code: u8) -> Result<VisualMarkKind, String> {
    match code {
        1 => Ok(VisualMarkKind::Signature),
        2 => Ok(VisualMarkKind::Initials),
        _ => Err("The decrypted signature payload has an invalid mark type.".to_string()),
    }
}

fn visual_mark_method_code(method: VisualMarkMethod) -> u8 {
    match method {
        VisualMarkMethod::Draw => 1,
        VisualMarkMethod::Image => 2,
        VisualMarkMethod::Type => 3,
    }
}

fn visual_mark_method_from_code(code: u8) -> Result<VisualMarkMethod, String> {
    match code {
        1 => Ok(VisualMarkMethod::Draw),
        2 => Ok(VisualMarkMethod::Image),
        3 => Ok(VisualMarkMethod::Type),
        _ => Err("The decrypted signature payload has an invalid creation method.".to_string()),
    }
}

fn read_u16(bytes: &[u8], offset: &mut usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(read_array(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, String> {
    Ok(u64::from_be_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], String> {
    let slice = take(bytes, offset, N)?;
    slice
        .try_into()
        .map_err(|_| "The encrypted signature file is incomplete.".to_string())
}

fn take<'a>(bytes: &'a [u8], offset: &mut usize, length: usize) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| "The encrypted signature file is incomplete.".to_string())?;
    let slice = &bytes[*offset..end];
    *offset = end;
    Ok(slice)
}

fn allocate_entry(root: &Path) -> Result<(String, PathBuf), String> {
    for _ in 0..8 {
        let mut random = [0_u8; 16];
        OsRng.fill_bytes(&mut random);
        let id = encode_hex(&random);
        let path = vault_path(root, &id);
        if !path.exists() {
            return Ok((id, path));
        }
    }
    Err("A unique encrypted signature identifier could not be allocated.".to_string())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("The encrypted signature file could not be created: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!(
            "The encrypted signature file could not be written safely: {error}"
        ));
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err("The encrypted signature identifier is invalid.".to_string())
    }
}

fn id_from_path(path: &Path) -> Option<String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("tpsig") {
        return None;
    }
    let id = path.file_stem()?.to_str()?;
    validate_id(id).ok()?;
    Some(id.to_string())
}

fn vault_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.tpsig"))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn timestamp_ms() -> u64 {
    system_time_ms(SystemTime::now())
}

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(unix)]
fn sync_directory(path: &Path) {
    let _ = File::open(path).and_then(|directory| directory.sync_all());
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba, RgbaImage};

    #[test]
    fn encrypted_signature_round_trips_and_rejects_wrong_or_tampered_data() {
        let directory = TestDirectory::new();
        let request = sample_request();
        let original_data_url = request.png_data_url.clone();
        let stored = store_at_root(&directory.path, request).unwrap();
        assert_eq!(list_at_root(&directory.path).unwrap().len(), 1);
        assert_eq!(stored.id.len(), 32);
        let unlocked = unlock_at_root(
            &directory.path,
            UnlockSignatureRequest {
                id: stored.id.clone(),
                passphrase: "correct horse battery staple".to_string(),
            },
        )
        .unwrap();
        assert_eq!(unlocked.label, "Main signature");
        assert_eq!(unlocked.kind, VisualMarkKind::Signature);
        assert_eq!(unlocked.method, VisualMarkMethod::Image);
        assert_eq!(unlocked.width, 12);
        assert_eq!(unlocked.height, 6);
        assert_eq!(unlocked.png_data_url, original_data_url);

        let wrong_error = unlock_at_root(
            &directory.path,
            UnlockSignatureRequest {
                id: stored.id.clone(),
                passphrase: "incorrect horse battery".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(
            wrong_error,
            "The passphrase is incorrect or the stored signature was altered."
        );

        let path = vault_path(&directory.path, &stored.id);
        let mut envelope = fs::read(&path).unwrap();
        let last = envelope.last_mut().unwrap();
        *last ^= 0x40;
        fs::write(&path, envelope).unwrap();
        let tamper_error = unlock_at_root(
            &directory.path,
            UnlockSignatureRequest {
                id: stored.id,
                passphrase: "correct horse battery staple".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(tamper_error, wrong_error);
    }

    #[test]
    fn validates_identifiers_passphrases_and_delete_confirmation() {
        assert!(validate_id("0123456789abcdef0123456789abcdef").is_ok());
        assert!(validate_id("../signature").is_err());
        assert!(validate_passphrase("short").is_err());
        assert!(validate_passphrase("a suitable local passphrase").is_ok());
    }

    #[test]
    fn command_errors_are_stable_codes_without_native_details() {
        assert_eq!(
            serde_json::to_string(&SignatureVaultErrorCode::PassphraseRejected).unwrap(),
            "\"passphrase-rejected\""
        );
        assert_eq!(
            unlock_error_code(PASSPHRASE_REJECTED_MESSAGE.to_string()),
            SignatureVaultErrorCode::PassphraseRejected
        );
        assert_eq!(
            unlock_error_code(
                "The encrypted signature could not be found: private path detail".to_string()
            ),
            SignatureVaultErrorCode::EntryUnavailable
        );
        assert_eq!(
            unlock_error_code("The encrypted signature file format is invalid.".to_string()),
            SignatureVaultErrorCode::EntryInvalid
        );
        assert_eq!(
            store_error_code(format!(
                "The local signature library may contain no more than {MAX_SIGNATURES} entries."
            )),
            SignatureVaultErrorCode::CapacityReached
        );
        assert_eq!(
            store_error_code("private native signature storage failure".to_string()),
            SignatureVaultErrorCode::SaveFailed
        );
    }

    #[test]
    fn legacy_payloads_unlock_as_image_signatures_and_new_metadata_is_strict() {
        let label = b"Legacy signature";
        let source = b"legacy.png";
        let png = b"bounded-png-placeholder";
        let mut legacy = Vec::new();
        legacy.push(LEGACY_PAYLOAD_VERSION);
        legacy.extend_from_slice(&123_u64.to_be_bytes());
        legacy.extend_from_slice(&12_u32.to_be_bytes());
        legacy.extend_from_slice(&6_u32.to_be_bytes());
        legacy.extend_from_slice(&(label.len() as u16).to_be_bytes());
        legacy.extend_from_slice(&(source.len() as u16).to_be_bytes());
        legacy.extend_from_slice(&(png.len() as u32).to_be_bytes());
        legacy.extend_from_slice(label);
        legacy.extend_from_slice(source);
        legacy.extend_from_slice(png);

        let parsed = parse_payload(&legacy).unwrap();
        assert_eq!(parsed.kind, VisualMarkKind::Signature);
        assert_eq!(parsed.method, VisualMarkMethod::Image);
        assert_eq!(parsed.label, "Legacy signature");

        let mut invalid = encode_payload(&SignaturePayload {
            label: "Initials",
            kind: VisualMarkKind::Initials,
            method: VisualMarkMethod::Type,
            source_name: "typed-initials.png",
            width: 12,
            height: 6,
            stored_at_ms: 123,
            png,
        })
        .unwrap();
        invalid[2] = 99;
        assert!(parse_payload(&invalid)
            .unwrap_err()
            .contains("invalid creation method"));
    }

    fn sample_request() -> StoreSignatureRequest {
        let image = RgbaImage::from_fn(12, 6, |x, y| {
            if y == 3 && (2..10).contains(&x) {
                Rgba([20, 30, 45, 255])
            } else {
                Rgba([255, 255, 255, 0])
            }
        });
        let mut cursor = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut cursor, ImageFormat::Png)
            .unwrap();
        StoreSignatureRequest {
            label: "Main signature".to_string(),
            kind: VisualMarkKind::Signature,
            method: VisualMarkMethod::Image,
            source_name: "signature.png".to_string(),
            width: 12,
            height: 6,
            png_data_url: format!(
                "data:image/png;base64,{}",
                BASE64_STANDARD.encode(cursor.into_inner())
            ),
            passphrase: "correct horse battery staple".to_string(),
            passphrase_confirmation: "correct horse battery staple".to_string(),
        }
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = crate::test_support::create_unique_test_directory(
                "tufekci-paperworks-signature-vault-test",
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
