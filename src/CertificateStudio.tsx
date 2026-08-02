import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  BadgeCheck,
  CheckCircle2,
  ChevronDown,
  Clock3,
  Eye,
  EyeOff,
  FileCheck2,
  FileKey2,
  FileSignature,
  FolderOpen,
  Loader2,
  Plus,
  ShieldQuestion,
  X
} from "lucide-react";
import {
  localiseCertificateFieldKind,
  localiseCertificateFieldName,
  localiseCertificateSigningTime,
  localiseCertificateSummary,
  localiseCertificateWarnings,
  safeCertificateDocumentText
} from "./certificateLocalisation";
import { useI18n } from "./I18nProvider";
import type { Translate, TranslationKey, TranslationValues } from "./i18n";
import { PdfJobProgress } from "./PdfJobProgress";
import { isInterruptedPdfJob, localisePdfJobFailure } from "./pdfJobs";
import { usePdfJob } from "./usePdfJob";

type CertificateMode = "sign" | "validate";
type CertificatePosition = "left" | "centre" | "right";
type CertificateValidationState =
  | "unsigned"
  | "valid"
  | "invalid"
  | "indeterminate"
  | "unavailable";

type CertificateCapabilities = {
  available: boolean;
  provider: string;
  version?: string | null;
  passfileSupported: boolean;
  detail: string;
};

type CertificateSignatureField = {
  name: string;
  signed: boolean;
  kind: string;
  reason?: string | null;
  location?: string | null;
  signingTime?: string | null;
};

type CertificateValidationReport = {
  inputPath: string;
  encrypted: boolean;
  signatureCount: number;
  timestampCount: number;
  fields: CertificateSignatureField[];
  state: CertificateValidationState;
  intact?: boolean | null;
  trusted?: boolean | null;
  engineVersion?: string | null;
  summary: string;
  details: string;
  warnings: string[];
};

type CertificateSignResult = {
  outputPath: string;
  bytesWritten: number;
  encrypted: boolean;
  fieldName: string;
  visible: boolean;
  timestamped: boolean;
  validation: CertificateValidationReport;
  warnings: string[];
};

type CertificateStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  pyhankoAvailable: boolean;
  workspaceHasPendingChanges: boolean;
};

type StatusMessage = {
  kind: "error" | "info" | "success";
  key: TranslationKey;
  values?: TranslationValues;
};

export function CertificateStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  pyhankoAvailable,
  workspaceHasPendingChanges
}: CertificateStudioProps) {
  const { formatDate, formatNumber, t } = useI18n();
  const [mode, setMode] = useState<CertificateMode>("sign");
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [inputPassword, setInputPassword] = useState(initialSourcePassword ?? "");
  const [pkcs12Path, setPkcs12Path] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [passphraseConfirmation, setPassphraseConfirmation] = useState("");
  const [showPasswords, setShowPasswords] = useState(false);
  const [visible, setVisible] = useState(true);
  const [pageNumber, setPageNumber] = useState(1);
  const [position, setPosition] = useState<CertificatePosition>("right");
  const [fieldName, setFieldName] = useState("Signature1");
  const [useTimestamp, setUseTimestamp] = useState(false);
  const [timestampUrl, setTimestampUrl] = useState("");
  const [embedValidationInfo, setEmbedValidationInfo] = useState(false);
  const [trustRoots, setTrustRoots] = useState<string[]>([]);
  const [capabilities, setCapabilities] = useState<CertificateCapabilities | null>(null);
  const [capabilitiesBusy, setCapabilitiesBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [signingCancelBusy, setSigningCancelBusy] = useState(false);
  const [validationCancelBusy, setValidationCancelBusy] = useState(false);
  const [status, setStatus] = useState<StatusMessage | null>(null);
  const [signingReport, setSigningReport] = useState<CertificateValidationReport | null>(null);
  const [validationReport, setValidationReport] =
    useState<CertificateValidationReport | null>(null);
  const signingJob = usePdfJob<CertificateSignResult>(desktopMode, "certificate");
  const validationJob = usePdfJob<CertificateValidationReport>(
    desktopMode,
    "certificate-validation"
  );
  const operationBusy = busy || signingJob.isActive || validationJob.isActive;
  const report = mode === "sign" ? signingReport : validationReport;
  const jobFailure =
    mode === "sign" && signingJob.job?.status === "failed"
      ? localisePdfJobFailure(signingJob.job, t)
      : mode === "validate" && validationJob.job?.status === "failed"
        ? localisePdfJobFailure(validationJob.job, t)
        : null;

  useEffect(() => {
    setSourcePath(initialSourcePath ?? null);
    setInputPassword(initialSourcePassword ?? "");
    setSigningReport(null);
    setValidationReport(null);
    setStatus(null);
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    if (signingJob.isActive || isInterruptedPdfJob(signingJob.job)) {
      setMode("sign");
    }
  }, [signingJob.isActive, signingJob.job?.jobId, signingJob.job?.status]);

  useEffect(() => {
    if (validationJob.isActive || isInterruptedPdfJob(validationJob.job)) {
      setMode("validate");
    }
  }, [validationJob.isActive, validationJob.job?.jobId, validationJob.job?.status]);

  useEffect(() => {
    const job = signingJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setSigningCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setSigningReport(job.result.validation);
      setStatus({
        kind: "success",
        key: job.result.encrypted
          ? "certificate.status.signedProtected"
          : "certificate.status.signed",
        values: {
          name: fileNameFromPath(job.result.outputPath),
          size: formatFileSize(job.result.bytesWritten, formatNumber)
        }
      });
    } else if (job.status === "cancelled") {
      setSigningReport(null);
      setStatus({
        kind: "info",
        key: "certificate.notice.signingCancelled"
      });
    } else if (job.status === "failed") {
      setSigningReport(null);
      setStatus(null);
    }
  }, [formatNumber, signingJob.job?.jobId, signingJob.job?.status]);

  useEffect(() => {
    const job = validationJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setValidationCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setValidationReport(job.result);
      setStatus(null);
    } else if (job.status === "cancelled") {
      setValidationReport(null);
      setStatus({
        kind: "info",
        key: "certificate.notice.validationCancelled"
      });
    } else if (job.status === "failed") {
      setValidationReport(null);
      setStatus(null);
    }
  }, [validationJob.job?.jobId, validationJob.job?.status]);

  useEffect(() => {
    if (!desktopMode) {
      setCapabilities(null);
      return;
    }

    let active = true;
    setCapabilitiesBusy(true);
    invoke<CertificateCapabilities>("certificate_capabilities")
      .then((result) => {
        if (active) {
          setCapabilities(result);
        }
      })
      .catch(() => {
        if (active) {
          setCapabilities({
            available: false,
            detail: "",
            passfileSupported: false,
            provider: "pyHanko",
            version: null
          });
        }
      })
      .finally(() => {
        if (active) {
          setCapabilitiesBusy(false);
        }
      });

    return () => {
      active = false;
    };
  }, [desktopMode, pyhankoAvailable]);

  const passphrasesValid = useMemo(
    () =>
      passphrase === passphraseConfirmation &&
      utf8Length(passphrase) <= 1024 &&
      !/[\r\n\0]/.test(passphrase),
    [passphrase, passphraseConfirmation]
  );
  const inputPasswordValid =
    utf8Length(inputPassword) <= 1024 && !/[\r\n\0]/.test(inputPassword);
  const fieldNameValid = /^[A-Za-z0-9._-]{1,64}$/.test(fieldName);
  const timestampValid = !useTimestamp || validTimestampUrl(timestampUrl);
  const preciseEngineReady = Boolean(
    capabilities?.available && capabilities.passfileSupported
  );
  const validationEngineReady = Boolean(capabilities?.available);
  const selectedEngineReady =
    mode === "sign" ? preciseEngineReady : validationEngineReady;
  const sourceIsOpenWorkspace = Boolean(
    sourcePath && initialSourcePath && pathsMatch(sourcePath, initialSourcePath)
  );
  const pendingWorkspaceBlocksSigning =
    sourceIsOpenWorkspace && workspaceHasPendingChanges;
  const canSign = Boolean(
    desktopMode &&
      preciseEngineReady &&
      sourcePath &&
      pkcs12Path &&
      inputPasswordValid &&
      passphrasesValid &&
      fieldNameValid &&
      (!visible || (Number.isInteger(pageNumber) && pageNumber > 0)) &&
      timestampValid &&
      !pendingWorkspaceBlocksSigning &&
      !operationBusy
  );
  const canValidate = Boolean(
    desktopMode && sourcePath && inputPasswordValid && !operationBusy
  );

  const chooseSource = async () => {
    setStatus(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("certificate.dialog.filterPdf"), extensions: ["pdf"] }],
        multiple: false,
        title:
          mode === "sign"
            ? t("certificate.dialog.chooseSignSource")
            : t("certificate.dialog.chooseValidationSource")
      });
      if (typeof selected === "string") {
        setSourcePath(selected);
        setInputPassword("");
        setSigningReport(null);
        setValidationReport(null);
        signingJob.clearJob();
        validationJob.clearJob();
      }
    } catch {
      setStatus({ kind: "error", key: "certificate.error.choosePdf" });
    }
  };

  const chooseCertificate = async () => {
    setStatus(null);
    try {
      const selected = await open({
        directory: false,
        filters: [
          { name: t("certificate.dialog.filterPkcs12"), extensions: ["p12", "pfx"] }
        ],
        multiple: false,
        title: t("certificate.dialog.choosePkcs12")
      });
      if (typeof selected === "string") {
        setPkcs12Path(selected);
      }
    } catch {
      setStatus({
        kind: "error",
        key: "certificate.error.choosePkcs12"
      });
    }
  };

  const chooseTrustRoots = async () => {
    setStatus(null);
    try {
      const selected = await open({
        directory: false,
        filters: [
          {
            name: t("certificate.dialog.filterTrust"),
            extensions: ["pem", "crt", "cer", "der"]
          }
        ],
        multiple: true,
        title: t("certificate.dialog.chooseTrust")
      });
      const paths = typeof selected === "string" ? [selected] : (selected ?? []);
      setTrustRoots((current) => Array.from(new Set([...current, ...paths])).slice(0, 16));
      setSigningReport(null);
      setValidationReport(null);
      signingJob.clearJob();
      validationJob.clearJob();
    } catch {
      setStatus({
        kind: "error",
        key: "certificate.error.chooseTrust"
      });
    }
  };

  const removeTrustRoot = (path: string) => {
    setTrustRoots((current) => current.filter((item) => item !== path));
    setSigningReport(null);
    setValidationReport(null);
    signingJob.clearJob();
    validationJob.clearJob();
  };

  const signPdf = async () => {
    if (!sourcePath || !pkcs12Path || !canSign) {
      return;
    }

    let commandStarted = false;
    setStatus(null);
    setSigningReport(null);
    try {
      const outputPath = await save({
        defaultPath: suggestedSignedPath(sourcePath),
        filters: [{ name: t("certificate.dialog.filterPdf"), extensions: ["pdf"] }],
        title: t("certificate.dialog.save")
      });
      if (typeof outputPath !== "string") {
        return;
      }

      setBusy(true);
      commandStarted = true;
      await signingJob.startJob({
        embedValidationInfo,
        fieldName,
        inputPassword: inputPassword || null,
        inputPath: sourcePath,
        outputPath,
        pageNumber: visible ? pageNumber : null,
        pkcs12Passphrase: passphrase,
        pkcs12PassphraseConfirmation: passphraseConfirmation,
        pkcs12Path,
        position: visible ? position : null,
        timestampUrl: useTimestamp ? timestampUrl.trim() : null,
        trustRoots,
        visible
      });
    } catch {
      setStatus({
        kind: "error",
        key: "certificate.error.startSigning"
      });
    } finally {
      if (commandStarted) {
        setInputPassword("");
        setPassphrase("");
        setPassphraseConfirmation("");
      }
      setBusy(false);
    }
  };

  const cancelSigning = async () => {
    if (!signingJob.isActive || signingCancelBusy) {
      return;
    }
    setSigningCancelBusy(true);
    try {
      await signingJob.cancelJob();
    } catch {
      setSigningCancelBusy(false);
      setStatus({
        kind: "error",
        key: "certificate.error.cancelSigning"
      });
    }
  };

  const validatePdf = async () => {
    if (!sourcePath || !canValidate) {
      return;
    }

    let commandStarted = false;
    setBusy(true);
    setStatus(null);
    setValidationReport(null);
    try {
      commandStarted = true;
      await validationJob.startJob({
        inputPassword: inputPassword || null,
        inputPath: sourcePath,
        trustRoots
      });
    } catch {
      setStatus({
        kind: "error",
        key: "certificate.error.startValidation"
      });
    } finally {
      if (commandStarted) {
        setInputPassword("");
      }
      setBusy(false);
    }
  };

  const cancelValidation = async () => {
    if (!validationJob.isActive || validationCancelBusy) {
      return;
    }
    setValidationCancelBusy(true);
    try {
      await validationJob.cancelJob();
    } catch {
      setValidationCancelBusy(false);
      setStatus({
        kind: "error",
        key: "certificate.error.cancelValidation"
      });
    }
  };

  const changeMode = (nextMode: CertificateMode) => {
    setMode(nextMode);
    setStatus(null);
  };

  return (
    <details className="certificate-studio">
      <summary>
        <span className="certificate-summary-icon">
          <BadgeCheck size={17} aria-hidden="true" />
        </span>
        <span>
          <strong>{t("certificate.heading.title")}</strong>
          <small>{t("certificate.heading.description")}</small>
        </span>
        <ChevronDown size={16} aria-hidden="true" />
      </summary>

      <div className="certificate-body">
        <fieldset className="certificate-operation-fieldset" disabled={operationBusy}>
        <div className="certificate-mode" aria-label={t("certificate.mode.aria")}>
          <button
            className={mode === "sign" ? "is-active" : ""}
            onClick={() => changeMode("sign")}
            type="button"
          >
            <FileSignature size={16} aria-hidden="true" />
            {t("certificate.mode.sign")}
          </button>
          <button
            className={mode === "validate" ? "is-active" : ""}
            onClick={() => changeMode("validate")}
            type="button"
          >
            <FileCheck2 size={16} aria-hidden="true" />
            {t("certificate.mode.validate")}
          </button>
        </div>

        <div
          className={
            selectedEngineReady
              ? "engine-state is-ready"
              : capabilitiesBusy
                ? "engine-state is-info"
                : "engine-state is-missing"
          }
        >
          {capabilitiesBusy ? (
            <Loader2 className="spin" size={16} aria-hidden="true" />
          ) : selectedEngineReady ? (
            <CheckCircle2 size={16} aria-hidden="true" />
          ) : (
            <AlertTriangle size={16} aria-hidden="true" />
          )}
          <span>
            {!desktopMode
              ? t("certificate.engine.desktopOnly")
              : capabilitiesBusy
                ? t("certificate.engine.checking")
                : preciseEngineReady
                  ? t("certificate.engine.ready")
                  : validationEngineReady
                    ? t("certificate.engine.validationOnly")
                    : pyhankoAvailable
                      ? t("certificate.engine.unsupported")
                      : t("certificate.engine.missing")}
          </span>
        </div>

        <button
          className="wide-button"
          disabled={!desktopMode || operationBusy}
          onClick={chooseSource}
          type="button"
        >
          <FolderOpen size={17} aria-hidden="true" />
          {sourcePath
            ? t("certificate.source.chooseAnother")
            : t("certificate.source.choose")}
        </button>

        {sourcePath ? (
          <SelectedPath
            description={t("certificate.source.local")}
            icon="pdf"
            path={sourcePath}
          />
        ) : null}

        {sourcePath ? (
          <>
            <label className="certificate-field">
              {t("certificate.password.pdfLabel")}
              <input
                autoComplete="current-password"
                onChange={(event) => setInputPassword(event.target.value)}
                spellCheck={false}
                type={showPasswords ? "text" : "password"}
                value={inputPassword}
              />
              <small>
                {t("certificate.password.pdfHelp")}
              </small>
            </label>

            <button
              className="show-passwords"
              onClick={() => setShowPasswords((current) => !current)}
              type="button"
            >
              {showPasswords ? (
                <EyeOff size={16} aria-hidden="true" />
              ) : (
                <Eye size={16} aria-hidden="true" />
              )}
              {showPasswords
                ? t("certificate.password.hide")
                : t("certificate.password.show")}
            </button>

            {!inputPasswordValid ? (
              <p className="field-error">
                {t("certificate.password.pdfError")}
              </p>
            ) : null}
          </>
        ) : null}

        {mode === "sign" ? (
          <>
            {pendingWorkspaceBlocksSigning ? (
              <div className="certificate-warning" role="alert">
                <AlertTriangle size={17} aria-hidden="true" />
                <span>
                  <strong>{t("certificate.pending.title")}</strong>
                  <small>{t("certificate.pending.description")}</small>
                </span>
              </div>
            ) : null}

            <button
              className="wide-button"
              disabled={!desktopMode || operationBusy}
              onClick={chooseCertificate}
              type="button"
            >
              <FileKey2 size={17} aria-hidden="true" />
              {pkcs12Path
                ? t("certificate.pkcs12.chooseAnother")
                : t("certificate.pkcs12.choose")}
            </button>

            {pkcs12Path ? (
              <SelectedPath
                description={t("certificate.pkcs12.local")}
                icon="certificate"
                path={pkcs12Path}
              />
            ) : null}

            <label className="certificate-field">
              {t("certificate.passphrase.label")}
              <input
                autoComplete="current-password"
                onChange={(event) => setPassphrase(event.target.value)}
                spellCheck={false}
                type={showPasswords ? "text" : "password"}
                value={passphrase}
              />
              <small>{t("certificate.passphrase.help")}</small>
            </label>

            <label className="certificate-field">
              {t("certificate.passphrase.confirm")}
              <input
                autoComplete="current-password"
                onChange={(event) => setPassphraseConfirmation(event.target.value)}
                spellCheck={false}
                type={showPasswords ? "text" : "password"}
                value={passphraseConfirmation}
              />
            </label>

            {passphraseConfirmation && !passphrasesValid ? (
              <p className="field-error">
                {t("certificate.passphrase.error")}
              </p>
            ) : null}

            <label className="signature-toggle">
              <input
                checked={visible}
                onChange={(event) => setVisible(event.target.checked)}
                type="checkbox"
              />
              <span>
                <strong>{t("certificate.field.visible")}</strong>
                <small>{t("certificate.field.visibleHelp")}</small>
              </span>
              <FileSignature size={17} aria-hidden="true" />
            </label>

            <div className="certificate-field-grid">
              <label className="certificate-field">
                {t("certificate.field.name")}
                <input
                  maxLength={64}
                  onChange={(event) => setFieldName(event.target.value)}
                  spellCheck={false}
                  value={fieldName}
                />
              </label>
              {visible ? (
                <label className="certificate-field">
                  {t("certificate.field.page")}
                  <input
                    min={1}
                    onChange={(event) => setPageNumber(Number(event.target.value))}
                    type="number"
                    value={pageNumber}
                  />
                </label>
              ) : null}
            </div>

            {!fieldNameValid ? (
              <p className="field-error">
                {t("certificate.field.nameError")}
              </p>
            ) : null}

            {visible ? (
              <fieldset className="certificate-position">
                <legend>{t("certificate.field.position")}</legend>
                <div className="segmented-control">
                  {(["left", "centre", "right"] as CertificatePosition[]).map((option) => (
                    <button
                      className={position === option ? "is-active" : ""}
                      key={option}
                      onClick={() => setPosition(option)}
                      type="button"
                    >
                      {t(`certificate.field.position.${option}` as TranslationKey)}
                    </button>
                  ))}
                </div>
              </fieldset>
            ) : null}

            <label className="signature-toggle">
              <input
                checked={useTimestamp}
                onChange={(event) => {
                  const checked = event.target.checked;
                  setUseTimestamp(checked);
                  if (!checked) {
                    setEmbedValidationInfo(false);
                  }
                }}
                type="checkbox"
              />
              <span>
                <strong>{t("certificate.timestamp.title")}</strong>
                <small>{t("certificate.timestamp.help")}</small>
              </span>
              <Clock3 size={17} aria-hidden="true" />
            </label>

            {useTimestamp ? (
              <>
                <label className="certificate-field">
                  {t("certificate.timestamp.url")}
                  <input
                    inputMode="url"
                    maxLength={2048}
                    onChange={(event) => setTimestampUrl(event.target.value)}
                    placeholder={t("certificate.timestamp.placeholder")}
                    spellCheck={false}
                    type="url"
                    value={timestampUrl}
                  />
                </label>
                {!timestampValid ? (
                  <p className="field-error">
                    {t("certificate.timestamp.error")}
                  </p>
                ) : null}

                <label className="signature-toggle">
                  <input
                    checked={embedValidationInfo}
                    onChange={(event) => setEmbedValidationInfo(event.target.checked)}
                    type="checkbox"
                  />
                  <span>
                    <strong>{t("certificate.pades.title")}</strong>
                    <small>{t("certificate.pades.help")}</small>
                  </span>
                  <BadgeCheck size={17} aria-hidden="true" />
                </label>
              </>
            ) : null}
          </>
        ) : null}

        <details className="certificate-trust">
          <summary>
            <span>
              <strong>{t("certificate.trust.title")}</strong>
              <small>
                {trustRoots.length === 0
                  ? t("certificate.trust.default")
                  : t(
                      trustRoots.length === 1
                        ? "certificate.trust.count.one"
                        : "certificate.trust.count.other",
                      { count: formatNumber(trustRoots.length) }
                    )}
              </small>
            </span>
            <ChevronDown size={15} aria-hidden="true" />
          </summary>
          <div className="certificate-trust-body">
            <button
              className="wide-button"
              disabled={!desktopMode || operationBusy || trustRoots.length >= 16}
              onClick={chooseTrustRoots}
              type="button"
            >
              <Plus size={16} aria-hidden="true" />
              {t("certificate.trust.add")}
            </button>
            {trustRoots.length > 0 ? (
              <ul className="certificate-trust-list">
                {trustRoots.map((path) => (
                  <li key={path}>
                    <span title={path}>{fileNameFromPath(path)}</span>
                    <button
                      aria-label={t("certificate.trust.removeAria", {
                        name: fileNameFromPath(path)
                      })}
                      className="icon-button"
                      onClick={() => removeTrustRoot(path)}
                      title={t("certificate.trust.removeTitle")}
                      type="button"
                    >
                      <X size={14} aria-hidden="true" />
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
            <small>
              {t("certificate.trust.help")}
            </small>
          </div>
        </details>
        </fieldset>

        {mode === "sign" && signingJob.job ? (
          <PdfJobProgress
            cancelling={signingCancelBusy}
            connectionError={signingJob.connectionError}
            job={signingJob.job}
            onCancel={() => void cancelSigning()}
            onRetry={() => void signPdf()}
            retryDisabled={!canSign}
          />
        ) : null}

        {mode === "validate" && validationJob.job ? (
          <PdfJobProgress
            cancelling={validationCancelBusy}
            connectionError={validationJob.connectionError}
            job={validationJob.job}
            onCancel={() => void cancelValidation()}
            onRetry={() => void validatePdf()}
            retryDisabled={!canValidate}
          />
        ) : null}

        {mode === "sign" && !signingJob.isActive && signingJob.connectionError ? (
          <div className="engine-state is-info" role="status">
            <AlertTriangle size={16} aria-hidden="true" />
            <span>{t("job.connectionError")}</span>
          </div>
        ) : null}

        {mode === "validate" &&
        !validationJob.isActive &&
        validationJob.connectionError ? (
          <div className="engine-state is-info" role="status">
            <AlertTriangle size={16} aria-hidden="true" />
            <span>{t("job.connectionError")}</span>
          </div>
        ) : null}

        <button
          className="primary wide-button"
          disabled={mode === "sign" ? !canSign : !canValidate}
          onClick={mode === "sign" ? signPdf : validatePdf}
          type="button"
        >
          {operationBusy ? (
            <Loader2 className="spin" size={17} aria-hidden="true" />
          ) : mode === "sign" ? (
            <FileSignature size={17} aria-hidden="true" />
          ) : (
            <ShieldQuestion size={17} aria-hidden="true" />
          )}
          {operationBusy
            ? mode === "sign"
              ? t("certificate.action.signing")
              : validationEngineReady
                ? t("certificate.action.validating")
                : t("certificate.action.inspecting")
            : mode === "sign"
              ? t("certificate.action.chooseAndSign")
              : validationEngineReady
                ? t("certificate.action.validate")
                : t("certificate.action.inspect")}
        </button>

        {status || jobFailure ? (
          <div
            className={`certificate-status is-${jobFailure ? "error" : status?.kind}`}
            role={jobFailure || status?.kind === "error" ? "alert" : "status"}
          >
            {!jobFailure && status?.kind === "success" ? (
              <CheckCircle2 size={17} aria-hidden="true" />
            ) : (
              <AlertTriangle size={17} aria-hidden="true" />
            )}
            <span>{jobFailure ?? (status ? t(status.key, status.values) : "")}</span>
          </div>
        ) : null}

        {report ? (
          <CertificateReport
            formatDate={formatDate}
            formatNumber={formatNumber}
            report={report}
            t={t}
          />
        ) : null}

        <p className="certificate-note">
          {t("certificate.note")}
        </p>
      </div>
    </details>
  );
}

function SelectedPath({
  description,
  icon,
  path
}: {
  description: string;
  icon: "certificate" | "pdf";
  path: string;
}) {
  return (
    <div className="certificate-selected-path">
      {icon === "certificate" ? (
        <FileKey2 size={17} aria-hidden="true" />
      ) : (
        <FileCheck2 size={17} aria-hidden="true" />
      )}
      <span>
        <strong title={path}>{fileNameFromPath(path)}</strong>
        <small>{description}</small>
      </span>
    </div>
  );
}

function CertificateReport({
  formatDate,
  formatNumber,
  report,
  t
}: {
  formatDate: (value: Date | number, options?: Intl.DateTimeFormatOptions) => string;
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  report: CertificateValidationReport;
  t: Translate;
}) {
  const Icon =
    report.state === "valid"
      ? BadgeCheck
      : report.state === "invalid"
        ? AlertTriangle
        : ShieldQuestion;
  const warnings = localiseCertificateWarnings(report.warnings, t, formatNumber);
  return (
    <section
      aria-label={t("certificate.report.aria")}
      className={`certificate-report is-${report.state}`}
      aria-live="polite"
    >
      <header>
        <Icon size={18} aria-hidden="true" />
        <span>
          <strong>{validationLabel(report.state, t)}</strong>
          <small>{localiseCertificateSummary(report, t)}</small>
        </span>
      </header>

      <dl className="certificate-metrics">
        <div>
          <dt>{t("certificate.report.metric.signatures")}</dt>
          <dd>{formatNumber(report.signatureCount)}</dd>
        </div>
        <div>
          <dt>{t("certificate.report.metric.timestamps")}</dt>
          <dd>{formatNumber(report.timestampCount)}</dd>
        </div>
        <div>
          <dt>{t("certificate.report.metric.protection")}</dt>
          <dd>
            {report.encrypted
              ? t("certificate.report.value.protected")
              : t("certificate.report.value.unprotected")}
          </dd>
        </div>
        <div>
          <dt>{t("certificate.report.metric.integrity")}</dt>
          <dd>{booleanLabel(report.intact, t)}</dd>
        </div>
        <div>
          <dt>{t("certificate.report.metric.trust")}</dt>
          <dd>{booleanLabel(report.trusted, t)}</dd>
        </div>
      </dl>

      {report.fields.length > 0 ? (
        <ul className="certificate-field-list">
          {report.fields.map((field, index) => {
            const reason = safeCertificateDocumentText(field.reason);
            const location = safeCertificateDocumentText(field.location);
            const signingTime = localiseCertificateSigningTime(field.signingTime, t, formatDate);
            return (
              <li key={`${field.name}-${index}`}>
                <span>
                  <strong>{localiseCertificateFieldName(field.name, t)}</strong>
                  <small>
                    {t("certificate.report.field.summary", {
                      kind: localiseCertificateFieldKind(field.kind, t),
                      state: field.signed
                        ? t("certificate.report.field.signed")
                        : t("certificate.report.field.empty")
                    })}
                  </small>
                  {signingTime ? <small>{signingTime}</small> : null}
                </span>
                {reason ? (
                  <small>{t("certificate.report.field.reason", { value: reason })}</small>
                ) : null}
                {location ? (
                  <small>{t("certificate.report.field.location", { value: location })}</small>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}

      {warnings.length > 0 ? (
        <ul className="certificate-report-warnings">
          {warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}

function validationLabel(
  state: CertificateValidationState,
  t: Translate
) {
  switch (state) {
    case "valid":
      return t("certificate.report.state.valid");
    case "invalid":
      return t("certificate.report.state.invalid");
    case "unsigned":
      return t("certificate.report.state.unsigned");
    case "unavailable":
      return t("certificate.report.state.unavailable");
    default:
      return t("certificate.report.state.indeterminate");
  }
}

function booleanLabel(value: boolean | null | undefined, t: Translate) {
  if (value === true) return t("certificate.report.value.passed");
  if (value === false) return t("certificate.report.value.failed");
  return t("certificate.report.value.notEstablished");
}

function validTimestampUrl(value: string) {
  try {
    const url = new URL(value.trim());
    if (url.username || url.password || url.search || url.hash || !url.hostname) {
      return false;
    }
    if (url.protocol === "https:") {
      return true;
    }
    return (
      url.protocol === "http:" &&
      ["localhost", "127.0.0.1", "::1"].includes(url.hostname.toLocaleLowerCase("en-GB"))
    );
  } catch {
    return false;
  }
}

function pathsMatch(first: string, second: string) {
  const normalise = (value: string) => value.replace(/\\/g, "/").replace(/\/+$/, "");
  const firstNormalised = normalise(first);
  const secondNormalised = normalise(second);
  const windowsPath = /^[A-Za-z]:\//.test(firstNormalised) || firstNormalised.startsWith("//");
  return windowsPath
    ? firstNormalised.toLocaleLowerCase("en-GB") ===
        secondNormalised.toLocaleLowerCase("en-GB")
    : firstNormalised === secondNormalised;
}

function suggestedSignedPath(sourcePath: string) {
  return sourcePath.replace(/\.pdf$/i, "-certificate-signed.pdf");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function utf8Length(value: string) {
  return new TextEncoder().encode(value).length;
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  const options = { maximumFractionDigits: 1, minimumFractionDigits: 1 };
  if (bytes < 1024 * 1024) return `${formatNumber(bytes / 1024, options)} KB`;
  return `${formatNumber(bytes / (1024 * 1024), options)} MB`;
}
