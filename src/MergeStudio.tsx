import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  Eye,
  EyeOff,
  Files,
  FolderPlus,
  GripVertical,
  ListTree,
  Loader2,
  Redo2,
  Undo2,
  Trash2
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
import { displayMergePath, reorderMergePlan } from "./mergePlan";
import {
  localiseMergeWarnings,
  mergeActionErrorTranslationKey,
  type MergeActionErrorCode
} from "./mergeLocalisation";
import {
  localisePdfJobConnectionError,
  localisePdfJobFailure
} from "./pdfJobs";
import { takeE2eOpenSelection, takeE2eSaveSelection } from "paperworks-e2e-bridge";
import { useI18n } from "./I18nProvider";
import { type Translate } from "./i18n";
import {
  toRecoveryMergeSources,
  type RecoveryMergeSource
} from "./recovery";

type MergeStudioProps = {
  desktopMode: boolean;
  initialRecoverySources?: RecoveryMergeSource[];
  initialSourcePassword?: string;
  initialSourcePath?: string;
  onRecoverySourcesChange?: (sources: RecoveryMergeSource[]) => void;
  qpdfAvailable: boolean;
};

type MergeSource = {
  id: string;
  pageRange: string;
  password: string;
  path: string;
};

type CombinePdfResult = {
  bookmarkCount: number;
  bytesWritten: number;
  encryption: "AES-256" | "None";
  omittedBookmarkCount: number;
  outputPath: string;
  pageCount: number;
  warnings: string[];
};

type MergeStatus =
  | { code: "cancelled"; kind: "info" }
  | { code: MergeActionErrorCode; kind: "error" }
  | { kind: "job-error" }
  | { kind: "success"; result: CombinePdfResult };

export function MergeStudio({
  desktopMode,
  initialRecoverySources,
  initialSourcePassword,
  initialSourcePath,
  onRecoverySourcesChange,
  qpdfAvailable
}: MergeStudioProps) {
  const { formatNumber, t } = useI18n();
  const pdfFilter = useMemo(
    () => [{ name: t("merge.filter.pdfDocuments"), extensions: ["pdf"] }],
    [t]
  );
  const {
    canRedo: canRedoSources,
    canUndo: canUndoSources,
    commit: commitSources,
    present: sources,
    redo: redoSources,
    replace: replaceSources,
    undo: undoSources
  } = useBoundedHistory<MergeSource[]>(
    fromRecoveryMergeSources(
      initialRecoverySources,
      initialSourcePath,
      initialSourcePassword
    ),
    sanitiseMergeHistorySources
  );
  const [showPasswords, setShowPasswords] = useState(false);
  const [preserveBookmarks, setPreserveBookmarks] = useState(true);
  const [draggedSourceId, setDraggedSourceId] = useState<string | null>(null);
  const [dropTargetSourceId, setDropTargetSourceId] = useState<string | null>(null);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [dialogBusy, setDialogBusy] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [status, setStatus] = useState<MergeStatus | null>(null);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const previousInitialSource = useRef(initialSourceKey(initialSourcePath, initialSourcePassword));
  const skipFirstInitialSource = useRef(Boolean(initialRecoverySources?.length));
  const mergeJob = usePdfJob<CombinePdfResult>(desktopMode, "merge");
  const safetyKey = sources
    .map((source) => `${source.id}\u0000${source.path}\u0000${source.password}`)
    .join("\u0001");
  const safetySources = useMemo(
    () =>
      sources.map((source) => ({
        id: source.id,
        label: fileNameFromPath(source.path),
        password: source.password,
        path: source.path
      })),
    [safetyKey]
  );
  const editSafety = usePdfEditSafety(desktopMode, safetySources, "merge");
  const busy = dialogBusy || mergeJob.isActive;
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const canCombine =
    desktopMode &&
    sources.length > 0 &&
    !busy &&
    outputProtectionIsValid(outputProtection, qpdfAvailable) &&
    editSafety.isReady &&
    certificateRiskAccepted;

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [safetyKey]);

  useEffect(() => {
    if (skipFirstInitialSource.current) {
      skipFirstInitialSource.current = false;
      return;
    }
    if (!initialSourcePath) {
      return;
    }
    const nextInitialSource = initialSourceKey(initialSourcePath, initialSourcePassword);
    const sourceChanged = previousInitialSource.current !== nextInitialSource;
    previousInitialSource.current = nextInitialSource;
    replaceSources((current) => {
      const existing = current.findIndex((source) => source.path === initialSourcePath);
      if (existing >= 0) {
        return current.map((source, index) =>
          index === existing
            ? { ...source, password: initialSourcePassword ?? source.password }
            : source
        );
      }
      return [createSource(initialSourcePath, initialSourcePassword), ...current];
    });
    if (sourceChanged) {
      setStatus(null);
      mergeJob.clearJob();
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    onRecoverySourcesChange?.(toRecoveryMergeSources(sources));
  }, [onRecoverySourcesChange, sources]);

  useEffect(() => {
    const job = mergeJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      setStatus({
        kind: "success",
        result: job.result
      });
    } else if (job.status === "cancelled") {
      setStatus({
        code: "cancelled",
        kind: "info"
      });
    } else if (job.status === "failed") {
      setStatus({ kind: "job-error" });
    }
  }, [mergeJob.job?.jobId, mergeJob.job?.status]);

  const addSources = async () => {
    setStatus(null);
    try {
      const selection =
        takeE2eOpenSelection() ??
        (await open({
          directory: false,
          filters: pdfFilter,
          multiple: true,
          title: t("merge.saveDialog.addTitle")
        }));
      const paths = Array.isArray(selection) ? selection : selection ? [selection] : [];
      if (paths.length === 0) {
        return;
      }
      commitSources((current) => {
        const existingPaths = new Set(current.map((source) => source.path));
        return [
          ...current,
          ...paths
            .filter((path) => !existingPaths.has(path))
            .map((path) => createSource(path))
        ];
      });
      mergeJob.clearJob();
    } catch {
      setStatus({ code: "add-sources-failed", kind: "error" });
    }
  };

  const updateSource = (id: string, changes: Partial<MergeSource>) => {
    const transform = (current: MergeSource[]) =>
      current.map((source) => (source.id === id ? { ...source, ...changes } : source));
    if ("password" in changes) {
      replaceSources(transform);
    } else {
      commitSources(transform);
    }
    setStatus(null);
    mergeJob.clearJob();
  };

  const moveSource = (index: number, direction: -1 | 1) => {
    const destination = index + direction;
    if (destination < 0 || destination >= sources.length) {
      return;
    }
    commitSources((current) => {
      const next = [...current];
      const [source] = next.splice(index, 1);
      next.splice(destination, 0, source);
      return next;
    });
    setStatus(null);
    mergeJob.clearJob();
  };

  const beginSourceDrag = (sourceId: string, event: DragEvent<HTMLElement>) => {
    if (busy) {
      event.preventDefault();
      return;
    }
    setDraggedSourceId(sourceId);
    setDropTargetSourceId(sourceId);
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", sourceId);
  };

  const dropSource = (targetId: string, event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    const sourceId = event.dataTransfer.getData("text/plain") || draggedSourceId;
    setDraggedSourceId(null);
    setDropTargetSourceId(null);
    if (!sourceId || sourceId === targetId || busy) {
      return;
    }
    commitSources((current) => reorderMergePlan(current, sourceId, targetId));
    setStatus(null);
    mergeJob.clearJob();
  };

  const endSourceDrag = () => {
    setDraggedSourceId(null);
    setDropTargetSourceId(null);
  };

  const removeSource = (id: string) => {
    commitSources((current) => current.filter((source) => source.id !== id));
    setStatus(null);
    mergeJob.clearJob();
  };

  const undoMergeChange = () => {
    undoSources();
    setStatus(null);
    mergeJob.clearJob();
  };

  const redoMergeChange = () => {
    redoSources();
    setStatus(null);
    mergeJob.clearJob();
  };

  const createCombinedPdf = async () => {
    if (!canCombine) {
      return;
    }
    setDialogBusy(true);
    setStatus(null);
    try {
      const destination =
        takeE2eSaveSelection() ??
        (await save({
          defaultPath: suggestedCombinedPath(sources[0].path),
          filters: pdfFilter,
          title: t("merge.saveDialog.outputTitle")
        }));
      if (!destination) {
        return;
      }
      await mergeJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        outputPath: destination,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        preserveBookmarks,
        sources: sources.map((source) => ({
          inputPassword: source.password || null,
          inputPath: source.path,
          pageRange: source.pageRange.trim() || "all"
        }))
      });
    } catch {
      setStatus({ code: "start-failed", kind: "error" });
    } finally {
      setDialogBusy(false);
    }
  };

  const cancelMerge = async () => {
    if (!mergeJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await mergeJob.cancelJob();
    } catch {
      setCancelBusy(false);
      setStatus({ code: "cancel-failed", kind: "error" });
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    setStatus(null);
    mergeJob.clearJob();
  };

  return (
    <section className="assembly-studio">
      <div className="assembly-heading">
        <div>
          <h3>{t("merge.heading.title")}</h3>
          <p>{t("merge.heading.description")}</p>
        </div>
        <div className="assembly-heading-actions">
          <button
            aria-label={t("merge.undo.aria")}
            className="icon-button"
            disabled={busy || !canUndoSources}
            onClick={undoMergeChange}
            title={t("common.undo")}
            type="button"
          >
            <Undo2 size={16} aria-hidden="true" />
          </button>
          <button
            aria-label={t("merge.redo.aria")}
            className="icon-button"
            disabled={busy || !canRedoSources}
            onClick={redoMergeChange}
            title={t("common.redo")}
            type="button"
          >
            <Redo2 size={16} aria-hidden="true" />
          </button>
          <Files size={18} aria-hidden="true" />
        </div>
      </div>

      <button className="wide-button" disabled={!desktopMode || busy} onClick={addSources} type="button">
        <FolderPlus size={17} aria-hidden="true" />
        {t("merge.add")}
      </button>

      {sources.length === 0 ? (
        <div className="assembly-empty">
          <Files size={22} aria-hidden="true" />
          <span>{t("merge.empty")}</span>
        </div>
      ) : (
        <ol className="assembly-sources">
          {sources.map((source, index) => (
            <li
              className={`${source.id === draggedSourceId ? "is-dragging" : ""}${
                source.id === dropTargetSourceId && source.id !== draggedSourceId
                  ? " is-drop-target"
                  : ""
              }`}
              key={source.id}
              onDragEnter={() => {
                if (draggedSourceId && draggedSourceId !== source.id) {
                  setDropTargetSourceId(source.id);
                }
              }}
              onDragOver={(event) => {
                if (draggedSourceId && !busy) {
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "move";
                }
              }}
              onDrop={(event) => dropSource(source.id, event)}
            >
              <div className="assembly-source-heading">
                <span className="source-order">{index + 1}</span>
                <span className="source-name">
                  <strong>{fileNameFromPath(source.path)}</strong>
                  <small title={displayMergePath(source.path)}>{displayMergePath(source.path)}</small>
                </span>
                <span className="source-actions">
                  <span
                    aria-label={t("merge.drag.aria", { name: fileNameFromPath(source.path) })}
                    className="source-drag-handle"
                    draggable={!busy}
                    onDragEnd={endSourceDrag}
                    onDragStart={(event) => beginSourceDrag(source.id, event)}
                    role="img"
                    title={t("merge.drag.title")}
                  >
                    <GripVertical size={15} aria-hidden="true" />
                  </span>
                  <button
                    aria-label={t("merge.moveEarlier.aria", {
                      name: fileNameFromPath(source.path)
                    })}
                    disabled={index === 0 || busy}
                    onClick={() => moveSource(index, -1)}
                    title={t("merge.moveEarlier.title")}
                    type="button"
                  >
                    <ArrowUp size={15} aria-hidden="true" />
                  </button>
                  <button
                    aria-label={t("merge.moveLater.aria", {
                      name: fileNameFromPath(source.path)
                    })}
                    disabled={index === sources.length - 1 || busy}
                    onClick={() => moveSource(index, 1)}
                    title={t("merge.moveLater.title")}
                    type="button"
                  >
                    <ArrowDown size={15} aria-hidden="true" />
                  </button>
                  <button
                    aria-label={t("merge.remove.aria", { name: fileNameFromPath(source.path) })}
                    disabled={busy}
                    onClick={() => removeSource(source.id)}
                    title={t("merge.remove.title")}
                    type="button"
                  >
                    <Trash2 size={15} aria-hidden="true" />
                  </button>
                </span>
              </div>
              <label className="assembly-field">
                {t("merge.pages")}
                <input
                  disabled={busy}
                  onChange={(event) => updateSource(source.id, { pageRange: event.target.value })}
                  placeholder="all"
                  spellCheck={false}
                  value={source.pageRange}
                />
              </label>
              <label className="assembly-field">
                {t("merge.passwordIfRequired")}
                <input
                  autoComplete="current-password"
                  disabled={busy}
                  onChange={(event) => updateSource(source.id, { password: event.target.value })}
                  spellCheck={false}
                  type={showPasswords ? "text" : "password"}
                  value={source.password}
                />
              </label>
            </li>
          ))}
        </ol>
      )}

      {sources.length > 0 ? (
        <button className="show-passwords" disabled={busy} onClick={() => setShowPasswords((value) => !value)} type="button">
          {showPasswords ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
          {showPasswords ? t("common.hidePasswords") : t("common.showPasswords")}
        </button>
      ) : null}

      <p className="assembly-help">
        {t("merge.rangeHelp.start")} <code>all</code>, <code>1-5, 8</code>, <code>odd</code>,{" "}
        <code>even</code>, {t("merge.rangeHelp.end")} <code>10-7</code>.{" "}
        {t("merge.rangeHelp.repeated")}
      </p>

      <label className="merge-navigation-toggle">
        <input
          checked={preserveBookmarks}
          disabled={busy}
          onChange={(event) => {
            setPreserveBookmarks(event.target.checked);
            setStatus(null);
            mergeJob.clearJob();
          }}
          type="checkbox"
        />
        <span>
          <strong>{t("merge.navigation.title")}</strong>
          <small>{t("merge.navigation.description")}</small>
        </span>
        <ListTree size={18} aria-hidden="true" />
      </label>

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
        rewriteDescription={t("merge.rewriteDescription")}
      />

      <button
        className="primary wide-button"
        disabled={!canCombine}
        onClick={createCombinedPdf}
        type="button"
      >
        {busy ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <Files size={17} aria-hidden="true" />}
        {mergeJob.isActive
          ? t("merge.button.combining")
          : dialogBusy
            ? t("merge.button.choosing")
            : t("merge.button.choose")}
      </button>

      {mergeJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={mergeJob.connectionError}
          job={mergeJob.job}
          onCancel={cancelMerge}
          onRetry={() => void createCombinedPdf()}
          retryDisabled={!canCombine}
        />
      ) : null}

      {!mergeJob.isActive && mergeJob.connectionError ? (
        <div className="assembly-status is-info" role="status">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{localisePdfJobConnectionError(mergeJob.connectionError, t)}</span>
        </div>
      ) : null}

      {status ? (
        <div
          className={`assembly-status is-${status.kind === "job-error" ? "error" : status.kind}`}
          role={status.kind === "error" || status.kind === "job-error" ? "alert" : "status"}
        >
          {status.kind === "success" ? (
            <CheckCircle2 size={17} aria-hidden="true" />
          ) : (
            <AlertCircle size={17} aria-hidden="true" />
          )}
          <span>
            {status.kind === "success"
              ? formatMergeSuccess(status.result, formatNumber, t)
              : status.kind === "info"
                ? t("merge.cancelled")
                : status.kind === "job-error" && mergeJob.job
                  ? localisePdfJobFailure(mergeJob.job, t)
                  : status.kind === "error"
                    ? t(mergeActionErrorTranslationKey(status.code))
                    : t("merge.failed")}
          </span>
          {status.kind === "success"
            ? localiseMergeWarnings(status.result.warnings, t, formatNumber).map(
                (warning) => <small key={warning}>{warning}</small>
              )
            : null}
        </div>
      ) : null}
    </section>
  );
}

function createSource(path: string, password = ""): MergeSource {
  return {
    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    pageRange: "all",
    password: password ?? "",
    path
  };
}

function suggestedCombinedPath(path: string) {
  return path.replace(/\.pdf$/i, "-combined.pdf");
}

function initialSourceKey(path?: string, password?: string) {
  return `${path ?? ""}\u0000${password ?? ""}`;
}

function sanitiseMergeHistorySources(sources: MergeSource[]) {
  return sources.map((source) => ({ ...source, password: "" }));
}

function fromRecoveryMergeSources(
  sources: RecoveryMergeSource[] | undefined,
  initialSourcePath: string | undefined,
  initialSourcePassword: string | undefined
): MergeSource[] {
  return (sources ?? []).map((source) => ({
    id: source.id,
    pageRange: source.pageRange,
    password:
      source.sourcePath === initialSourcePath ? initialSourcePassword ?? "" : "",
    path: source.sourcePath
  }));
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

function formatMergeSuccess(
  result: CombinePdfResult,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string,
  t: Translate
) {
  return t(result.pageCount === 1 ? "merge.success.one" : "merge.success.other", {
    bookmarks: formatBookmarkResult(
      result.bookmarkCount,
      result.omittedBookmarkCount,
      formatNumber,
      t
    ),
    count: formatNumber(result.pageCount),
    encryption: result.encryption === "None" ? t("common.none") : result.encryption,
    fileName: fileNameFromPath(result.outputPath),
    fileSize: formatFileSize(result.bytesWritten, formatNumber)
  });
}

function formatBookmarkResult(
  bookmarkCount: number,
  omittedBookmarkCount: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string,
  t: Translate
) {
  const preserved = t(
    bookmarkCount === 1
      ? "merge.bookmarks.preserved.one"
      : "merge.bookmarks.preserved.other",
    { count: formatNumber(bookmarkCount) }
  );
  return omittedBookmarkCount > 0
    ? t(
        omittedBookmarkCount === 1
          ? "merge.bookmarks.omitted.one"
          : "merge.bookmarks.omitted.other",
        { count: formatNumber(omittedBookmarkCount), preserved }
      )
    : `${preserved}.`;
}
