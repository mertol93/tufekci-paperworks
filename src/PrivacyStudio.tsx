import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Code,
  Database,
  Eye,
  EyeOff,
  FileArchive,
  FileText,
  FolderOpen,
  ImageOff,
  Layers3,
  Loader2,
  MessageSquare,
  Paperclip,
  ScanSearch,
  ShieldX,
  Tags,
  TextSearch
} from "lucide-react";
import { useI18n } from "./I18nProvider";
import type { Translate, TranslationKey } from "./i18n";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { localisePdfJobFailure } from "./pdfJobs";
import { OutputProtectionFields } from "./OutputProtectionFields";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

type PrivacyStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PrivacyOptions = {
  removeActiveContent: boolean;
  removeAnnotationsAndForms: boolean;
  removeAttachments: boolean;
  removeMetadata: boolean;
  removeThumbnails: boolean;
};

type PrivacyOptionKey = keyof PrivacyOptions;

type CleanPdfPrivacyResult = {
  activeContentStructuresRemoved: number;
  annotationStructuresRemoved: number;
  attachmentStructuresRemoved: number;
  bytesWritten: number;
  encryption: "AES-256" | "None";
  metadataStructuresRemoved: number;
  outputPath: string;
  pageCount: number;
  thumbnailStructuresRemoved: number;
  unreachableObjectsPruned: number;
  webCaptureStructuresRemoved: number;
  warnings: string[];
};

type PrivacyFindingSeverity = "danger" | "info" | "warning";
type PrivacyInspectionStatus = "clear" | "review" | "risk";

type PrivacyFinding = {
  cleanOption: PrivacyOptionKey | null;
  code: string;
  detail: string;
  pageNumber: number | null;
  severity: PrivacyFindingSeverity;
  title: string;
};

type PrivacyInspectionSummary = {
  activeContentStructures: number;
  annotationAndFormStructures: number;
  attachmentStructures: number;
  croppedContentRiskPages: number[];
  defaultHiddenOptionalContentGroups: number;
  embeddedSearchIndexes: number;
  hiddenAnnotationCount: number;
  hiddenAnnotationPages: number[];
  hiddenOptionalContentPages: number[];
  incompletePageInspections: number[];
  invisibleTextOperations: number;
  invisibleTextPages: number[];
  metadataStructures: number;
  nodeInspectionTruncated: boolean;
  optionalContentGroups: number;
  optionalContentPages: number[];
  privateExtensionStructures: number;
  thumbnailStructures: number;
  webCaptureStructures: number;
  zeroOpacityPages: number[];
};

type PdfPrivacyInspectionResult = {
  dangerCount: number;
  fileName: string;
  findings: PrivacyFinding[];
  infoCount: number;
  pageCount: number;
  pdfVersion: string;
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  status: PrivacyInspectionStatus;
  summary: PrivacyInspectionSummary;
  warningCount: number;
};

type PrivacyStatus =
  | { kind: "error"; text: string }
  | { kind: "info"; text: string }
  | { kind: "success"; result: CleanPdfPrivacyResult; text: string };

const defaultOptions: PrivacyOptions = {
  removeActiveContent: true,
  removeAnnotationsAndForms: false,
  removeAttachments: true,
  removeMetadata: true,
  removeThumbnails: true
};

const optionRows: Array<{
  descriptionKey: TranslationKey;
  icon: typeof Tags;
  key: PrivacyOptionKey;
  labelKey: TranslationKey;
  warning?: boolean;
}> = [
  {
    descriptionKey: "privacy.option.metadata.description",
    icon: Tags,
    key: "removeMetadata",
    labelKey: "privacy.option.metadata.label"
  },
  {
    descriptionKey: "privacy.option.active.description",
    icon: Code,
    key: "removeActiveContent",
    labelKey: "privacy.option.active.label"
  },
  {
    descriptionKey: "privacy.option.attachments.description",
    icon: Paperclip,
    key: "removeAttachments",
    labelKey: "privacy.option.attachments.label"
  },
  {
    descriptionKey: "privacy.option.annotations.description",
    icon: MessageSquare,
    key: "removeAnnotationsAndForms",
    labelKey: "privacy.option.annotations.label",
    warning: true
  },
  {
    descriptionKey: "privacy.option.thumbnails.description",
    icon: ImageOff,
    key: "removeThumbnails",
    labelKey: "privacy.option.thumbnails.label"
  }
];

export function PrivacyStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: PrivacyStudioProps) {
  const { formatNumber, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [options, setOptions] = useState<PrivacyOptions>(defaultOptions);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [inspectionBusy, setInspectionBusy] = useState(false);
  const [inspectionCancelBusy, setInspectionCancelBusy] = useState(false);
  const [cleanCancelBusy, setCleanCancelBusy] = useState(false);
  const [inspection, setInspection] = useState<PdfPrivacyInspectionResult | null>(null);
  const [status, setStatus] = useState<PrivacyStatus | null>(null);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const inspectionJob = usePdfJob<PdfPrivacyInspectionResult>(
    desktopMode,
    "privacy-inspection"
  );
  const privacyJob = usePdfJob<CleanPdfPrivacyResult>(desktopMode, "privacy");
  const inspectionOperationBusy = inspectionBusy || inspectionJob.isActive;
  const busy = inspectionBusy || inspectionJob.isActive || privacyJob.isActive;
  const selectedCount = useMemo(
    () => Object.values(options).filter(Boolean).length,
    [options]
  );
  const safetySources = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "privacy-source",
              label: fileNameFromPath(sourcePath),
              password,
              path: sourcePath
            }
          ]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, safetySources, "privacy");
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const canClean =
    desktopMode &&
    Boolean(sourcePath) &&
    selectedCount > 0 &&
    Boolean(inspection) &&
    !busy &&
    outputProtectionIsValid(outputProtection, qpdfAvailable) &&
    editSafety.isReady &&
    certificateRiskAccepted;
  const canInspect = desktopMode && Boolean(sourcePath) && !busy;
  const concealmentPageCount = useMemo(() => {
    if (!inspection) {
      return 0;
    }
    return new Set([
      ...inspection.summary.croppedContentRiskPages,
      ...inspection.summary.hiddenAnnotationPages,
      ...inspection.summary.hiddenOptionalContentPages,
      ...inspection.summary.invisibleTextPages,
      ...inspection.summary.zeroOpacityPages
    ]).size;
  }, [inspection]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [safetySources]);

  useEffect(() => {
    if (initialSourcePath) {
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      setInspection(null);
      setStatus(null);
      inspectionJob.clearJob();
      privacyJob.clearJob();
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    const job = inspectionJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setInspectionCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setInspection(job.result);
      setStatus(null);
    } else if (job.status === "cancelled") {
      setInspection(null);
      setStatus({
        kind: "info",
        text: t("privacy.inspection.cancelled")
      });
    } else if (job.status === "failed") {
      setInspection(null);
      setStatus({
        kind: "error",
        text: localisePdfJobFailure(job, t)
      });
    }
  }, [inspectionJob.job?.jobId, inspectionJob.job?.status, t]);

  useEffect(() => {
    const job = privacyJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCleanCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      setStatus({
        kind: "success",
        result: job.result,
        text: t(
          job.result.pageCount === 1
            ? "privacy.clean.success.one"
            : "privacy.clean.success.other",
          { count: formatNumber(job.result.pageCount) }
        )
      });
    } else if (job.status === "cancelled") {
      setStatus({
        kind: "info",
        text: t("privacy.clean.cancelled")
      });
    } else if (job.status === "failed") {
      setStatus({ kind: "error", text: localisePdfJobFailure(job, t) });
    }
  }, [formatNumber, privacyJob.job?.jobId, privacyJob.job?.status, t]);

  const chooseSource = async () => {
    setStatus(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("privacy.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("privacy.dialog.choose")
      });
      if (typeof selected === "string") {
        setSourcePath(selected);
        setPassword("");
        setInspection(null);
        inspectionJob.clearJob();
        privacyJob.clearJob();
      }
    } catch {
      setStatus({ kind: "error", text: t("privacy.error.choose") });
    }
  };

  const toggleOption = (key: PrivacyOptionKey) => {
    setOptions((current) => ({ ...current, [key]: !current[key] }));
    setStatus(null);
  };

  const selectOption = (key: PrivacyOptionKey) => {
    setOptions((current) => ({ ...current, [key]: true }));
    setStatus(null);
  };

  const inspectPdf = async () => {
    if (!canInspect || !sourcePath) {
      return;
    }
    setInspectionBusy(true);
    setInspection(null);
    setStatus(null);
    privacyJob.clearJob();
    try {
      await inspectionJob.startJob({
        inputPassword: password || null,
        inputPath: sourcePath
      });
    } catch {
      setStatus({ kind: "error", text: t("privacy.error.inspectStart") });
    } finally {
      setInspectionBusy(false);
    }
  };

  const cancelPrivacyInspection = async () => {
    if (!inspectionJob.isActive || inspectionCancelBusy) {
      return;
    }
    setInspectionCancelBusy(true);
    try {
      await inspectionJob.cancelJob();
    } catch {
      setInspectionCancelBusy(false);
      setStatus({ kind: "error", text: t("privacy.error.inspectCancel") });
    }
  };

  const cleanPdf = async () => {
    if (!canClean || !sourcePath) {
      return;
    }
    setStatus(null);
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath),
        filters: [{ name: t("privacy.dialog.filter"), extensions: ["pdf"] }],
        title: t("privacy.dialog.save")
      });
      if (typeof outputPath !== "string") {
        return;
      }
      await privacyJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        expectedSourceModifiedAtMs: inspection?.sourceModifiedAtMs ?? null,
        expectedSourceSize: inspection?.sourceSize ?? 0,
        inputPassword: password || null,
        inputPath: sourcePath,
        options,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable)
      });
    } catch {
      setStatus({ kind: "error", text: t("privacy.error.cleanStart") });
    }
  };

  const cancelPrivacyCleaning = async () => {
    if (!privacyJob.isActive || cleanCancelBusy) {
      return;
    }
    setCleanCancelBusy(true);
    try {
      await privacyJob.cancelJob();
    } catch {
      setCleanCancelBusy(false);
      setStatus({ kind: "error", text: t("privacy.error.cleanCancel") });
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    setStatus(null);
  };

  return (
    <section className="privacy-studio">
      <div className="privacy-heading">
        <div>
          <h3>{t("privacy.heading.title")}</h3>
          <p>{t("privacy.heading.description")}</p>
        </div>
        <ShieldX size={18} aria-hidden="true" />
      </div>

      <button className="wide-button" disabled={!desktopMode || busy} onClick={chooseSource} type="button">
        <FolderOpen size={17} aria-hidden="true" />
        {sourcePath ? t("privacy.action.chooseAnother") : t("privacy.action.choose")}
      </button>

      {sourcePath ? (
        <div className="privacy-source">
          <FileText size={17} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(sourcePath)}</strong>
            <small title={sourcePath}>{sourcePath}</small>
          </span>
        </div>
      ) : null}

      <label className="assembly-field">
        {t("privacy.password.label")}
        <input
          autoComplete="current-password"
          disabled={busy}
          onChange={(event) => {
            setPassword(event.target.value);
            setInspection(null);
            setStatus(null);
            inspectionJob.clearJob();
            privacyJob.clearJob();
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
        className="wide-button"
        disabled={!canInspect}
        onClick={inspectPdf}
        type="button"
      >
        {inspectionOperationBusy ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : (
          <ScanSearch size={17} aria-hidden="true" />
        )}
        {inspectionOperationBusy
          ? t("privacy.inspection.running")
          : inspection
            ? t("privacy.inspection.runAgain")
            : t("privacy.inspection.run")}
      </button>

      {inspectionJob.job ? (
        <PdfJobProgress
          cancelling={inspectionCancelBusy}
          connectionError={inspectionJob.connectionError}
          job={inspectionJob.job}
          onCancel={cancelPrivacyInspection}
          onRetry={() => void inspectPdf()}
          retryDisabled={!canInspect}
        />
      ) : null}

      {!inspectionJob.isActive && inspectionJob.connectionError ? (
        <div className="privacy-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {status?.kind === "error" ? (
        <div className="privacy-status is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{status.text}</span>
        </div>
      ) : null}

      {status?.kind === "info" ? (
        <div className="privacy-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{status.text}</span>
        </div>
      ) : null}

      {inspection ? (
        <section className={`privacy-inspection is-${inspection.status}`} aria-live="polite">
          <header>
            {inspection.status === "risk" ? (
              <ShieldX size={19} aria-hidden="true" />
            ) : inspection.status === "review" ? (
              <AlertTriangle size={19} aria-hidden="true" />
            ) : (
              <CheckCircle2 size={19} aria-hidden="true" />
            )}
            <span>
              <strong>{privacyStatusLabel(inspection.status, t)}</strong>
              <small>
                {t(
                  inspection.pageCount === 1
                    ? "privacy.inspection.summary.one"
                    : "privacy.inspection.summary.other",
                  {
                    count: formatNumber(inspection.pageCount),
                    size: formatFileSize(inspection.sourceSize, formatNumber),
                    version: inspection.pdfVersion
                  }
                )}
              </small>
            </span>
            <b>
              {t("privacy.inspection.counts", {
                risks: formatNumber(inspection.dangerCount),
                warnings: formatNumber(inspection.warningCount)
              })}
            </b>
          </header>

          <div
            className="privacy-inspection-stats"
            aria-label={t("privacy.inspection.stats.aria")}
          >
            <article>
              <Layers3 size={15} aria-hidden="true" />
              <span>
                <strong>{formatNumber(inspection.summary.optionalContentGroups)}</strong>
                <small>{t("privacy.inspection.stats.layers")}</small>
              </span>
            </article>
            <article>
              <TextSearch size={15} aria-hidden="true" />
              <span>
                <strong>{formatNumber(concealmentPageCount)}</strong>
                <small>{t("privacy.inspection.stats.pages")}</small>
              </span>
            </article>
            <article>
              <FileArchive size={15} aria-hidden="true" />
              <span>
                <strong>
                  {formatNumber(
                    inspection.summary.webCaptureStructures +
                      inspection.summary.embeddedSearchIndexes
                  )}
                </strong>
                <small>{t("privacy.inspection.stats.provenance")}</small>
              </span>
            </article>
            <article>
              <Database size={15} aria-hidden="true" />
              <span>
                <strong>{formatNumber(inspection.summary.privateExtensionStructures)}</strong>
                <small>{t("privacy.inspection.stats.extensions")}</small>
              </span>
            </article>
          </div>

          {inspection.findings.length === 0 ? (
            <div className="privacy-inspection-clear">
              <CheckCircle2 size={16} aria-hidden="true" />
              <span>{t("privacy.inspection.clear")}</span>
            </div>
          ) : (
            <ul className="privacy-inspection-findings">
              {inspection.findings.map((finding) => {
                const FindingIcon =
                  finding.severity === "danger"
                    ? ShieldX
                    : finding.severity === "warning"
                      ? AlertTriangle
                      : ScanSearch;
                const cleanOption = finding.cleanOption;
                const optionSelected = cleanOption ? options[cleanOption] : false;
                const localisedFinding = localisePrivacyFinding(finding, t);
                return (
                  <li className={`is-${finding.severity}`} key={finding.code}>
                    <FindingIcon size={16} aria-hidden="true" />
                    <span>
                      <strong>
                        {localisedFinding.title}
                        {finding.pageNumber ? (
                          <em>
                            {t("privacy.finding.page", {
                              page: formatNumber(finding.pageNumber)
                            })}
                          </em>
                        ) : null}
                      </strong>
                      <small>{localisedFinding.detail}</small>
                      {cleanOption ? (
                        <button
                          className="privacy-finding-option"
                          disabled={optionSelected || busy}
                          onClick={() => selectOption(cleanOption)}
                          type="button"
                        >
                          {optionSelected ? <CheckCircle2 size={13} aria-hidden="true" /> : null}
                          {optionSelected
                            ? t("privacy.finding.optionSelected", {
                                option: privacyOptionLabel(cleanOption, t)
                              })
                            : t("privacy.finding.selectOption", {
                                option: privacyOptionLabel(cleanOption, t)
                              })}
                        </button>
                      ) : null}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}

          <p>{t("privacy.inspection.note")}</p>
        </section>
      ) : null}

      <fieldset className="privacy-options" disabled={busy}>
        <legend>{t("privacy.options.legend")}</legend>
        {optionRows.map((option) => {
          const OptionIcon = option.icon;
          return (
            <label className={option.warning ? "privacy-option is-caution" : "privacy-option"} key={option.key}>
              <input
                checked={options[option.key]}
                onChange={() => toggleOption(option.key)}
                type="checkbox"
              />
              <span>
                <strong>{t(option.labelKey)}</strong>
                <small>{t(option.descriptionKey)}</small>
              </span>
              <OptionIcon size={17} aria-hidden="true" />
            </label>
          );
        })}
      </fieldset>

      {options.removeAnnotationsAndForms ? (
        <div className="privacy-caution">
          <AlertTriangle size={17} aria-hidden="true" />
          <span>{t("privacy.options.annotationsCaution")}</span>
        </div>
      ) : null}

      <OutputProtectionFields
        disabled={busy}
        onChange={changeOutputProtection}
        qpdfAvailable={qpdfAvailable}
        value={outputProtection}
      />

      <PdfEditSafetyNotice
        acknowledged={signatureRiskAcknowledged}
        busy={busy}
        editSafety={editSafety}
        onAcknowledgedChange={setSignatureRiskAcknowledged}
        rewriteDescription={t("privacy.rewriteDescription")}
      />

      <button
        className="primary wide-button"
        disabled={!canClean}
        onClick={cleanPdf}
        type="button"
      >
        {privacyJob.isActive ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <ShieldX size={17} aria-hidden="true" />}
        {privacyJob.isActive
          ? t("privacy.clean.running")
          : t("privacy.clean.run", { count: formatNumber(selectedCount) })}
      </button>

      {privacyJob.job ? (
        <PdfJobProgress
          cancelling={cleanCancelBusy}
          connectionError={privacyJob.connectionError}
          job={privacyJob.job}
          onCancel={cancelPrivacyCleaning}
          onRetry={() => void cleanPdf()}
          retryDisabled={!canClean}
        />
      ) : null}

      {!privacyJob.isActive && privacyJob.connectionError ? (
        <div className="privacy-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {status?.kind === "success" ? (
        <div className="privacy-result" aria-live="polite">
          <div className="privacy-status is-success">
            <CheckCircle2 size={17} aria-hidden="true" />
            <span>
              <strong>{status.text}</strong>
              <small title={status.result.outputPath}>
                {t("privacy.clean.details", {
                  encryption:
                    status.result.encryption === "None"
                      ? t("common.none")
                      : status.result.encryption,
                  name: fileNameFromPath(status.result.outputPath),
                  size: formatFileSize(status.result.bytesWritten, formatNumber)
                })}
              </small>
            </span>
          </div>
          <div className="privacy-counts" aria-label={t("privacy.clean.counts.aria")}>
            <span>
              <strong>{formatNumber(status.result.metadataStructuresRemoved)}</strong>{" "}
              {t("privacy.clean.counts.metadata")}
            </span>
            <span>
              <strong>{formatNumber(status.result.activeContentStructuresRemoved)}</strong>{" "}
              {t("privacy.clean.counts.active")}
            </span>
            <span>
              <strong>{formatNumber(status.result.attachmentStructuresRemoved)}</strong>{" "}
              {t("privacy.clean.counts.attachments")}
            </span>
            <span>
              <strong>{formatNumber(status.result.annotationStructuresRemoved)}</strong>{" "}
              {t("privacy.clean.counts.annotations")}
            </span>
            <span>
              <strong>{formatNumber(status.result.thumbnailStructuresRemoved)}</strong>{" "}
              {t("privacy.clean.counts.thumbnails")}
            </span>
            <span>
              <strong>{formatNumber(status.result.webCaptureStructuresRemoved)}</strong>{" "}
              {t("privacy.clean.counts.webCapture")}
            </span>
            <span>
              <strong>{formatNumber(status.result.unreachableObjectsPruned)}</strong>{" "}
              {t("privacy.clean.counts.pruned")}
            </span>
          </div>
          {localisePrivacyWarnings(status.result.warnings, t).map((warning) => (
            <div className="privacy-warning" key={warning}>
              <AlertTriangle size={16} aria-hidden="true" />
              <span>{warning}</span>
            </div>
          ))}
        </div>
      ) : null}

      <p className="privacy-note">{t("privacy.note")}</p>
    </section>
  );
}

function suggestedOutputPath(path: string) {
  return /\.pdf$/i.test(path) ? path.replace(/\.pdf$/i, "-clean.pdf") : `${path}-clean.pdf`;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function privacyStatusLabel(status: PrivacyInspectionStatus, t: Translate) {
  if (status === "risk") return t("privacy.status.risk");
  if (status === "review") return t("privacy.status.review");
  return t("privacy.status.clear");
}

function privacyOptionLabel(option: PrivacyOptionKey, t: Translate) {
  const keys: Record<PrivacyOptionKey, TranslationKey> = {
    removeActiveContent: "privacy.optionName.active",
    removeAnnotationsAndForms: "privacy.optionName.annotations",
    removeAttachments: "privacy.optionName.attachments",
    removeMetadata: "privacy.optionName.metadata",
    removeThumbnails: "privacy.optionName.thumbnails"
  };
  return t(keys[option]);
}

const privacyFindingTitleKeys: Readonly<Record<string, TranslationKey>> = {
  "active-content": "privacy.finding.title.activeContent",
  "annotations-and-forms": "privacy.finding.title.annotations",
  attachments: "privacy.finding.title.attachments",
  "cropped-content-risk": "privacy.finding.title.croppedContent",
  "embedded-search-indexes": "privacy.finding.title.searchIndexes",
  "hidden-annotations": "privacy.finding.title.hiddenAnnotations",
  "hidden-optional-content": "privacy.finding.title.hiddenLayers",
  "invisible-text": "privacy.finding.title.invisibleText",
  metadata: "privacy.finding.title.metadata",
  "node-inspection-limit": "privacy.finding.title.nodeLimit",
  "optional-content": "privacy.finding.title.layers",
  "optional-content-malformed": "privacy.finding.title.layersMalformed",
  "page-inspection-incomplete": "privacy.finding.title.pageIncomplete",
  "page-thumbnails": "privacy.finding.title.thumbnails",
  "private-extensions": "privacy.finding.title.privateExtensions",
  "web-capture-data": "privacy.finding.title.webCapture",
  "zero-opacity-content": "privacy.finding.title.zeroOpacity"
};

function localisePrivacyFinding(finding: PrivacyFinding, t: Translate) {
  return {
    detail: t(
      finding.cleanOption
        ? "privacy.finding.detail.cleanable"
        : "privacy.finding.detail.review"
    ),
    title: t(
      privacyFindingTitleKeys[finding.code] ?? "privacy.finding.title.generic"
    )
  };
}

function localisePrivacyWarnings(warnings: string[], t: Translate) {
  const keys: Readonly<Record<string, TranslationKey>> = {
    "Cleaning changes the PDF and invalidates any existing certificate signature.":
      "privacy.warning.certificate",
    "The cleaned copy is not password-protected. Use Protect to apply new encryption.":
      "privacy.warning.unprotected",
    "The cleaned copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
      "privacy.warning.protected"
  };
  return [
    ...new Set(
      warnings.map((warning) =>
        t(keys[warning] ?? "privacy.warning.generic")
      )
    )
  ];
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
