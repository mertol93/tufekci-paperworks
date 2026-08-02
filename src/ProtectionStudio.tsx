import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Eye,
  EyeOff,
  FileKey2,
  FolderOpen,
  KeyRound,
  Loader2,
  ShieldCheck,
  UnlockKeyhole
} from "lucide-react";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";
import { useI18n } from "./I18nProvider";
import { type Translate } from "./i18n";
import { localisePdfJobFailure } from "./pdfJobs";

type ProtectionMode = "protect" | "remove";
type PrintPermission = "none" | "low" | "full";
type ModificationPermission = "none" | "assembly" | "form" | "annotate" | "all";

type ProtectionResult = {
  outputPath: string;
  bytesWritten: number;
  encryption: string;
};

type StatusMessage = {
  kind: "success" | "error" | "info";
  text: string;
};

type ProtectionStudioProps = {
  desktopMode: boolean;
  qpdfAvailable: boolean;
};

export function ProtectionStudio({ desktopMode, qpdfAvailable }: ProtectionStudioProps) {
  const { formatNumber, locale, t } = useI18n();
  const pdfFilter = useMemo(
    () => [{ name: t("protect.filter.pdfDocuments"), extensions: ["pdf"] }],
    [t]
  );
  const [protectionMode, setProtectionMode] = useState<ProtectionMode>("protect");
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [openPassword, setOpenPassword] = useState("");
  const [openPasswordConfirmation, setOpenPasswordConfirmation] = useState("");
  const [ownerPassword, setOwnerPassword] = useState("");
  const [ownerPasswordConfirmation, setOwnerPasswordConfirmation] = useState("");
  const [inputPassword, setInputPassword] = useState("");
  const [removePassword, setRemovePassword] = useState("");
  const [restrictActions, setRestrictActions] = useState(false);
  const [printPermission, setPrintPermission] = useState<PrintPermission>("full");
  const [modificationPermission, setModificationPermission] =
    useState<ModificationPermission>("none");
  const [allowCopying, setAllowCopying] = useState(true);
  const [showPasswords, setShowPasswords] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [dialogBusy, setDialogBusy] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [status, setStatus] = useState<StatusMessage | null>(null);
  const protectionJob = usePdfJob<ProtectionResult>(desktopMode, "protection");
  const busy = dialogBusy || protectionJob.isActive;

  const passwordStrength = useMemo(() => assessPassword(openPassword), [openPassword]);
  const openPasswordValid =
    openPassword.length >= 8 &&
    utf8Length(openPassword) <= 127 &&
    !containsLineBreak(openPassword) &&
    openPassword === openPasswordConfirmation;
  const ownerPasswordValid =
    !restrictActions ||
    (ownerPassword.length >= 8 &&
      utf8Length(ownerPassword) <= 127 &&
      !containsLineBreak(ownerPassword) &&
      ownerPassword === ownerPasswordConfirmation &&
      ownerPassword !== openPassword);
  const sourcePassword = protectionMode === "protect" ? inputPassword : removePassword;
  const safetySources = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "protection-source",
              label: fileNameFromPath(sourcePath),
              password: sourcePassword,
              path: sourcePath
            }
          ]
        : [],
    [sourcePassword, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, safetySources, "protection");
  const sourceFingerprint = editSafety.checks.find(
    (check) => check.id === "protection-source" && check.status === "ready"
  )?.result;
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const canRun =
    desktopMode &&
    qpdfAvailable &&
    Boolean(sourcePath) &&
    !busy &&
    editSafety.isReady &&
    Boolean(sourceFingerprint) &&
    certificateRiskAccepted &&
    (protectionMode === "remove" || (openPasswordValid && ownerPasswordValid));
  const sourceName = sourcePath ? fileNameFromPath(sourcePath) : null;

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [safetySources]);

  useEffect(() => {
    const job = protectionJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setStatus({
        kind: "success",
        text: t(
          job.result.encryption === "None"
            ? "protect.success.unlocked"
            : "protect.success.protected",
          {
            name: fileNameFromPath(job.result.outputPath),
            size: formatFileSize(job.result.bytesWritten, formatNumber)
          }
        )
      });
      clearPasswords();
    } else if (job.status === "cancelled") {
      setStatus({
        kind: "info",
        text: t("protect.cancelled")
      });
    } else if (job.status === "failed") {
      setStatus({
        kind: "error",
        text: localisePdfJobFailure(job, t)
      });
    }
  }, [locale, protectionJob.job?.jobId, protectionJob.job?.status]);

  const chooseSource = async () => {
    setStatus(null);

    try {
      const selected = await open({
        directory: false,
        filters: pdfFilter,
        multiple: false,
        title:
          protectionMode === "protect"
            ? t("protect.dialog.sourceProtect")
            : t("protect.dialog.sourceRemove")
      });

      if (typeof selected === "string") {
        setSourcePath(selected);
        setSignatureRiskAcknowledged(false);
        protectionJob.clearJob();
      }
    } catch (error) {
      setStatus({ kind: "error", text: t("protect.error.chooseSource") });
    }
  };

  const runProtection = async () => {
    if (!sourcePath || !sourceFingerprint || !canRun) {
      return;
    }

    setDialogBusy(true);
    setCancelBusy(false);
    setStatus(null);
    protectionJob.clearJob();

    try {
      const destination = await save({
        defaultPath: suggestedOutputPath(sourcePath, protectionMode),
        filters: pdfFilter,
        title:
          protectionMode === "protect"
            ? t("protect.dialog.saveProtected")
            : t("protect.dialog.saveUnlocked")
      });

      if (!destination) {
        return;
      }

      await protectionJob.startJob(
        protectionMode === "protect"
          ? {
              operation: "protect",
              request: {
                acknowledgeCertificateSignatures: signatureRiskAcknowledged,
                allowCopying: restrictActions ? allowCopying : true,
                expectedSourceModifiedAtMs: sourceFingerprint.sourceModifiedAtMs,
                expectedSourceSize: sourceFingerprint.sourceSize,
                inputPassword: inputPassword || null,
                inputPath: sourcePath,
                modificationPermission: restrictActions ? modificationPermission : "all",
                openPassword,
                outputPath: destination,
                ownerPassword: restrictActions ? ownerPassword : openPassword,
                printPermission: restrictActions ? printPermission : "full"
              }
            }
          : {
              operation: "remove",
              request: {
                acknowledgeCertificateSignatures: signatureRiskAcknowledged,
                expectedSourceModifiedAtMs: sourceFingerprint.sourceModifiedAtMs,
                expectedSourceSize: sourceFingerprint.sourceSize,
                inputPath: sourcePath,
                outputPath: destination,
                password: removePassword
              }
            }
      );
    } catch (error) {
      setStatus({ kind: "error", text: t("protect.error.start") });
    } finally {
      setDialogBusy(false);
    }
  };

  const cancelProtection = async () => {
    if (!protectionJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await protectionJob.cancelJob();
    } catch (error) {
      setCancelBusy(false);
      setStatus({ kind: "error", text: t("protect.error.cancel") });
    }
  };

  const changeMode = (nextMode: ProtectionMode) => {
    if (busy) {
      return;
    }
    setProtectionMode(nextMode);
    setSourcePath(null);
    setStatus(null);
    protectionJob.clearJob();
    clearPasswords();
  };

  const clearPasswords = () => {
    setOpenPassword("");
    setOpenPasswordConfirmation("");
    setOwnerPassword("");
    setOwnerPasswordConfirmation("");
    setInputPassword("");
    setRemovePassword("");
  };

  const updatePassword = (
    setter: (value: string) => void,
    value: string
  ) => {
    setter(value);
    setStatus(null);
    protectionJob.clearJob();
  };

  return (
    <section className="protection-studio">
      <div className="protection-heading">
        <div>
          <h3>{t("protect.heading.title")}</h3>
          <p>{t("protect.heading.description")}</p>
        </div>
        <ShieldCheck size={18} aria-hidden="true" />
      </div>

      <div className="protection-mode" aria-label={t("protect.mode.aria")}>
        <button
          className={protectionMode === "protect" ? "is-active" : ""}
          disabled={busy}
          onClick={() => changeMode("protect")}
          type="button"
        >
          <FileKey2 size={16} aria-hidden="true" />
          {t("protect.mode.add")}
        </button>
        <button
          className={protectionMode === "remove" ? "is-active" : ""}
          disabled={busy}
          onClick={() => changeMode("remove")}
          type="button"
        >
          <UnlockKeyhole size={16} aria-hidden="true" />
          {t("protect.mode.remove")}
        </button>
      </div>

      <div className={qpdfAvailable ? "engine-state is-ready" : "engine-state is-missing"}>
        {qpdfAvailable ? (
          <CheckCircle2 size={16} aria-hidden="true" />
        ) : (
          <AlertTriangle size={16} aria-hidden="true" />
        )}
        <span>
          {!desktopMode
            ? t("protect.engine.desktopOnly")
            : qpdfAvailable
              ? t("protect.engine.ready")
              : t("protect.engine.missing")}
        </span>
      </div>

      <button
        className="wide-button"
        disabled={!desktopMode || busy}
        onClick={chooseSource}
        type="button"
      >
        <FolderOpen size={17} aria-hidden="true" />
        {sourceName ? t("protect.source.chooseAnother") : t("protect.source.choose")}
      </button>

      {sourceName ? (
        <div className="protection-source">
          <FileKey2 size={17} aria-hidden="true" />
          <span>
            <strong>{sourceName}</strong>
            <small>{sourcePath}</small>
          </span>
        </div>
      ) : null}

      {protectionMode === "protect" ? (
        <>
          <PasswordField
            autoComplete="new-password"
            disabled={busy}
            label={t("protection.openingPassword")}
            onChange={(value) => updatePassword(setOpenPassword, value)}
            showPassword={showPasswords}
            value={openPassword}
          />
          <PasswordField
            autoComplete="new-password"
            disabled={busy}
            label={t("protection.confirmOpeningPassword")}
            onChange={(value) => updatePassword(setOpenPasswordConfirmation, value)}
            showPassword={showPasswords}
            value={openPasswordConfirmation}
          />

          <div className="password-strength" aria-live="polite">
            <div>
              <span>{t("protect.strength.label")}</span>
              <strong>
                {localisePasswordStrength(passwordStrength.score, Boolean(openPassword), t)}
              </strong>
            </div>
            <progress max="4" value={passwordStrength.score} />
            <small>{t("protect.strength.help")}</small>
          </div>

          <PasswordField
            autoComplete="current-password"
            disabled={busy}
            label={t("protect.password.currentSource")}
            onChange={(value) => updatePassword(setInputPassword, value)}
            showPassword={showPasswords}
            value={inputPassword}
          />

          <label className="protection-toggle">
            <input
              checked={restrictActions}
              disabled={busy}
              onChange={(event) => setRestrictActions(event.target.checked)}
              type="checkbox"
            />
            <span>
              <strong>{t("protect.restrict.title")}</strong>
              <small>{t("protect.restrict.description")}</small>
            </span>
            <KeyRound size={17} aria-hidden="true" />
          </label>

          {restrictActions ? (
            <fieldset className="permission-settings">
              <legend>{t("protect.permissions.legend")}</legend>
              <label>
                {t("protect.permissions.printing")}
                <select
                  disabled={busy}
                  onChange={(event) => setPrintPermission(event.target.value as PrintPermission)}
                  value={printPermission}
                >
                  <option value="full">{t("protect.permissions.print.full")}</option>
                  <option value="low">{t("protect.permissions.print.low")}</option>
                  <option value="none">{t("protect.permissions.print.none")}</option>
                </select>
              </label>
              <label>
                {t("protect.permissions.changes")}
                <select
                  disabled={busy}
                  onChange={(event) =>
                    setModificationPermission(event.target.value as ModificationPermission)
                  }
                  value={modificationPermission}
                >
                  <option value="none">{t("protect.permissions.change.none")}</option>
                  <option value="assembly">{t("protect.permissions.change.assembly")}</option>
                  <option value="form">{t("protect.permissions.change.form")}</option>
                  <option value="annotate">{t("protect.permissions.change.annotate")}</option>
                  <option value="all">{t("protect.permissions.change.all")}</option>
                </select>
              </label>
              <label className="permission-checkbox">
                <input
                  checked={allowCopying}
                  disabled={busy}
                  onChange={(event) => setAllowCopying(event.target.checked)}
                  type="checkbox"
                />
                {t("protect.permissions.copy")}
              </label>

              <PasswordField
                autoComplete="new-password"
                disabled={busy}
                label={t("protection.administratorPassword")}
                onChange={(value) => updatePassword(setOwnerPassword, value)}
                showPassword={showPasswords}
                value={ownerPassword}
              />
              <PasswordField
                autoComplete="new-password"
                disabled={busy}
                label={t("protection.confirmAdministratorPassword")}
                onChange={(value) => updatePassword(setOwnerPasswordConfirmation, value)}
                showPassword={showPasswords}
                value={ownerPasswordConfirmation}
              />
              <small className="permission-help">
                {t("protect.permissions.adminHelp")}
              </small>
            </fieldset>
          ) : null}
        </>
      ) : (
        <>
          <PasswordField
            autoComplete="current-password"
            disabled={busy}
            label={t("protect.password.currentPdf")}
            onChange={(value) => updatePassword(setRemovePassword, value)}
            showPassword={showPasswords}
            value={removePassword}
          />
          <p className="protection-hint">
            {t("protect.remove.help")}
          </p>
        </>
      )}

      <button className="show-passwords" disabled={busy} onClick={() => setShowPasswords((value) => !value)} type="button">
        {showPasswords ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
        {showPasswords ? t("common.hidePasswords") : t("common.showPasswords")}
      </button>

      {protectionMode === "protect" && openPasswordConfirmation && !openPasswordValid ? (
        <p className="field-error">
          {t("protect.validation.opening")}
        </p>
      ) : null}
      {restrictActions && ownerPasswordConfirmation && !ownerPasswordValid ? (
        <p className="field-error">
          {t("protect.validation.administrator")}
        </p>
      ) : null}

      <PdfEditSafetyNotice
        acknowledged={signatureRiskAcknowledged}
        busy={busy}
        editSafety={editSafety}
        onAcknowledgedChange={setSignatureRiskAcknowledged}
        rewriteDescription={t("protect.rewriteDescription")}
      />

      <button className="primary wide-button" disabled={!canRun} onClick={runProtection} type="button">
        {busy ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : protectionMode === "protect" ? (
          <ShieldCheck size={17} aria-hidden="true" />
        ) : (
          <UnlockKeyhole size={17} aria-hidden="true" />
        )}
        {protectionJob.isActive
          ? t("protect.action.running")
          : dialogBusy
            ? t("protect.action.choosing")
          : protectionMode === "protect"
            ? t("protect.action.protect")
            : t("protect.action.remove")}
      </button>

      {protectionJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={protectionJob.connectionError}
          job={protectionJob.job}
          onCancel={() => void cancelProtection()}
          onRetry={() => void runProtection()}
          retryDisabled={!canRun}
        />
      ) : null}

      {!protectionJob.isActive && protectionJob.connectionError ? (
        <div className="protection-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {status ? (
        <div className={`protection-status is-${status.kind}`} role="status">
          {status.kind === "success" ? (
            <CheckCircle2 size={17} aria-hidden="true" />
          ) : status.kind === "info" ? (
            <AlertCircle size={17} aria-hidden="true" />
          ) : (
            <AlertTriangle size={17} aria-hidden="true" />
          )}
          <span>{status.text}</span>
        </div>
      ) : null}

      <div className="protection-note">
        {t("protect.note")}
      </div>
    </section>
  );
}

type PasswordFieldProps = {
  autoComplete: string;
  disabled: boolean;
  label: string;
  onChange: (value: string) => void;
  showPassword: boolean;
  value: string;
};

function PasswordField({
  autoComplete,
  disabled,
  label,
  onChange,
  showPassword,
  value
}: PasswordFieldProps) {
  return (
    <label className="protection-field">
      {label}
      <input
        autoComplete={autoComplete}
        disabled={disabled}
        maxLength={127}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        type={showPassword ? "text" : "password"}
        value={value}
      />
    </label>
  );
}

function assessPassword(password: string) {
  if (!password) {
    return { score: 0 };
  }

  let score = 0;
  if (password.length >= 8) score += 1;
  if (password.length >= 12) score += 1;
  if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score += 1;
  if (/\d/.test(password) && /[^A-Za-z0-9]/.test(password)) score += 1;

  return { score };
}

function suggestedOutputPath(sourcePath: string, mode: ProtectionMode) {
  const suffix = mode === "protect" ? "-protected.pdf" : "-unlocked.pdf";
  return sourcePath.replace(/\.pdf$/i, suffix);
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function utf8Length(value: string) {
  return new TextEncoder().encode(value).length;
}

function containsLineBreak(value: string) {
  return /[\r\n\0]/.test(value);
}

function localisePasswordStrength(score: number, isSet: boolean, t: Translate) {
  if (!isSet) {
    return t("protect.strength.notSet");
  }
  const keys = [
    "protect.strength.veryWeak",
    "protect.strength.weak",
    "protect.strength.fair",
    "protect.strength.good",
    "protect.strength.strong"
  ] as const;
  return t(keys[Math.max(0, Math.min(keys.length - 1, score))]);
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KB`;
  }
  return `${formatNumber(bytes / (1024 * 1024), { maximumFractionDigits: 1 })} MB`;
}
