import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Download,
  Eye,
  EyeOff,
  FileSearch,
  FileText,
  FolderOpen,
  Languages,
  Loader2
} from "lucide-react";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { useI18n } from "./I18nProvider";
import {
  localiseOcrLanguage,
  localiseSearchableOcrWarnings
} from "./ocrLocalisation";
import { localisePdfJobFailure } from "./pdfJobs";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

type OcrLanguage = {
  code: string;
  name: string;
};

type SearchableOcrResult = {
  bytesWritten: number;
  deskewRequested: boolean;
  encryption: "AES-256" | "None";
  language: string;
  outputPath: string;
  pageCount: number;
  pagesWithoutSearchableText: number;
  searchableTextPages: number;
  warnings: string[];
};

type SearchableOcrStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  ocrLanguages: OcrLanguage[];
  ocrReadinessBusy: boolean;
  ocrReadinessDetail?: string | null;
  ocrReady: boolean;
  onLanguageChange: (language: string) => void;
  qpdfAvailable: boolean;
  selectedLanguage: string;
};

export function SearchableOcrStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  ocrLanguages,
  ocrReadinessBusy,
  ocrReadinessDetail,
  ocrReady,
  onLanguageChange,
  qpdfAvailable,
  selectedLanguage
}: SearchableOcrStudioProps) {
  const { formatNumber, t } = useI18n();
  const pdfFilter = useMemo(
    () => [{ name: t("ocr.filter.pdfDocuments"), extensions: ["pdf"] }],
    [t]
  );
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [straighten, setStraighten] = useState(true);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [result, setResult] = useState<SearchableOcrResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [cancelBusy, setCancelBusy] = useState(false);
  const ocrJob = usePdfJob<SearchableOcrResult>(desktopMode, "searchable-ocr");
  const safetySources = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "searchable-ocr-source",
              label: fileNameFromPath(sourcePath),
              password,
              path: sourcePath
            }
          ]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(
    desktopMode,
    safetySources,
    "searchable-ocr"
  );
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const languageAvailable = ocrLanguages.some(
    (language) => language.code === selectedLanguage
  );
  const canRun = Boolean(
    desktopMode &&
      sourcePath &&
      ocrReady &&
      languageAvailable &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      !ocrJob.isActive
  );

  useEffect(() => {
    setSourcePath(initialSourcePath ?? null);
    setPassword(initialSourcePassword ?? "");
    setSignatureRiskAcknowledged(false);
    setResult(null);
    setError(null);
    setNotice(null);
    ocrJob.clearJob();
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    const job = ocrJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setResult(job.result);
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      setError(null);
      setNotice(null);
    } else if (job.status === "cancelled") {
      setResult(null);
      setError(null);
      setNotice(
        t("ocr.cancelled")
      );
    } else if (job.status === "failed") {
      setResult(null);
      setNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [ocrJob.job?.jobId, ocrJob.job?.status, t]);

  const clearOutcome = () => {
    setResult(null);
    setError(null);
    setNotice(null);
    ocrJob.clearJob();
  };

  const chooseSource = async () => {
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: pdfFilter,
        multiple: false,
        title: t("ocr.dialog.chooseSource")
      });
      if (typeof selected === "string") {
        setSourcePath(selected);
        setPassword("");
        setSignatureRiskAcknowledged(false);
        clearOutcome();
      }
    } catch (reason) {
      void reason;
      setError(t("ocr.error.chooseSource"));
    }
  };

  const runOcr = async () => {
    if (!canRun || !sourcePath) {
      return;
    }
    setResult(null);
    setError(null);
    setNotice(null);
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath),
        filters: pdfFilter,
        title: t("ocr.dialog.save")
      });
      if (typeof outputPath !== "string") {
        return;
      }
      await ocrJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        inputPassword: password || null,
        inputPath: sourcePath,
        language: selectedLanguage,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        straighten
      });
    } catch (reason) {
      void reason;
      setError(t("ocr.error.failed"));
    }
  };

  const cancelOcr = async () => {
    if (!ocrJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await ocrJob.cancelJob();
    } catch (reason) {
      setCancelBusy(false);
      void reason;
      setError(t("ocr.error.cancel"));
    }
  };

  return (
    <section className="compression-studio searchable-ocr-studio">
      <div className="compression-heading">
        <div>
          <h3>{t("ocr.heading.title")}</h3>
          <p>{t("ocr.heading.description")}</p>
        </div>
        <FileSearch size={18} aria-hidden="true" />
      </div>

      <button
        className="wide-button"
        disabled={!desktopMode || ocrJob.isActive}
        onClick={chooseSource}
        type="button"
      >
        <FolderOpen size={17} aria-hidden="true" />
        {sourcePath ? t("ocr.source.chooseAnother") : t("ocr.source.choose")}
      </button>

      {sourcePath ? (
        <div className="compression-source">
          <FileText size={17} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(sourcePath)}</strong>
            <small title={sourcePath}>{sourcePath}</small>
          </span>
        </div>
      ) : null}

      <div
        className={`compression-status ${ocrReady ? "is-success" : "is-info"}`}
        role="status"
      >
        {ocrReadinessBusy ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : ocrReady ? (
          <CheckCircle2 size={17} aria-hidden="true" />
        ) : (
          <AlertCircle size={17} aria-hidden="true" />
        )}
        <span>
          <strong>
            {ocrReadinessBusy
              ? t("ocr.engine.checkingTitle")
              : ocrReady
                ? t("ocr.engine.readyTitle")
                : t("ocr.engine.requiredTitle")}
          </strong>
          <small>
            {ocrReadinessBusy
              ? t("ocr.engine.checkingDetail")
              : ocrReadinessDetail ||
                t("ocr.engine.requiredDetail")}
          </small>
        </span>
      </div>

      <label className="assembly-field searchable-ocr-language">
        <span>
          <Languages size={15} aria-hidden="true" />
          {t("ocr.language.label")}
        </span>
        <select
          disabled={ocrJob.isActive || ocrReadinessBusy || ocrLanguages.length === 0}
          onChange={(event) => {
            onLanguageChange(event.target.value);
            clearOutcome();
          }}
          value={languageAvailable ? selectedLanguage : ""}
        >
          {ocrLanguages.length === 0 ? (
            <option value="">{t("ocr.language.none")}</option>
          ) : (
            ocrLanguages.map((language) => (
              <option key={language.code} value={language.code}>
                {localiseOcrLanguage(language.code, language.name, t)} ({language.code})
              </option>
            ))
          )}
        </select>
      </label>

      <label className="searchable-ocr-option">
        <input
          checked={straighten}
          disabled={ocrJob.isActive}
          onChange={(event) => {
            setStraighten(event.target.checked);
            clearOutcome();
          }}
          type="checkbox"
        />
        <span>
          <strong>{t("ocr.deskew.title")}</strong>
          <small>{t("ocr.deskew.help")}</small>
        </span>
      </label>

      <label className="assembly-field">
        {t("ocr.sourcePassword")}
        <input
          autoComplete="current-password"
          disabled={ocrJob.isActive}
          onChange={(event) => {
            setPassword(event.target.value);
            clearOutcome();
          }}
          spellCheck={false}
          type={showPassword ? "text" : "password"}
          value={password}
        />
      </label>
      <button
        className="show-passwords"
        disabled={ocrJob.isActive}
        onClick={() => setShowPassword((value) => !value)}
        type="button"
      >
        {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
        {showPassword ? t("common.hidePasswords") : t("common.showPasswords")}
      </button>

      <PdfEditSafetyNotice
        acknowledged={signatureRiskAcknowledged}
        busy={ocrJob.isActive}
        editSafety={editSafety}
        onAcknowledgedChange={setSignatureRiskAcknowledged}
        rewriteDescription={
          t("ocr.rewriteDescription")
        }
      />

      <OutputProtectionFields
        disabled={ocrJob.isActive}
        onChange={(value) => {
          setOutputProtection(value);
          clearOutcome();
        }}
        qpdfAvailable={qpdfAvailable}
        value={outputProtection}
      />

      <button
        className="primary wide-button"
        disabled={!canRun}
        onClick={runOcr}
        title={!ocrReady ? t("ocr.action.installTitle") : undefined}
        type="button"
      >
        {ocrJob.isActive ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : (
          <Download size={17} aria-hidden="true" />
        )}
        {ocrJob.isActive ? t("ocr.action.recognising") : t("ocr.action.run")}
      </button>

      {ocrJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={ocrJob.connectionError}
          job={ocrJob.job}
          onCancel={cancelOcr}
          onRetry={() => void runOcr()}
          retryDisabled={!canRun}
        />
      ) : null}

      {error ? (
        <div className="compression-status is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {notice ? (
        <div className="compression-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{notice}</span>
        </div>
      ) : null}

      {!ocrJob.isActive && ocrJob.connectionError ? (
        <div className="compression-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {result ? (
        <div className="compression-export-result" aria-live="polite">
          <div className="compression-status is-success">
            <CheckCircle2 size={17} aria-hidden="true" />
            <span>
              <strong>
                {t(
                  result.pageCount === 1
                    ? "ocr.result.summary.one"
                    : "ocr.result.summary.other",
                  { count: formatNumber(result.pageCount) }
                )}
              </strong>
              <small title={result.outputPath}>
                {fileNameFromPath(result.outputPath)} | {formatFileSize(result.bytesWritten, formatNumber)} | {result.encryption === "None" ? t("common.none") : result.encryption}
              </small>
            </span>
          </div>
          <div className="compression-counts" aria-label={t("ocr.result.aria")}>
            <span>
              <strong>
                {t(
                  result.searchableTextPages === 1
                    ? "ocr.result.searchable.one"
                    : "ocr.result.searchable.other",
                  { count: formatNumber(result.searchableTextPages) }
                )}
              </strong>
            </span>
            <span>
              <strong>
                {t(
                  result.pagesWithoutSearchableText === 1
                    ? "ocr.result.review.one"
                    : "ocr.result.review.other",
                  { count: formatNumber(result.pagesWithoutSearchableText) }
                )}
              </strong>
            </span>
          </div>
          {localiseSearchableOcrWarnings(result.warnings, t).map((warning) => (
            <div className="compression-warning" key={warning}>
              <AlertTriangle size={16} aria-hidden="true" />
              <span>{warning}</span>
            </div>
          ))}
        </div>
      ) : null}

      <p className="compression-note">
        {t("ocr.note")}
      </p>
    </section>
  );
}

function suggestedOutputPath(path: string) {
  return /\.pdf$/i.test(path)
    ? path.replace(/\.pdf$/i, "-searchable.pdf")
    : `${path}-searchable.pdf`;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
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
