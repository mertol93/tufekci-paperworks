import {
  type CSSProperties,
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
  Crop,
  Droplets,
  Eye,
  EyeOff,
  FileOutput,
  FileText,
  FolderOpen,
  Hash,
  Info,
  Layers,
  Loader2,
  LockKeyhole,
  Ruler,
  Save,
  ShieldAlert,
  Type,
  X
} from "lucide-react";
import {
  colourToPdfComponents,
  computeFinishPreview,
  expandFinishTemplate,
  finishPaperPresets,
  formatBatesNumber,
  millimetresToPoints,
  parseFinishPageRange,
  pointsToMillimetres,
  type CropPoints,
  type FinishPageInfo,
  type ResizePoints
} from "./pageFinish";
import {
  localiseFinishPaperName,
  localiseFinishRangeError,
  localisePageFinishWarnings
} from "./pageFinishLocalisation";
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

type PageFinishStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PdfFinishingInspection = {
  annotationCount: number;
  certificateSignature: boolean;
  fileName: string;
  hasBookmarks: boolean;
  hasForms: boolean;
  pageCount: number;
  pages: FinishPageInfo[];
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  warnings: string[];
  wasEncrypted: boolean;
};

type ExportPdfFinishingResult = {
  adjustedAnnotationCount: number;
  batesNumberCount: number;
  bytesWritten: number;
  changedPageCount: number;
  croppedPageCount: number;
  encryption: "AES-256" | "None";
  markedPageCount: number;
  outputPath: string;
  pageCount: number;
  resizedPageCount: number;
  warnings: string[];
};

type PanelTab = "layout" | "marks" | "numbering";
type PageScope = "all" | "current" | "custom";
type Orientation = "portrait" | "landscape";
type Alignment = "left" | "centre" | "right";
type BatesPosition =
  | "topLeft"
  | "topCentre"
  | "topRight"
  | "bottomLeft"
  | "bottomCentre"
  | "bottomRight";

type CropMarginsMm = {
  bottom: number;
  left: number;
  right: number;
  top: number;
};

const colourSwatches = ["#20242c", "#687385", "#235dd8", "#147543", "#a52b2b"];

export function PageFinishStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: PageFinishStudioProps) {
  const { formatNumber, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState<"review" | "export" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [inspection, setInspection] = useState<PdfFinishingInspection | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [pageNumber, setPageNumber] = useState(1);
  const [panelTab, setPanelTab] = useState<PanelTab>("layout");
  const [pageScope, setPageScope] = useState<PageScope>("all");
  const [customRange, setCustomRange] = useState("1");
  const [cropEnabled, setCropEnabled] = useState(false);
  const [cropLinked, setCropLinked] = useState(true);
  const [cropMarginsMm, setCropMarginsMm] = useState<CropMarginsMm>({
    top: 0,
    right: 0,
    bottom: 0,
    left: 0
  });
  const [resizeEnabled, setResizeEnabled] = useState(false);
  const [paperPresetId, setPaperPresetId] = useState("a4");
  const [orientation, setOrientation] = useState<Orientation>("portrait");
  const [customWidthMm, setCustomWidthMm] = useState(210);
  const [customHeightMm, setCustomHeightMm] = useState(297);
  const [resizeMarginMm, setResizeMarginMm] = useState(10);
  const [watermarkEnabled, setWatermarkEnabled] = useState(false);
  const [watermarkText, setWatermarkText] = useState(() =>
    t("finish.default.watermark")
  );
  const [watermarkSize, setWatermarkSize] = useState(72);
  const [watermarkOpacity, setWatermarkOpacity] = useState(0.18);
  const [watermarkAngle, setWatermarkAngle] = useState(-35);
  const [watermarkColour, setWatermarkColour] = useState("#687385");
  const [watermarkOver, setWatermarkOver] = useState(false);
  const [headerFooterEnabled, setHeaderFooterEnabled] = useState(false);
  const [headerText, setHeaderText] = useState("{file}");
  const [footerText, setFooterText] = useState(() =>
    t("finish.default.footer")
  );
  const [headerAlignment, setHeaderAlignment] = useState<Alignment>("left");
  const [footerAlignment, setFooterAlignment] = useState<Alignment>("centre");
  const [headerFooterSize, setHeaderFooterSize] = useState(9);
  const [headerFooterMarginMm, setHeaderFooterMarginMm] = useState(8);
  const [headerFooterColour, setHeaderFooterColour] = useState("#20242c");
  const [batesEnabled, setBatesEnabled] = useState(false);
  const [batesPrefix, setBatesPrefix] = useState("TF-");
  const [batesSuffix, setBatesSuffix] = useState("");
  const [batesStart, setBatesStart] = useState(1);
  const [batesDigits, setBatesDigits] = useState(6);
  const [batesPosition, setBatesPosition] = useState<BatesPosition>("bottomRight");
  const [batesSize, setBatesSize] = useState(8);
  const [batesMarginMm, setBatesMarginMm] = useState(8);
  const [batesColour, setBatesColour] = useState("#20242c");
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [exportResult, setExportResult] = useState<ExportPdfFinishingResult | null>(null);
  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [previewWidth, setPreviewWidth] = useState(680);
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const mountedRef = useRef(true);
  const requestRunRef = useRef(0);
  const previewHostRef = useRef<HTMLDivElement>(null);
  const sourceList = useMemo(
    () =>
      sourcePath
        ? [{ id: "finish-source", label: fileNameFromPath(sourcePath), password, path: sourcePath }]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, sourceList, "page-finish");
  const finishingJob = usePdfJob<ExportPdfFinishingResult>(desktopMode, "finishing");
  const finishingInspectionJob = usePdfJob<PdfFinishingInspection>(
    desktopMode,
    "finishing-inspection"
  );
  const operationBusy =
    busy !== null || finishingJob.isActive || finishingInspectionJob.isActive;
  const effectivePageRange =
    pageScope === "all" ? "all" : pageScope === "current" ? String(pageNumber) : customRange;
  const parsedRange = useMemo(
    () => parseFinishPageRange(effectivePageRange, inspection?.pageCount ?? 0),
    [effectivePageRange, inspection?.pageCount]
  );
  const parsedRangeError = useMemo(
    () => localiseFinishRangeError(parsedRange.error, t),
    [parsedRange.error, t]
  );
  const selectedPageSet = useMemo(() => new Set(parsedRange.pages), [parsedRange.pages]);
  const selectedIndex = parsedRange.pages.indexOf(pageNumber);
  const currentPage = inspection?.pages[pageNumber - 1] ?? null;
  const selectedPreset =
    finishPaperPresets.find((preset) => preset.id === paperPresetId) ?? finishPaperPresets[0];
  const basePaper =
    selectedPreset.id === "custom"
      ? { widthMm: customWidthMm, heightMm: customHeightMm }
      : selectedPreset;
  const paperDimensionsMm =
    orientation === "portrait"
      ? {
          widthMm: Math.min(basePaper.widthMm, basePaper.heightMm),
          heightMm: Math.max(basePaper.widthMm, basePaper.heightMm)
        }
      : {
          widthMm: Math.max(basePaper.widthMm, basePaper.heightMm),
          heightMm: Math.min(basePaper.widthMm, basePaper.heightMm)
        };
  const cropPoints: CropPoints | null = cropEnabled
    ? {
        topPt: millimetresToPoints(cropMarginsMm.top),
        rightPt: millimetresToPoints(cropMarginsMm.right),
        bottomPt: millimetresToPoints(cropMarginsMm.bottom),
        leftPt: millimetresToPoints(cropMarginsMm.left)
      }
    : null;
  const resizePoints: ResizePoints | null = resizeEnabled
    ? {
        widthPt: millimetresToPoints(paperDimensionsMm.widthMm),
        heightPt: millimetresToPoints(paperDimensionsMm.heightMm),
        marginPt: millimetresToPoints(resizeMarginMm)
      }
    : null;
  const previewLayout = currentPage
    ? computeFinishPreview(currentPage, cropPoints, resizePoints)
    : null;
  const invalidLayoutPage = useMemo(
    () =>
      inspection?.pages.find(
        (page) =>
          selectedPageSet.has(page.pageNumber) &&
          !computeFinishPreview(page, cropPoints, resizePoints)
      ) ?? null,
    [cropPoints, inspection?.pages, resizePoints, selectedPageSet]
  );
  const hasCrop = Boolean(
    cropEnabled && Object.values(cropMarginsMm).some((value) => Number.isFinite(value) && value > 0)
  );
  const hasOperation = Boolean(
    hasCrop ||
      resizeEnabled ||
      (watermarkEnabled && watermarkText.trim()) ||
      (headerFooterEnabled && (headerText.trim() || footerText.trim())) ||
      batesEnabled
  );
  const settingsError = useMemo(() => {
    if (parsedRangeError) return parsedRangeError;
    if (!hasOperation) return t("finish.validation.operation");
    if (invalidLayoutPage) {
      return t("finish.validation.visibleArea", {
        page: formatNumber(invalidLayoutPage.pageNumber)
      });
    }
    if (watermarkEnabled && !watermarkText.trim()) {
      return t("finish.validation.watermarkText");
    }
    if (
      watermarkEnabled &&
      (!Number.isFinite(watermarkSize) || watermarkSize < 12 || watermarkSize > 240)
    ) {
      return t("finish.validation.watermarkSize");
    }
    if (
      watermarkEnabled &&
      (!Number.isFinite(watermarkOpacity) || watermarkOpacity < 0.05 || watermarkOpacity > 0.9)
    ) {
      return t("finish.validation.watermarkOpacity");
    }
    if (headerFooterEnabled && !headerText.trim() && !footerText.trim()) {
      return t("finish.validation.headerFooterText");
    }
    if (
      headerFooterEnabled &&
      (!Number.isFinite(headerFooterSize) || headerFooterSize < 6 || headerFooterSize > 36)
    ) {
      return t("finish.validation.headerFooterSize");
    }
    if (
      headerFooterEnabled &&
      inspection?.pages.some((page) => {
        if (!selectedPageSet.has(page.pageNumber)) return false;
        const layout = computeFinishPreview(page, cropPoints, resizePoints);
        return Boolean(
          layout &&
          millimetresToPoints(headerFooterMarginMm) + headerFooterSize > layout.outputHeightPt
        );
      })
    ) {
      return t("finish.validation.headerFooterMargin");
    }
    if (batesEnabled && (!Number.isSafeInteger(batesStart) || batesStart < 0)) {
      return t("finish.validation.batesStart");
    }
    if (batesEnabled && (!Number.isInteger(batesDigits) || batesDigits < 1 || batesDigits > 12)) {
      return t("finish.validation.batesDigits");
    }
    if (!Number.isFinite(batesSize) || batesSize < 6 || batesSize > 36) {
      return batesEnabled ? t("finish.validation.batesSize") : null;
    }
    return null;
  }, [
    batesDigits,
    batesEnabled,
    batesSize,
    batesStart,
    cropPoints,
    footerText,
    hasOperation,
    headerFooterEnabled,
    headerFooterMarginMm,
    headerFooterSize,
    headerText,
    invalidLayoutPage,
    inspection?.pages,
    formatNumber,
    parsedRangeError,
    resizePoints,
    selectedPageSet,
    watermarkEnabled,
    watermarkOpacity,
    watermarkSize,
    watermarkText,
    t
  ]);
  const hasCertificateRisk = Boolean(
    inspection?.certificateSignature || editSafety.signedSources.length > 0
  );
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      inspection &&
      pdfDocument &&
      !settingsError &&
      editSafety.isReady &&
      (!hasCertificateRisk || signatureRiskAcknowledged) &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      busy === null &&
      !finishingJob.isActive
  );
  const resetExportOutcome = () => {
    setExportResult(null);
    setJobNotice(null);
    finishingJob.clearJob();
  };
  const pageSelected = selectedPageSet.has(pageNumber);
  const headerPreview =
    inspection && pageSelected && headerFooterEnabled
      ? expandFinishTemplate(headerText, pageNumber, inspection.pageCount, inspection.fileName)
      : "";
  const footerPreview =
    inspection && pageSelected && headerFooterEnabled
      ? expandFinishTemplate(footerText, pageNumber, inspection.pageCount, inspection.fileName)
      : "";
  const batesPreview =
    pageSelected && batesEnabled && selectedIndex >= 0
      ? formatBatesNumber(batesPrefix, batesSuffix, batesStart, batesDigits, selectedIndex)
      : "";
  const cropBoundary = useMemo(() => {
    if (!currentPage || !previewLayout) return null;
    const left = previewLayout.sourceLeftPercent +
      ((cropPoints?.leftPt ?? 0) / currentPage.widthPt) * previewLayout.sourceWidthPercent;
    const top = previewLayout.sourceTopPercent +
      ((cropPoints?.topPt ?? 0) / currentPage.heightPt) * previewLayout.sourceHeightPercent;
    return {
      left,
      top,
      width: (previewLayout.cropWidthPt / currentPage.widthPt) * previewLayout.sourceWidthPercent,
      height: (previewLayout.cropHeightPt / currentPage.heightPt) * previewLayout.sourceHeightPercent
    };
  }, [cropPoints, currentPage, previewLayout]);

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
    setPageNumber(1);
    setExportResult(null);
    setError(null);
    finishingInspectionJob.clearJob();
  }, [finishingInspectionJob.clearJob]);

  useEffect(() => {
    if (initialSourcePath) {
      closeWorkspace();
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
    }
  }, [closeWorkspace, initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [password, sourcePath]);

  useEffect(() => {
    const job = finishingJob.job;
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
      setJobNotice(t("finish.export.cancelled"));
    } else if (job.status === "failed") {
      setExportResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [finishingJob.job?.jobId, finishingJob.job?.status, t]);

  useEffect(() => {
    resetExportOutcome();
  }, [
    batesColour,
    batesDigits,
    batesEnabled,
    batesMarginMm,
    batesPosition,
    batesPrefix,
    batesSize,
    batesStart,
    batesSuffix,
    cropEnabled,
    cropMarginsMm,
    effectivePageRange,
    footerAlignment,
    footerText,
    headerAlignment,
    headerFooterColour,
    headerFooterEnabled,
    headerFooterMarginMm,
    headerFooterSize,
    headerText,
    orientation,
    paperPresetId,
    resizeEnabled,
    resizeMarginMm,
    customHeightMm,
    customWidthMm,
    watermarkAngle,
    watermarkColour,
    watermarkEnabled,
    watermarkOpacity,
    watermarkOver,
    watermarkSize,
    watermarkText
  ]);

  useEffect(() => {
    if (!workspaceOpen || !previewHostRef.current || !("ResizeObserver" in window)) return;
    const host = previewHostRef.current;
    const update = () => setPreviewWidth(Math.max(280, Math.min(720, host.clientWidth - 28)));
    update();
    const observer = new ResizeObserver(update);
    observer.observe(host);
    return () => observer.disconnect();
  }, [workspaceOpen]);

  useEffect(() => {
    if (!workspaceOpen || operationBusy) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const editing = Boolean((event.target as HTMLElement | null)?.closest("input, textarea, select"));
      if (event.key === "Escape") {
        closeWorkspace();
      } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s" && !editing) {
        event.preventDefault();
        if (canExport) void exportFinishedPdf();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [canExport, closeWorkspace, operationBusy, workspaceOpen]);

  const chooseSource = async () => {
    if (operationBusy) return;
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("finish.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("finish.dialog.choose")
      });
      if (typeof selected === "string" && mountedRef.current) {
        closeWorkspace();
        setSourcePath(selected);
        setPassword("");
        setOutputProtection(createOutputProtectionDraft());
        resetExportOutcome();
      }
    } catch {
      if (mountedRef.current) setError(t("finish.error.choose"));
    }
  };

  const reviewFinishing = async () => {
    if (!desktopMode || !sourcePath || operationBusy) return;
    const runId = requestRunRef.current + 1;
    requestRunRef.current = runId;
    setBusy("review");
    setReviewCancelBusy(false);
    setError(null);
    resetExportOutcome();
    finishingInspectionJob.clearJob();
    const previousTask = loadingTaskRef.current;
    loadingTaskRef.current = null;
    if (previousTask) await previousTask.destroy();
    if (!mountedRef.current || requestRunRef.current !== runId) return;
    let task: ReturnType<typeof createPdfLoadingTask> | null = null;
    try {
      const [report, source] = await Promise.all([
        finishingInspectionJob.startJobAndWait({
          inputPassword: password || null,
          inputPath: sourcePath
        }),
        invoke<PdfRangeSource>("open_local_pdf", { path: sourcePath })
      ]);
      if (!mountedRef.current || requestRunRef.current !== runId) return;
      if (report.sourceSize !== source.size || report.sourceModifiedAtMs !== source.modifiedAtMs) {
        throw new PageFinishUserError(t("finish.error.sourceChanged"));
      }
      task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure = "";
      task.onPassword = (_updatePassword: (value: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? t("finish.error.passwordIncorrect")
          : t("finish.error.passwordRequired");
        void task?.destroy();
      };
      let document: PDFDocumentProxy;
      try {
        document = await task.promise;
      } catch (reason) {
        if (passwordFailure) throw new PageFinishUserError(passwordFailure);
        const name = reason instanceof Error ? reason.name : "";
        if (name === "InvalidPDFException") {
          throw new PageFinishUserError(t("finish.error.damaged"));
        }
        if (name === "ResponseException" || name === "MissingPDFException") {
          throw new PageFinishUserError(t("finish.error.read"));
        }
        throw new PageFinishUserError(t("finish.error.open"));
      }
      if (!mountedRef.current || requestRunRef.current !== runId) {
        if (loadingTaskRef.current === task) loadingTaskRef.current = null;
        await task.destroy();
        return;
      }
      if (document.numPages !== report.pageCount) {
        throw new PageFinishUserError(t("finish.error.pageCountMismatch"));
      }
      setInspection(report);
      setPdfDocument(document);
      setPageNumber(1);
      setPageScope("all");
      setCustomRange(`1-${report.pageCount}`);
      setPanelTab("layout");
      setSignatureRiskAcknowledged(false);
      setWorkspaceOpen(true);
      finishingInspectionJob.clearJob();
    } catch (reason) {
      if (loadingTaskRef.current === task) loadingTaskRef.current = null;
      void task?.destroy();
      if (mountedRef.current && requestRunRef.current === runId) {
        setInspection(null);
        setPdfDocument(null);
        setWorkspaceOpen(false);
        setError(
          reason instanceof Error && reason.message === "The PDF job was cancelled."
            ? t("finish.review.cancelled")
            : reason instanceof PageFinishUserError
              ? reason.message
              : t("finish.error.review")
        );
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setReviewCancelBusy(false);
        setBusy(null);
      }
    }
  };

  const exportFinishedPdf = async () => {
    if (!canExport || !sourcePath || !inspection) return;
    const runId = requestRunRef.current;
    setBusy("export");
    setError(null);
    resetExportOutcome();
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath),
        filters: [{ name: t("finish.dialog.filter"), extensions: ["pdf"] }],
        title: t("finish.dialog.save")
      });
      if (typeof outputPath !== "string") return;
      if (!mountedRef.current || requestRunRef.current !== runId) return;
      await finishingJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        bates: batesEnabled
          ? {
              colour: colourToPdfComponents(batesColour),
              digits: batesDigits,
              fontSizePt: batesSize,
              marginPt: millimetresToPoints(batesMarginMm),
              position: batesPosition,
              prefix: batesPrefix,
              startNumber: batesStart,
              suffix: batesSuffix
            }
          : null,
        crop: hasCrop ? cropPoints : null,
        expectedSourceModifiedAtMs: inspection.sourceModifiedAtMs,
        expectedSourceSize: inspection.sourceSize,
        headerFooter: headerFooterEnabled
          ? {
              colour: colourToPdfComponents(headerFooterColour),
              fontSizePt: headerFooterSize,
              footerAlignment,
              footerText,
              headerAlignment,
              headerText,
              marginPt: millimetresToPoints(headerFooterMarginMm)
            }
          : null,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        pageRange: effectivePageRange,
        resize: resizeEnabled ? resizePoints : null,
        watermark: watermarkEnabled
          ? {
              angleDegrees: watermarkAngle,
              colour: colourToPdfComponents(watermarkColour),
              fontSizePt: watermarkSize,
              opacity: watermarkOpacity,
              overContent: watermarkOver,
              text: watermarkText
            }
          : null
      });
    } catch {
      if (mountedRef.current && requestRunRef.current === runId) {
        setError(t("finish.error.exportStart"));
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) setBusy(null);
    }
  };

  const cancelFinishingExport = async () => {
    if (!finishingJob.isActive || cancelBusy) return;
    setCancelBusy(true);
    try {
      await finishingJob.cancelJob();
    } catch {
      setCancelBusy(false);
      setError(t("finish.error.exportCancel"));
    }
  };

  const cancelFinishingReview = async () => {
    if (!finishingInspectionJob.isActive || reviewCancelBusy) return;
    setReviewCancelBusy(true);
    try {
      await finishingInspectionJob.cancelJob();
    } catch {
      setError(t("finish.error.reviewCancel"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    resetExportOutcome();
  };

  const changePage = (next: number) => {
    if (!inspection) return;
    setPageNumber(Math.max(1, Math.min(inspection.pageCount, Math.round(next) || 1)));
  };

  const changeCropMargin = (side: keyof CropMarginsMm, value: number) => {
    const next = Number.isFinite(value) ? Math.max(0, value) : 0;
    setCropMarginsMm((current) =>
      cropLinked
        ? { top: next, right: next, bottom: next, left: next }
        : { ...current, [side]: next }
    );
    resetExportOutcome();
  };

  const previewPaperStyle = previewLayout
    ? ({ aspectRatio: `${previewLayout.outputWidthPt} / ${previewLayout.outputHeightPt}` } as CSSProperties)
    : undefined;
  const previewSourceStyle = previewLayout
    ? ({
        left: `${previewLayout.sourceLeftPercent}%`,
        top: `${previewLayout.sourceTopPercent}%`,
        width: `${previewLayout.sourceWidthPercent}%`,
        height: `${previewLayout.sourceHeightPercent}%`
      } as CSSProperties)
    : undefined;
  const sourceRenderWidth = previewLayout
    ? Math.max(280, Math.min(1_400, (previewWidth * previewLayout.sourceWidthPercent) / 100))
    : previewWidth;
  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    escapeDisabled: operationBusy,
    onEscape: closeWorkspace
  });

  return (
    <>
      <section className="finish-studio">
        <div className="finish-heading">
          <div>
            <h3>{t("finish.heading.title")}</h3>
            <p>{t("finish.heading.description")}</p>
          </div>
          <Crop size={18} aria-hidden="true" />
        </div>
        <button className="wide-button" disabled={!desktopMode || operationBusy} onClick={() => void chooseSource()} type="button">
          <FolderOpen size={17} aria-hidden="true" />
          {sourcePath
            ? t("finish.action.chooseAnother")
            : t("finish.action.choose")}
        </button>
        {sourcePath ? (
          <div className="finish-source">
            <FileText size={17} aria-hidden="true" />
            <span><strong>{fileNameFromPath(sourcePath)}</strong><small title={sourcePath}>{sourcePath}</small></span>
          </div>
        ) : null}
        {sourcePath ? (
          <label className="finish-password">
            <span>
              {t("finish.password.label")}{" "}
              <small>{t("finish.password.optional")}</small>
            </span>
            <span>
              <input autoComplete="off" disabled={operationBusy} onChange={(event) => { setPassword(event.target.value); resetExportOutcome(); }} placeholder={t("finish.password.placeholder")} type={showPassword ? "text" : "password"} value={password} />
              <button aria-label={showPassword ? t("finish.password.hide") : t("finish.password.show")} className="icon-button" disabled={operationBusy} onClick={() => setShowPassword((visible) => !visible)} title={showPassword ? t("finish.password.hide") : t("finish.password.show")} type="button">
                {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
              </button>
            </span>
          </label>
        ) : null}
        {!desktopMode ? <div className="engine-state is-info"><Info size={16} aria-hidden="true" /><span>{t("finish.desktopOnly")}</span></div> : null}
        {error && !workspaceOpen ? <div className="engine-state is-missing" role="alert"><AlertCircle size={16} aria-hidden="true" /><span>{error}</span></div> : null}
        <button className="primary wide-button" disabled={!desktopMode || !sourcePath || operationBusy} onClick={() => void reviewFinishing()} type="button">
          {busy === "review" || finishingInspectionJob.isActive ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <FileOutput size={17} aria-hidden="true" />}
          {busy === "review" || finishingInspectionJob.isActive
            ? t("finish.action.inspecting")
            : t("finish.action.open")}
        </button>
        {!workspaceOpen && finishingInspectionJob.job ? (
          <PdfJobProgress
            cancelling={reviewCancelBusy}
            connectionError={finishingInspectionJob.connectionError}
            job={finishingInspectionJob.job}
            onCancel={() => void cancelFinishingReview()}
            onRetry={() => void reviewFinishing()}
            retryDisabled={!desktopMode || !sourcePath || operationBusy}
          />
        ) : null}
        {!workspaceOpen && finishingJob.job ? (
          <PdfJobProgress
            cancelling={cancelBusy}
            connectionError={finishingJob.connectionError}
            job={finishingJob.job}
            onCancel={() => void cancelFinishingExport()}
            onRetry={() => void exportFinishedPdf()}
            retryDisabled={!canExport}
          />
        ) : null}
        {!workspaceOpen &&
        !finishingInspectionJob.job &&
        !finishingJob.isActive &&
        finishingJob.connectionError ? (
          <div className="engine-state is-info" role="status"><AlertCircle size={16} aria-hidden="true" /><span>{t("job.connectionError")}</span></div>
        ) : null}
        {!workspaceOpen && jobNotice ? (
          <div className="engine-state is-info" role="status"><Info size={16} aria-hidden="true" /><span>{jobNotice}</span></div>
        ) : null}
        {!workspaceOpen && exportResult ? <FinishExportResultPanel result={exportResult} /> : null}
      </section>

      {workspaceOpen && inspection && pdfDocument ? (
        <div className="dialog-backdrop finish-backdrop" role="presentation">
          <section aria-labelledby="finish-dialog-title" aria-modal="true" className="finish-dialog" data-dialog-root ref={dialogRef} role="dialog" tabIndex={-1}>
            <header>
              <div className="dialog-icon" aria-hidden="true"><Crop size={24} /></div>
              <div><span className="eyebrow">{t("finish.dialog.eyebrow")}</span><h2 id="finish-dialog-title">{inspection.fileName}</h2></div>
              <div className="finish-header-actions">
                <span>
                  {t("finish.dialog.selection", {
                    pages: formatNumber(inspection.pageCount),
                    selected: formatNumber(parsedRange.pages.length)
                  })}
                </span>
                <button className="primary" disabled={!canExport} onClick={() => void exportFinishedPdf()} type="button">
                  {operationBusy ? <Loader2 className="spin" size={16} aria-hidden="true" /> : <Save size={16} aria-hidden="true" />}
                  {finishingJob.isActive
                    ? t("finish.action.exporting")
                    : busy === "export"
                      ? t("finish.action.choosing")
                      : t("finish.action.save")}
                </button>
                <button aria-label={t("finish.close.aria")} className="icon-button" data-dialog-initial-focus disabled={operationBusy} onClick={closeWorkspace} title={t("finish.close.title")} type="button"><X size={18} aria-hidden="true" /></button>
              </div>
            </header>

            {finishingJob.job ? (
              <PdfJobProgress
                cancelling={cancelBusy}
                connectionError={finishingJob.connectionError}
                job={finishingJob.job}
                onCancel={() => void cancelFinishingExport()}
                onRetry={() => void exportFinishedPdf()}
                retryDisabled={!canExport}
              />
            ) : null}
            {!finishingJob.isActive && finishingJob.connectionError ? (
              <div className="finish-warning" role="status"><AlertCircle size={16} aria-hidden="true" /><span>{t("job.connectionError")}</span></div>
            ) : null}
            {jobNotice ? (
              <div className="finish-warning" role="status"><Info size={16} aria-hidden="true" /><span>{jobNotice}</span></div>
            ) : null}

            <fieldset className="finish-workspace-fieldset" disabled={operationBusy}>
            <div className="finish-workspace">
              <aside className="finish-control-panel">
                <div className="finish-tabs" role="tablist" aria-label={t("finish.tabs.aria")}>
                  <button aria-selected={panelTab === "layout"} className={panelTab === "layout" ? "is-active" : ""} onClick={() => setPanelTab("layout")} role="tab" type="button"><Ruler size={15} aria-hidden="true" />{t("finish.tabs.layout")}</button>
                  <button aria-selected={panelTab === "marks"} className={panelTab === "marks" ? "is-active" : ""} onClick={() => setPanelTab("marks")} role="tab" type="button"><Type size={15} aria-hidden="true" />{t("finish.tabs.marks")}</button>
                  <button aria-selected={panelTab === "numbering"} className={panelTab === "numbering" ? "is-active" : ""} onClick={() => setPanelTab("numbering")} role="tab" type="button"><Hash size={15} aria-hidden="true" />{t("finish.tabs.numbering")}</button>
                </div>

                {panelTab === "layout" ? (
                  <div className="finish-settings-scroll">
                    <section className="finish-setting-section">
                      <header><div><strong>{t("finish.pages.title")}</strong><small>{t("finish.pages.description")}</small></div></header>
                      <div className="finish-segmented">
                        {(["all", "current", "custom"] as PageScope[]).map((scope) => <button className={pageScope === scope ? "is-active" : ""} key={scope} onClick={() => setPageScope(scope)} type="button">{pageScopeLabel(scope, t)}</button>)}
                      </div>
                      {pageScope === "custom" ? <label className="finish-field"><span>{t("finish.pages.rangeLabel")}</span><input onChange={(event) => setCustomRange(event.target.value)} placeholder={t("finish.pages.rangePlaceholder")} type="text" value={customRange} /></label> : null}
                      {parsedRangeError ? <div className="finish-inline-error"><AlertCircle size={14} aria-hidden="true" />{parsedRangeError}</div> : <small className="finish-field-note">{t(parsedRange.pages.length === 1 ? "finish.pages.selected.one" : "finish.pages.selected.other", { count: formatNumber(parsedRange.pages.length) })}</small>}
                    </section>

                    <section className="finish-setting-section">
                      <header><div><strong>{t("finish.crop.title")}</strong><small>{t("finish.crop.description")}</small></div><label className="compact-toggle"><input checked={cropEnabled} onChange={(event) => setCropEnabled(event.target.checked)} type="checkbox" /><span /></label></header>
                      {cropEnabled ? <>
                        <div className="finish-section-actions"><button onClick={() => setCropLinked((linked) => !linked)} type="button"><Layers size={14} aria-hidden="true" />{cropLinked ? t("finish.crop.linked") : t("finish.crop.separate")}</button><button onClick={() => setCropMarginsMm({ top: 0, right: 0, bottom: 0, left: 0 })} type="button">{t("finish.crop.reset")}</button></div>
                        <div className="finish-margin-grid">
                          {(["top", "right", "bottom", "left"] as const).map((side) => <label key={side}><span>{cropSideLabel(side, t)}</span><input min={0} onChange={(event) => changeCropMargin(side, Number(event.target.value))} step={0.5} type="number" value={cropMarginsMm[side]} /><small>mm</small></label>)}
                        </div>
                        <div className="finish-crop-notice"><Info size={14} aria-hidden="true" /><span>{t("finish.crop.notice")}</span></div>
                      </> : null}
                    </section>

                    <section className="finish-setting-section">
                      <header><div><strong>{t("finish.paper.title")}</strong><small>{t("finish.paper.description")}</small></div><label className="compact-toggle"><input checked={resizeEnabled} onChange={(event) => setResizeEnabled(event.target.checked)} type="checkbox" /><span /></label></header>
                      {resizeEnabled ? <>
                        <label className="finish-field"><span>{t("finish.paper.size")}</span><select onChange={(event) => setPaperPresetId(event.target.value)} value={paperPresetId}>{finishPaperPresets.map((preset) => <option key={preset.id} value={preset.id}>{localiseFinishPaperName(preset.id, preset.name, t)}</option>)}</select></label>
                        {paperPresetId === "custom" ? <div className="finish-two-fields"><label className="finish-field"><span>{t("finish.paper.width")}</span><input min={20} onChange={(event) => setCustomWidthMm(Number(event.target.value))} step={0.1} type="number" value={customWidthMm} /></label><label className="finish-field"><span>{t("finish.paper.height")}</span><input min={20} onChange={(event) => setCustomHeightMm(Number(event.target.value))} step={0.1} type="number" value={customHeightMm} /></label></div> : null}
                        <div className="finish-segmented"><button className={orientation === "portrait" ? "is-active" : ""} onClick={() => setOrientation("portrait")} type="button">{t("finish.orientation.portrait")}</button><button className={orientation === "landscape" ? "is-active" : ""} onClick={() => setOrientation("landscape")} type="button">{t("finish.orientation.landscape")}</button></div>
                        <label className="finish-field"><span>{t("finish.paper.margin")} <output>{formatNumber(resizeMarginMm, { maximumFractionDigits: 1 })} mm</output></span><input max={50} min={0} onChange={(event) => setResizeMarginMm(Number(event.target.value))} step={0.5} type="range" value={resizeMarginMm} /></label>
                      </> : null}
                    </section>
                  </div>
                ) : null}

                {panelTab === "marks" ? (
                  <div className="finish-settings-scroll">
                    <section className="finish-setting-section">
                      <header><div><strong>{t("finish.watermark.title")}</strong><small>{t("finish.watermark.description")}</small></div><label className="compact-toggle"><input checked={watermarkEnabled} onChange={(event) => setWatermarkEnabled(event.target.checked)} type="checkbox" /><span /></label></header>
                      {watermarkEnabled ? <>
                        <label className="finish-field"><span>{t("finish.watermark.text")}</span><input maxLength={512} onChange={(event) => setWatermarkText(event.target.value)} type="text" value={watermarkText} /></label>
                        <div className="finish-two-fields"><label className="finish-field"><span>{t("finish.common.sizePt")}</span><input max={240} min={12} onChange={(event) => setWatermarkSize(Number(event.target.value))} type="number" value={watermarkSize} /></label><label className="finish-field"><span>{t("finish.watermark.angle")}</span><input max={180} min={-180} onChange={(event) => setWatermarkAngle(Number(event.target.value))} type="number" value={watermarkAngle} /></label></div>
                        <label className="finish-field"><span>{t("finish.watermark.opacity")} <output>{formatNumber(Math.round(watermarkOpacity * 100))}%</output></span><input max={0.9} min={0.05} onChange={(event) => setWatermarkOpacity(Number(event.target.value))} step={0.01} type="range" value={watermarkOpacity} /></label>
                        <ColourControl colour={watermarkColour} onChange={setWatermarkColour} />
                        <div className="finish-segmented"><button className={!watermarkOver ? "is-active" : ""} onClick={() => setWatermarkOver(false)} type="button">{t("finish.watermark.below")}</button><button className={watermarkOver ? "is-active" : ""} onClick={() => setWatermarkOver(true)} type="button">{t("finish.watermark.above")}</button></div>
                      </> : null}
                    </section>

                    <section className="finish-setting-section">
                      <header><div><strong>{t("finish.headerFooter.title")}</strong><small>{t("finish.headerFooter.description")}</small></div><label className="compact-toggle"><input checked={headerFooterEnabled} onChange={(event) => setHeaderFooterEnabled(event.target.checked)} type="checkbox" /><span /></label></header>
                      {headerFooterEnabled ? <>
                        <label className="finish-field"><span>{t("finish.headerFooter.header")}</span><input maxLength={512} onChange={(event) => setHeaderText(event.target.value)} placeholder="{file}" type="text" value={headerText} /></label>
                        <label className="finish-field"><span>{t("finish.headerFooter.headerAlignment")}</span><select onChange={(event) => setHeaderAlignment(event.target.value as Alignment)} value={headerAlignment}><option value="left">{t("finish.alignment.left")}</option><option value="centre">{t("finish.alignment.centre")}</option><option value="right">{t("finish.alignment.right")}</option></select></label>
                        <label className="finish-field"><span>{t("finish.headerFooter.footer")}</span><input maxLength={512} onChange={(event) => setFooterText(event.target.value)} placeholder={t("finish.default.footer")} type="text" value={footerText} /></label>
                        <label className="finish-field"><span>{t("finish.headerFooter.footerAlignment")}</span><select onChange={(event) => setFooterAlignment(event.target.value as Alignment)} value={footerAlignment}><option value="left">{t("finish.alignment.left")}</option><option value="centre">{t("finish.alignment.centre")}</option><option value="right">{t("finish.alignment.right")}</option></select></label>
                        <div className="finish-token-row" aria-label={t("finish.headerFooter.tokensAria")}><code>{"{page}"}</code><code>{"{pages}"}</code><code>{"{file}"}</code></div>
                        <div className="finish-two-fields"><label className="finish-field"><span>{t("finish.common.sizePt")}</span><input max={36} min={6} onChange={(event) => setHeaderFooterSize(Number(event.target.value))} type="number" value={headerFooterSize} /></label><label className="finish-field"><span>{t("finish.common.marginMm")}</span><input max={50} min={0} onChange={(event) => setHeaderFooterMarginMm(Number(event.target.value))} step={0.5} type="number" value={headerFooterMarginMm} /></label></div>
                        <ColourControl colour={headerFooterColour} onChange={setHeaderFooterColour} />
                      </> : null}
                    </section>
                  </div>
                ) : null}

                {panelTab === "numbering" ? (
                  <div className="finish-settings-scroll">
                    <section className="finish-setting-section">
                      <header><div><strong>{t("finish.bates.title")}</strong><small>{t("finish.bates.description")}</small></div><label className="compact-toggle"><input checked={batesEnabled} onChange={(event) => setBatesEnabled(event.target.checked)} type="checkbox" /><span /></label></header>
                      {batesEnabled ? <>
                        <div className="finish-two-fields"><label className="finish-field"><span>{t("finish.bates.prefix")}</span><input maxLength={512} onChange={(event) => setBatesPrefix(event.target.value)} type="text" value={batesPrefix} /></label><label className="finish-field"><span>{t("finish.bates.suffix")}</span><input maxLength={512} onChange={(event) => setBatesSuffix(event.target.value)} type="text" value={batesSuffix} /></label></div>
                        <div className="finish-two-fields"><label className="finish-field"><span>{t("finish.bates.start")}</span><input min={0} onChange={(event) => setBatesStart(Number(event.target.value))} step={1} type="number" value={batesStart} /></label><label className="finish-field"><span>{t("finish.bates.digits")}</span><input max={12} min={1} onChange={(event) => setBatesDigits(Number(event.target.value))} step={1} type="number" value={batesDigits} /></label></div>
                        <label className="finish-field"><span>{t("finish.bates.position")}</span><select onChange={(event) => setBatesPosition(event.target.value as BatesPosition)} value={batesPosition}><option value="topLeft">{t("finish.position.topLeft")}</option><option value="topCentre">{t("finish.position.topCentre")}</option><option value="topRight">{t("finish.position.topRight")}</option><option value="bottomLeft">{t("finish.position.bottomLeft")}</option><option value="bottomCentre">{t("finish.position.bottomCentre")}</option><option value="bottomRight">{t("finish.position.bottomRight")}</option></select></label>
                        <div className="finish-two-fields"><label className="finish-field"><span>{t("finish.common.sizePt")}</span><input max={36} min={6} onChange={(event) => setBatesSize(Number(event.target.value))} type="number" value={batesSize} /></label><label className="finish-field"><span>{t("finish.common.marginMm")}</span><input max={50} min={0} onChange={(event) => setBatesMarginMm(Number(event.target.value))} step={0.5} type="number" value={batesMarginMm} /></label></div>
                        <ColourControl colour={batesColour} onChange={setBatesColour} />
                        <div className="finish-bates-sample"><Hash size={15} aria-hidden="true" /><span><small>{t("finish.bates.sample")}</small><strong>{formatBatesNumber(batesPrefix, batesSuffix, batesStart, batesDigits, 0)}</strong></span></div>
                      </> : null}
                    </section>
                  </div>
                ) : null}
              </aside>

              <main className="finish-preview-panel">
                <div className="finish-page-nav">
                  <button aria-label={t("finish.navigation.previous")} className="icon-button" disabled={pageNumber <= 1} onClick={() => changePage(pageNumber - 1)} title={t("finish.navigation.previous")} type="button"><ChevronLeft size={17} aria-hidden="true" /></button>
                  <label><span>{t("finish.navigation.page")}</span><input max={inspection.pageCount} min={1} onChange={(event) => changePage(Number(event.target.value))} type="number" value={pageNumber} /><span>{t("finish.navigation.of", { count: formatNumber(inspection.pageCount) })}</span></label>
                  <button aria-label={t("finish.navigation.next")} className="icon-button" disabled={pageNumber >= inspection.pageCount} onClick={() => changePage(pageNumber + 1)} title={t("finish.navigation.next")} type="button"><ChevronRight size={17} aria-hidden="true" /></button>
                  <span className={pageSelected ? "is-selected" : ""}>{pageSelected ? t("finish.preview.included") : t("finish.preview.only")}</span>
                </div>
                <div className="finish-preview-host" ref={previewHostRef}>
                  {previewLayout && currentPage ? (
                    <div className="finish-preview-paper" style={previewPaperStyle}>
                      <div className="finish-preview-source" style={previewSourceStyle}><PdfPageCanvas document={pdfDocument} pageNumber={pageNumber} targetWidth={sourceRenderWidth} variant="page" /></div>
                      {cropBoundary ? <div className="finish-content-boundary" style={{ left: `${cropBoundary.left}%`, top: `${cropBoundary.top}%`, width: `${cropBoundary.width}%`, height: `${cropBoundary.height}%` }} /> : null}
                      {pageSelected && watermarkEnabled ? <div className={watermarkOver ? "finish-watermark is-over" : "finish-watermark"} style={{ color: watermarkColour, fontSize: `${Math.max(10, Math.min(42, (watermarkSize / previewLayout.outputWidthPt) * previewWidth))}px`, opacity: watermarkOpacity, transform: `translate(-50%, -50%) rotate(${watermarkAngle}deg)` }}>{watermarkText}</div> : null}
                      {headerPreview ? <div className={`finish-page-label is-header is-${headerAlignment}`} style={{ color: headerFooterColour, fontSize: `${Math.max(7, Math.min(16, (headerFooterSize / previewLayout.outputWidthPt) * previewWidth))}px`, top: `${(millimetresToPoints(headerFooterMarginMm) / previewLayout.outputHeightPt) * 100}%` }}>{headerPreview}</div> : null}
                      {footerPreview ? <div className={`finish-page-label is-footer is-${footerAlignment}`} style={{ bottom: `${(millimetresToPoints(headerFooterMarginMm) / previewLayout.outputHeightPt) * 100}%`, color: headerFooterColour, fontSize: `${Math.max(7, Math.min(16, (headerFooterSize / previewLayout.outputWidthPt) * previewWidth))}px` }}>{footerPreview}</div> : null}
                      {batesPreview ? <div className={`finish-bates-label is-${batesPosition}`} style={{ color: batesColour, fontSize: `${Math.max(7, Math.min(16, (batesSize / previewLayout.outputWidthPt) * previewWidth))}px`, "--finish-bates-margin": `${(millimetresToPoints(batesMarginMm) / previewLayout.outputWidthPt) * 100}%` } as CSSProperties}>{batesPreview}</div> : null}
                    </div>
                  ) : <div className="finish-preview-invalid"><AlertCircle size={26} aria-hidden="true" /><strong>{t("finish.preview.unavailable")}</strong><span>{t("finish.preview.reviewSettings")}</span></div>}
                </div>
              </main>

              <aside className="finish-summary-panel">
                <section className="finish-output-summary">
                  <header><FileOutput size={17} aria-hidden="true" /><div><strong>{t("finish.summary.title")}</strong><small>{t("finish.summary.description")}</small></div></header>
                  <dl>
                    <div><dt>{t("finish.summary.pages")}</dt><dd>{formatNumber(parsedRange.pages.length)}</dd></div>
                    <div><dt>{t("finish.summary.visibleSize")}</dt><dd>{previewLayout ? t("finish.summary.dimensions", { height: formatNumber(pointsToMillimetres(previewLayout.outputHeightPt), { maximumFractionDigits: 1 }), width: formatNumber(pointsToMillimetres(previewLayout.outputWidthPt), { maximumFractionDigits: 1 }) }) : t("finish.summary.invalid")}</dd></div>
                    <div><dt>{t("finish.summary.crop")}</dt><dd>{hasCrop ? t("finish.summary.applied") : t("common.none")}</dd></div>
                    <div><dt>{t("finish.summary.paperFit")}</dt><dd>{resizeEnabled ? t("finish.summary.paperValue", { orientation: orientationLabel(orientation, t), paper: localiseFinishPaperName(selectedPreset.id, selectedPreset.name, t) }) : t("finish.summary.original")}</dd></div>
                    <div><dt>{t("finish.summary.marks")}</dt><dd>{formatNumber([watermarkEnabled, headerFooterEnabled, batesEnabled].filter(Boolean).length)}</dd></div>
                    <div><dt>{t("finish.summary.annotations")}</dt><dd>{t("finish.summary.preserved", { count: formatNumber(inspection.annotationCount) })}</dd></div>
                  </dl>
                  {settingsError ? <div className="finish-summary-error"><AlertCircle size={15} aria-hidden="true" /><span>{settingsError}</span></div> : <div className="finish-summary-ready"><CheckCircle2 size={15} aria-hidden="true" /><span>{t("finish.summary.ready")}</span></div>}
                </section>

                <section className="finish-notices">
                  {inspection.hasForms ? <div className="finish-warning"><LockKeyhole size={16} aria-hidden="true" /><span>{t("finish.notice.forms")}</span></div> : null}
                  {inspection.hasBookmarks && resizeEnabled ? <div className="finish-warning"><Info size={16} aria-hidden="true" /><span>{t("finish.notice.bookmarks")}</span></div> : null}
                  {hasCrop ? <div className="finish-crop-warning"><ShieldAlert size={16} aria-hidden="true" /><span><strong>{t("finish.notice.cropTitle")}</strong><small>{t("finish.notice.cropDetail")}</small></span></div> : null}
                  {localisePageFinishWarnings(inspection.warnings, t, formatNumber).map((warning) => <div className="finish-warning" key={warning}><Info size={16} aria-hidden="true" /><span>{warning}</span></div>)}
                  <PdfEditSafetyNotice acknowledged={signatureRiskAcknowledged} busy={operationBusy} editSafety={editSafety} onAcknowledgedChange={setSignatureRiskAcknowledged} rewriteDescription={t("finish.rewriteDescription")} />
                  <OutputProtectionFields disabled={operationBusy} onChange={changeOutputProtection} qpdfAvailable={qpdfAvailable} value={outputProtection} />
                  {error ? <div className="finish-error" role="alert"><AlertCircle size={16} aria-hidden="true" /><span>{error}</span></div> : null}
                  {exportResult ? <FinishExportResultPanel result={exportResult} /> : null}
                </section>
              </aside>
            </div>
            </fieldset>
          </section>
        </div>
      ) : null}
    </>
  );
}

function FinishExportResultPanel({ result }: { result: ExportPdfFinishingResult }) {
  const { formatNumber, t } = useI18n();
  return (
    <div className="finish-export-result">
      <CheckCircle2 size={18} aria-hidden="true" />
      <span>
        <strong>{t("finish.result.title")}</strong>
        <small>
          {t("finish.result.summary", {
            changed: formatNumber(result.changedPageCount),
            cropped: formatNumber(result.croppedPageCount),
            encryption:
              result.encryption === "AES-256"
                ? t("finish.result.protected")
                : t("finish.result.unprotected"),
            numbered: formatNumber(result.batesNumberCount),
            resized: formatNumber(result.resizedPageCount),
            size: formatBytes(result.bytesWritten, formatNumber)
          })}
        </small>
        <small title={result.outputPath}>{fileNameFromPath(result.outputPath)}</small>
        {localisePageFinishWarnings(result.warnings, t, formatNumber).map((warning) => (
          <small key={warning}>{warning}</small>
        ))}
      </span>
    </div>
  );
}

function ColourControl({ colour, onChange }: { colour: string; onChange: (value: string) => void }) {
  const { t } = useI18n();
  return (
    <div className="finish-colour-control">
      <span>{t("finish.colour.label")}</span>
      <div>{colourSwatches.map((swatch) => <button aria-label={t("finish.colour.use", { colour: swatch })} className={colour.toLowerCase() === swatch ? "is-active" : ""} key={swatch} onClick={() => onChange(swatch)} style={{ background: swatch }} title={swatch} type="button" />)}<input aria-label={t("finish.colour.custom")} onChange={(event) => onChange(event.target.value)} type="color" value={colour} /></div>
    </div>
  );
}

function suggestedOutputPath(sourcePath: string) {
  return sourcePath.replace(/\.pdf$/i, "-finished.pdf");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function pageScopeLabel(scope: PageScope, t: ReturnType<typeof useI18n>["t"]) {
  if (scope === "all") return t("finish.pages.all");
  if (scope === "current") return t("finish.pages.current");
  return t("finish.pages.range");
}

function cropSideLabel(
  side: keyof CropMarginsMm,
  t: ReturnType<typeof useI18n>["t"]
) {
  if (side === "top") return t("finish.crop.top");
  if (side === "right") return t("finish.crop.right");
  if (side === "bottom") return t("finish.crop.bottom");
  return t("finish.crop.left");
}

function orientationLabel(
  orientation: Orientation,
  t: ReturnType<typeof useI18n>["t"]
) {
  return orientation === "portrait"
    ? t("finish.orientation.portrait")
    : t("finish.orientation.landscape");
}

function formatBytes(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KB`;
  }
  return `${formatNumber(bytes / (1024 * 1024), { maximumFractionDigits: 1 })} MB`;
}

class PageFinishUserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PageFinishUserError";
  }
}
