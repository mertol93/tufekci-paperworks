import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  CheckCircle2,
  Eye,
  EyeOff,
  FolderOpen,
  Loader2,
  Redo2,
  Undo2,
  Scissors
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
import { useBoundedHistory } from "./useBoundedHistory";
import { usePdfJob } from "./usePdfJob";
import { useI18n } from "./I18nProvider";
import { type Translate } from "./i18n";
import { localisePdfJobFailure } from "./pdfJobs";
import {
  toRecoverySplitPlan,
  type RecoverySplitPlan
} from "./recovery";

type SplitStudioProps = {
  desktopMode: boolean;
  initialRecoveryPlan?: RecoverySplitPlan | null;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  onRecoveryPlanChange?: (plan: RecoverySplitPlan | null) => void;
  qpdfAvailable: boolean;
};

type SplitPdfResult = {
  encryption: "AES-256" | "None";
  outputs: Array<{
    bytesWritten: number;
    outputPath: string;
    pageCount: number;
  }>;
  totalPages: number;
  warnings: string[];
};

type SplitStatus = {
  kind: "error" | "info" | "success";
  text: string;
  outputs?: string[];
  warnings?: string[];
};

export function SplitStudio({
  desktopMode,
  initialRecoveryPlan,
  initialSourcePassword,
  initialSourcePath,
  onRecoveryPlanChange,
  qpdfAvailable
}: SplitStudioProps) {
  const { formatNumber, locale, t } = useI18n();
  const pdfFilter = useMemo(
    () => [{ name: t("split.filter.pdfDocuments"), extensions: ["pdf"] }],
    [t]
  );
  const [sourcePath, setSourcePath] = useState<string | null>(
    initialRecoveryPlan?.sourcePath ?? initialSourcePath ?? null
  );
  const [password, setPassword] = useState(
    !initialRecoveryPlan || initialRecoveryPlan.sourcePath === initialSourcePath
      ? initialSourcePassword ?? ""
      : ""
  );
  const {
    canRedo: canRedoGroups,
    canUndo: canUndoGroups,
    commit: commitGroups,
    present: groupsText,
    redo: redoGroups,
    reset: resetGroups,
    undo: undoGroups
  } = useBoundedHistory(initialRecoveryPlan?.pageGroups ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [dialogBusy, setDialogBusy] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [status, setStatus] = useState<SplitStatus | null>(null);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const previousInitialSource = useRef(initialSourceKey(initialSourcePath, initialSourcePassword));
  const skipFirstInitialSource = useRef(Boolean(initialRecoveryPlan));
  const splitJob = usePdfJob<SplitPdfResult>(desktopMode, "split");
  const busy = dialogBusy || splitJob.isActive;
  const pageGroups = groupsText
    .split(";")
    .map((group) => group.trim())
    .filter(Boolean);
  const safetySources = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "split-source",
              label: fileNameFromPath(sourcePath),
              password,
              path: sourcePath
            }
          ]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, safetySources, "split");
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const canSplit =
    desktopMode &&
    Boolean(sourcePath) &&
    pageGroups.length > 0 &&
    !busy &&
    outputProtectionIsValid(outputProtection, qpdfAvailable) &&
    editSafety.isReady &&
    certificateRiskAccepted;

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [safetySources]);

  useEffect(() => {
    if (skipFirstInitialSource.current) {
      skipFirstInitialSource.current = false;
      return;
    }
    if (initialSourcePath) {
      const nextInitialSource = initialSourceKey(initialSourcePath, initialSourcePassword);
      const sourceChanged = previousInitialSource.current !== nextInitialSource;
      previousInitialSource.current = nextInitialSource;
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      if (sourceChanged) {
        resetGroups("");
        setStatus(null);
        splitJob.clearJob();
      }
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    onRecoveryPlanChange?.(toRecoverySplitPlan(sourcePath, groupsText));
  }, [groupsText, onRecoveryPlanChange, sourcePath]);

  useEffect(() => {
    const job = splitJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      setStatus({
        kind: "success",
        outputs: job.result.outputs.map(
          (output) =>
            t(
              output.pageCount === 1
                ? "split.success.output.one"
                : "split.success.output.other",
              {
                count: formatNumber(output.pageCount),
                name: fileNameFromPath(output.outputPath)
              }
            )
        ),
        text: t("split.success.summary", {
          encryption:
            job.result.encryption === "None" ? t("common.none") : job.result.encryption,
          files: t(
            job.result.outputs.length === 1
              ? "split.count.files.one"
              : "split.count.files.other",
            { count: formatNumber(job.result.outputs.length) }
          ),
          pages: t(
            job.result.totalPages === 1
              ? "split.count.pages.one"
              : "split.count.pages.other",
            { count: formatNumber(job.result.totalPages) }
          )
        }),
        warnings: localiseSplitWarnings(job.result.warnings, t)
      });
    } else if (job.status === "cancelled") {
      setStatus({
        kind: "info",
        text: t("split.cancelled")
      });
    } else if (job.status === "failed") {
      setStatus({ kind: "error", text: localisePdfJobFailure(job, t) });
    }
  }, [locale, splitJob.job?.jobId, splitJob.job?.status]);

  const chooseSource = async () => {
    setStatus(null);
    try {
      const selected = await open({
        directory: false,
        filters: pdfFilter,
        multiple: false,
        title: t("split.dialog.sourceTitle")
      });
      if (typeof selected === "string") {
        if (selected !== sourcePath) {
          resetGroups("");
        }
        setSourcePath(selected);
        setPassword("");
        splitJob.clearJob();
      }
    } catch (reason) {
      setStatus({ kind: "error", text: t("split.error.chooseSource") });
    }
  };

  const runSplit = async () => {
    if (!canSplit || !sourcePath) {
      return;
    }
    setDialogBusy(true);
    setStatus(null);
    try {
      const outputDirectory = await open({
        directory: true,
        multiple: false,
        title: t("split.dialog.outputTitle")
      });
      if (typeof outputDirectory !== "string") {
        return;
      }
      await splitJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputDirectory,
        pageGroups,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable)
      });
    } catch (reason) {
      setStatus({ kind: "error", text: t("split.error.start") });
    } finally {
      setDialogBusy(false);
    }
  };

  const cancelSplit = async () => {
    if (!splitJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await splitJob.cancelJob();
    } catch (reason) {
      setCancelBusy(false);
      setStatus({ kind: "error", text: t("split.error.cancel") });
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    setStatus(null);
    splitJob.clearJob();
  };

  const undoSplitChange = () => {
    undoGroups();
    setStatus(null);
    splitJob.clearJob();
  };

  const redoSplitChange = () => {
    redoGroups();
    setStatus(null);
    splitJob.clearJob();
  };

  return (
    <section className="assembly-studio">
      <div className="assembly-heading">
        <div>
          <h3>{t("split.heading.title")}</h3>
          <p>{t("split.heading.description")}</p>
        </div>
        <div className="assembly-heading-actions">
          <button
            aria-label={t("split.undo.aria")}
            className="icon-button"
            disabled={busy || !canUndoGroups}
            onClick={undoSplitChange}
            title={t("common.undo")}
            type="button"
          >
            <Undo2 size={16} aria-hidden="true" />
          </button>
          <button
            aria-label={t("split.redo.aria")}
            className="icon-button"
            disabled={busy || !canRedoGroups}
            onClick={redoSplitChange}
            title={t("common.redo")}
            type="button"
          >
            <Redo2 size={16} aria-hidden="true" />
          </button>
          <Scissors size={18} aria-hidden="true" />
        </div>
      </div>

      <button className="wide-button" disabled={!desktopMode || busy} onClick={chooseSource} type="button">
        <FolderOpen size={17} aria-hidden="true" />
        {sourcePath ? t("split.source.chooseAnother") : t("split.source.choose")}
      </button>

      {sourcePath ? (
        <div className="split-source">
          <Scissors size={17} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(sourcePath)}</strong>
            <small title={sourcePath}>{sourcePath}</small>
          </span>
        </div>
      ) : null}

      <label className="assembly-field">
        {t("split.groups.label")}
        <textarea
          disabled={busy}
          onChange={(event) => {
            const value = event.target.value;
            commitGroups(() => value);
            setStatus(null);
            splitJob.clearJob();
          }}
          placeholder="1-3; 4-6; 7, 9, 11"
          rows={3}
          spellCheck={false}
          value={groupsText}
        />
      </label>
      <p className="assembly-help">
        {t("split.groups.helpBefore")} <code>odd</code> {t("split.groups.helpOr")}{" "}
        <code>even</code>{t("split.groups.helpAfter")}
      </p>

      <label className="assembly-field">
        {t("split.password.label")}
        <input
          autoComplete="current-password"
          disabled={busy}
          onChange={(event) => {
            setPassword(event.target.value);
            setStatus(null);
            splitJob.clearJob();
          }}
          spellCheck={false}
          type={showPassword ? "text" : "password"}
          value={password}
        />
      </label>
      <button className="show-passwords" disabled={busy} onClick={() => setShowPassword((value) => !value)} type="button">
        {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
        {showPassword ? t("common.hidePassword") : t("common.showPassword")}
      </button>

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
        rewriteDescription={t("split.rewriteDescription")}
      />

      <button
        className="primary wide-button"
        disabled={!canSplit}
        onClick={runSplit}
        type="button"
      >
        {busy ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <Scissors size={17} aria-hidden="true" />}
        {splitJob.isActive
          ? t("split.action.running")
          : dialogBusy
            ? t("split.action.choosing")
            : t("split.action.run")}
      </button>

      {splitJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={splitJob.connectionError}
          job={splitJob.job}
          onCancel={cancelSplit}
          onRetry={() => void runSplit()}
          retryDisabled={!canSplit}
        />
      ) : null}

      {!splitJob.isActive && splitJob.connectionError ? (
        <div className="assembly-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {status ? (
        <div className={`assembly-status is-${status.kind}`} role="status">
          {status.kind === "success" ? (
            <CheckCircle2 size={17} aria-hidden="true" />
          ) : (
            <AlertCircle size={17} aria-hidden="true" />
          )}
          <span>{status.text}</span>
          {status.outputs?.map((output) => <small key={output}>{output}</small>)}
          {status.warnings?.map((warning) => <small key={warning}>{warning}</small>)}
        </div>
      ) : null}
    </section>
  );
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function initialSourceKey(path?: string, password?: string) {
  return `${path ?? ""}\u0000${password ?? ""}`;
}

function localiseSplitWarnings(warnings: string[], t: Translate): string[] {
  const localised = warnings.map((warning) => {
    if (
      warning ===
      "Every split copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application."
    ) {
      return t("split.warning.protected");
    }

    const encryptedProtected = warning.match(
      /^(.*) was encrypted\. Its source security settings are replaced by the new AES-256 output passwords\.$/u
    );
    if (encryptedProtected) {
      return t("split.warning.sourceProtectionReplaced", { name: encryptedProtected[1] });
    }

    const encryptedPlain = warning.match(
      /^(.*) was encrypted\. The combined output is not password-protected\.$/u
    );
    if (encryptedPlain) {
      return t("split.warning.sourceUnprotected", { name: encryptedPlain[1] });
    }

    const certificate = warning.match(
      /^(.*) contains a certificate signature that is invalidated by merging or extraction\.$/u
    );
    if (certificate) {
      return t("split.warning.certificate", { name: certificate[1] });
    }

    const forms = warning.match(
      /^(.*) contains form fields\. Check their appearances in the combined output\.$/u
    );
    if (forms) {
      return t("split.warning.forms", { name: forms[1] });
    }

    const bookmarks = warning.match(
      /^(.*) contains bookmarks\. Source bookmarks are not copied into the combined output\.$/u
    );
    if (bookmarks) {
      return t("split.warning.bookmarks", { name: bookmarks[1] });
    }

    return t("split.warning.generic");
  });

  return [...new Set(localised)];
}
