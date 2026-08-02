import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Download,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  Gauge,
  Image,
  Loader2
} from "lucide-react";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { OutputProtectionFields } from "./OutputProtectionFields";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";
import { useI18n } from "./I18nProvider";
import { type Translate } from "./i18n";
import { localisePdfJobFailure } from "./pdfJobs";

type CompressionStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PdfCompressionPreview = {
  canReduce: boolean;
  compatibleImageCount: number;
  compressedPreviewDataUrl?: string | null;
  estimatedBytes: number;
  fileName: string;
  imageCount: number;
  imagesRecompressed: number;
  jpegQuality: number;
  objectsPruned: number;
  originalBytes: number;
  pageCount: number;
  processingLimitReached: boolean;
  sampleCompressedBytes?: number | null;
  sampleHeight?: number | null;
  sampleOriginalBytes?: number | null;
  sampleWidth?: number | null;
  sampleWouldBeRecompressed: boolean;
  savingBytes: number;
  savingPercent: number;
  skippedImageCount: number;
  sourcePreviewDataUrl?: string | null;
  unchangedCompatibleImageCount: number;
  warnings: string[];
};

type ExportCompressedPdfResult = {
  bytesWritten: number;
  encryption: "AES-256" | "None";
  imagesRecompressed: number;
  originalBytes: number;
  outputPath: string;
  pageCount: number;
  savedBytes: number;
  savedPercent: number;
  skippedImageCount: number;
  warnings: string[];
};

export function CompressionStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: CompressionStudioProps) {
  const { formatNumber, locale, t } = useI18n();
  const pdfFilter = useMemo(
    () => [{ name: t("compression.filter.pdfDocuments"), extensions: ["pdf"] }],
    [t]
  );
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [quality, setQuality] = useState(78);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewCancelBusy, setPreviewCancelBusy] = useState(false);
  const [exportCancelBusy, setExportCancelBusy] = useState(false);
  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [preview, setPreview] = useState<PdfCompressionPreview | null>(null);
  const [exportResult, setExportResult] = useState<ExportCompressedPdfResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const previewJob = usePdfJob<PdfCompressionPreview>(desktopMode, "compression-preview");
  const compressionJob = usePdfJob<ExportCompressedPdfResult>(desktopMode, "compression");
  const previewOperationBusy = previewBusy || previewJob.isActive;
  const busy = previewBusy || previewJob.isActive || compressionJob.isActive;
  const safetySources = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "compression-source",
              label: fileNameFromPath(sourcePath),
              password,
              path: sourcePath
            }
          ]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, safetySources, "compression");
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const canPreview = desktopMode && Boolean(sourcePath) && !busy;
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      preview?.canReduce &&
      preview.jpegQuality === quality &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      !busy
  );

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [safetySources]);

  useEffect(() => {
    if (initialSourcePath) {
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      setPreview(null);
      setExportResult(null);
      setError(null);
      setJobNotice(null);
      previewJob.clearJob();
      compressionJob.clearJob();
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    const job = previewJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setPreviewCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setPreview(job.result);
      setExportResult(null);
      setJobNotice(null);
      setError(null);
    } else if (job.status === "cancelled") {
      setPreview(null);
      setExportResult(null);
      setJobNotice(t("compression.preview.cancelled"));
      setError(null);
    } else if (job.status === "failed") {
      setPreview(null);
      setExportResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [locale, previewJob.job?.jobId, previewJob.job?.status]);

  useEffect(() => {
    const job = compressionJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setExportCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      setExportResult(job.result);
      setJobNotice(null);
      setError(null);
    } else if (job.status === "cancelled") {
      setExportResult(null);
      setJobNotice(t("compression.export.cancelled"));
      setError(null);
    } else if (job.status === "failed") {
      setExportResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [compressionJob.job?.jobId, compressionJob.job?.status, locale]);

  const clearResults = () => {
    setPreview(null);
    setExportResult(null);
    setError(null);
    setJobNotice(null);
    previewJob.clearJob();
    compressionJob.clearJob();
  };

  const chooseSource = async () => {
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: pdfFilter,
        multiple: false,
        title: t("compression.dialog.sourceTitle")
      });
      if (typeof selected === "string") {
        setSourcePath(selected);
        setPassword("");
        setSignatureRiskAcknowledged(false);
        setPreview(null);
        setExportResult(null);
        setJobNotice(null);
        previewJob.clearJob();
        compressionJob.clearJob();
      }
    } catch (reason) {
      setError(t("compression.error.chooseSource"));
    }
  };

  const previewCompression = async () => {
    if (!canPreview || !sourcePath) {
      return;
    }
    setPreviewBusy(true);
    setPreview(null);
    setError(null);
    setExportResult(null);
    setJobNotice(null);
    compressionJob.clearJob();
    try {
      await previewJob.startJob({
        inputPassword: password || null,
        inputPath: sourcePath,
        jpegQuality: quality
      });
    } catch (reason) {
      setPreview(null);
      setError(t("compression.error.startPreview"));
    } finally {
      setPreviewBusy(false);
    }
  };

  const cancelPreview = async () => {
    if (!previewJob.isActive || previewCancelBusy) {
      return;
    }
    setPreviewCancelBusy(true);
    try {
      await previewJob.cancelJob();
    } catch (reason) {
      setPreviewCancelBusy(false);
      setError(t("compression.error.cancelPreview"));
    }
  };

  const exportCompressedPdf = async () => {
    if (!canExport || !sourcePath || !preview) {
      return;
    }
    setError(null);
    setJobNotice(null);
    setExportResult(null);
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath),
        filters: pdfFilter,
        title: t("compression.dialog.outputTitle")
      });
      if (typeof outputPath !== "string") {
        return;
      }
      await compressionJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        inputPassword: password || null,
        inputPath: sourcePath,
        jpegQuality: quality,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable)
      });
    } catch (reason) {
      setError(t("compression.error.startExport"));
    }
  };

  const cancelCompression = async () => {
    if (!compressionJob.isActive || exportCancelBusy) {
      return;
    }
    setExportCancelBusy(true);
    try {
      await compressionJob.cancelJob();
    } catch (reason) {
      setExportCancelBusy(false);
      setError(t("compression.error.cancelExport"));
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    setExportResult(null);
    setError(null);
    setJobNotice(null);
    compressionJob.clearJob();
  };

  return (
    <section className="compression-studio">
      <div className="compression-heading">
        <div>
          <h3>{t("compression.heading.title")}</h3>
          <p>{t("compression.heading.description")}</p>
        </div>
        <Gauge size={18} aria-hidden="true" />
      </div>

      <button
        className="wide-button"
        disabled={!desktopMode || busy}
        onClick={chooseSource}
        type="button"
      >
        <FolderOpen size={17} aria-hidden="true" />
        {sourcePath ? t("compression.source.chooseAnother") : t("compression.source.choose")}
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

      <label className="compression-quality">
        <span>
          <strong>{t("compression.quality.label")}</strong>
          <small>
            {qualityLabel(quality, t)} | {formatNumber(quality)}/100
          </small>
        </span>
        <input
          aria-label={t("compression.quality.aria")}
          disabled={busy}
          max="95"
          min="40"
          onChange={(event) => {
            setQuality(Number(event.target.value));
            clearResults();
          }}
          step="1"
          type="range"
          value={quality}
        />
        <span className="compression-quality-scale" aria-hidden="true">
          <small>{t("compression.quality.smaller")}</small>
          <small>{t("compression.quality.crisper")}</small>
        </span>
      </label>

      <label className="assembly-field">
        {t("compression.password.label")}
        <input
          autoComplete="current-password"
          disabled={busy}
          onChange={(event) => {
            setPassword(event.target.value);
            clearResults();
          }}
          spellCheck={false}
          type={showPassword ? "text" : "password"}
          value={password}
        />
      </label>
      <button className="show-passwords" onClick={() => setShowPassword((value) => !value)} type="button">
        {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
        {showPassword ? t("common.hidePassword") : t("common.showPassword")}
      </button>

      <button
        className="primary wide-button"
        disabled={!canPreview}
        onClick={previewCompression}
        type="button"
      >
        {previewOperationBusy ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : (
          <Gauge size={17} aria-hidden="true" />
        )}
        {previewOperationBusy
          ? t("compression.preview.running")
          : preview
            ? t("compression.preview.recalculate")
            : t("compression.preview.run")}
      </button>

      {previewJob.job ? (
        <PdfJobProgress
          cancelling={previewCancelBusy}
          connectionError={previewJob.connectionError}
          job={previewJob.job}
          onCancel={cancelPreview}
          onRetry={() => void previewCompression()}
          retryDisabled={!canPreview}
        />
      ) : null}

      {!previewJob.isActive && previewJob.connectionError ? (
        <div className="compression-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {error ? (
        <div className="compression-status is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {preview ? (
        <div className="compression-report" aria-live="polite">
          <div className={preview.canReduce ? "compression-saving is-ready" : "compression-saving is-neutral"}>
            {preview.canReduce ? (
              <CheckCircle2 size={20} aria-hidden="true" />
            ) : (
              <AlertCircle size={20} aria-hidden="true" />
            )}
            <span>
              <strong>
                {preview.canReduce
                  ? t("compression.preview.smaller", {
                      percent: formatPercentage(preview.savingPercent, formatNumber)
                    })
                  : t("compression.preview.noReduction")}
              </strong>
              <small>
                {formatFileSize(preview.originalBytes, formatNumber)}{" "}
                <ArrowRight size={12} aria-hidden="true" />{" "}
                {formatFileSize(preview.estimatedBytes, formatNumber)}
              </small>
            </span>
          </div>

          <div className="compression-counts" aria-label={t("compression.analysis.aria")}>
            <span>
              <strong>{formatNumber(preview.pageCount)}</strong>{" "}
              {t(preview.pageCount === 1 ? "compression.count.page.one" : "compression.count.page.other")}
            </span>
            <span>
              <strong>{formatNumber(preview.imagesRecompressed)}</strong>{" "}
              {t(
                preview.imagesRecompressed === 1
                  ? "compression.count.reduced.one"
                  : "compression.count.reduced.other"
              )}
            </span>
            <span>
              <strong>{formatNumber(preview.skippedImageCount)}</strong>{" "}
              {t(
                preview.skippedImageCount === 1
                  ? "compression.count.preserved.one"
                  : "compression.count.preserved.other"
              )}
            </span>
            <span>
              <strong>{formatNumber(preview.objectsPruned)}</strong>{" "}
              {t(
                preview.objectsPruned === 1
                  ? "compression.count.object.one"
                  : "compression.count.object.other"
              )}
            </span>
          </div>

          {preview.sourcePreviewDataUrl && preview.compressedPreviewDataUrl ? (
            <div className="compression-comparison">
              <div className="compression-comparison-heading">
                <Image size={16} aria-hidden="true" />
                <span>
                  <strong>{t("compression.sample.title")}</strong>
                  <small>
                    {t("compression.sample.dimensions", {
                      height: formatNumber(preview.sampleHeight ?? 0),
                      width: formatNumber(preview.sampleWidth ?? 0)
                    })}
                    {preview.sampleWouldBeRecompressed
                      ? ` | ${t("compression.sample.selected")}`
                      : ` | ${t("compression.sample.unchanged")}`}
                  </small>
                </span>
              </div>
              <div className="compression-images">
                <figure>
                  <img alt={t("compression.sample.sourceAlt")} src={preview.sourcePreviewDataUrl} />
                  <figcaption>
                    <strong>{t("compression.sample.source")}</strong>
                    <small>
                      {formatOptionalFileSize(preview.sampleOriginalBytes, formatNumber, t)}
                    </small>
                  </figcaption>
                </figure>
                <figure>
                  <img
                    alt={t("compression.sample.compressedAlt", {
                      quality: formatNumber(quality)
                    })}
                    src={preview.compressedPreviewDataUrl}
                  />
                  <figcaption>
                    <strong>
                      {t("compression.sample.quality", { quality: formatNumber(quality) })}
                    </strong>
                    <small>
                      {formatOptionalFileSize(preview.sampleCompressedBytes, formatNumber, t)}
                    </small>
                  </figcaption>
                </figure>
              </div>
              <p>{t("compression.sample.help")}</p>
            </div>
          ) : (
            <p className="compression-note">
              {t("compression.sample.unavailable")}
            </p>
          )}

          {localiseCompressionWarnings(preview.warnings, t, formatNumber).map((warning) => (
            <div className="compression-warning" key={warning}>
              <AlertTriangle size={16} aria-hidden="true" />
              <span>{warning}</span>
            </div>
          ))}
        </div>
      ) : null}

      <PdfEditSafetyNotice
        acknowledged={signatureRiskAcknowledged}
        busy={busy}
        editSafety={editSafety}
        onAcknowledgedChange={setSignatureRiskAcknowledged}
        rewriteDescription={t("compression.rewriteDescription")}
      />

      <OutputProtectionFields
        disabled={busy}
        onChange={changeOutputProtection}
        qpdfAvailable={qpdfAvailable}
        value={outputProtection}
      />

      <button
        className="wide-button"
        disabled={!canExport}
        onClick={exportCompressedPdf}
        title={preview && !preview.canReduce ? t("compression.export.lowerQualityTitle") : undefined}
        type="button"
      >
        {compressionJob.isActive ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : (
          <Download size={17} aria-hidden="true" />
        )}
        {compressionJob.isActive
          ? t("compression.export.running")
          : t("compression.export.run")}
      </button>

      {compressionJob.job ? (
        <PdfJobProgress
          cancelling={exportCancelBusy}
          connectionError={compressionJob.connectionError}
          job={compressionJob.job}
          onCancel={cancelCompression}
          onRetry={() => void exportCompressedPdf()}
          retryDisabled={!canExport}
        />
      ) : null}

      {jobNotice ? (
        <div className="compression-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{jobNotice}</span>
        </div>
      ) : null}

      {!compressionJob.isActive && compressionJob.connectionError ? (
        <div className="compression-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {exportResult ? (
        <div className="compression-export-result" aria-live="polite">
          <div className="compression-status is-success">
            <CheckCircle2 size={17} aria-hidden="true" />
            <span>
              <strong>
                {t(
                  exportResult.pageCount === 1
                    ? "compression.export.success.one"
                    : "compression.export.success.other",
                  { count: formatNumber(exportResult.pageCount) }
                )}
              </strong>
              <small title={exportResult.outputPath}>
                {t("compression.export.details", {
                  encryption:
                    exportResult.encryption === "None"
                      ? t("common.none")
                      : exportResult.encryption,
                  name: fileNameFromPath(exportResult.outputPath),
                  percent: formatPercentage(exportResult.savedPercent, formatNumber),
                  size: formatFileSize(exportResult.bytesWritten, formatNumber)
                })}
              </small>
            </span>
          </div>
          {localiseCompressionWarnings(exportResult.warnings, t, formatNumber).map((warning) => (
            <div className="compression-warning" key={warning}>
              <AlertTriangle size={16} aria-hidden="true" />
              <span>{warning}</span>
            </div>
          ))}
        </div>
      ) : null}

      <p className="compression-note">
        {t("compression.note")}
      </p>
    </section>
  );
}

function qualityLabel(quality: number, t: Translate) {
  if (quality >= 88) return t("compression.quality.crisp");
  if (quality >= 74) return t("compression.quality.balanced");
  if (quality >= 58) return t("compression.quality.compact");
  return t("compression.quality.smallest");
}

function suggestedOutputPath(path: string) {
  return /\.pdf$/i.test(path)
    ? path.replace(/\.pdf$/i, "-compressed.pdf")
    : `${path}-compressed.pdf`;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function formatOptionalFileSize(
  bytes: number | null | undefined,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string,
  t: Translate
) {
  return typeof bytes === "number"
    ? formatFileSize(bytes, formatNumber)
    : t("compression.size.unavailable");
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

function formatPercentage(
  value: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  return formatNumber(value / 100, {
    maximumFractionDigits: value >= 10 ? 0 : 1,
    style: "percent"
  });
}

function localiseCompressionWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
): string[] {
  const localised = warnings.map((warning) => {
    const recompressed = warning.match(
      /^(\d+) compatible raster images? will be recompressed with JPEG quality (\d+)\. Text, vectors, links, forms, and OCR text layers remain PDF content\.$/u
    );
    if (recompressed) {
      const count = Number(recompressed[1]);
      return t(
        count === 1
          ? "compression.warning.recompressed.one"
          : "compression.warning.recompressed.other",
        {
          count: formatNumber(count),
          quality: formatNumber(Number(recompressed[2]))
        }
      );
    }

    const preserved = warning.match(
      /^(\d+) images? use colour spaces, masks, filters, dimensions, or data that this preservation-first pass does not JPEG-recompress\. Lossless stream optimisation may still change their encoded representation\.$/u
    );
    if (preserved) {
      const count = Number(preserved[1]);
      return t(
        count === 1
          ? "compression.warning.preserved.one"
          : "compression.warning.preserved.other",
        { count: formatNumber(count) }
      );
    }

    const exactWarnings: Record<string, Parameters<Translate>[0]> = {
      "The bounded image-work limit was reached. Remaining images stay unchanged; split very large PDFs before compressing them more deeply.":
        "compression.warning.limit",
      "The compressed copy is not password-protected. Use Protect to apply new AES-256 encryption.":
        "compression.warning.unprotected",
      "Compression rewrites the PDF and invalidates any existing certificate signature.":
        "compression.warning.certificate",
      "Interactive form structures are preserved and checked, but their appearance should be reviewed in the compressed copy.":
        "compression.warning.forms",
      "The selected quality does not produce a smaller verified rewrite. Try a lower quality or keep the source.":
        "compression.warning.notSmaller",
      "The compressed copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "compression.warning.protected"
    };
    const key = exactWarnings[warning];
    return key ? t(key) : t("compression.warning.generic");
  });

  return [...new Set(localised)];
}
