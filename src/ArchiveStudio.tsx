import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  Archive,
  CheckCircle2,
  Eye,
  EyeOff,
  FileCheck2,
  FileSearch,
  FolderOpen,
  Loader2,
  RefreshCw,
  ShieldCheck,
  XCircle
} from "lucide-react";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { useI18n } from "./I18nProvider";
import {
  localiseArchiveOutcome,
  localiseArchiveProfileDescription,
  localiseArchiveRule,
  localiseArchiveScope,
  localiseArchiveValidator,
  localiseArchiveWarnings
} from "./archiveLocalisation";
import type { TranslationKey } from "./i18n";
import { localisePdfJobFailure } from "./pdfJobs";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

export type PdfArchiveEngineStatus = {
  available: boolean;
  command: string;
  detail?: string | null;
  name: string;
  version?: string | null;
};

export type PdfArchiveReadiness = {
  conversionReady: boolean;
  detail: string;
  formalValidationReady: boolean;
  ghostscript: PdfArchiveEngineStatus;
  ocrMyPdf: PdfArchiveEngineStatus;
  ready: boolean;
  veraPdf: PdfArchiveEngineStatus;
};

type PdfArchiveMode = "convert" | "validate";
type PdfConformanceProfile =
  | "pdfa-1b"
  | "pdfa-2b"
  | "pdfa-3b"
  | "pdfua-1"
  | "pdfua-2"
  | "pdfx-1a-2001"
  | "pdfx-3-2002"
  | "pdfx-4";
type PdfConformanceAssessment = "independent-validation" | "structural-preflight";
type PdfConformanceOutcome =
  | "conforms"
  | "does-not-conform"
  | "preflight-passed"
  | "preflight-failed";

type PdfArchiveRuleFailure = {
  clause?: string | null;
  description?: string | null;
  failedChecks: number;
  specification?: string | null;
  testNumber?: string | null;
};

type PdfArchiveValidationReport = {
  assessment: PdfConformanceAssessment;
  failedChecks: number;
  failedRuleSummaries: PdfArchiveRuleFailure[];
  failedRules: number;
  outcome: PdfConformanceOutcome;
  passed: boolean;
  passedChecks: number;
  passedRules: number;
  profile: PdfConformanceProfile;
  profileName: string;
  rulesTruncated: boolean;
  scopeNote: string;
  validatorName: string;
  validatorVersion?: string | null;
};

type PdfArchiveResult = {
  bytesWritten: number;
  mode: PdfArchiveMode;
  outputPath?: string | null;
  pageCount: number;
  profile: PdfConformanceProfile;
  report: PdfArchiveValidationReport;
  searchableTextPages: number;
  sourceSize: number;
  warnings: string[];
};

type ArchiveStudioProps = {
  archiveReadiness: PdfArchiveReadiness | null;
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  ocrEngineAvailable: boolean;
  ocrLanguages: ReadonlyArray<{ code: string; name: string }>;
  onRefreshReadiness: () => Promise<void>;
  qpdfAvailable: boolean;
  readinessBusy: boolean;
};

type ProfileOption = {
  assessment: PdfConformanceAssessment;
  convertible: boolean;
  family: "PDF/A" | "PDF/UA" | "PDF/X";
  label: string;
  value: PdfConformanceProfile;
};

const profiles: ProfileOption[] = [
  {
    assessment: "independent-validation",
    convertible: true,
    family: "PDF/A",
    label: "PDF/A-1b",
    value: "pdfa-1b"
  },
  {
    assessment: "independent-validation",
    convertible: true,
    family: "PDF/A",
    label: "PDF/A-2b",
    value: "pdfa-2b"
  },
  {
    assessment: "independent-validation",
    convertible: true,
    family: "PDF/A",
    label: "PDF/A-3b",
    value: "pdfa-3b"
  },
  {
    assessment: "independent-validation",
    convertible: false,
    family: "PDF/UA",
    label: "PDF/UA-1",
    value: "pdfua-1"
  },
  {
    assessment: "independent-validation",
    convertible: false,
    family: "PDF/UA",
    label: "PDF/UA-2",
    value: "pdfua-2"
  },
  {
    assessment: "structural-preflight",
    convertible: false,
    family: "PDF/X",
    label: "PDF/X-1a:2001",
    value: "pdfx-1a-2001"
  },
  {
    assessment: "structural-preflight",
    convertible: false,
    family: "PDF/X",
    label: "PDF/X-3:2002",
    value: "pdfx-3-2002"
  },
  {
    assessment: "structural-preflight",
    convertible: false,
    family: "PDF/X",
    label: "PDF/X-4",
    value: "pdfx-4"
  }
];

const profileFamilies: ProfileOption["family"][] = ["PDF/A", "PDF/UA", "PDF/X"];

export function ArchiveStudio({
  archiveReadiness,
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  ocrEngineAvailable,
  ocrLanguages,
  onRefreshReadiness,
  qpdfAvailable,
  readinessBusy
}: ArchiveStudioProps) {
  const { formatNumber, t } = useI18n();
  const [mode, setMode] = useState<PdfArchiveMode>("convert");
  const [profile, setProfile] = useState<PdfConformanceProfile>("pdfa-2b");
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [recogniseText, setRecogniseText] = useState(false);
  const [ocrLanguage, setOcrLanguage] = useState("eng");
  const [straighten, setStraighten] = useState(true);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [notice, setNotice] = useState<TranslationKey | null>(null);
  const [error, setError] = useState<TranslationKey | null>(null);
  const [result, setResult] = useState<PdfArchiveResult | null>(null);
  const archiveJob = usePdfJob<PdfArchiveResult>(desktopMode, "archive");
  const busy = archiveJob.isActive;
  const selectedProfile = profiles.find((entry) => entry.value === profile) ?? profiles[1];
  const languageAvailable = ocrLanguages.some((language) => language.code === ocrLanguage);
  const safetySources = useMemo(
    () =>
      sourcePath
        ? [{ id: "archive-source", label: fileNameFromPath(sourcePath), password, path: sourcePath }]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(
    desktopMode,
    mode === "convert" ? safetySources : [],
    "archive"
  );
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const engineReady =
    mode === "convert"
      ? Boolean(archiveReadiness?.conversionReady ?? archiveReadiness?.ready)
      : selectedProfile.assessment === "structural-preflight"
        ? true
        : Boolean(archiveReadiness?.formalValidationReady ?? archiveReadiness?.veraPdf.available);
  const exactSourceReady = !(
    mode === "validate" && selectedProfile.family === "PDF/UA" && password
  );
  const protectedInputReady = !password || selectedProfile.family === "PDF/UA" || qpdfAvailable;
  const recognitionReady =
    mode !== "convert" || !recogniseText || (ocrEngineAvailable && languageAvailable);
  const canStart = Boolean(
    desktopMode &&
      sourcePath &&
      engineReady &&
      exactSourceReady &&
      protectedInputReady &&
      recognitionReady &&
      (mode === "validate" || (editSafety.isReady && certificateRiskAccepted)) &&
      !readinessBusy &&
      !busy
  );
  const jobFailure =
    archiveJob.job?.status === "failed"
      ? localisePdfJobFailure(archiveJob.job, t)
      : null;
  const requiredEngines: Array<{
    fallbackName: string;
    status: PdfArchiveEngineStatus | null | undefined;
  }> =
    mode === "convert"
      ? [
          { fallbackName: "OCRmyPDF", status: archiveReadiness?.ocrMyPdf },
          { fallbackName: "Ghostscript", status: archiveReadiness?.ghostscript },
          { fallbackName: "veraPDF", status: archiveReadiness?.veraPdf }
        ]
      : selectedProfile.assessment === "independent-validation"
        ? [{ fallbackName: "veraPDF", status: archiveReadiness?.veraPdf }]
        : [];

  useEffect(() => {
    if (initialSourcePath) {
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      setResult(null);
      setError(null);
      setNotice(null);
      archiveJob.clearJob();
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [sourcePath, password]);

  useEffect(() => {
    const job = archiveJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setResult(job.result);
      setError(null);
      setNotice(
        job.result.mode === "convert"
          ? "archive.notice.converted"
          : job.result.report.assessment === "structural-preflight"
            ? "archive.notice.preflightReady"
            : "archive.notice.validationReady"
      );
    } else if (job.status === "cancelled") {
      setResult(null);
      setError(null);
      setNotice("archive.notice.cancelled");
    } else if (job.status === "failed") {
      setResult(null);
      setNotice(null);
      setError(null);
    }
  }, [archiveJob.job?.jobId, archiveJob.job?.status]);

  const chooseSource = async () => {
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("archive.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("archive.dialog.choose")
      });
      if (typeof selected === "string") {
        setSourcePath(selected);
        setPassword("");
        setResult(null);
        setNotice(null);
        archiveJob.clearJob();
      }
    } catch (reason) {
      void reason;
      setError("archive.error.choose");
    }
  };

  const startArchiveJob = async () => {
    if (!canStart || !sourcePath) {
      return;
    }
    setError(null);
    setNotice(null);
    setResult(null);
    try {
      let outputPath: string | null = null;
      if (mode === "convert") {
        outputPath = await save({
          defaultPath: suggestedArchiveName(sourcePath, profile),
          filters: [{ name: t("archive.dialog.filter"), extensions: ["pdf"] }],
          title: t("archive.dialog.save", { profile: profileLabel(profile) })
        });
        if (!outputPath) {
          return;
        }
      }
      await archiveJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        inputPassword: password || null,
        inputPath: sourcePath,
        mode,
        ocrLanguage,
        outputPath,
        profile,
        recogniseText: mode === "convert" && recogniseText,
        straighten: mode === "convert" && recogniseText && straighten
      });
    } catch (reason) {
      void reason;
      setError("archive.error.start");
    }
  };

  const cancelArchiveJob = async () => {
    if (!archiveJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await archiveJob.cancelJob();
    } catch (reason) {
      void reason;
      setCancelBusy(false);
      setError("archive.error.cancel");
    }
  };

  const selectMode = (nextMode: PdfArchiveMode) => {
    if (busy || nextMode === mode) {
      return;
    }
    setMode(nextMode);
    if (nextMode === "convert" && !selectedProfile.convertible) {
      setProfile("pdfa-2b");
    }
    setResult(null);
    setError(null);
    setNotice(null);
    archiveJob.clearJob();
  };

  return (
    <section className="archive-studio">
      <div className="archive-heading">
        <div>
          <h3>{t("archive.heading.title")}</h3>
          <p>{t("archive.heading.description")}</p>
        </div>
        <Archive size={19} aria-hidden="true" />
      </div>

      <div className="archive-mode" role="group" aria-label={t("archive.mode.aria")}>
        <button
          className={mode === "convert" ? "is-active" : ""}
          disabled={busy}
          onClick={() => selectMode("convert")}
          type="button"
        >
          <FileCheck2 size={16} aria-hidden="true" />
          {t("archive.mode.create")}
        </button>
        <button
          className={mode === "validate" ? "is-active" : ""}
          disabled={busy}
          onClick={() => selectMode("validate")}
          type="button"
        >
          <FileSearch size={16} aria-hidden="true" />
          {t("archive.mode.check")}
        </button>
      </div>

      <div className="archive-source">
        <FileSearch size={18} aria-hidden="true" />
        <span>
          <strong>
            {sourcePath ? fileNameFromPath(sourcePath) : t("archive.source.none")}
          </strong>
          <small title={sourcePath ?? undefined}>
            {sourcePath ? t("archive.source.local") : t("archive.source.help")}
          </small>
        </span>
        <button disabled={!desktopMode || busy} onClick={chooseSource} type="button">
          <FolderOpen size={16} aria-hidden="true" />
          {t("archive.source.choose")}
        </button>
      </div>

      {sourcePath ? (
        <label className="archive-password">
          <span>{t("archive.password.label")}</span>
          <span className="archive-password-control">
            <input
              autoComplete="off"
              disabled={busy}
              onChange={(event) => setPassword(event.target.value)}
              placeholder={t("archive.password.placeholder")}
              type={showPassword ? "text" : "password"}
              value={password}
            />
            <button
              aria-label={
                showPassword
                  ? t("archive.password.hide")
                  : t("archive.password.show")
              }
              disabled={busy}
              onClick={() => setShowPassword((current) => !current)}
              title={showPassword ? t("common.hidePassword") : t("common.showPassword")}
              type="button"
            >
              {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
            </button>
          </span>
        </label>
      ) : null}

      <fieldset className="archive-profile" disabled={busy}>
        <legend>
          {mode === "convert"
            ? t("archive.profile.legend.convert")
            : t("archive.profile.legend.check")}
        </legend>
        <label className="archive-profile-control">
          <span>{t("archive.profile.standard")}</span>
          <select
            onChange={(event) => {
              setProfile(event.target.value as PdfConformanceProfile);
              setResult(null);
              setError(null);
              setNotice(null);
              archiveJob.clearJob();
            }}
            value={profile}
          >
            {profileFamilies.map((family) => {
              const familyProfiles = profiles.filter(
                (entry) => entry.family === family && (mode === "validate" || entry.convertible)
              );
              return familyProfiles.length > 0 ? (
                <optgroup key={family} label={family}>
                  {familyProfiles.map((entry) => (
                    <option key={entry.value} value={entry.value}>
                      {entry.label}
                    </option>
                  ))}
                </optgroup>
              ) : null;
            })}
          </select>
        </label>
        <div className="archive-profile-detail">
          <span>
            <strong>{selectedProfile.label}</strong>
            <small>{localiseArchiveProfileDescription(selectedProfile.value, t)}</small>
          </span>
          <em>
            {selectedProfile.assessment === "independent-validation"
              ? t("archive.profile.assessment.independent")
              : t("archive.profile.assessment.preflight")}
          </em>
        </div>
      </fieldset>

      {mode === "convert" ? (
        <fieldset className="archive-ocr" disabled={busy}>
          <legend>{t("archive.ocr.title")}</legend>
          <label>
            <input
              checked={recogniseText}
              onChange={(event) => setRecogniseText(event.target.checked)}
              type="checkbox"
            />
            <span>
              <strong>{t("archive.ocr.recognise")}</strong>
              <small>{t("archive.ocr.help")}</small>
            </span>
          </label>
          {recogniseText ? (
            <div className="archive-ocr-controls">
              <label>
                <span>{t("archive.ocr.language")}</span>
                <select
                  disabled={ocrLanguages.length === 0 || busy}
                  onChange={(event) => setOcrLanguage(event.target.value)}
                  value={ocrLanguage}
                >
                  {ocrLanguages.length === 0 ? (
                    <option value="eng">{t("archive.ocr.languagesMissing")}</option>
                  ) : (
                    ocrLanguages.map((language) => (
                      <option key={language.code} value={language.code}>
                        {language.name} ({language.code})
                      </option>
                    ))
                  )}
                </select>
              </label>
              <label>
                <input
                  checked={straighten}
                  onChange={(event) => setStraighten(event.target.checked)}
                  type="checkbox"
                />
                {t("archive.ocr.deskew")}
              </label>
            </div>
          ) : null}
        </fieldset>
      ) : null}

      {requiredEngines.length > 0 ? (
        <div
          className={`archive-engines has-${requiredEngines.length}`}
          aria-label={t("archive.engine.aria")}
        >
          {requiredEngines.map(({ fallbackName, status: engine }) => (
            <div
              className={engine?.available ? "is-ready" : "is-missing"}
              key={engine?.command ?? fallbackName}
            >
              {engine?.available ? (
                <CheckCircle2 size={16} aria-hidden="true" />
              ) : (
                <AlertCircle size={16} aria-hidden="true" />
              )}
              <span>
                <strong>{fallbackName}</strong>
                <small>
                  {engine?.available ? t("app.engine.ready") : t("app.engine.missing")}
                </small>
              </span>
            </div>
          ))}
          <button
            aria-label={t("archive.engine.refreshAria")}
            className="icon-button"
            disabled={!desktopMode || readinessBusy || busy}
            onClick={() => void onRefreshReadiness()}
            title={t("archive.engine.refreshTitle")}
            type="button"
          >
            <RefreshCw className={readinessBusy ? "spin" : ""} size={16} aria-hidden="true" />
          </button>
        </div>
      ) : (
        <div className="archive-message is-info" role="status">
          <ShieldCheck size={17} aria-hidden="true" />
          <span>{t("archive.engine.structuralBuiltIn")}</span>
        </div>
      )}

      {!engineReady ? (
        <div className="archive-message is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>
            {mode === "validate"
              ? t("archive.error.validationUnavailable")
              : !archiveReadiness
                ? t("archive.error.readiness")
                : !archiveReadiness.ocrMyPdf.available
                ? t("archive.error.ocrMyPdfUnavailable")
                : !archiveReadiness.ghostscript.available
                  ? t("archive.error.ghostscriptUnavailable")
                  : !archiveReadiness.veraPdf.available
                    ? t("archive.error.veraPdfUnavailable")
                    : t("archive.error.readiness")}
          </span>
        </div>
      ) : null}

      {!exactSourceReady ? (
        <div className="archive-message is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("archive.error.sourceExact")}</span>
        </div>
      ) : null}

      {!protectedInputReady ? (
        <div className="archive-message is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("archive.error.protectedInput")}</span>
        </div>
      ) : null}

      {!recognitionReady ? (
        <div className="archive-message is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("archive.error.ocrUnavailable")}</span>
        </div>
      ) : null}

      {mode === "convert" ? (
        <PdfEditSafetyNotice
          acknowledged={signatureRiskAcknowledged}
          busy={busy}
          editSafety={editSafety}
          onAcknowledgedChange={setSignatureRiskAcknowledged}
          rewriteDescription={t("archive.signature.rewrite")}
        />
      ) : null}

      <div className="archive-actions">
        <button className="primary-action" disabled={!canStart} onClick={() => void startArchiveJob()} type="button">
          {busy ? <Loader2 className="spin" size={16} aria-hidden="true" /> : mode === "convert" ? <Archive size={16} aria-hidden="true" /> : <ShieldCheck size={16} aria-hidden="true" />}
          {mode === "convert"
            ? t("archive.action.create", { profile: selectedProfile.label })
            : selectedProfile.assessment === "structural-preflight"
              ? t("archive.action.preflight", { profile: selectedProfile.label })
              : t("archive.action.validate", { profile: selectedProfile.label })}
        </button>
      </div>

      {archiveJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={archiveJob.connectionError}
          job={archiveJob.job}
          onCancel={() => void cancelArchiveJob()}
        />
      ) : null}

      {notice ? (
        <div className="archive-message is-info" role="status">
          <CheckCircle2 size={17} aria-hidden="true" />
          <span>{t(notice)}</span>
        </div>
      ) : null}

      {error || jobFailure ? (
        <div className="archive-message is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{error ? t(error) : jobFailure}</span>
        </div>
      ) : null}

      {result ? <ArchiveReport result={result} /> : null}

      <p className="archive-note">
        <AlertTriangle size={15} aria-hidden="true" />
        {mode === "convert"
          ? t("archive.note.convert")
          : selectedProfile.family === "PDF/UA"
            ? t("archive.note.pdfua")
            : selectedProfile.family === "PDF/X"
              ? t("archive.note.pdfx")
              : t("archive.note.pdfa")}
      </p>
    </section>
  );
}

function ArchiveReport({ result }: { result: PdfArchiveResult }) {
  const { formatNumber, t } = useI18n();
  const report = result.report;
  const preflight = report.assessment === "structural-preflight";
  const localisedWarnings = localiseArchiveWarnings(result.warnings, t);
  return (
    <section className="archive-report" aria-live="polite">
      <div className={report.passed ? "archive-verdict is-compliant" : "archive-verdict is-noncompliant"}>
        {report.passed ? <CheckCircle2 size={21} aria-hidden="true" /> : <XCircle size={21} aria-hidden="true" />}
        <span>
          <strong>{localiseArchiveOutcome(report.outcome, t)}</strong>
          <small>{profileLabel(report.profile)}</small>
        </span>
      </div>
      <div className="archive-counts">
        <span><strong>{formatNumber(report.passedRules)}</strong><small>{t("archive.report.rulesPassed")}</small></span>
        <span><strong>{formatNumber(report.failedRules)}</strong><small>{t("archive.report.rulesFailed")}</small></span>
        <span><strong>{formatNumber(report.passedChecks)}</strong><small>{t("archive.report.checksPassed")}</small></span>
        <span><strong>{formatNumber(report.failedChecks)}</strong><small>{t("archive.report.checksFailed")}</small></span>
        <span><strong>{formatNumber(result.searchableTextPages)}/{formatNumber(result.pageCount)}</strong><small>{t("archive.report.pagesSearchable")}</small></span>
      </div>
      {result.outputPath ? (
        <div className="archive-output">
          <FileCheck2 size={17} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(result.outputPath)}</strong>
            <small>
              {t(
                result.pageCount === 1
                  ? "archive.report.output.one"
                  : "archive.report.output.other",
                {
                  count: formatNumber(result.pageCount),
                  size: formatFileSize(result.bytesWritten, formatNumber)
                }
              )}
            </small>
          </span>
        </div>
      ) : null}
      {report.failedRuleSummaries.length > 0 ? (
        <div className="archive-failures">
          <strong>
            {preflight
              ? t("archive.report.failedPreflight")
              : t("archive.report.failedConformance")}
          </strong>
          <ul>
            {report.failedRuleSummaries.map((failure, index) => (
              <li key={`${failure.specification}-${failure.clause}-${failure.testNumber}-${index}`}>
                <span>
                  <strong>{localiseArchiveRule(failure, preflight, t)}</strong>
                </span>
                <em>
                  {t(
                    failure.failedChecks === 1
                      ? "archive.report.failureCount.one"
                      : "archive.report.failureCount.other",
                    { count: formatNumber(failure.failedChecks) }
                  )}
                </em>
              </li>
            ))}
          </ul>
          {report.rulesTruncated ? <small>{t("archive.report.moreOmitted")}</small> : null}
        </div>
      ) : null}
      {localisedWarnings.length > 0 ? (
        <div className="archive-warnings">
          <strong>{t("archive.report.notes")}</strong>
          <ul>
            {localisedWarnings.map((warning, index) => (
              <li key={`${warning}-${index}`}>{warning}</li>
            ))}
          </ul>
        </div>
      ) : null}
      <p className="archive-scope">{localiseArchiveScope(report.assessment, t)}</p>
      <small className="archive-validator">
        {localiseArchiveValidator(report.assessment, report.validatorVersion, t)}
      </small>
    </section>
  );
}

function profileLabel(profile: PdfConformanceProfile) {
  return profiles.find((entry) => entry.value === profile)?.label ?? "PDF/A";
}

function suggestedArchiveName(path: string, profile: PdfConformanceProfile) {
  const fileName = fileNameFromPath(path);
  const stem = fileName.replace(/\.pdf$/iu, "");
  return `${stem}-${profile}.pdf`;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/u).pop() || "document.pdf";
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${formatNumber(bytes / (1024 * 1024), { maximumFractionDigits: 1 })} MB`;
  }
  return `${formatNumber(bytes / (1024 * 1024 * 1024), { maximumFractionDigits: 1 })} GB`;
}
