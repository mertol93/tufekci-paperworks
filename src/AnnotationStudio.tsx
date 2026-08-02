import {
  type ChangeEvent,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useDialogFocus } from "./accessibility";
import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Circle,
  Copy,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  Highlighter,
  ImagePlus,
  Info,
  Loader2,
  Minus,
  MousePointer2,
  PenTool,
  Redo2,
  RectangleHorizontal,
  Save,
  Shapes,
  Stamp,
  Trash2,
  Type,
  Undo2,
  X
} from "lucide-react";
import {
  annotationChangeSet,
  annotationDraftFromInspection,
  annotationPayload,
  annotationBounds,
  commitAnnotationHistory,
  createAnnotationHistory,
  normalisedPoint,
  rectBetween,
  redoAnnotationHistory,
  translateAnnotation,
  undoAnnotationHistory,
  type AnnotationDraft,
  type AnnotationHistory,
  type AnnotationKind,
  type InspectedAnnotationPayload,
  type NormalisedPoint,
  type NormalisedRect
} from "./annotationDraft";
import {
  localiseAnnotationKind,
  localiseAnnotationStamp,
  localiseAnnotationWarnings
} from "./annotationLocalisation";
import { useI18n } from "./I18nProvider";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { PdfPageCanvas } from "./PdfPageCanvas";
import { localisePdfJobFailure } from "./pdfJobs";
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
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

type AnnotationStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PdfAnnotationInspection = {
  annotationsPerPage: number[];
  certificateSignature: boolean;
  editableAnnotationCount: number;
  editableAnnotations: InspectedAnnotationPayload[];
  editableAnnotationsPerPage: number[];
  existingAnnotationCount: number;
  fileName: string;
  pageCount: number;
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  readOnlyAnnotationCount: number;
  readOnlyAnnotationsPerPage: number[];
  warnings: string[];
  wasEncrypted: boolean;
};

type ExportPdfAnnotationsResult = {
  addedAnnotationCount: number;
  bytesWritten: number;
  encryption: "AES-256" | "None";
  outputPath: string;
  pageCount: number;
  removedAnnotationCount: number;
  totalAnnotationCount: number;
  updatedAnnotationCount: number;
  warnings: string[];
};

type AnnotationTool = "select" | AnnotationKind;

type PendingImage = {
  aspectRatio: number;
  dataUrl: string;
  name: string;
};

type DrawInteraction = {
  draft: AnnotationDraft;
  mode: "draw";
  pointerId: number;
  start: NormalisedPoint;
};

type MoveInteraction = {
  mode: "move";
  original: AnnotationDraft;
  pointerId: number;
  preview: AnnotationDraft;
  start: NormalisedPoint;
};

type PointerInteraction = DrawInteraction | MoveInteraction;

const colourSwatches = ["#235dd8", "#c83349", "#efb810", "#198754", "#20242c"];
const stampOptions = ["APPROVED", "DRAFT", "CONFIDENTIAL", "COPY", "REVIEWED"];
const toolOptions = [
  { id: "select", labelKey: "annotation.tool.select", icon: MousePointer2 },
  { id: "text", labelKey: "annotation.tool.text", icon: Type },
  { id: "highlight", labelKey: "annotation.tool.highlight", icon: Highlighter },
  { id: "stamp", labelKey: "annotation.tool.stamp", icon: Stamp },
  { id: "freehand", labelKey: "annotation.tool.freehand", icon: PenTool },
  { id: "rectangle", labelKey: "annotation.tool.rectangle", icon: RectangleHorizontal },
  { id: "ellipse", labelKey: "annotation.tool.ellipse", icon: Circle },
  { id: "line", labelKey: "annotation.tool.line", icon: Minus },
  { id: "image", labelKey: "annotation.tool.image", icon: ImagePlus }
] as const;

export function AnnotationStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: AnnotationStudioProps) {
  const { formatNumber, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState<"export" | "review" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [inspection, setInspection] = useState<PdfAnnotationInspection | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [history, setHistory] = useState<AnnotationHistory>(() => createAnnotationHistory());
  const [initialEditableAnnotations, setInitialEditableAnnotations] = useState<AnnotationDraft[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeTool, setActiveTool] = useState<AnnotationTool>("select");
  const [pageNumber, setPageNumber] = useState(1);
  const [interaction, setInteraction] = useState<PointerInteraction | null>(null);
  const [pendingImage, setPendingImage] = useState<PendingImage | null>(null);
  const [imageBusy, setImageBusy] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [exportResult, setExportResult] = useState<ExportPdfAnnotationsResult | null>(null);
  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const mountedRef = useRef(true);
  const requestRunRef = useRef(0);
  const nextIdRef = useRef(1);
  const previewHostRef = useRef<HTMLDivElement>(null);
  const [previewWidth, setPreviewWidth] = useState(680);
  const sourceList = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "annotation-source",
              label: fileNameFromPath(sourcePath),
              password,
              path: sourcePath
            }
          ]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, sourceList, "annotations");
  const annotationJob = usePdfJob<ExportPdfAnnotationsResult>(desktopMode, "annotations");
  const annotationInspectionJob = usePdfJob<PdfAnnotationInspection>(
    desktopMode,
    "annotation-inspection"
  );
  const operationBusy =
    busy !== null || annotationJob.isActive || annotationInspectionJob.isActive;
  const hasCertificateRisk = Boolean(
    inspection?.certificateSignature || editSafety.signedSources.length > 0
  );
  const certificateRiskAccepted = !hasCertificateRisk || signatureRiskAcknowledged;
  const selectedAnnotation = history.present.find((annotation) => annotation.id === selectedId) ?? null;
  const pageAnnotations = history.present.filter(
    (annotation) => annotation.pageNumber === pageNumber
  );
  const changes = useMemo(
    () => annotationChangeSet(initialEditableAnnotations, history.present),
    [history.present, initialEditableAnnotations]
  );
  const changeCount =
    changes.newAnnotations.length +
    changes.updatedAnnotations.length +
    changes.removedExistingAnnotationIds.length;
  const hiddenViewerAnnotationIds = useMemo(
    () =>
      inspection?.editableAnnotations
        .filter((annotation) => annotation.pageNumber === pageNumber)
        .map((annotation) => annotation.viewerAnnotationId) ?? [],
    [inspection, pageNumber]
  );
  const visibleAnnotations = useMemo(() => {
    if (!interaction) {
      return history.present;
    }
    if (interaction.mode === "draw") {
      return [...history.present, interaction.draft];
    }
    return history.present.map((annotation) =>
      annotation.id === interaction.original.id ? interaction.preview : annotation
    );
  }, [history.present, interaction]);
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      inspection &&
      changeCount > 0 &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      busy === null &&
      !annotationJob.isActive &&
      !imageBusy &&
      !interaction
  );
  const resetExportOutcome = () => {
    setExportResult(null);
    setJobNotice(null);
    annotationJob.clearJob();
  };

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

  const closeWorkspace = useCallback(() => {
    requestRunRef.current += 1;
    const task = loadingTaskRef.current;
    loadingTaskRef.current = null;
    void task?.destroy();
    setWorkspaceOpen(false);
    setInspection(null);
    setPdfDocument(null);
    setHistory(createAnnotationHistory());
    setInitialEditableAnnotations([]);
    setSelectedId(null);
    setActiveTool("select");
    setPageNumber(1);
    setInteraction(null);
    setPendingImage(null);
    setExportResult(null);
    annotationInspectionJob.clearJob();
  }, [annotationInspectionJob.clearJob]);

  useEffect(() => {
    if (initialSourcePath) {
      closeWorkspace();
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      setError(null);
    }
  }, [closeWorkspace, initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [password, sourcePath]);

  useEffect(() => {
    const job = annotationJob.job;
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
      setJobNotice(t("annotation.export.cancelled"));
    } else if (job.status === "failed") {
      setExportResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [annotationJob.job?.jobId, annotationJob.job?.status, t]);

  useEffect(() => {
    if (!workspaceOpen || !previewHostRef.current || !("ResizeObserver" in window)) {
      return;
    }
    const host = previewHostRef.current;
    const update = () => setPreviewWidth(Math.max(280, Math.min(760, host.clientWidth - 32)));
    update();
    const observer = new ResizeObserver(update);
    observer.observe(host);
    return () => observer.disconnect();
  }, [workspaceOpen]);

  useEffect(() => {
    if (selectedId && !history.present.some((annotation) => annotation.id === selectedId)) {
      setSelectedId(null);
    }
  }, [history.present, selectedId]);

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
          event.shiftKey ? redoAnnotationHistory(current) : undoAnnotationHistory(current)
        );
        resetExportOutcome();
        setInteraction(null);
        return;
      }
      if (
        (event.ctrlKey || event.metaKey) &&
        !editingText &&
        event.key.toLowerCase() === "y"
      ) {
        event.preventDefault();
        setHistory(redoAnnotationHistory);
        resetExportOutcome();
        setInteraction(null);
        return;
      }
      if (!editingText && (event.key === "Delete" || event.key === "Backspace") && selectedId) {
        event.preventDefault();
        deleteSelected();
        return;
      }
      if (event.key === "Escape") {
        if (interaction) {
          setInteraction(null);
        } else if (activeTool !== "select") {
          setActiveTool("select");
        } else {
          closeWorkspace();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    activeTool,
    closeWorkspace,
    interaction,
    operationBusy,
    selectedId,
    workspaceOpen
  ]);

  const commitAnnotations = useCallback(
    (updater: (annotations: AnnotationDraft[]) => AnnotationDraft[]) => {
      setHistory((current) => commitAnnotationHistory(current, updater(current.present)));
      setExportResult(null);
      setJobNotice(null);
      annotationJob.clearJob();
    },
    [annotationJob.clearJob]
  );

  const chooseSource = async () => {
    if (operationBusy) {
      return;
    }
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("annotation.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("annotation.dialog.choose")
      });
      if (typeof selected === "string" && mountedRef.current) {
        closeWorkspace();
        setSourcePath(selected);
        setPassword("");
        setSignatureRiskAcknowledged(false);
        setOutputProtection(createOutputProtectionDraft());
        resetExportOutcome();
      }
    } catch (reason) {
      if (mountedRef.current) {
        setError(t("annotation.error.choose"));
      }
    }
  };

  const reviewAnnotations = async () => {
    if (!desktopMode || !sourcePath || operationBusy) {
      return;
    }
    const runId = requestRunRef.current + 1;
    requestRunRef.current = runId;
    setBusy("review");
    setReviewCancelBusy(false);
    setError(null);
    resetExportOutcome();
    annotationInspectionJob.clearJob();
    const previousTask = loadingTaskRef.current;
    loadingTaskRef.current = null;
    if (previousTask) {
      await previousTask.destroy();
    }
    if (!mountedRef.current || requestRunRef.current !== runId) {
      return;
    }

    let task: ReturnType<typeof createPdfLoadingTask> | null = null;
    try {
      const [report, source] = await Promise.all([
        annotationInspectionJob.startJobAndWait({
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
        throw new AnnotationUserError(t("annotation.error.sourceChanged"));
      }
      task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure = "";
      task.onPassword = (_updatePassword: (value: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? t("annotation.error.passwordIncorrect")
          : t("annotation.error.passwordRequired");
        void task?.destroy();
      };
      let document: PDFDocumentProxy;
      try {
        document = await task.promise;
      } catch (reason) {
        if (passwordFailure) {
          throw new Error(passwordFailure);
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
        throw new AnnotationUserError(t("annotation.error.pageCountMismatch"));
      }
      const editableAnnotations = report.editableAnnotations.map(annotationDraftFromInspection);
      setInspection(report);
      setPdfDocument(document);
      setInitialEditableAnnotations(editableAnnotations);
      setHistory(createAnnotationHistory(editableAnnotations));
      setSelectedId(null);
      setActiveTool("select");
      setPageNumber(1);
      setInteraction(null);
      setPendingImage(null);
      setWorkspaceOpen(true);
      setSignatureRiskAcknowledged(false);
      annotationInspectionJob.clearJob();
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
            ? t("annotation.review.cancelled")
            : reason instanceof AnnotationUserError
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

  const updateSelected = (updates: Partial<AnnotationDraft>) => {
    if (!selectedId) {
      return;
    }
    commitAnnotations((annotations) =>
      annotations.map((annotation) =>
        annotation.id === selectedId ? { ...annotation, ...updates } : annotation
      )
    );
  };

  const deleteSelected = () => {
    if (!selectedId) {
      return;
    }
    commitAnnotations((annotations) =>
      annotations.filter((annotation) => annotation.id !== selectedId)
    );
    setSelectedId(null);
  };

  const duplicateSelected = () => {
    if (!selectedAnnotation) {
      return;
    }
    const duplicate = translateAnnotation(
      {
        ...selectedAnnotation,
        id: nextAnnotationId(nextIdRef),
        points: selectedAnnotation.points.map((point) => ({ ...point })),
        rect: selectedAnnotation.rect ? { ...selectedAnnotation.rect } : null,
        sourceAnnotationId: null,
        start: selectedAnnotation.start ? { ...selectedAnnotation.start } : null,
        end: selectedAnnotation.end ? { ...selectedAnnotation.end } : null,
        viewerAnnotationId: null
      },
      0.018,
      0.018
    );
    commitAnnotations((annotations) => [...annotations, duplicate]);
    setSelectedId(duplicate.id);
  };

  const changePage = (next: number) => {
    if (!inspection) {
      return;
    }
    setPageNumber(Math.max(1, Math.min(inspection.pageCount, Math.round(next) || 1)));
    setInteraction(null);
    setSelectedId(null);
  };

  const startDrawing = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (activeTool === "select" || busy || imageBusy) {
      if (activeTool === "select") {
        setSelectedId(null);
      }
      return;
    }
    if (activeTool === "image" && !pendingImage) {
      setError(t("annotation.error.imageRequired"));
      return;
    }
    const start = eventPoint(event, event.currentTarget);
    const draft = createDraft(
      activeTool,
      pageNumber,
      start,
      nextAnnotationId(nextIdRef),
      pendingImage,
      t("annotation.default.text")
    );
    event.currentTarget.setPointerCapture(event.pointerId);
    setError(null);
    setSelectedId(draft.id);
    setInteraction({ draft, mode: "draw", pointerId: event.pointerId, start });
  };

  const startMoving = (
    event: ReactPointerEvent<SVGGElement>,
    annotation: AnnotationDraft
  ) => {
    if (activeTool !== "select" || busy) {
      return;
    }
    const svg = event.currentTarget.ownerSVGElement;
    if (!svg) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    svg.setPointerCapture(event.pointerId);
    const start = eventPoint(event, svg);
    setSelectedId(annotation.id);
    setInteraction({
      mode: "move",
      original: annotation,
      pointerId: event.pointerId,
      preview: annotation,
      start
    });
  };

  const continuePointer = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!interaction || interaction.pointerId !== event.pointerId) {
      return;
    }
    const point = eventPoint(event, event.currentTarget);
    setInteraction((current) => {
      if (!current || current.pointerId !== event.pointerId) {
        return current;
      }
      if (current.mode === "move") {
        return {
          ...current,
          preview: translateAnnotation(
            current.original,
            point.x - current.start.x,
            point.y - current.start.y
          )
        };
      }
      return {
        ...current,
        draft: updateDrawingDraft(current.draft, current.start, point)
      };
    });
  };

  const finishPointer = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!interaction || interaction.pointerId !== event.pointerId) {
      return;
    }
    const point = eventPoint(event, event.currentTarget);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (interaction.mode === "move") {
      const preview = translateAnnotation(
        interaction.original,
        point.x - interaction.start.x,
        point.y - interaction.start.y
      );
      if (
        Math.abs(point.x - interaction.start.x) > 0.0001 ||
        Math.abs(point.y - interaction.start.y) > 0.0001
      ) {
        commitAnnotations((annotations) =>
          annotations.map((annotation) =>
            annotation.id === interaction.original.id ? preview : annotation
          )
        );
      }
      setInteraction(null);
      return;
    }
    const finalDraft = updateDrawingDraft(interaction.draft, interaction.start, point);
    const completed = completeDrawingDraft(
      finalDraft,
      interaction.start,
      pendingImage
    );
    if (completed) {
      commitAnnotations((annotations) => [...annotations, completed]);
      setSelectedId(completed.id);
      if (completed.kind === "image") {
        setPendingImage(null);
        setActiveTool("select");
      }
    } else {
      setSelectedId(null);
    }
    setInteraction(null);
  };

  const handleImage = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }
    setImageBusy(true);
    setError(null);
    try {
      const image = await prepareAnnotationImage(file, t);
      if (mountedRef.current) {
        setPendingImage(image);
        setActiveTool("image");
      }
    } catch (reason) {
      if (mountedRef.current) {
        setPendingImage(null);
        setError(
          reason instanceof AnnotationUserError
            ? reason.message
            : t("annotation.error.imagePrepare")
        );
      }
    } finally {
      if (mountedRef.current) {
        setImageBusy(false);
      }
    }
  };

  const exportAnnotations = async () => {
    if (!canExport || !sourcePath || !inspection) {
      return;
    }
    const runId = requestRunRef.current;
    setBusy("export");
    setError(null);
    resetExportOutcome();
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath),
        filters: [{ name: t("annotation.dialog.filter"), extensions: ["pdf"] }],
        title: t("annotation.dialog.save")
      });
      if (typeof outputPath !== "string") {
        return;
      }
      if (!mountedRef.current || requestRunRef.current !== runId) {
        return;
      }
      await annotationJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        annotations: changes.newAnnotations.map(annotationPayload),
        expectedSourceModifiedAtMs: inspection.sourceModifiedAtMs,
        expectedSourceSize: inspection.sourceSize,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        removedExistingAnnotationIds: changes.removedExistingAnnotationIds,
        updatedAnnotations: changes.updatedAnnotations.map(annotationPayload)
      });
    } catch (reason) {
      if (mountedRef.current && requestRunRef.current === runId) {
        setError(t("annotation.error.export"));
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setBusy(null);
      }
    }
  };

  const cancelAnnotationExport = async () => {
    if (!annotationJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await annotationJob.cancelJob();
    } catch {
      setCancelBusy(false);
      setError(t("annotation.error.exportCancel"));
    }
  };

  const cancelAnnotationReview = async () => {
    if (!annotationInspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await annotationInspectionJob.cancelJob();
    } catch {
      setError(t("annotation.error.reviewCancel"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    resetExportOutcome();
  };

  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    escapeDisabled: operationBusy,
    onEscape: () => {
      if (interaction) {
        setInteraction(null);
      } else if (activeTool !== "select") {
        setActiveTool("select");
      } else {
        closeWorkspace();
      }
    }
  });

  return (
    <>
      <section className="annotation-studio">
        <div className="annotation-heading">
          <div>
            <h3>{t("annotation.heading.title")}</h3>
            <p>{t("annotation.heading.description")}</p>
          </div>
          <Shapes size={18} aria-hidden="true" />
        </div>
        <button
          className="wide-button"
          disabled={!desktopMode || operationBusy}
          onClick={() => void chooseSource()}
          type="button"
        >
          <FolderOpen size={17} aria-hidden="true" />
          {sourcePath
            ? t("annotation.action.chooseAnother")
            : t("annotation.action.choose")}
        </button>
        {sourcePath ? (
          <div className="annotation-source">
            <FileText size={17} aria-hidden="true" />
            <span>
              <strong>{fileNameFromPath(sourcePath)}</strong>
              <small title={sourcePath}>{sourcePath}</small>
            </span>
          </div>
        ) : null}
        {sourcePath ? (
          <label className="annotation-password">
            <span>
              {t("annotation.password.label")} <small>{t("common.optional")}</small>
            </span>
            <span>
              <input
                autoComplete="off"
                disabled={operationBusy}
                onChange={(event) => {
                  setPassword(event.target.value);
                  resetExportOutcome();
                }}
                placeholder={t("annotation.password.placeholder")}
                type={showPassword ? "text" : "password"}
                value={password}
              />
              <button
                aria-label={
                  showPassword
                    ? t("annotation.password.hide")
                    : t("annotation.password.show")
                }
                className="icon-button"
                disabled={operationBusy}
                onClick={() => setShowPassword((visible) => !visible)}
                title={
                  showPassword
                    ? t("annotation.password.hide")
                    : t("annotation.password.show")
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
            <span>{t("annotation.desktopOnly")}</span>
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
          onClick={() => void reviewAnnotations()}
          type="button"
        >
          {busy === "review" || annotationInspectionJob.isActive ? (
            <Loader2 className="spin" size={17} aria-hidden="true" />
          ) : (
            <Shapes size={17} aria-hidden="true" />
          )}
          {busy === "review" || annotationInspectionJob.isActive
            ? t("annotation.action.opening")
            : t("annotation.action.open")}
        </button>
        {!workspaceOpen && annotationInspectionJob.job ? (
          <PdfJobProgress
            cancelling={reviewCancelBusy}
            connectionError={annotationInspectionJob.connectionError}
            job={annotationInspectionJob.job}
            onCancel={() => void cancelAnnotationReview()}
            onRetry={() => void reviewAnnotations()}
            retryDisabled={!desktopMode || !sourcePath || operationBusy}
          />
        ) : null}
        {!workspaceOpen && annotationJob.job ? (
          <PdfJobProgress
            cancelling={cancelBusy}
            connectionError={annotationJob.connectionError}
            job={annotationJob.job}
            onCancel={() => void cancelAnnotationExport()}
            onRetry={() => void exportAnnotations()}
            retryDisabled={!canExport}
          />
        ) : null}
        {!workspaceOpen &&
        !annotationInspectionJob.job &&
        !annotationJob.isActive &&
        annotationJob.connectionError ? (
          <div className="engine-state is-info" role="status">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t("job.connectionError")}</span>
          </div>
        ) : null}
        {!workspaceOpen && jobNotice ? (
          <div className="engine-state is-info" role="status">
            <Info size={16} aria-hidden="true" />
            <span>{jobNotice}</span>
          </div>
        ) : null}
        {!workspaceOpen && exportResult ? (
          <AnnotationExportResultPanel
            formatNumber={formatNumber}
            result={exportResult}
            t={t}
          />
        ) : null}
      </section>

      {workspaceOpen && inspection && pdfDocument ? (
        <div className="dialog-backdrop annotation-backdrop" role="presentation">
          <section
            aria-labelledby="annotation-dialog-title"
            aria-modal="true"
            className="annotation-dialog"
            data-dialog-root
            ref={dialogRef}
            role="dialog"
            tabIndex={-1}
          >
            <header>
              <div className="dialog-icon" aria-hidden="true">
                <Shapes size={24} />
              </div>
              <div>
                <span className="eyebrow">{t("annotation.workspace.eyebrow")}</span>
                <h2 id="annotation-dialog-title">{inspection.fileName}</h2>
              </div>
              <div className="annotation-header-actions">
                <span>
                  {t("annotation.workspace.summary", {
                    editable: formatNumber(inspection.editableAnnotationCount),
                    pending: formatNumber(changeCount),
                    readOnly: formatNumber(inspection.readOnlyAnnotationCount)
                  })}
                </span>
                <button
                  className="primary"
                  disabled={!canExport}
                  onClick={() => void exportAnnotations()}
                  type="button"
                >
                  {operationBusy ? (
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                  ) : (
                    <Save size={16} aria-hidden="true" />
                  )}
                  {annotationJob.isActive
                    ? t("annotation.action.exporting")
                    : busy === "export"
                      ? t("annotation.action.choosing")
                      : t("annotation.action.save")}
                </button>
                <button
                  aria-label={t("annotation.action.close")}
                  className="icon-button"
                  data-dialog-initial-focus
                  disabled={operationBusy}
                  onClick={closeWorkspace}
                  title={t("annotation.action.close")}
                  type="button"
                >
                  <X size={18} aria-hidden="true" />
                </button>
              </div>
            </header>

            {annotationJob.job ? (
              <PdfJobProgress
                cancelling={cancelBusy}
                connectionError={annotationJob.connectionError}
                job={annotationJob.job}
                onCancel={() => void cancelAnnotationExport()}
                onRetry={() => void exportAnnotations()}
                retryDisabled={!canExport}
              />
            ) : null}
            {!annotationJob.isActive && annotationJob.connectionError ? (
              <div className="annotation-warning" role="status">
                <AlertCircle size={16} aria-hidden="true" />
                <span>{t("job.connectionError")}</span>
              </div>
            ) : null}
            {jobNotice ? (
              <div className="annotation-warning" role="status">
                <Info size={16} aria-hidden="true" />
                <span>{jobNotice}</span>
              </div>
            ) : null}

            <fieldset className="annotation-editing-fieldset" disabled={operationBusy}>
            <div
              className="annotation-toolbar"
              role="toolbar"
              aria-label={t("annotation.toolbar.aria")}
            >
              <div className="annotation-tool-list">
                {toolOptions.map((tool) => {
                  const Icon = tool.icon;
                  const label = t(tool.labelKey);
                  return (
                    <button
                      aria-label={label}
                      aria-pressed={activeTool === tool.id}
                      className={activeTool === tool.id ? "is-active" : undefined}
                      disabled={operationBusy || imageBusy}
                      key={tool.id}
                      onClick={() => {
                        setActiveTool(tool.id);
                        setInteraction(null);
                      }}
                      title={label}
                      type="button"
                    >
                      <Icon size={17} aria-hidden="true" />
                      <span>{label}</span>
                    </button>
                  );
                })}
              </div>
              <div className="annotation-history-actions">
                <label
                  className={imageBusy ? "button-like is-disabled" : "button-like"}
                  title={t("annotation.image.choose")}
                >
                  {imageBusy ? (
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                  ) : (
                    <ImagePlus size={16} aria-hidden="true" />
                  )}
                  <span>
                    {pendingImage ? pendingImage.name : t("annotation.image.choose")}
                  </span>
                  <input
                    accept="image/png,image/jpeg,image/webp,image/bmp,image/gif"
                    className="visually-hidden"
                    disabled={operationBusy || imageBusy}
                    onChange={(event) => void handleImage(event)}
                    type="file"
                  />
                </label>
                <button
                  aria-label={t("annotation.action.undoAria")}
                  className="icon-button"
                  disabled={history.past.length === 0 || operationBusy}
                  onClick={() => {
                    setHistory(undoAnnotationHistory);
                    setInteraction(null);
                    resetExportOutcome();
                  }}
                  title={t("common.undo")}
                  type="button"
                >
                  <Undo2 size={17} aria-hidden="true" />
                </button>
                <button
                  aria-label={t("annotation.action.redoAria")}
                  className="icon-button"
                  disabled={history.future.length === 0 || operationBusy}
                  onClick={() => {
                    setHistory(redoAnnotationHistory);
                    setInteraction(null);
                    resetExportOutcome();
                  }}
                  title={t("common.redo")}
                  type="button"
                >
                  <Redo2 size={17} aria-hidden="true" />
                </button>
              </div>
            </div>

            <div className="annotation-workspace">
              <main className="annotation-canvas-panel">
                <div className="annotation-page-nav">
                  <button
                    aria-label={t("common.previousPage")}
                    className="icon-button"
                    disabled={pageNumber <= 1}
                    onClick={() => changePage(pageNumber - 1)}
                    title={t("common.previousPage")}
                    type="button"
                  >
                    <ChevronLeft size={17} aria-hidden="true" />
                  </button>
                  <label>
                    <span>{t("common.page")}</span>
                    <input
                      max={inspection.pageCount}
                      min={1}
                      onChange={(event) => changePage(Number(event.target.value))}
                      type="number"
                      value={pageNumber}
                    />
                    <span>
                      {t("common.ofCount", {
                        count: formatNumber(inspection.pageCount)
                      })}
                    </span>
                  </label>
                  <button
                    aria-label={t("common.nextPage")}
                    className="icon-button"
                    disabled={pageNumber >= inspection.pageCount}
                    onClick={() => changePage(pageNumber + 1)}
                    title={t("common.nextPage")}
                    type="button"
                  >
                    <ChevronRight size={17} aria-hidden="true" />
                  </button>
                  <span>
                    {t("annotation.page.summary", {
                      editable: formatNumber(pageAnnotations.length),
                      readOnly: formatNumber(
                        inspection.readOnlyAnnotationsPerPage[pageNumber - 1] ?? 0
                      )
                    })}
                  </span>
                </div>
                <div className="annotation-preview-host" ref={previewHostRef}>
                  <div className="annotation-page-surface">
                    <PdfPageCanvas
                      document={pdfDocument}
                      hiddenAnnotationIds={hiddenViewerAnnotationIds}
                      pageNumber={pageNumber}
                      targetWidth={previewWidth}
                      variant="page"
                    />
                    <svg
                      aria-label={t("annotation.canvas.aria", {
                        page: formatNumber(pageNumber)
                      })}
                      className={`annotation-overlay is-tool-${activeTool}`}
                      onPointerCancel={() => setInteraction(null)}
                      onPointerDown={startDrawing}
                      onPointerMove={continuePointer}
                      onPointerUp={finishPointer}
                      preserveAspectRatio="none"
                      role="application"
                      viewBox="0 0 1000 1000"
                    >
                      {visibleAnnotations
                        .filter((annotation) => annotation.pageNumber === pageNumber)
                        .map((annotation) => (
                          <AnnotationMark
                            annotation={annotation}
                            defaultText={t("annotation.default.text")}
                            key={annotation.id}
                            onPointerDown={startMoving}
                            selected={annotation.id === selectedId}
                          />
                        ))}
                    </svg>
                  </div>
                </div>
              </main>

              <aside className="annotation-inspector">
                <section className="annotation-list-panel">
                  <header>
                    <strong>{t("annotation.list.title")}</strong>
                    <small>
                      {t(
                        pageAnnotations.length === 1
                          ? "annotation.list.count.one"
                          : "annotation.list.count.other",
                        { count: formatNumber(pageAnnotations.length) }
                      )}
                    </small>
                  </header>
                  <div className="annotation-list">
                    {pageAnnotations.length > 0 ? (
                      pageAnnotations.map((annotation, index) => (
                        <button
                          aria-current={annotation.id === selectedId ? "true" : undefined}
                          className={annotation.id === selectedId ? "is-active" : undefined}
                          key={annotation.id}
                          onClick={() => {
                            setSelectedId(annotation.id);
                            setActiveTool("select");
                          }}
                          type="button"
                        >
                          <AnnotationKindIcon kind={annotation.kind} />
                          <span>
                            <strong>{localiseAnnotationKind(annotation.kind, t)}</strong>
                            <small>
                              {annotation.sourceAnnotationId
                                ? changes.updatedAnnotations.some(
                                    (updated) =>
                                      updated.sourceAnnotationId === annotation.sourceAnnotationId
                                  )
                                  ? t("annotation.item.changed")
                                  : t("annotation.item.existing")
                                : t("annotation.item.new")}{" "}
                              |{" "}
                              {annotationSummary(annotation, t, formatNumber)}
                            </small>
                          </span>
                          <em>{index + 1}</em>
                        </button>
                      ))
                    ) : (
                      <div className="annotation-list-empty">
                        <Shapes size={22} aria-hidden="true" />
                        <strong>{t("annotation.list.empty")}</strong>
                      </div>
                    )}
                  </div>
                </section>

                {selectedAnnotation ? (
                  <section className="annotation-properties">
                    <header>
                      <div>
                        <span className="eyebrow">
                          {selectedAnnotation.sourceAnnotationId
                            ? t("annotation.item.existingLong")
                            : t("annotation.item.newLong")}
                        </span>
                        <strong>{localiseAnnotationKind(selectedAnnotation.kind, t)}</strong>
                      </div>
                      <div>
                        <button
                          aria-label={t("annotation.action.duplicate")}
                          className="icon-button"
                          onClick={duplicateSelected}
                          title={t("annotation.action.duplicate")}
                          type="button"
                        >
                          <Copy size={16} aria-hidden="true" />
                        </button>
                        <button
                          aria-label={t("annotation.action.delete")}
                          className="icon-button is-danger"
                          onClick={deleteSelected}
                          title={t("annotation.action.delete")}
                          type="button"
                        >
                          <Trash2 size={16} aria-hidden="true" />
                        </button>
                      </div>
                    </header>

                    {selectedAnnotation.kind === "text" ? (
                      <label>
                        <span>{t("annotation.property.text")}</span>
                        <textarea
                          maxLength={4096}
                          onChange={(event) => updateSelected({ text: event.target.value })}
                          rows={4}
                          value={selectedAnnotation.text ?? ""}
                        />
                      </label>
                    ) : null}

                    {selectedAnnotation.kind === "stamp" ? (
                      <label>
                        <span>{t("annotation.property.stamp")}</span>
                        <select
                          onChange={(event) => updateSelected({ stamp: event.target.value })}
                          value={selectedAnnotation.stamp ?? stampOptions[0]}
                        >
                          {selectedAnnotation.stamp &&
                          !stampOptions.includes(selectedAnnotation.stamp) ? (
                            <option value={selectedAnnotation.stamp}>
                              {selectedAnnotation.stamp}
                            </option>
                          ) : null}
                          {stampOptions.map((stamp) => (
                            <option key={stamp} value={stamp}>
                              {localiseAnnotationStamp(stamp, t)}
                            </option>
                          ))}
                        </select>
                      </label>
                    ) : null}

                    <div className="annotation-colour-control">
                      <span>{t("annotation.property.colour")}</span>
                      <div className="annotation-swatches">
                        {colourSwatches.map((colour) => (
                          <button
                            aria-label={t("annotation.colour.use", { colour })}
                            aria-pressed={selectedAnnotation.colour === colour}
                            className={selectedAnnotation.colour === colour ? "is-active" : undefined}
                            key={colour}
                            onClick={() => updateSelected({ colour })}
                            style={{ backgroundColor: colour }}
                            title={colour}
                            type="button"
                          />
                        ))}
                        <input
                          aria-label={t("annotation.colour.custom")}
                          onChange={(event) => updateSelected({ colour: event.target.value })}
                          title={t("annotation.colour.custom")}
                          type="color"
                          value={selectedAnnotation.colour}
                        />
                      </div>
                    </div>

                    {selectedAnnotation.kind === "rectangle" ||
                    selectedAnnotation.kind === "ellipse" ? (
                      <label className="annotation-fill-control">
                        <input
                          checked={selectedAnnotation.fillColour !== null}
                          onChange={(event) =>
                            updateSelected({
                              fillColour: event.target.checked
                                ? selectedAnnotation.fillColour ?? "#dce7fb"
                                : null
                            })
                          }
                          type="checkbox"
                        />
                        {t("annotation.property.fill")}
                        {selectedAnnotation.fillColour ? (
                          <input
                            aria-label={t("annotation.colour.fill")}
                            onChange={(event) =>
                              updateSelected({ fillColour: event.target.value })
                            }
                            type="color"
                            value={selectedAnnotation.fillColour}
                          />
                        ) : null}
                      </label>
                    ) : null}

                    <label className="annotation-range-control">
                      <span>
                        {t("annotation.property.opacity")}{" "}
                        <output>
                          {formatNumber(Math.round(selectedAnnotation.opacity * 100))}%
                        </output>
                      </span>
                      <input
                        max={100}
                        min={5}
                        onChange={(event) =>
                          updateSelected({ opacity: Number(event.target.value) / 100 })
                        }
                        type="range"
                        value={Math.round(selectedAnnotation.opacity * 100)}
                      />
                    </label>

                    {selectedAnnotation.kind !== "highlight" &&
                    selectedAnnotation.kind !== "image" &&
                    selectedAnnotation.kind !== "text" &&
                    selectedAnnotation.kind !== "stamp" ? (
                      <label className="annotation-range-control">
                        <span>
                          {t("annotation.property.lineWidth")}{" "}
                          <output>
                            {formatNumber(selectedAnnotation.lineWidth, {
                              minimumFractionDigits: 1,
                              maximumFractionDigits: 1
                            })}{" "}
                            pt
                          </output>
                        </span>
                        <input
                          max={20}
                          min={0.5}
                          onChange={(event) =>
                            updateSelected({ lineWidth: Number(event.target.value) })
                          }
                          step={0.5}
                          type="range"
                          value={selectedAnnotation.lineWidth}
                        />
                      </label>
                    ) : null}

                    {selectedAnnotation.kind === "text" ? (
                      <label className="annotation-range-control">
                        <span>
                          {t("annotation.property.fontSize")}{" "}
                          <output>
                            {formatNumber(selectedAnnotation.fontSize)} pt
                          </output>
                        </span>
                        <input
                          max={48}
                          min={6}
                          onChange={(event) =>
                            updateSelected({ fontSize: Number(event.target.value) })
                          }
                          type="range"
                          value={selectedAnnotation.fontSize}
                        />
                      </label>
                    ) : null}

                    {selectedAnnotation.kind === "image" ? (
                      <div className="annotation-image-name">
                        <ImagePlus size={16} aria-hidden="true" />
                        <span>{t("annotation.property.embeddedImage")}</span>
                      </div>
                    ) : null}
                  </section>
                ) : (
                  <section className="annotation-properties is-empty">
                    <MousePointer2 size={22} aria-hidden="true" />
                    <strong>{t("annotation.property.empty")}</strong>
                  </section>
                )}

                <div className="annotation-notices">
                  {localiseAnnotationWarnings(
                    inspection.warnings,
                    t,
                    formatNumber
                  ).map((warning) => (
                    <div className="annotation-warning" key={warning}>
                      <Info size={16} aria-hidden="true" />
                      <span>{warning}</span>
                    </div>
                  ))}
                  <PdfEditSafetyNotice
                    acknowledged={signatureRiskAcknowledged}
                    busy={operationBusy}
                    editSafety={editSafety}
                    onAcknowledgedChange={setSignatureRiskAcknowledged}
                    rewriteDescription={t("annotation.signature.rewrite")}
                  />
                  <OutputProtectionFields
                    disabled={operationBusy}
                    onChange={changeOutputProtection}
                    qpdfAvailable={qpdfAvailable}
                    value={outputProtection}
                  />
                  {error ? (
                    <div className="annotation-error" role="alert">
                      <AlertCircle size={16} aria-hidden="true" />
                      <span>{error}</span>
                    </div>
                  ) : null}
                  {exportResult ? (
                    <AnnotationExportResultPanel
                      formatNumber={formatNumber}
                      result={exportResult}
                      t={t}
                    />
                  ) : null}
                </div>
              </aside>
            </div>
            </fieldset>
          </section>
        </div>
      ) : null}
    </>
  );
}

function AnnotationExportResultPanel({
  formatNumber,
  result,
  t
}: {
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  result: ExportPdfAnnotationsResult;
  t: ReturnType<typeof useI18n>["t"];
}) {
  return (
    <div className="annotation-export-result">
      <CheckCircle2 size={18} aria-hidden="true" />
      <span>
        <strong>{t("annotation.result.title")}</strong>
        <small>
          {t("annotation.result.summary", {
            added: formatNumber(result.addedAnnotationCount),
            encryption:
              result.encryption === "AES-256"
                ? t("common.encryption.protected")
                : t("common.encryption.unprotected"),
            removed: formatNumber(result.removedAnnotationCount),
            size: formatBytes(result.bytesWritten, formatNumber),
            total: formatNumber(result.totalAnnotationCount),
            updated: formatNumber(result.updatedAnnotationCount)
          })}
        </small>
        <small title={result.outputPath}>{fileNameFromPath(result.outputPath)}</small>
        {localiseAnnotationWarnings(result.warnings, t, formatNumber).map((warning) => (
          <small key={warning}>{warning}</small>
        ))}
      </span>
    </div>
  );
}

function AnnotationMark({
  annotation,
  defaultText,
  onPointerDown,
  selected
}: {
  annotation: AnnotationDraft;
  defaultText: string;
  onPointerDown: (event: ReactPointerEvent<SVGGElement>, annotation: AnnotationDraft) => void;
  selected: boolean;
}) {
  const rect = annotationBounds(annotation);
  const x = rect.x * 1000;
  const y = rect.y * 1000;
  const width = rect.width * 1000;
  const height = rect.height * 1000;
  const lineWidth = Math.max(1.5, Math.min(24, annotation.lineWidth * 1.6));
  const common = {
    opacity: annotation.opacity,
    pointerEvents: "visiblePainted" as const,
    vectorEffect: "non-scaling-stroke" as const
  };
  return (
    <g
      className={selected ? "annotation-mark is-selected" : "annotation-mark"}
      data-kind={annotation.kind}
      onPointerDown={(event) => onPointerDown(event, annotation)}
    >
      {annotation.kind === "freehand" ? (
        <polyline
          {...common}
          fill="none"
          points={annotation.points.map((point) => `${point.x * 1000},${point.y * 1000}`).join(" ")}
          stroke={annotation.colour}
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={lineWidth}
        />
      ) : null}
      {annotation.kind === "line" && annotation.start && annotation.end ? (
        <line
          {...common}
          stroke={annotation.colour}
          strokeLinecap="round"
          strokeWidth={lineWidth}
          x1={annotation.start.x * 1000}
          x2={annotation.end.x * 1000}
          y1={annotation.start.y * 1000}
          y2={annotation.end.y * 1000}
        />
      ) : null}
      {annotation.kind === "highlight" && annotation.rect ? (
        <rect
          {...common}
          fill={annotation.colour}
          height={height}
          width={width}
          x={x}
          y={y}
        />
      ) : null}
      {annotation.kind === "rectangle" && annotation.rect ? (
        <rect
          {...common}
          fill={annotation.fillColour ?? "none"}
          height={height}
          stroke={annotation.colour}
          strokeWidth={lineWidth}
          width={width}
          x={x}
          y={y}
        />
      ) : null}
      {annotation.kind === "ellipse" && annotation.rect ? (
        <ellipse
          {...common}
          cx={x + width / 2}
          cy={y + height / 2}
          fill={annotation.fillColour ?? "none"}
          rx={width / 2}
          ry={height / 2}
          stroke={annotation.colour}
          strokeWidth={lineWidth}
        />
      ) : null}
      {annotation.kind === "text" && annotation.rect ? (
        <g {...common}>
          <rect
            fill="#fffef5"
            height={height}
            stroke={annotation.colour}
            strokeWidth={Math.max(1.5, lineWidth / 2)}
            width={width}
            x={x}
            y={y}
          />
          <text
            fill={annotation.colour}
            fontFamily="Arial, sans-serif"
            fontSize={Math.max(14, annotation.fontSize * 1.8)}
            x={x + 8}
            y={y + Math.max(20, annotation.fontSize * 2)}
          >
            {truncate(annotation.text ?? defaultText, 42)}
          </text>
        </g>
      ) : null}
      {annotation.kind === "stamp" && annotation.rect ? (
        <g {...common}>
          <rect
            fill="#ffffff"
            height={height}
            rx={6}
            stroke={annotation.colour}
            strokeWidth={Math.max(3, lineWidth)}
            width={width}
            x={x}
            y={y}
          />
          <text
            dominantBaseline="middle"
            fill={annotation.colour}
            fontFamily="Arial, sans-serif"
            fontSize={Math.max(18, Math.min(48, height * 0.4))}
            fontWeight="700"
            textAnchor="middle"
            x={x + width / 2}
            y={y + height / 2}
          >
            {annotation.stamp ?? "APPROVED"}
          </text>
        </g>
      ) : null}
      {annotation.kind === "image" && annotation.rect && annotation.imageDataUrl ? (
        <image
          {...common}
          height={height}
          href={annotation.imageDataUrl}
          preserveAspectRatio="none"
          width={width}
          x={x}
          y={y}
        />
      ) : null}
      {selected ? (
        <rect
          className="annotation-selection-box"
          fill="none"
          height={Math.max(4, height)}
          pointerEvents="none"
          stroke="#235dd8"
          strokeDasharray="7 5"
          strokeWidth={2}
          vectorEffect="non-scaling-stroke"
          width={Math.max(4, width)}
          x={x}
          y={y}
        />
      ) : null}
    </g>
  );
}

function AnnotationKindIcon({ kind }: { kind: AnnotationKind }) {
  const Icon =
    kind === "text"
      ? Type
      : kind === "highlight"
        ? Highlighter
        : kind === "stamp"
          ? Stamp
          : kind === "freehand"
            ? PenTool
            : kind === "rectangle"
              ? RectangleHorizontal
              : kind === "ellipse"
                ? Circle
                : kind === "line"
                  ? Minus
                  : ImagePlus;
  return <Icon size={15} aria-hidden="true" />;
}

function createDraft(
  kind: AnnotationKind,
  pageNumber: number,
  start: NormalisedPoint,
  id: string,
  image: PendingImage | null,
  defaultText: string
): AnnotationDraft {
  const colour =
    kind === "highlight" ? "#efc929" : kind === "stamp" ? "#c83349" : "#235dd8";
  const draft: AnnotationDraft = {
    colour,
    fillColour: null,
    fontSize: 14,
    id,
    imageDataUrl: kind === "image" ? image?.dataUrl ?? null : null,
    kind,
    lineWidth: kind === "freehand" ? 2.5 : 2,
    opacity: kind === "highlight" ? 0.45 : 0.9,
    pageNumber,
    points: kind === "freehand" ? [start] : [],
    rect:
      kind === "line" || kind === "freehand"
        ? null
        : { height: 0.002, width: 0.002, x: start.x, y: start.y },
    sourceAnnotationId: null,
    stamp: kind === "stamp" ? stampOptions[0] : null,
    start: kind === "line" ? start : null,
    end: kind === "line" ? start : null,
    text: kind === "text" ? defaultText : null,
    viewerAnnotationId: null
  };
  return draft;
}

function updateDrawingDraft(
  draft: AnnotationDraft,
  start: NormalisedPoint,
  point: NormalisedPoint
) {
  if (draft.kind === "freehand") {
    const last = draft.points[draft.points.length - 1] ?? start;
    if (draft.points.length >= 10_000 || Math.hypot(point.x - last.x, point.y - last.y) < 0.0015) {
      return draft;
    }
    return { ...draft, points: [...draft.points, point] };
  }
  if (draft.kind === "line") {
    return { ...draft, end: point };
  }
  return { ...draft, rect: rectBetween(start, point) };
}

function completeDrawingDraft(
  draft: AnnotationDraft,
  start: NormalisedPoint,
  image: PendingImage | null
) {
  if (draft.kind === "freehand") {
    return draft.points.length >= 2 ? draft : null;
  }
  if (draft.kind === "line") {
    if (!draft.end || Math.hypot(draft.end.x - start.x, draft.end.y - start.y) < 0.008) {
      return null;
    }
    return draft;
  }
  if (!draft.rect) {
    return null;
  }
  if (draft.rect.width >= 0.01 && draft.rect.height >= 0.01) {
    return draft;
  }
  const aspect = draft.kind === "image" ? image?.aspectRatio ?? 1.4 : defaultAspect(draft.kind);
  const width = Math.min(0.32, 1 - start.x);
  const height = Math.min(width / aspect, 1 - start.y);
  if (width < 0.01 || height < 0.01) {
    return null;
  }
  return { ...draft, rect: { height, width, x: start.x, y: start.y } };
}

function defaultAspect(kind: AnnotationKind) {
  if (kind === "highlight") {
    return 7;
  }
  if (kind === "stamp") {
    return 3;
  }
  if (kind === "text") {
    return 2.4;
  }
  return 1.4;
}

function eventPoint(
  event: ReactPointerEvent<SVGElement>,
  svg: SVGSVGElement
): NormalisedPoint {
  const bounds = svg.getBoundingClientRect();
  return normalisedPoint(
    bounds.width > 0 ? (event.clientX - bounds.left) / bounds.width : 0,
    bounds.height > 0 ? (event.clientY - bounds.top) / bounds.height : 0
  );
}

async function prepareAnnotationImage(
  file: File,
  t: ReturnType<typeof useI18n>["t"]
): Promise<PendingImage> {
  if (file.size === 0 || file.size > 40 * 1024 * 1024) {
    throw new AnnotationUserError(t("annotation.error.imageSize"));
  }
  let image: HTMLImageElement;
  try {
    image = await loadImage(file);
  } catch {
    throw new AnnotationUserError(t("annotation.error.imageFormat"));
  }
  const scale = Math.min(1, 2048 / Math.max(image.naturalWidth, image.naturalHeight));
  const width = Math.max(1, Math.round(image.naturalWidth * scale));
  const height = Math.max(1, Math.round(image.naturalHeight * scale));
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) {
    throw new AnnotationUserError(t("annotation.error.imagePrepare"));
  }
  canvas.width = width;
  canvas.height = height;
  context.drawImage(image, 0, 0, width, height);
  const dataUrl = canvas.toDataURL("image/png");
  if (dataUrl.length > 16 * 1024 * 1024) {
    throw new AnnotationUserError(t("annotation.error.imagePreparedSize"));
  }
  return { aspectRatio: width / height, dataUrl, name: file.name };
}

async function loadImage(file: File) {
  const url = URL.createObjectURL(file);
  try {
    return await new Promise<HTMLImageElement>((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () => reject(new Error());
      image.src = url;
    });
  } finally {
    URL.revokeObjectURL(url);
  }
}

function annotationSummary(
  annotation: AnnotationDraft,
  t: ReturnType<typeof useI18n>["t"],
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (annotation.kind === "text") {
    return truncate(annotation.text ?? t("annotation.default.text"), 30);
  }
  if (annotation.kind === "stamp") {
    return annotation.stamp
      ? localiseAnnotationStamp(annotation.stamp, t)
      : t("annotation.kind.stamp");
  }
  if (annotation.kind === "image") {
    return t("annotation.summary.image");
  }
  return t("annotation.summary.opacity", {
    percent: formatNumber(Math.round(annotation.opacity * 100))
  });
}

function nextAnnotationId(reference: { current: number }) {
  return `annotation-${reference.current++}`;
}

function suggestedOutputPath(sourcePath: string) {
  return sourcePath.replace(/\.pdf$/i, "-annotated.pdf");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || "Document.pdf";
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

function pdfOpeningError(
  reason: unknown,
  t: ReturnType<typeof useI18n>["t"]
) {
  const name =
    reason && typeof reason === "object" && "name" in reason
      ? String(reason.name)
      : "";
  if (name === "InvalidPDFException") {
    return t("annotation.error.damaged");
  }
  if (name === "MissingPDFException" || name === "UnexpectedResponseException") {
    return t("annotation.error.read");
  }
  return t("annotation.error.review");
}

class AnnotationUserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AnnotationUserError";
  }
}
