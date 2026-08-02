import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  CheckCircle2,
  Eye,
  EyeOff,
  KeyRound,
  Loader2,
  LockKeyhole,
  RefreshCw,
  Save,
  Trash2
} from "lucide-react";
import {
  createVisualSignatureAsset,
  createVisualSignatureId,
  type VisualMarkKind,
  type VisualMarkMethod,
  type VisualSignatureAsset
} from "./visualSignatures";
import { useI18n } from "./I18nProvider";
import {
  MAX_SIGNATURE_VAULT_PASSPHRASE_BYTES,
  classifySignatureVaultError,
  localiseSignatureVaultError,
  localiseSignatureVaultStatus,
  signatureVaultPassphraseIsValid,
  type SignatureVaultErrorCode,
  type SignatureVaultStatus
} from "./signatureVaultOutcomes";

type SignatureVaultEntry = {
  bytesOnDisk: number;
  id: string;
  storedAtMs: number;
};

type UnlockedSignature = {
  height: number;
  id: string;
  kind: VisualMarkKind;
  label: string;
  method: VisualMarkMethod;
  pngDataUrl: string;
  sourceName: string;
  storedAtMs: number;
  width: number;
};

type DeleteSignatureResult = {
  deleted: boolean;
  id: string;
};

type SignatureVaultProps = {
  asset: VisualSignatureAsset | null;
  desktopMode: boolean;
  onAssetLoad: (asset: VisualSignatureAsset) => void;
};

export function SignatureVault({ asset, desktopMode, onAssetLoad }: SignatureVaultProps) {
  const { formatDate, formatNumber, t } = useI18n();
  const [entries, setEntries] = useState<SignatureVaultEntry[]>([]);
  const [knownLabels, setKnownLabels] = useState<Record<string, string>>({});
  const [libraryName, setLibraryName] = useState("");
  const [savePassphrase, setSavePassphrase] = useState("");
  const [saveConfirmation, setSaveConfirmation] = useState("");
  const [unlockId, setUnlockId] = useState<string | null>(null);
  const [unlockPassphrase, setUnlockPassphrase] = useState("");
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [showPassphrases, setShowPassphrases] = useState(false);
  const [busy, setBusy] = useState<"deleting" | "loading" | "saving" | "unlocking" | null>(null);
  const [error, setError] = useState<SignatureVaultErrorCode | null>(null);
  const [status, setStatus] = useState<SignatureVaultStatus | null>(null);

  const refresh = useCallback(async () => {
    if (!desktopMode) {
      setEntries([]);
      return;
    }
    setBusy("loading");
    setError(null);
    setStatus(null);
    try {
      setEntries(await invoke<SignatureVaultEntry[]>("list_signature_vault"));
    } catch (reason) {
      setError(classifySignatureVaultError(reason, "list"));
    } finally {
      setBusy(null);
    }
  }, [desktopMode]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (asset) {
      setLibraryName(asset.name);
    }
  }, [asset]);

  const savePassphraseValid = useMemo(
    () =>
      signatureVaultPassphraseIsValid(savePassphrase) &&
      savePassphrase === saveConfirmation &&
      Boolean(libraryName.trim()),
    [libraryName, saveConfirmation, savePassphrase]
  );

  const storeSignature = async () => {
    if (!desktopMode || !asset || !savePassphraseValid || busy) {
      return;
    }
    setBusy("saving");
    setError(null);
    setStatus(null);
    try {
      const stored = await invoke<SignatureVaultEntry>("store_signature_vault", {
        request: {
          height: asset.height,
          kind: asset.kind,
          label: libraryName.trim(),
          method: asset.method,
          passphrase: savePassphrase,
          passphraseConfirmation: saveConfirmation,
          pngDataUrl: asset.dataUrl,
          sourceName: asset.sourceName,
          width: asset.width
        }
      });
      setKnownLabels((current) => ({ ...current, [stored.id]: libraryName.trim() }));
      setEntries((current) => [stored, ...current.filter((entry) => entry.id !== stored.id)]);
      setSavePassphrase("");
      setSaveConfirmation("");
      setStatus({ code: "saved" });
    } catch (reason) {
      setError(classifySignatureVaultError(reason, "save"));
    } finally {
      setBusy(null);
    }
  };

  const unlockSignature = async (id: string) => {
    if (!desktopMode || !signatureVaultPassphraseIsValid(unlockPassphrase) || busy) {
      return;
    }
    setBusy("unlocking");
    setError(null);
    setStatus(null);
    try {
      const unlocked = await invoke<UnlockedSignature>("unlock_signature_vault", {
        request: { id, passphrase: unlockPassphrase }
      });
      setKnownLabels((current) => ({ ...current, [id]: unlocked.label }));
      onAssetLoad(
        createVisualSignatureAsset(
          createVisualSignatureId("asset"),
          unlocked.label,
          unlocked.kind,
          unlocked.method,
          {
            dataUrl: unlocked.pngDataUrl,
            height: unlocked.height,
            sourceName: unlocked.sourceName,
            width: unlocked.width
          }
        )
      );
      setUnlockId(null);
      setUnlockPassphrase("");
      setStatus({ code: "unlocked", name: unlocked.label });
    } catch (reason) {
      setError(classifySignatureVaultError(reason, "unlock"));
    } finally {
      setBusy(null);
    }
  };

  const deleteSignature = async (id: string) => {
    if (!desktopMode || busy) {
      return;
    }
    setBusy("deleting");
    setError(null);
    setStatus(null);
    try {
      const result = await invoke<DeleteSignatureResult>("delete_signature_vault", {
        request: { confirm: true, id }
      });
      if (result.deleted) {
        setEntries((current) => current.filter((entry) => entry.id !== result.id));
        setKnownLabels((current) => {
          const updated = { ...current };
          delete updated[result.id];
          return updated;
        });
        setDeleteId(null);
        setStatus({ code: "deleted" });
      }
    } catch (reason) {
      setError(classifySignatureVaultError(reason, "delete"));
    } finally {
      setBusy(null);
    }
  };

  return (
    <details className="signature-vault">
      <summary>
        <span className="signature-vault-summary-icon" aria-hidden="true">
          <LockKeyhole size={17} />
        </span>
        <span>
          <strong>{t("signature.vault.heading")}</strong>
          <small>{t("signature.vault.savedCount", { count: entries.length })}</small>
        </span>
      </summary>

      <div className="signature-vault-body">
        {!desktopMode ? (
          <div className="signature-vault-message is-info">
            <LockKeyhole size={16} aria-hidden="true" />
            <span>{t("signature.vault.desktopOnly")}</span>
          </div>
        ) : (
          <>
            {asset ? (
              <div className="signature-vault-save">
                <div className="signature-vault-section-heading">
                  <strong>{t("signature.vault.saveSelected")}</strong>
                  <small>{t("signature.vault.saveSelectedHelp")}</small>
                </div>
                <label>
                  {t("signature.vault.name")}
                  <input
                    maxLength={80}
                    onChange={(event) => {
                      setLibraryName(event.target.value);
                      setError(null);
                      setStatus(null);
                    }}
                    value={libraryName}
                  />
                </label>
                <VaultPasswordField
                  label={t("signature.vault.libraryPassphrase")}
                  onChange={(value) => {
                    setSavePassphrase(value);
                    setError(null);
                    setStatus(null);
                  }}
                  show={showPassphrases}
                  value={savePassphrase}
                />
                <VaultPasswordField
                  label={t("signature.vault.confirmPassphrase")}
                  onChange={(value) => {
                    setSaveConfirmation(value);
                    setError(null);
                    setStatus(null);
                  }}
                  show={showPassphrases}
                  value={saveConfirmation}
                />
                {(savePassphrase || saveConfirmation) && !savePassphraseValid ? (
                  <small className="signature-vault-validation">
                    {t("signature.vault.validation")}
                  </small>
                ) : null}
                <button
                  className="wide-button"
                  disabled={!savePassphraseValid || Boolean(busy)}
                  onClick={() => void storeSignature()}
                  type="button"
                >
                  {busy === "saving" ? (
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                  ) : (
                    <Save size={16} aria-hidden="true" />
                  )}
                  {busy === "saving"
                    ? t("signature.vault.encrypting")
                    : t("signature.vault.encryptSave")}
                </button>
              </div>
            ) : null}

            <div className="signature-vault-list-heading">
              <div>
                <strong>{t("signature.vault.savedHeading")}</strong>
                <small>{t("signature.vault.namesEncrypted")}</small>
              </div>
              <button
                aria-label={t("signature.vault.refreshAria")}
                className="icon-button"
                disabled={Boolean(busy)}
                onClick={() => void refresh()}
                title={t("signature.vault.refreshTitle")}
                type="button"
              >
                <RefreshCw
                  className={busy === "loading" ? "spin" : undefined}
                  size={15}
                  aria-hidden="true"
                />
              </button>
            </div>

            {entries.length === 0 && busy !== "loading" ? (
              <div className="signature-vault-empty">
                <LockKeyhole size={18} aria-hidden="true" />
                <span>{t("signature.vault.empty")}</span>
              </div>
            ) : (
              <div className="signature-vault-list">
                {entries.map((entry) => (
                  <div className="signature-vault-entry" key={entry.id}>
                    <div className="signature-vault-entry-heading">
                      <LockKeyhole size={16} aria-hidden="true" />
                      <div>
                        <strong>{knownLabels[entry.id] ?? t("signature.vault.encryptedName")}</strong>
                        <small>
                          {formatStoredTime(entry.storedAtMs, formatDate)} |{" "}
                          {formatFileSize(entry.bytesOnDisk, formatNumber)}
                        </small>
                      </div>
                    </div>

                    {unlockId === entry.id ? (
                      <div className="signature-vault-unlock">
                        <VaultPasswordField
                          label={t("signature.vault.passphrase")}
                          onChange={(value) => {
                            setUnlockPassphrase(value);
                            setError(null);
                            setStatus(null);
                          }}
                          show={showPassphrases}
                          value={unlockPassphrase}
                        />
                        <div>
                          <button
                            disabled={Boolean(busy)}
                            onClick={() => {
                              setUnlockId(null);
                              setUnlockPassphrase("");
                              setError(null);
                            }}
                            type="button"
                          >
                            {t("common.cancel")}
                          </button>
                          <button
                            className="primary"
                            disabled={
                              !signatureVaultPassphraseIsValid(unlockPassphrase) ||
                              Boolean(busy)
                            }
                            onClick={() => void unlockSignature(entry.id)}
                            type="button"
                          >
                            {busy === "unlocking" ? (
                              <Loader2 className="spin" size={15} aria-hidden="true" />
                            ) : (
                              <KeyRound size={15} aria-hidden="true" />
                            )}
                            {t("signature.vault.unlock")}
                          </button>
                        </div>
                      </div>
                    ) : deleteId === entry.id ? (
                      <div className="signature-vault-delete">
                        <p>{t("signature.vault.deleteConfirm")}</p>
                        <div>
                          <button
                            onClick={() => {
                              setDeleteId(null);
                              setError(null);
                            }}
                            type="button"
                          >
                            {t("common.keep")}
                          </button>
                          <button
                            className="danger-button"
                            disabled={Boolean(busy)}
                            onClick={() => void deleteSignature(entry.id)}
                            type="button"
                          >
                            {busy === "deleting" ? (
                              <Loader2 className="spin" size={15} aria-hidden="true" />
                            ) : (
                              <Trash2 size={15} aria-hidden="true" />
                            )}
                            {t("signature.vault.deleteCopy")}
                          </button>
                        </div>
                      </div>
                    ) : (
                      <div className="signature-vault-entry-actions">
                        <button
                          disabled={Boolean(busy)}
                          onClick={() => {
                            setDeleteId(null);
                            setUnlockId(entry.id);
                            setUnlockPassphrase("");
                            setError(null);
                            setStatus(null);
                          }}
                          type="button"
                        >
                          <KeyRound size={15} aria-hidden="true" />
                          {t("signature.vault.unlock")}
                        </button>
                        <button
                          aria-label={t("signature.vault.deleteAria")}
                          className="icon-button"
                          disabled={Boolean(busy)}
                          onClick={() => {
                            setUnlockId(null);
                            setDeleteId(entry.id);
                            setError(null);
                            setStatus(null);
                          }}
                          title={t("signature.vault.deleteTitle")}
                          type="button"
                        >
                          <Trash2 size={15} aria-hidden="true" />
                        </button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}

            <button
              className="signature-vault-show-passphrases"
              onClick={() => setShowPassphrases((current) => !current)}
              type="button"
            >
              {showPassphrases ? (
                <EyeOff size={15} aria-hidden="true" />
              ) : (
                <Eye size={15} aria-hidden="true" />
              )}
              {showPassphrases
                ? t("signature.vault.hidePassphrases")
                : t("signature.vault.showPassphrases")}
            </button>
          </>
        )}

        {error ? (
          <div className="signature-vault-message is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{localiseSignatureVaultError(error, t)}</span>
          </div>
        ) : status ? (
          <div className="signature-vault-message is-success" role="status">
            <CheckCircle2 size={16} aria-hidden="true" />
            <span>{localiseSignatureVaultStatus(status, t)}</span>
          </div>
        ) : null}

        <p className="signature-vault-note">
          {t("signature.vault.note")}
        </p>
      </div>
    </details>
  );
}

function VaultPasswordField({
  label,
  onChange,
  show,
  value
}: {
  label: string;
  onChange: (value: string) => void;
  show: boolean;
  value: string;
}) {
  return (
    <label>
      {label}
      <input
        autoComplete="new-password"
        maxLength={MAX_SIGNATURE_VAULT_PASSPHRASE_BYTES}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        type={show ? "text" : "password"}
        value={value}
      />
    </label>
  );
}

function formatStoredTime(
  timestamp: number,
  formatDate: (value: Date | number, options?: Intl.DateTimeFormatOptions) => string
) {
  return formatDate(timestamp, {
    dateStyle: "medium",
    timeStyle: "short"
  });
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) {
    return `${formatNumber(bytes)} B`;
  }
  return `${formatNumber(bytes / 1024, {
    maximumFractionDigits: bytes >= 10 * 1024 ? 0 : 1,
    minimumFractionDigits: 0
  })} KB`;
}
