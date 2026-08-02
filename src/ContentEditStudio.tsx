import {
  type ChangeEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  Image as ImageIcon,
  ImagePlus,
  Info,
  Loader2,
  Move,
  Redo2,
  RotateCcw,
  Save,
  Trash2,
  Type,
  Undo2,
  X
} from "lucide-react";
import { useDialogFocus } from "./accessibility";
import { takeE2eSaveSelection } from "paperworks-e2e-bridge";
import {
  boundedContentRect,
  commitContentEditHistory,
  contentDraftFromInspection,
  contentEditCount,
  contentEditPayload,
  createContentEditHistory,
  redoContentEditHistory,
  translateContentImage,
  undoContentEditHistory,
  updateContentImage,
  updateContentText,
  type ContentEditDraft,
  type ContentEditHistory,
  type ContentImageDraft,
  type ContentRect,
  type InspectedContentImage,
  type InspectedContentText
} from "./contentEditDraft";
import {
  localiseContentSelectionKind,
  localiseContentWarnings
} from "./contentEditLocalisation";
import { useI18n } from "./I18nProvider";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { PdfJobProgress } from "./PdfJobProgress";
import { PdfPageCanvas } from "./PdfPageCanvas";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import {
  createPdfLoadingTask,
  isIncorrectPasswordReason,
  type PDFDocumentProxy,
  type PdfRangeSource
} from "./pdf";
import { localisePdfJobFailure } from "./pdfJobs";
import { usePdfJob } from "./usePdfJob";

type ContentEditStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PdfContentInspection = {
  certificateSignature: boolean;
  editableImageCount: number;
  editableImages: InspectedContentImage[];
  editableTextCount: number;
  editableTextRuns: InspectedContentText[];
  fileName: string;
  pageCount: number;
  pagesWithUnsupportedContent: number[];
  readOnlyImageCount: number;
  readOnlyTextCount: number;
  sourceModifiedAtMs: number | null;
  sourceSha256: string;
  sourceSize: number;
  warnings: string[];
  wasEncrypted: boolean;
};

type ExportPdfContentResult = {
  bytesWritten: number;
  deletedImageCount: number;
  encryption: "AES-256" | "None";
  imageEditCount: number;
  outputPath: string;
  outputSha256: string;
  pageCount: number;
  replacedImageCount: number;
  repositionedImageCount: number;
  textEditCount: number;
  warnings: string[];
};

type Selection = {
  kind: "image" | "text";
  sourceId: string;
};

type ImageDrag = {
  currentRect: ContentRect;
  original: ContentImageDraft;
  pointerId: number;
  start: { x: number; y: number };
};

const replacementImageTypes = ".png,.jpg,.jpeg,.webp,.bmp,.gif,.tif,.tiff,.pnm,.pbm,.pgm,.ppm";
const MAX_REPLACEMENT_IMAGE_BYTES = 24 * 1024 * 1024;

export function ContentEditStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: ContentEditStudioProps) {
  const { formatNumber, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [inspection, setInspection] = useState<PdfContentInspection | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [history, setHistory] = useState<ContentEditHistory>(() =>
    createContentEditHistory()
  );
  const [selection, setSelection] = useState<Selection | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [zoom, setZoom] = useState(100);
  const [drag, setDrag] = useState<ImageDrag | null>(null);
  const [busy, setBusy] = useState<"export" | "image" | "review" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [exportResult, setExportResult] = useState<ExportPdfContentResult | null>(null);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const mountedRef = useRef(true);
  const requestRunRef = useRef(0);
  const contentJob = usePdfJob<ExportPdfContentResult>(desktopMode, "content");
  const inspectionJob = usePdfJob<PdfContentInspection>(
    desktopMode,
    "content-inspection"
  );
  const operationBusy = busy !== null || contentJob.isActive || inspectionJob.isActive;
  const draft = history.present;
  const changeCount = contentEditCount(draft);
  const selectedText =
    selection?.kind === "text"
      ? draft.text.find((item) => item.sourceId === selection.sourceId) ?? null
      : null;
  const selectedImage =
    selection?.kind === "image"
      ? draft.images.find((item) => item.sourceId === selection.sourceId) ?? null
      : null;
  const pageText = draft.text.filter((item) => item.pageNumber === pageNumber);
  const pageImages = draft.images.filter((item) => item.pageNumber === pageNumber);
  const hasCertificateRisk = inspection?.certificateSignature ?? false;
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      inspection &&
      changeCount > 0 &&
      (!hasCertificateRisk || signatureRiskAcknowledged) &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      !operationBusy &&
      !drag
  );

  const resetExportOutcome = useCallback(() => {
    setExportResult(null);
    setJobNotice(null);
    contentJob.clearJob();
  }, [contentJob.clearJob]);

  const closeWorkspace = useCallback(() => {
    if (operationBusy) {
      return;
    }
    requestRunRef.current += 1;
    const task = loadingTaskRef.current;
    loadingTaskRef.current = null;
    void task?.destroy();
    setWorkspaceOpen(false);
    setInspection(null);
    setPdfDocument(null);
    setHistory(createContentEditHistory());
    setSelection(null);
    setPageNumber(1);
    setDrag(null);
    inspectionJob.clearJob();
  }, [inspectionJob.clearJob, operationBusy]);

  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    escapeDisabled: operationBusy || drag !== null,
    onEscape: closeWorkspace
  });

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestRunRef.current += 1;
      const task = loadingTaskRef.current;
      loadingTaskRef.current = null;
      void task?.destroy();
    };
  }, []);

  useEffect(() => {
    if (!initialSourcePath || initialSourcePath === sourcePath) {
      return;
    }
    closeWorkspace();
    setSourcePath(initialSourcePath);
    setPassword(initialSourcePassword ?? "");
    setError(null);
  }, [closeWorkspace, initialSourcePassword, initialSourcePath, sourcePath]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [password, sourcePath]);

  useEffect(() => {
    const job = contentJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setError(null);
      setJobNotice(null);
      setExportResult(job.result);
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
    } else if (job.status === "cancelled") {
      setExportResult(null);
      setJobNotice(t("content.export.cancelled"));
    } else if (job.status === "failed") {
      setExportResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [contentJob.job?.jobId, contentJob.job?.status, t]);

  useEffect(() => {
    if (!selection) {
      return;
    }
    const exists =
      selection.kind === "text"
        ? draft.text.some((item) => item.sourceId === selection.sourceId)
        : draft.images.some((item) => item.sourceId === selection.sourceId);
    if (!exists) {
      setSelection(null);
    }
  }, [draft.images, draft.text, selection]);

  useEffect(() => {
    if (!workspaceOpen || operationBusy) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editingText = Boolean(target?.closest("input, textarea, select"));
      if ((event.ctrlKey || event.metaKey) && !editingText && event.key.toLowerCase() === "z") {
        event.preventDefault();
        setHistory((current) =>
          event.shiftKey
            ? redoContentEditHistory(current)
            : undoContentEditHistory(current)
        );
        resetExportOutcome();
      } else if (
        (event.ctrlKey || event.metaKey) &&
        !editingText &&
        event.key.toLowerCase() === "y"
      ) {
        event.preventDefault();
        setHistory(redoContentEditHistory);
        resetExportOutcome();
      } else if (
        !editingText &&
        selection?.kind === "image" &&
        (event.key === "Delete" || event.key === "Backspace")
      ) {
        event.preventDefault();
        commitDraft((current) =>
          updateContentImage(current, selection.sourceId, { deleted: true })
        );
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [operationBusy, resetExportOutcome, selection, workspaceOpen]);

  const commitDraft = useCallback(
    (updater: (current: ContentEditDraft) => ContentEditDraft) => {
      setHistory((current) =>
        commitContentEditHistory(current, updater(current.present))
      );
      setError(null);
      resetExportOutcome();
    },
    [resetExportOutcome]
  );

  const chooseSource = async () => {
    if (operationBusy) {
      return;
    }
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("content.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("content.dialog.choose")
      });
      if (typeof selected === "string" && mountedRef.current) {
        closeWorkspace();
        setSourcePath(selected);
        setPassword("");
        setOutputProtection(createOutputProtectionDraft());
        resetExportOutcome();
      }
    } catch {
      if (mountedRef.current) {
        setError(t("content.error.choose"));
      }
    }
  };

  const reviewContent = async () => {
    if (!desktopMode || !sourcePath || operationBusy) {
      return;
    }
    const runId = requestRunRef.current + 1;
    requestRunRef.current = runId;
    setBusy("review");
    setReviewCancelBusy(false);
    setError(null);
    resetExportOutcome();
    inspectionJob.clearJob();
    const previousTask = loadingTaskRef.current;
    loadingTaskRef.current = null;
    if (previousTask) {
      await previousTask.destroy();
    }
    let task: ReturnType<typeof createPdfLoadingTask> | null = null;
    try {
      const [report, source] = await Promise.all([
        inspectionJob.startJobAndWait({
          inputPassword: password || null,
          inputPath: sourcePath
        }),
        invoke<PdfRangeSource>("open_local_pdf", { path: sourcePath })
      ]);
      if (!mountedRef.current || requestRunRef.current !== runId) {
        return;
      }
      if (
        report.sourceSize !== source.size ||
        report.sourceModifiedAtMs !== source.modifiedAtMs
      ) {
        throw new ContentUserError(t("content.error.sourceChanged"));
      }
      task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure = "";
      task.onPassword = (_updatePassword: (value: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? t("content.error.passwordIncorrect")
          : t("content.error.passwordRequired");
        void task?.destroy();
      };
      let document: PDFDocumentProxy;
      try {
        document = await task.promise;
      } catch (reason) {
        if (passwordFailure) {
          throw new ContentUserError(passwordFailure);
        }
        throw reason;
      }
      if (!mountedRef.current || requestRunRef.current !== runId) {
        if (loadingTaskRef.current === task) {
          loadingTaskRef.current = null;
        }
        await task.destroy();
        return;
      }
      if (document.numPages !== report.pageCount) {
        throw new ContentUserError(t("content.error.pageCountMismatch"));
      }
      const nextDraft = contentDraftFromInspection(
        report.editableTextRuns,
        report.editableImages
      );
      setInspection(report);
      setPdfDocument(document);
      setHistory(createContentEditHistory(nextDraft));
      setSelection(firstSelection(nextDraft));
      setPageNumber(firstEditablePage(nextDraft));
      setWorkspaceOpen(true);
      setSignatureRiskAcknowledged(false);
      inspectionJob.clearJob();
    } catch (reason) {
      if (loadingTaskRef.current === task) {
        loadingTaskRef.current = null;
      }
      void task?.destroy();
      if (mountedRef.current && requestRunRef.current === runId) {
        setInspection(null);
        setPdfDocument(null);
        setWorkspaceOpen(false);
        setError(
          reason instanceof Error && reason.message === "The PDF job was cancelled."
            ? t("content.review.cancelled")
            : reason instanceof ContentUserError
              ? reason.message
              : pdfOpeningError(reason, t)
        );
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setReviewCancelBusy(false);
        setBusy(null);
      }
    }
  };

  const exportContent = async () => {
    if (!canExport || !sourcePath || !inspection) {
      return;
    }
    const runId = requestRunRef.current;
    setBusy("export");
    setError(null);
    resetExportOutcome();
    try {
      const outputPath =
        takeE2eSaveSelection() ??
        (await save({
          defaultPath: suggestedOutputPath(sourcePath),
          filters: [{ name: t("content.dialog.filter"), extensions: ["pdf"] }],
          title: t("content.dialog.save")
        }));
      if (typeof outputPath !== "string") {
        return;
      }
      if (!mountedRef.current || requestRunRef.current !== runId) {
        return;
      }
      const payload = contentEditPayload(history.present);
      await contentJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        expectedSourceModifiedAtMs: inspection.sourceModifiedAtMs,
        expectedSourceSha256: inspection.sourceSha256,
        expectedSourceSize: inspection.sourceSize,
        imageEdits: payload.imageEdits,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        textEdits: payload.textEdits
      });
    } catch {
      if (mountedRef.current && requestRunRef.current === runId) {
        setError(t("content.error.export"));
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setBusy(null);
      }
    }
  };

  const cancelExport = async () => {
    if (!contentJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await contentJob.cancelJob();
    } catch {
      setError(t("content.error.exportCancel"));
    } finally {
      setCancelBusy(false);
    }
  };

  const cancelReview = async () => {
    if (!inspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await inspectionJob.cancelJob();
    } catch {
      setError(t("content.error.reviewCancel"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const replaceSelectedImage = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file || !selectedImage) {
      return;
    }
    setBusy("image");
    setError(null);
    try {
      const dataUrl = await replacementImageDataUrl(file, t);
      if (mountedRef.current) {
        commitDraft((current) =>
          updateContentImage(current, selectedImage.sourceId, {
            deleted: false,
            replacementImageDataUrl: dataUrl
          })
        );
      }
    } catch (reason) {
      if (mountedRef.current) {
        setError(
          reason instanceof ContentUserError
            ? reason.message
            : t("content.error.imageRead")
        );
      }
    } finally {
      if (mountedRef.current) {
        setBusy(null);
      }
    }
  };

  const selectObject = (next: Selection, nextPage: number) => {
    setSelection(next);
    setPageNumber(nextPage);
  };

  const beginImageDrag = (
    event: ReactPointerEvent<SVGRectElement>,
    image: ContentImageDraft
  ) => {
    if (operationBusy || image.deleted) {
      return;
    }
    const svg = event.currentTarget.ownerSVGElement;
    if (!svg) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    svg.setPointerCapture(event.pointerId);
    setSelection({ kind: "image", sourceId: image.sourceId });
    setDrag({
      currentRect: image.rect,
      original: image,
      pointerId: event.pointerId,
      start: eventPoint(event, svg)
    });
  };

  const continueImageDrag = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    const point = eventPoint(event, event.currentTarget);
    setDrag((current) =>
      current
        ? {
            ...current,
            currentRect: translateContentImage(
              current.original,
              point.x - current.start.x,
              point.y - current.start.y
            )
          }
        : null
    );
  };

  const finishImageDrag = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    const completed = drag;
    setDrag(null);
    if (!sameRect(completed.currentRect, completed.original.rect)) {
      commitDraft((current) =>
        updateContentImage(current, completed.original.sourceId, {
          rect: completed.currentRect
        })
      );
    }
  };

  const visiblePageImages = useMemo(
    () =>
      pageImages.map((image) =>
        drag?.original.sourceId === image.sourceId
          ? { ...image, rect: drag.currentRect }
          : image
      ),
    [drag, pageImages]
  );

  const selectOverlayObject = (
    event: ReactKeyboardEvent<SVGGElement>,
    next: Selection
  ) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    setSelection(next);
  };

  const moveOverlayImage = (
    event: ReactKeyboardEvent<SVGGElement>,
    image: ContentImageDraft
  ) => {
    if (!event.key.startsWith("Arrow") || image.deleted) {
      selectOverlayObject(event, { kind: "image", sourceId: image.sourceId });
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const distance = event.shiftKey ? 0.02 : 0.005;
    const offset = {
      x: event.key === "ArrowLeft" ? -distance : event.key === "ArrowRight" ? distance : 0,
      y: event.key === "ArrowUp" ? -distance : event.key === "ArrowDown" ? distance : 0
    };
    setSelection({ kind: "image", sourceId: image.sourceId });
    commitDraft((current) =>
      updateContentImage(current, image.sourceId, {
        rect: translateContentImage(image, offset.x, offset.y)
      })
    );
  };

  return (
    <>
      <section className="content-edit-studio">
        <div className="content-edit-heading">
          <div className="content-edit-icon" aria-hidden="true">
            <FileText size={20} />
          </div>
          <div>
            <h3>{t("content.heading.title")}</h3>
            <p>{t("content.heading.description")}</p>
          </div>
        </div>
        <label className="field-label">
          {t("content.source.label")}
          <span className="content-edit-path-control">
            <input
              aria-label={t("content.source.aria")}
              readOnly
              title={sourcePath ?? undefined}
              value={sourcePath ? fileNameFromPath(sourcePath) : t("content.source.placeholder")}
            />
            <button
              className="icon-button"
              disabled={!desktopMode || operationBusy}
              onClick={() => void chooseSource()}
              title={t("content.source.choose")}
              type="button"
            >
              <FolderOpen size={17} aria-hidden="true" />
            </button>
          </span>
        </label>
        {sourcePath ? (
          <label className="field-label">
            {t("content.password.label")}
            <span className="content-edit-path-control">
              <input
                autoComplete="off"
                disabled={operationBusy}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={t("content.password.placeholder")}
                type={showPassword ? "text" : "password"}
                value={password}
              />
              <button
                className="icon-button"
                disabled={operationBusy}
                onClick={() => setShowPassword((visible) => !visible)}
                aria-label={
                  showPassword
                    ? t("content.password.hide")
                    : t("content.password.show")
                }
                title={
                  showPassword
                    ? t("content.password.hide")
                    : t("content.password.show")
                }
                type="button"
              >
                {showPassword ? (
                  <EyeOff size={16} aria-hidden="true" />
                ) : (
                  <Eye size={16} aria-hidden="true" />
                )}
              </button>
            </span>
          </label>
        ) : null}
        {!desktopMode ? (
          <div className="engine-state is-info">
            <Info size={16} aria-hidden="true" />
            <span>{t("content.desktopOnly")}</span>
          </div>
        ) : null}
        {error && !workspaceOpen ? (
          <div className="engine-state is-missing" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}
        <button
          className="primary wide-button"
          disabled={!desktopMode || !sourcePath || operationBusy}
          onClick={() => void reviewContent()}
          type="button"
        >
          {busy === "review" || inspectionJob.isActive ? (
            <Loader2 className="spin" size={17} aria-hidden="true" />
          ) : (
            <FileText size={17} aria-hidden="true" />
          )}
          {busy === "review" || inspectionJob.isActive
            ? t("content.action.opening")
            : t("content.action.open")}
        </button>
        {!workspaceOpen && inspectionJob.job ? (
          <PdfJobProgress
            cancelling={reviewCancelBusy}
            connectionError={inspectionJob.connectionError}
            job={inspectionJob.job}
            onCancel={() => void cancelReview()}
            onRetry={() => void reviewContent()}
            retryDisabled={!desktopMode || !sourcePath || operationBusy}
          />
        ) : null}
        {!workspaceOpen && contentJob.job ? (
          <PdfJobProgress
            cancelling={cancelBusy}
            connectionError={contentJob.connectionError}
            job={contentJob.job}
            onCancel={() => void cancelExport()}
            onRetry={() => void exportContent()}
            retryDisabled={!canExport}
          />
        ) : null}
        {!workspaceOpen && jobNotice ? (
          <div className="engine-state is-info" role="status">
            <Info size={16} aria-hidden="true" />
            <span>{jobNotice}</span>
          </div>
        ) : null}
        {!workspaceOpen && exportResult ? (
          <ContentExportResult
            formatNumber={formatNumber}
            result={exportResult}
            t={t}
          />
        ) : null}
      </section>

      {workspaceOpen && inspection && pdfDocument ? (
        <div className="dialog-backdrop content-edit-backdrop" role="presentation">
          <section
            aria-labelledby="content-edit-dialog-title"
            aria-modal="true"
            className="content-edit-dialog"
            data-dialog-root
            ref={dialogRef}
            role="dialog"
            tabIndex={-1}
          >
            <header className="content-edit-dialog-header">
              <div className="dialog-icon" aria-hidden="true">
                <FileText size={24} />
              </div>
              <div>
                <span className="eyebrow">{t("content.workspace.eyebrow")}</span>
                <h2 id="content-edit-dialog-title">{inspection.fileName}</h2>
              </div>
              <div className="content-edit-header-actions">
                <span>
                  {t("content.workspace.summary", {
                    editable: formatNumber(
                      inspection.editableTextCount + inspection.editableImageCount
                    ),
                    pending: formatNumber(changeCount),
                    readOnly: formatNumber(
                      inspection.readOnlyTextCount + inspection.readOnlyImageCount
                    )
                  })}
                </span>
                <button
                  className="primary"
                  disabled={!canExport}
                  onClick={() => void exportContent()}
                  type="button"
                >
                  {contentJob.isActive ? (
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                  ) : (
                    <Save size={16} aria-hidden="true" />
                  )}
                  {contentJob.isActive
                    ? t("content.action.exporting")
                    : t("content.action.save")}
                </button>
                <button
                  aria-label={t("content.action.close")}
                  className="icon-button"
                  data-dialog-initial-focus
                  disabled={operationBusy}
                  onClick={closeWorkspace}
                  title={t("content.action.close")}
                  type="button"
                >
                  <X size={18} aria-hidden="true" />
                </button>
              </div>
            </header>

            {contentJob.job ? (
              <PdfJobProgress
                cancelling={cancelBusy}
                connectionError={contentJob.connectionError}
                job={contentJob.job}
                onCancel={() => void cancelExport()}
                onRetry={() => void exportContent()}
                retryDisabled={!canExport}
              />
            ) : null}
            {jobNotice ? (
              <div className="content-edit-notice" role="status">
                <Info size={16} aria-hidden="true" />
                <span>{jobNotice}</span>
              </div>
            ) : null}
            {error ? (
              <div className="content-edit-error" role="alert">
                <AlertCircle size={16} aria-hidden="true" />
                <span>{error}</span>
              </div>
            ) : null}

            <div
              className="content-edit-toolbar"
              role="toolbar"
              aria-label={t("content.toolbar.aria")}
            >
              <button
                aria-label={t("common.undo")}
                className="icon-button"
                disabled={operationBusy || history.past.length === 0}
                onClick={() => {
                  setHistory(undoContentEditHistory);
                  resetExportOutcome();
                }}
                title={t("common.undo")}
                type="button"
              >
                <Undo2 size={17} aria-hidden="true" />
              </button>
              <button
                aria-label={t("common.redo")}
                className="icon-button"
                disabled={operationBusy || history.future.length === 0}
                onClick={() => {
                  setHistory(redoContentEditHistory);
                  resetExportOutcome();
                }}
                title={t("common.redo")}
                type="button"
              >
                <Redo2 size={17} aria-hidden="true" />
              </button>
              <span className="content-edit-toolbar-separator" />
              <button
                aria-label={t("common.previousPage")}
                className="icon-button"
                disabled={operationBusy || pageNumber <= 1}
                onClick={() => {
                  setPageNumber((current) => Math.max(1, current - 1));
                  setSelection(null);
                }}
                title={t("common.previousPage")}
                type="button"
              >
                <ChevronLeft size={17} aria-hidden="true" />
              </button>
              <label>
                {t("common.page")}
                <input
                  disabled={operationBusy}
                  max={inspection.pageCount}
                  min={1}
                  onChange={(event) => {
                    setPageNumber(
                      Math.max(1, Math.min(inspection.pageCount, Number(event.target.value) || 1))
                    );
                    setSelection(null);
                  }}
                  type="number"
                  value={pageNumber}
                />
                <span>
                  {t("common.ofCount", { count: formatNumber(inspection.pageCount) })}
                </span>
              </label>
              <button
                aria-label={t("common.nextPage")}
                className="icon-button"
                disabled={operationBusy || pageNumber >= inspection.pageCount}
                onClick={() => {
                  setPageNumber((current) => Math.min(inspection.pageCount, current + 1));
                  setSelection(null);
                }}
                title={t("common.nextPage")}
                type="button"
              >
                <ChevronRight size={17} aria-hidden="true" />
              </button>
              <span className="content-edit-toolbar-spacer" />
              <label>
                {t("content.toolbar.zoom")}
                <input
                  disabled={operationBusy}
                  max={160}
                  min={60}
                  onChange={(event) => setZoom(Number(event.target.value))}
                  step={10}
                  type="range"
                  value={zoom}
                />
                <output>
                  {formatNumber(zoom / 100, {
                    style: "percent",
                    maximumFractionDigits: 0
                  })}
                </output>
              </label>
            </div>

            <fieldset className="content-edit-fieldset" disabled={operationBusy}>
              <div className="content-edit-workspace">
                <aside
                  className="content-edit-object-list"
                  aria-label={t("content.objects.aria")}
                >
                  <div className="content-edit-pane-heading">
                    <div>
                      <strong>
                        {t("content.objects.page", { page: formatNumber(pageNumber) })}
                      </strong>
                      <span>
                        {t("content.objects.summary", {
                          images: formatNumber(pageImages.length),
                          text: formatNumber(pageText.length)
                        })}
                      </span>
                    </div>
                  </div>
                  {pageText.length === 0 && pageImages.length === 0 ? (
                    <div className="content-edit-empty">
                      <Info size={18} aria-hidden="true" />
                      <span>{t("content.objects.empty")}</span>
                    </div>
                  ) : null}
                  {pageText.map((item) => (
                    <button
                      aria-pressed={selection?.sourceId === item.sourceId}
                      className={selection?.sourceId === item.sourceId ? "is-selected" : undefined}
                      key={item.sourceId}
                      onClick={() => selectObject({ kind: "text", sourceId: item.sourceId }, item.pageNumber)}
                      type="button"
                    >
                      <Type size={16} aria-hidden="true" />
                      <span>
                        <strong>
                          {truncate(item.text || t("content.objects.emptyText"), 36)}
                        </strong>
                        <small>
                          {t("content.objects.font", {
                            font: item.fontLabel,
                            size: formatNumber(Math.round(item.fontSize))
                          })}
                        </small>
                      </span>
                      {item.text !== item.originalText ? (
                        <i>{t("content.objects.changed")}</i>
                      ) : null}
                    </button>
                  ))}
                  {pageImages.map((item, index) => (
                    <button
                      aria-pressed={selection?.sourceId === item.sourceId}
                      className={selection?.sourceId === item.sourceId ? "is-selected" : undefined}
                      key={item.sourceId}
                      onClick={() => selectObject({ kind: "image", sourceId: item.sourceId }, item.pageNumber)}
                      type="button"
                    >
                      <ImageIcon size={16} aria-hidden="true" />
                      <span>
                        <strong>
                          {t("content.objects.image", {
                            number: formatNumber(index + 1)
                          })}
                        </strong>
                        <small>
                          {t("content.objects.pixels", {
                            height: formatNumber(item.pixelHeight),
                            width: formatNumber(item.pixelWidth)
                          })}
                        </small>
                      </span>
                      {item.deleted ? (
                        <i>{t("content.objects.removed")}</i>
                      ) : item.replacementImageDataUrl ? (
                        <i>{t("content.objects.replaced")}</i>
                      ) : !sameRect(item.rect, item.originalRect) ? (
                        <i>{t("content.objects.moved")}</i>
                      ) : null}
                    </button>
                  ))}
                </aside>

                <main className="content-edit-preview-scroll">
                  <div
                    className="content-edit-page"
                    style={{ width: `${Math.round(680 * (zoom / 100))}px` }}
                  >
                    <PdfPageCanvas
                      document={pdfDocument}
                      pageNumber={pageNumber}
                      targetWidth={Math.round(680 * (zoom / 100))}
                      variant="page"
                    />
                    <svg
                      aria-label={t("content.canvas.aria", {
                        page: formatNumber(pageNumber)
                      })}
                      className="content-edit-overlay"
                      onPointerCancel={finishImageDrag}
                      onPointerMove={continueImageDrag}
                      onPointerUp={finishImageDrag}
                      preserveAspectRatio="none"
                      role="group"
                      viewBox="0 0 1000 1000"
                    >
                      {pageText.map((item) => (
                        <g
                          aria-label={t("content.canvas.textAria", {
                            text: truncate(item.text || t("content.objects.emptyText"), 48)
                          })}
                          aria-pressed={selection?.sourceId === item.sourceId}
                          className={selection?.sourceId === item.sourceId ? "is-selected is-text" : "is-text"}
                          key={item.sourceId}
                          onClick={(event) => {
                            event.stopPropagation();
                            setSelection({ kind: "text", sourceId: item.sourceId });
                          }}
                          onKeyDown={(event) =>
                            selectOverlayObject(event, {
                              kind: "text",
                              sourceId: item.sourceId
                            })
                          }
                          role="button"
                          tabIndex={0}
                        >
                          {item.text !== item.originalText ? (
                            <rect
                              className="content-edit-text-preview-bg"
                              height={item.rect.height * 1000}
                              width={item.rect.width * 1000}
                              x={item.rect.x * 1000}
                              y={item.rect.y * 1000}
                            />
                          ) : null}
                          <rect
                            className="content-edit-object-box"
                            height={item.rect.height * 1000}
                            width={item.rect.width * 1000}
                            x={item.rect.x * 1000}
                            y={item.rect.y * 1000}
                          />
                          {item.text !== item.originalText ? (
                            <text
                              className="content-edit-text-preview"
                              fontSize={Math.max(9, Math.min(30, item.rect.height * 650))}
                              x={item.rect.x * 1000 + 4}
                              y={(item.rect.y + item.rect.height * 0.72) * 1000}
                            >
                              {truncate(item.text || t("content.canvas.removedText"), 42)}
                            </text>
                          ) : null}
                        </g>
                      ))}
                      {visiblePageImages.map((item) => (
                        <g
                          aria-label={t(
                            item.deleted
                              ? "content.canvas.imageRemovedAria"
                              : "content.canvas.imageAria",
                            {
                              height: formatNumber(item.pixelHeight),
                              width: formatNumber(item.pixelWidth)
                            }
                          )}
                          aria-pressed={selection?.sourceId === item.sourceId}
                          className={`${selection?.sourceId === item.sourceId ? "is-selected " : ""}is-image${item.deleted ? " is-deleted" : ""}`}
                          key={item.sourceId}
                          onClick={(event) => {
                            event.stopPropagation();
                            setSelection({ kind: "image", sourceId: item.sourceId });
                          }}
                          onKeyDown={(event) => moveOverlayImage(event, item)}
                          role="button"
                          tabIndex={0}
                        >
                          {item.replacementImageDataUrl && !item.deleted ? (
                            <image
                              height={item.rect.height * 1000}
                              href={item.replacementImageDataUrl}
                              preserveAspectRatio="none"
                              width={item.rect.width * 1000}
                              x={item.rect.x * 1000}
                              y={item.rect.y * 1000}
                            />
                          ) : null}
                          <rect
                            className="content-edit-object-box"
                            height={item.rect.height * 1000}
                            onPointerDown={(event) => beginImageDrag(event, item)}
                            width={item.rect.width * 1000}
                            x={item.rect.x * 1000}
                            y={item.rect.y * 1000}
                          />
                          {item.deleted ? (
                            <g className="content-edit-delete-cross">
                              <line
                                x1={item.rect.x * 1000}
                                x2={(item.rect.x + item.rect.width) * 1000}
                                y1={item.rect.y * 1000}
                                y2={(item.rect.y + item.rect.height) * 1000}
                              />
                              <line
                                x1={(item.rect.x + item.rect.width) * 1000}
                                x2={item.rect.x * 1000}
                                y1={item.rect.y * 1000}
                                y2={(item.rect.y + item.rect.height) * 1000}
                              />
                            </g>
                          ) : null}
                        </g>
                      ))}
                    </svg>
                  </div>
                </main>

                <aside className="content-edit-properties">
                  <div className="content-edit-pane-heading">
                    <div>
                      <strong>{t("content.properties.title")}</strong>
                      <span>
                        {selection
                          ? t("content.properties.selection", {
                              kind: localiseContentSelectionKind(selection.kind, t)
                            })
                          : t("content.properties.select")}
                      </span>
                    </div>
                  </div>
                  {selectedText ? (
                    <div className="content-edit-properties-body">
                      <div className="content-edit-property-title">
                        <Type size={17} aria-hidden="true" />
                        <span>
                          <strong>{t("content.text.existing")}</strong>
                          <small>
                            {t("content.objects.font", {
                              font: selectedText.fontLabel,
                              size: formatNumber(Math.round(selectedText.fontSize))
                            })}
                          </small>
                        </span>
                      </div>
                      <label>
                        {t("content.text.replacement")}
                        <textarea
                          maxLength={4096}
                          onChange={(event) =>
                            commitDraft((current) =>
                              updateContentText(
                                current,
                                selectedText.sourceId,
                                event.target.value
                              )
                            )
                          }
                          rows={5}
                          value={selectedText.text}
                        />
                        <small>
                          {t("content.text.characters", {
                            count: formatNumber(selectedText.text.length),
                            maximum: formatNumber(4096)
                          })}
                        </small>
                      </label>
                      <div className="content-edit-inline-info">
                        <Info size={15} aria-hidden="true" />
                        <span>{t("content.text.fontWarning")}</span>
                      </div>
                      <button
                        disabled={selectedText.text === selectedText.originalText}
                        onClick={() =>
                          commitDraft((current) =>
                            updateContentText(
                              current,
                              selectedText.sourceId,
                              selectedText.originalText
                            )
                          )
                        }
                        type="button"
                      >
                        <RotateCcw size={15} aria-hidden="true" />
                        {t("content.action.restoreOriginal")}
                      </button>
                    </div>
                  ) : selectedImage ? (
                    <div className="content-edit-properties-body">
                      <div className="content-edit-property-title">
                        <ImageIcon size={17} aria-hidden="true" />
                        <span>
                          <strong>{t("content.image.existing")}</strong>
                          <small>
                            {t("content.objects.pixels", {
                              height: formatNumber(selectedImage.pixelHeight),
                              width: formatNumber(selectedImage.pixelWidth)
                            })}
                          </small>
                        </span>
                      </div>
                      <label className="content-edit-file-button">
                        <input
                          accept={replacementImageTypes}
                          onChange={(event) => void replaceSelectedImage(event)}
                          type="file"
                        />
                        <ImagePlus size={16} aria-hidden="true" />
                        {selectedImage.replacementImageDataUrl
                          ? t("content.action.chooseAnotherImage")
                          : t("content.action.replaceImage")}
                      </label>
                      <div className="content-edit-rect-grid">
                        {(["x", "y", "width", "height"] as const).map((key) => (
                          <label key={key}>
                            {rectLabel(key, t)}
                            <span>
                              <input
                                max={100}
                                min={key === "width" || key === "height" ? 0.2 : 0}
                                onChange={(event) => {
                                  const rect = boundedContentRect({
                                    ...selectedImage.rect,
                                    [key]: Number(event.target.value) / 100
                                  });
                                  commitDraft((current) =>
                                    updateContentImage(current, selectedImage.sourceId, { rect })
                                  );
                                }}
                                step={0.1}
                                type="number"
                                value={roundPercentage(selectedImage.rect[key])}
                              />
                              <i>%</i>
                            </span>
                          </label>
                        ))}
                      </div>
                      <div className="content-edit-inline-info">
                        <Move size={15} aria-hidden="true" />
                        <span>{t("content.image.drag")}</span>
                      </div>
                      <div className="content-edit-property-actions">
                        <button
                          className={selectedImage.deleted ? "is-danger" : undefined}
                          onClick={() =>
                            commitDraft((current) =>
                              updateContentImage(current, selectedImage.sourceId, {
                                deleted: !selectedImage.deleted
                              })
                            )
                          }
                          type="button"
                        >
                          {selectedImage.deleted ? (
                            <RotateCcw size={15} aria-hidden="true" />
                          ) : (
                            <Trash2 size={15} aria-hidden="true" />
                          )}
                          {selectedImage.deleted
                            ? t("content.action.restoreImage")
                            : t("content.action.removeImage")}
                        </button>
                        <button
                          disabled={
                            !selectedImage.replacementImageDataUrl &&
                            sameRect(selectedImage.rect, selectedImage.originalRect) &&
                            !selectedImage.deleted
                          }
                          onClick={() =>
                            commitDraft((current) =>
                              updateContentImage(current, selectedImage.sourceId, {
                                deleted: false,
                                rect: selectedImage.originalRect,
                                replacementImageDataUrl: null
                              })
                            )
                          }
                          type="button"
                        >
                          <RotateCcw size={15} aria-hidden="true" />
                          {t("content.action.reset")}
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="content-edit-empty is-properties">
                      <FileText size={20} aria-hidden="true" />
                      <span>{t("content.properties.empty")}</span>
                    </div>
                  )}

                  <div className="content-edit-safety">
                    {localiseContentWarnings(
                      inspection.warnings,
                      t,
                      formatNumber
                    ).map((warning) => (
                      <div className="content-edit-inline-info" key={warning}>
                        <Info size={15} aria-hidden="true" />
                        <span>{warning}</span>
                      </div>
                    ))}
                    {hasCertificateRisk ? (
                      <label className="content-edit-risk-check">
                        <input
                          checked={signatureRiskAcknowledged}
                          onChange={(event) =>
                            setSignatureRiskAcknowledged(event.target.checked)
                          }
                          type="checkbox"
                        />
                        <span>{t("content.signature.acknowledgement")}</span>
                      </label>
                    ) : null}
                    <OutputProtectionFields
                      disabled={operationBusy}
                      onChange={setOutputProtection}
                      qpdfAvailable={qpdfAvailable}
                      value={outputProtection}
                    />
                  </div>
                </aside>
              </div>
            </fieldset>

            {exportResult ? (
              <ContentExportResult
                formatNumber={formatNumber}
                result={exportResult}
                t={t}
              />
            ) : null}
          </section>
        </div>
      ) : null}
    </>
  );
}

function ContentExportResult({
  formatNumber,
  result,
  t
}: {
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  result: ExportPdfContentResult;
  t: ReturnType<typeof useI18n>["t"];
}) {
  return (
    <div className="content-edit-result" role="status">
      <CheckCircle2 size={18} aria-hidden="true" />
      <div>
        <strong>{t("content.result.title")}</strong>
        <span>
          {t("content.result.summary", {
            encryption:
              result.encryption === "AES-256"
                ? t("common.encryption.protected")
                : t("common.encryption.unprotected"),
            images: formatNumber(result.imageEditCount),
            size: formatBytes(result.bytesWritten, formatNumber),
            text: formatNumber(result.textEditCount)
          })}
        </span>
        <small>{t("content.result.hash", { hash: result.outputSha256 })}</small>
        {localiseContentWarnings(result.warnings, t, formatNumber).map((warning) => (
          <small key={warning}>{warning}</small>
        ))}
      </div>
    </div>
  );
}

function firstSelection(draft: ContentEditDraft): Selection | null {
  if (draft.text[0]) {
    return { kind: "text", sourceId: draft.text[0].sourceId };
  }
  if (draft.images[0]) {
    return { kind: "image", sourceId: draft.images[0].sourceId };
  }
  return null;
}

function firstEditablePage(draft: ContentEditDraft) {
  return draft.text[0]?.pageNumber ?? draft.images[0]?.pageNumber ?? 1;
}

function eventPoint(
  event: { clientX: number; clientY: number },
  element: SVGSVGElement
) {
  const bounds = element.getBoundingClientRect();
  return {
    x: Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)),
    y: Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height))
  };
}

function replacementImageDataUrl(
  file: File,
  t: ReturnType<typeof useI18n>["t"]
): Promise<string> {
  if (file.size === 0 || file.size > MAX_REPLACEMENT_IMAGE_BYTES) {
    return Promise.reject(new ContentUserError(t("content.error.imageSize")));
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new ContentUserError(t("content.error.imageRead")));
    reader.onload = () => {
      const value = typeof reader.result === "string" ? reader.result : "";
      if (!value.startsWith("data:image/")) {
        reject(new ContentUserError(t("content.error.imageFormat")));
      } else {
        resolve(value);
      }
    };
    reader.readAsDataURL(file);
  });
}

function sameRect(left: ContentRect, right: ContentRect) {
  return (
    left.x === right.x &&
    left.y === right.y &&
    left.width === right.width &&
    left.height === right.height
  );
}

function rectLabel(key: keyof ContentRect, t: ReturnType<typeof useI18n>["t"]) {
  return key === "x"
    ? t("content.image.left")
    : key === "y"
      ? t("content.image.top")
      : key === "width"
        ? t("content.image.width")
        : t("content.image.height");
}

function roundPercentage(value: number) {
  return Math.round(value * 1_000) / 10;
}

function suggestedOutputPath(sourcePath: string) {
  return sourcePath.replace(/\.pdf$/i, "-content-edited.pdf");
}

function formatBytes(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) {
    return `${formatNumber(bytes)} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KB`;
  }
  return `${formatNumber(bytes / (1024 * 1024), {
    maximumFractionDigits: 1
  })} MB`;
}

function truncate(value: string, limit: number) {
  const normalised = value.replace(/\s+/g, " ").trim();
  return normalised.length > limit ? `${normalised.slice(0, limit - 3)}...` : normalised;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || "Document.pdf";
}

function pdfOpeningError(
  reason: unknown,
  t: ReturnType<typeof useI18n>["t"]
) {
  const name =
    reason && typeof reason === "object" && "name" in reason
      ? String(reason.name)
      : "";
  if (name === "InvalidPDFException") {
    return t("content.error.damaged");
  }
  if (name === "MissingPDFException" || name === "UnexpectedResponseException") {
    return t("content.error.read");
  }
  return t("content.error.review");
}

class ContentUserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ContentUserError";
  }
}
