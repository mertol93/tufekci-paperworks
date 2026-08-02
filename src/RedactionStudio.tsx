import {
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
  AlertTriangle,
  Braces,
  Check,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  Highlighter,
  Info,
  Loader2,
  Mail,
  MousePointer2,
  Redo2,
  Save,
  Search,
  ShieldX,
  Trash2,
  Undo2,
  X
} from "lucide-react";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { PdfPageCanvas } from "./PdfPageCanvas";
import { useI18n } from "./I18nProvider";
import { OutputProtectionFields } from "./OutputProtectionFields";
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
import {
  buildPageSearchIndex,
  commitRedactionHistory,
  createRedactionHistory,
  findPageSearchMatches,
  isUsableRedactionRect,
  normalisedPoint,
  rectBetween,
  redoRedactionHistory,
  toRedactionRegionInput,
  translateRedactionRect,
  undoRedactionHistory,
  type NormalisedPoint,
  type NormalisedRect,
  type PdfSearchTextItem,
  type RedactionColour,
  type RedactionDraft,
  type RedactionHistory,
  type SearchMode
} from "./redactionDraft";
import {
  localiseRedactionSearchError,
  localiseRedactionWarnings
} from "./redactionLocalisation";
import { localisePdfJobFailure } from "./pdfJobs";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

type RedactionStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type RedactionPageInspection = {
  heightPt: number;
  pageNumber: number;
  rotation: number;
  widthPt: number;
};

type PdfRedactionInspection = {
  annotationCount: number;
  certificateSignature: boolean;
  fileName: string;
  hasBookmarks: boolean;
  hasForms: boolean;
  pageCount: number;
  pages: RedactionPageInspection[];
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  taggedPdf: boolean;
  warnings: string[];
  wasEncrypted: boolean;
};

type ExportPdfRedactionResult = {
  bytesWritten: number;
  encryption: "AES-256" | "None";
  outputPath: string;
  pageCount: number;
  privacyStructuresRemoved: number;
  rasterPixelCount: number;
  redactedPageCount: number;
  redactionCount: number;
  unreachableObjectsPruned: number;
  warnings: string[];
};

type SearchSuggestion = {
  id: string;
  pageNumber: number;
  rects: NormalisedRect[];
  selected: boolean;
  text: string;
};

type DrawInteraction = {
  draft: RedactionDraft;
  mode: "draw";
  pointerId: number;
  start: NormalisedPoint;
};

type MoveInteraction = {
  mode: "move";
  original: RedactionDraft;
  pointerId: number;
  preview: RedactionDraft;
  start: NormalisedPoint;
};

type PointerInteraction = DrawInteraction | MoveInteraction;
type RedactionTool = "redact" | "select";
type SearchScope = "current" | "document";
type BusyState = "dialog" | "raster" | "review" | "search" | null;

const MAX_SEARCH_SUGGESTIONS = 2_000;
const MAX_SEARCH_ITEMS_PER_PAGE = 100_000;
const MAX_SEARCH_CHARACTERS_PER_PAGE = 2_000_000;
const MAX_REDACTED_PAGES = 256;
const MAX_REDACTIONS_PER_PAGE = 10_000;
const MAX_TOTAL_REDACTIONS = 100_000;
const MAX_RASTER_DIMENSION = 8_192;
const MAX_PAGE_RASTER_PIXELS = 40_000_000;
const MAX_TOTAL_RASTER_PIXELS = 300_000_000;
const rasterOptions = [144, 180, 240, 300];

export function RedactionStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: RedactionStudioProps) {
  const { formatNumber, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState<BusyState>(null);
  const [error, setError] = useState<string | null>(null);
  const [inspection, setInspection] = useState<PdfRedactionInspection | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [history, setHistory] = useState<RedactionHistory>(() => createRedactionHistory());
  const [pageNumber, setPageNumber] = useState(1);
  const [activeTool, setActiveTool] = useState<RedactionTool>("redact");
  const [redactionColour, setRedactionColour] = useState<RedactionColour>("black");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [interaction, setInteraction] = useState<PointerInteraction | null>(null);
  const [searchMode, setSearchMode] = useState<SearchMode>("literal");
  const [searchScope, setSearchScope] = useState<SearchScope>("current");
  const [searchQuery, setSearchQuery] = useState("");
  const [matchCase, setMatchCase] = useState(false);
  const [suggestions, setSuggestions] = useState<SearchSuggestion[]>([]);
  const [searchProgress, setSearchProgress] = useState("");
  const [reviewAcknowledged, setReviewAcknowledged] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [rasterDpi, setRasterDpi] = useState(180);
  const [cancelJobBusy, setCancelJobBusy] = useState(false);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [exportProgress, setExportProgress] = useState("");
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const [exportResult, setExportResult] = useState<ExportPdfRedactionResult | null>(null);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [previewWidth, setPreviewWidth] = useState(720);
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const mountedRef = useRef(true);
  const reviewRunRef = useRef(0);
  const searchRunRef = useRef(0);
  const cancelRasterRef = useRef(false);
  const nextIdRef = useRef(1);
  const previewHostRef = useRef<HTMLDivElement>(null);
  const redactionJob = usePdfJob<ExportPdfRedactionResult>(desktopMode, "redaction");
  const redactionInspectionJob = usePdfJob<PdfRedactionInspection>(
    desktopMode,
    "redaction-inspection"
  );
  const interfaceBusy =
    busy !== null || redactionJob.isActive || redactionInspectionJob.isActive;
  const sourceList = useMemo(
    () =>
      sourcePath
        ? [{ id: "redaction-source", label: fileNameFromPath(sourcePath), password, path: sourcePath }]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, sourceList, "redaction");
  const hasCertificateRisk = Boolean(
    inspection?.certificateSignature || editSafety.signedSources.length > 0
  );
  const certificateRiskAccepted = !hasCertificateRisk || signatureRiskAcknowledged;
  const currentRedactions = history.present.filter(
    (redaction) => redaction.pageNumber === pageNumber
  );
  const currentSuggestions = suggestions.filter(
    (suggestion) => suggestion.pageNumber === pageNumber
  );
  const selectedSuggestionCount = suggestions.filter((suggestion) => suggestion.selected).length;
  const redactedPageNumbers = useMemo(
    () => [...new Set(history.present.map((redaction) => redaction.pageNumber))].sort((a, b) => a - b),
    [history.present]
  );
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      inspection &&
      pdfDocument &&
      history.present.length > 0 &&
      history.present.length <= MAX_TOTAL_REDACTIONS &&
      redactedPageNumbers.length <= MAX_REDACTED_PAGES &&
      reviewAcknowledged &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      !interfaceBusy &&
      !interaction
  );
  const visibleRedactions = useMemo(() => {
    if (!interaction) {
      return history.present;
    }
    if (interaction.mode === "draw") {
      return [...history.present, interaction.draft];
    }
    return history.present.map((redaction) =>
      redaction.id === interaction.original.id ? interaction.preview : redaction
    );
  }, [history.present, interaction]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      reviewRunRef.current += 1;
      searchRunRef.current += 1;
      cancelRasterRef.current = true;
      const task = loadingTaskRef.current;
      loadingTaskRef.current = null;
      void task?.destroy();
    };
  }, []);

  const closeWorkspace = useCallback(() => {
    reviewRunRef.current += 1;
    searchRunRef.current += 1;
    cancelRasterRef.current = true;
    const task = loadingTaskRef.current;
    loadingTaskRef.current = null;
    void task?.destroy();
    setWorkspaceOpen(false);
    setInspection(null);
    setPdfDocument(null);
    setHistory(createRedactionHistory());
    setPageNumber(1);
    setSelectedId(null);
    setInteraction(null);
    setSuggestions([]);
    setSearchProgress("");
    setExportProgress("");
    setExportNotice(null);
    setExportResult(null);
    setReviewAcknowledged(false);
    setBusy(null);
    redactionInspectionJob.clearJob();
  }, [redactionInspectionJob.clearJob]);

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
    const job = redactionJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelJobBusy(false);
    setExportProgress("");
    if (job.status === "succeeded" && job.result) {
      setExportResult(job.result);
      setExportNotice(null);
      setError(null);
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
    } else if (job.status === "cancelled") {
      setExportResult(null);
      setExportNotice(t("redaction.export.cancelled"));
      setError(null);
    } else if (job.status === "failed") {
      setExportResult(null);
      setExportNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [redactionJob.job?.jobId, redactionJob.job?.status, t]);

  useEffect(() => {
    if (!workspaceOpen || !previewHostRef.current || !("ResizeObserver" in window)) {
      return;
    }
    const host = previewHostRef.current;
    const update = () => setPreviewWidth(Math.max(280, Math.min(820, host.clientWidth - 28)));
    update();
    const observer = new ResizeObserver(update);
    observer.observe(host);
    return () => observer.disconnect();
  }, [workspaceOpen]);

  useEffect(() => {
    if (selectedId && !history.present.some((redaction) => redaction.id === selectedId)) {
      setSelectedId(null);
    }
  }, [history.present, selectedId]);

  useEffect(() => {
    if (!workspaceOpen) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editingText = Boolean(target?.closest("input, textarea, select"));
      if (event.key === "Escape") {
        if (interaction) {
          setInteraction(null);
        } else if (busy === "search") {
          cancelSearch();
        } else if (!interfaceBusy) {
          closeWorkspace();
        }
        return;
      }
      if (interfaceBusy) {
        return;
      }
      if ((event.ctrlKey || event.metaKey) && !editingText && event.key.toLowerCase() === "z") {
        event.preventDefault();
        setHistory((current) =>
          event.shiftKey ? redoRedactionHistory(current) : undoRedactionHistory(current)
        );
        setReviewAcknowledged(false);
        setExportResult(null);
        setInteraction(null);
        return;
      }
      if ((event.ctrlKey || event.metaKey) && !editingText && event.key.toLowerCase() === "y") {
        event.preventDefault();
        setHistory(redoRedactionHistory);
        setReviewAcknowledged(false);
        setExportResult(null);
        setInteraction(null);
        return;
      }
      if (!editingText && selectedId && (event.key === "Delete" || event.key === "Backspace")) {
        event.preventDefault();
        deleteSelected();
        return;
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  const commitRedactions = useCallback(
    (updater: (redactions: RedactionDraft[]) => RedactionDraft[]) => {
      if (interfaceBusy) {
        return;
      }
      setHistory((current) => commitRedactionHistory(current, updater(current.present)));
      setReviewAcknowledged(false);
      setExportNotice(null);
      setExportResult(null);
    },
    [interfaceBusy]
  );

  const chooseSource = async () => {
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("redaction.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("redaction.dialog.choose")
      });
      if (typeof selected === "string" && mountedRef.current) {
        closeWorkspace();
        redactionJob.clearJob();
        setSourcePath(selected);
        setPassword("");
        setExportNotice(null);
      }
    } catch {
      if (mountedRef.current) {
        setError(t("redaction.error.choose"));
      }
    }
  };

  const reviewPdf = async () => {
    if (!desktopMode || !sourcePath || interfaceBusy) {
      return;
    }
    const runId = reviewRunRef.current + 1;
    reviewRunRef.current = runId;
    setBusy("review");
    setReviewCancelBusy(false);
    setError(null);
    setExportNotice(null);
    setExportResult(null);
    redactionJob.clearJob();
    redactionInspectionJob.clearJob();
    const previousTask = loadingTaskRef.current;
    loadingTaskRef.current = null;
    if (previousTask) {
      await previousTask.destroy();
    }
    if (!mountedRef.current || reviewRunRef.current !== runId) {
      return;
    }

    let task: ReturnType<typeof createPdfLoadingTask> | null = null;
    try {
      const [report, source] = await Promise.all([
        redactionInspectionJob.startJobAndWait({
          inputPassword: password || null,
          inputPath: sourcePath
        }),
        invoke<PdfRangeSource>("open_local_pdf", { path: sourcePath })
      ]);
      if (!mountedRef.current || reviewRunRef.current !== runId) {
        return;
      }
      if (report.sourceSize !== source.size || report.sourceModifiedAtMs !== source.modifiedAtMs) {
        throw new RedactionUserError(t("redaction.error.sourceChanged"));
      }
      task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure = "";
      task.onPassword = (_updatePassword: (value: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? t("redaction.error.passwordIncorrect")
          : t("redaction.error.passwordRequired");
        void task?.destroy();
      };
      let document: PDFDocumentProxy;
      try {
        document = await task.promise;
      } catch (reason) {
        if (passwordFailure) {
          throw new RedactionUserError(passwordFailure);
        }
        throw reason;
      }
      if (!mountedRef.current || reviewRunRef.current !== runId) {
        if (loadingTaskRef.current === task) {
          loadingTaskRef.current = null;
        }
        await task.destroy();
        return;
      }
      if (document.numPages !== report.pageCount) {
        throw new RedactionUserError(t("redaction.error.pageCountMismatch"));
      }
      setInspection(report);
      setPdfDocument(document);
      setHistory(createRedactionHistory());
      setPageNumber(1);
      setActiveTool("redact");
      setSelectedId(null);
      setInteraction(null);
      setSuggestions([]);
      setSearchProgress("");
      setExportProgress("");
      setReviewAcknowledged(false);
      setSignatureRiskAcknowledged(false);
      setWorkspaceOpen(true);
      redactionInspectionJob.clearJob();
    } catch (reason) {
      if (loadingTaskRef.current === task) {
        loadingTaskRef.current = null;
      }
      void task?.destroy();
      if (mountedRef.current && reviewRunRef.current === runId) {
        setInspection(null);
        setPdfDocument(null);
        setWorkspaceOpen(false);
        setError(
          reason instanceof Error && reason.message === "The PDF job was cancelled."
            ? t("redaction.review.cancelled")
            : reason instanceof RedactionUserError
              ? reason.message
              : pdfOpeningError(reason, t)
        );
      }
    } finally {
      if (mountedRef.current && reviewRunRef.current === runId) {
        setReviewCancelBusy(false);
        setBusy(null);
      }
    }
  };

  const changePage = (value: number) => {
    if (!inspection) {
      return;
    }
    setPageNumber(Math.min(inspection.pageCount, Math.max(1, Math.round(value) || 1)));
    setSelectedId(null);
    setInteraction(null);
  };

  const startPointer = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (interfaceBusy || event.button !== 0) {
      return;
    }
    const point = normalisedPoint(
      event.clientX,
      event.clientY,
      event.currentTarget.getBoundingClientRect()
    );
    const target = event.target as SVGElement;
    const redactionId = target.closest<SVGElement>("[data-redaction-id]")?.dataset.redactionId;
    if (activeTool === "select" && redactionId) {
      const redaction = history.present.find((item) => item.id === redactionId);
      if (!redaction) {
        return;
      }
      event.currentTarget.setPointerCapture(event.pointerId);
      setSelectedId(redaction.id);
      setInteraction({
        mode: "move",
        original: redaction,
        pointerId: event.pointerId,
        preview: redaction,
        start: point
      });
      return;
    }
    if (activeTool !== "redact") {
      setSelectedId(null);
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    const draft: RedactionDraft = {
      colour: redactionColour,
      id: `redaction-${nextIdRef.current++}`,
      pageNumber,
      rect: { x: point.x, y: point.y, width: 0, height: 0 },
      source: "manual"
    };
    setSelectedId(draft.id);
    setInteraction({ draft, mode: "draw", pointerId: event.pointerId, start: point });
  };

  const continuePointer = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!interaction || interaction.pointerId !== event.pointerId) {
      return;
    }
    const point = normalisedPoint(
      event.clientX,
      event.clientY,
      event.currentTarget.getBoundingClientRect()
    );
    setInteraction((current) => {
      if (!current || current.pointerId !== event.pointerId) {
        return current;
      }
      if (current.mode === "draw") {
        return { ...current, draft: { ...current.draft, rect: rectBetween(current.start, point) } };
      }
      return {
        ...current,
        preview: {
          ...current.preview,
          rect: translateRedactionRect(current.original.rect, current.start, point)
        }
      };
    });
  };

  const finishPointer = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!interaction || interaction.pointerId !== event.pointerId) {
      return;
    }
    const finished = interaction;
    setInteraction(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (finished.mode === "draw") {
      if (!isUsableRedactionRect(finished.draft.rect)) {
        setSelectedId(null);
        return;
      }
      if (history.present.length >= MAX_TOTAL_REDACTIONS) {
        setError(
          t("redaction.error.regionLimit", {
            count: formatNumber(MAX_TOTAL_REDACTIONS)
          })
        );
        setSelectedId(null);
        return;
      }
      if (currentRedactions.length >= MAX_REDACTIONS_PER_PAGE) {
        setError(
          t("redaction.error.pageRegionLimit", {
            count: formatNumber(MAX_REDACTIONS_PER_PAGE),
            page: formatNumber(pageNumber)
          })
        );
        setSelectedId(null);
        return;
      }
      commitRedactions((redactions) => [...redactions, finished.draft]);
    } else {
      commitRedactions((redactions) =>
        redactions.map((redaction) =>
          redaction.id === finished.original.id ? finished.preview : redaction
        )
      );
    }
  };

  const deleteSelected = () => {
    if (!selectedId) {
      return;
    }
    commitRedactions((redactions) => redactions.filter((redaction) => redaction.id !== selectedId));
    setSelectedId(null);
  };

  const clearCurrentPage = () => {
    commitRedactions((redactions) =>
      redactions.filter((redaction) => redaction.pageNumber !== pageNumber)
    );
    setSelectedId(null);
  };

  const setSelectedColour = (colour: RedactionColour) => {
    setRedactionColour(colour);
    if (selectedId) {
      commitRedactions((redactions) =>
        redactions.map((redaction) =>
          redaction.id === selectedId ? { ...redaction, colour } : redaction
        )
      );
    }
  };

  const runSearch = async () => {
    if (!pdfDocument || !inspection || interfaceBusy) {
      return;
    }
    const runId = searchRunRef.current + 1;
    searchRunRef.current = runId;
    setBusy("search");
    setError(null);
    setSearchProgress("");
    setSuggestions([]);
    const pages =
      searchScope === "current"
        ? [pageNumber]
        : Array.from({ length: inspection.pageCount }, (_, index) => index + 1);
    const found: SearchSuggestion[] = [];
    let truncated = false;
    try {
      for (let index = 0; index < pages.length; index += 1) {
        if (!mountedRef.current || searchRunRef.current !== runId) {
          return;
        }
        const candidatePage = pages[index];
        setSearchProgress(
          t("redaction.search.progress", {
            current: formatNumber(index + 1),
            page: formatNumber(candidatePage),
            total: formatNumber(pages.length)
          })
        );
        const page = await pdfDocument.getPage(candidatePage);
        const viewport = page.getViewport({ scale: 1 });
        const content = await page.getTextContent({ disableNormalization: false });
        if (content.items.length > MAX_SEARCH_ITEMS_PER_PAGE) {
          throw new RedactionUserError(
            t("redaction.error.searchItemLimit", {
              count: formatNumber(MAX_SEARCH_ITEMS_PER_PAGE),
              page: formatNumber(candidatePage)
            })
          );
        }
        const items: PdfSearchTextItem[] = content.items.flatMap((item) => {
          if (!("str" in item)) {
            return [];
          }
          return [
            {
              hasEOL: item.hasEOL,
              height: item.height,
              str: item.str,
              transform: [...item.transform],
              width: item.width
            }
          ];
        });
        const pageIndex = buildPageSearchIndex(
          items,
          [...viewport.transform],
          viewport.width,
          viewport.height
        );
        if (pageIndex.text.length > MAX_SEARCH_CHARACTERS_PER_PAGE) {
          throw new RedactionUserError(
            t("redaction.error.searchCharacterLimit", {
              count: formatNumber(MAX_SEARCH_CHARACTERS_PER_PAGE),
              page: formatNumber(candidatePage)
            })
          );
        }
        const remaining = Math.max(1, MAX_SEARCH_SUGGESTIONS - found.length);
        const result = findPageSearchMatches(
          pageIndex,
          searchMode,
          searchQuery,
          matchCase,
          remaining
        );
        if (result.error) {
          throw new RedactionUserError(localiseRedactionSearchError(result.error, t));
        }
        for (const match of result.matches) {
          found.push({
            id: `suggestion-${nextIdRef.current++}`,
            pageNumber: candidatePage,
            rects: match.rects,
            selected: false,
            text: compactText(match.text)
          });
        }
        if (result.truncated || found.length >= MAX_SEARCH_SUGGESTIONS) {
          truncated = true;
          break;
        }
        await yieldToInterface();
      }
      if (!mountedRef.current || searchRunRef.current !== runId) {
        return;
      }
      setSuggestions(found);
      setSearchProgress(
        found.length === 0
          ? t("redaction.search.noMatches")
          : t(
              truncated
                ? found.length === 1
                  ? "redaction.search.resultLimited.one"
                  : "redaction.search.resultLimited.other"
                : found.length === 1
                  ? "redaction.search.result.one"
                  : "redaction.search.result.other",
              {
                count: formatNumber(found.length),
                maximum: formatNumber(MAX_SEARCH_SUGGESTIONS)
              }
            )
      );
      if (found.length > 0) {
        changePage(found[0].pageNumber);
      }
    } catch (reason) {
      if (mountedRef.current && searchRunRef.current === runId) {
        setError(
          reason instanceof RedactionUserError
            ? reason.message
            : t("redaction.error.search")
        );
        setSearchProgress("");
      }
    } finally {
      if (mountedRef.current && searchRunRef.current === runId) {
        setBusy(null);
      }
    }
  };

  const cancelSearch = () => {
    searchRunRef.current += 1;
    setBusy(null);
    setSearchProgress(t("redaction.search.cancelled"));
  };

  const toggleSuggestion = (id: string) => {
    if (interfaceBusy) {
      return;
    }
    setSuggestions((current) =>
      current.map((suggestion) =>
        suggestion.id === id ? { ...suggestion, selected: !suggestion.selected } : suggestion
      )
    );
  };

  const selectCurrentSuggestions = (selected: boolean) => {
    if (interfaceBusy) {
      return;
    }
    setSuggestions((current) =>
      current.map((suggestion) =>
        suggestion.pageNumber === pageNumber ? { ...suggestion, selected } : suggestion
      )
    );
  };

  const addSelectedSuggestions = () => {
    if (interfaceBusy) {
      return;
    }
    const selected = suggestions.filter((suggestion) => suggestion.selected);
    if (selected.length === 0) {
      return;
    }
    const additions = selected.flatMap((suggestion) =>
      suggestion.rects.map((rect) => ({
        colour: redactionColour,
        id: `redaction-${nextIdRef.current++}`,
        label: suggestion.text,
        pageNumber: suggestion.pageNumber,
        rect,
        source: "search" as const
      }))
    );
    if (history.present.length + additions.length > MAX_TOTAL_REDACTIONS) {
      setError(
        t("redaction.error.regionLimit", {
          count: formatNumber(MAX_TOTAL_REDACTIONS)
        })
      );
      return;
    }
    const additionsPerPage = new Map<number, number>();
    for (const addition of additions) {
      additionsPerPage.set(
        addition.pageNumber,
        (additionsPerPage.get(addition.pageNumber) ?? 0) + 1
      );
    }
    for (const [candidatePage, additionCount] of additionsPerPage) {
      const existingCount = history.present.filter(
        (redaction) => redaction.pageNumber === candidatePage
      ).length;
      if (existingCount + additionCount > MAX_REDACTIONS_PER_PAGE) {
        setError(
          t("redaction.error.pageRegionLimit", {
            count: formatNumber(MAX_REDACTIONS_PER_PAGE),
            page: formatNumber(candidatePage)
          })
        );
        return;
      }
    }
    commitRedactions((redactions) => [...redactions, ...additions]);
    setSuggestions((current) => current.filter((suggestion) => !suggestion.selected));
    setSelectedId(additions[0]?.id ?? null);
    if (additions[0]) {
      changePage(additions[0].pageNumber);
    }
  };

  const exportRedactedPdf = async () => {
    if (!canExport || !sourcePath || !inspection || !pdfDocument) {
      return;
    }
    setBusy("dialog");
    setCancelJobBusy(false);
    setError(null);
    setExportNotice(null);
    setExportResult(null);
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath),
        filters: [{ name: t("redaction.dialog.filter"), extensions: ["pdf"] }],
        title: t("redaction.dialog.save")
      });
      if (typeof outputPath !== "string" || !mountedRef.current) {
        return;
      }
      cancelRasterRef.current = false;
      setBusy("raster");
      const pages = await rasteriseRedactedPages(
        pdfDocument,
        history.present,
        rasterDpi,
        cancelRasterRef,
        (current, total) =>
          setExportProgress(
            t("redaction.export.renderingProgress", {
              current: formatNumber(current),
              total: formatNumber(total)
            })
          ),
        t,
        formatNumber
      );
      if (cancelRasterRef.current) {
        setExportNotice(t("redaction.export.renderCancelled"));
        return;
      }
      setBusy("dialog");
      setExportProgress("");
      await redactionJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        expectedSourceModifiedAtMs: inspection.sourceModifiedAtMs,
        expectedSourceSize: inspection.sourceSize,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        pages
      });
    } catch (reason) {
      if (mountedRef.current) {
        setError(
          reason instanceof RedactionUserError
            ? reason.message
            : t("redaction.error.export")
        );
        setExportProgress("");
      }
    } finally {
      if (mountedRef.current) {
        setBusy(null);
      }
    }
  };

  const cancelRaster = () => {
    cancelRasterRef.current = true;
    setExportProgress(t("redaction.export.cancellingRaster"));
  };

  const cancelRedactionJob = async () => {
    if (!redactionJob.isActive || cancelJobBusy) {
      return;
    }
    setCancelJobBusy(true);
    try {
      await redactionJob.cancelJob();
    } catch {
      setCancelJobBusy(false);
      setError(t("redaction.error.exportCancel"));
    }
  };

  const cancelRedactionReview = async () => {
    if (!redactionInspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await redactionInspectionJob.cancelJob();
    } catch {
      setError(t("redaction.error.reviewCancel"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    setExportNotice(null);
    setExportResult(null);
  };

  const exportJobPanel = redactionJob.job ? (
    <PdfJobProgress
      cancelling={cancelJobBusy}
      connectionError={redactionJob.connectionError}
      job={redactionJob.job}
      onCancel={() => void cancelRedactionJob()}
      onRetry={() => void exportRedactedPdf()}
      retryDisabled={!canExport}
    />
  ) : null;
  const reviewJobPanel = redactionInspectionJob.job ? (
    <PdfJobProgress
      cancelling={reviewCancelBusy}
      connectionError={redactionInspectionJob.connectionError}
      job={redactionInspectionJob.job}
      onCancel={() => void cancelRedactionReview()}
      onRetry={() => void reviewPdf()}
      retryDisabled={
        !desktopMode ||
        !sourcePath ||
        interfaceBusy ||
        !editSafety.isReady ||
        !certificateRiskAccepted
      }
    />
  ) : null;
  const exportFeedback = (
    <>
      {exportNotice ? (
        <div className="redaction-notice" role="status">
          <Info size={16} aria-hidden="true" />
          <span>{exportNotice}</span>
        </div>
      ) : null}
      {!redactionInspectionJob.job &&
      !redactionJob.isActive &&
      redactionJob.connectionError ? (
        <div className="redaction-notice" role="status">
          <Info size={16} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}
      {exportResult ? (
        <div className="redaction-result" aria-live="polite">
          <CheckCircle2 size={17} aria-hidden="true" />
          <span>
            <strong>
              {t("redaction.result.title", {
                pages: redactionPageCount(
                  exportResult.redactedPageCount,
                  t,
                  formatNumber
                ),
                regions: redactionRegionCount(
                  exportResult.redactionCount,
                  t,
                  formatNumber
                )
              })}
            </strong>
            <small title={exportResult.outputPath}>
              {t("redaction.result.file", {
                encryption:
                  exportResult.encryption === "AES-256"
                    ? t("common.encryption.protected")
                    : t("common.encryption.unprotected"),
                name: fileNameFromPath(exportResult.outputPath),
                size: formatFileSize(exportResult.bytesWritten, formatNumber)
              })}
            </small>
            <small>
              {t("redaction.result.details", {
                pixels: formatPixelCount(exportResult.rasterPixelCount, formatNumber, t),
                pruned: formatNumber(exportResult.unreachableObjectsPruned),
                removed: formatNumber(exportResult.privacyStructuresRemoved)
              })}
            </small>
          </span>
        </div>
      ) : null}
      {localiseRedactionWarnings(
        exportResult?.warnings ?? [],
        t,
        formatNumber
      ).map((warning) => (
        <div className="redaction-warning" key={warning}><AlertTriangle size={16} aria-hidden="true" /><span>{warning}</span></div>
      ))}
    </>
  );

  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    onEscape: () => {
      if (interaction) {
        setInteraction(null);
      } else if (busy === "search") {
        cancelSearch();
      } else if (!interfaceBusy) {
        closeWorkspace();
      }
    }
  });

  return (
    <section className="redaction-studio">
      <div className="redaction-heading">
        <div>
          <h3>{t("redaction.heading.title")}</h3>
          <p>{t("redaction.heading.description")}</p>
        </div>
        <ShieldX size={19} aria-hidden="true" />
      </div>

      <button className="wide-button" disabled={!desktopMode || interfaceBusy} onClick={chooseSource} type="button">
        <FolderOpen size={17} aria-hidden="true" />
        {sourcePath
          ? t("redaction.action.chooseAnother")
          : t("redaction.action.choose")}
      </button>

      {sourcePath ? (
        <div className="redaction-source">
          <FileText size={17} aria-hidden="true" />
          <span title={sourcePath}>
            <strong>{fileNameFromPath(sourcePath)}</strong>
            <small>{t("redaction.source.local")}</small>
          </span>
        </div>
      ) : null}

      <label className="redaction-password">
        <span>
          <strong>{t("redaction.password.label")}</strong>
          <small>{t("redaction.password.help")}</small>
        </span>
        <span>
          <input
            autoComplete="current-password"
            disabled={interfaceBusy}
            onChange={(event) => {
              setPassword(event.target.value);
              setError(null);
              setExportNotice(null);
            }}
            spellCheck={false}
            type={showPassword ? "text" : "password"}
            value={password}
          />
          <button
            aria-label={
              showPassword
                ? t("redaction.password.hide")
                : t("redaction.password.show")
            }
            className="icon-button"
            onClick={() => setShowPassword((value) => !value)}
            title={
              showPassword
                ? t("redaction.password.hide")
                : t("redaction.password.show")
            }
            type="button"
          >
            {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
          </button>
        </span>
      </label>

      <div className="redaction-caution">
        <AlertTriangle size={18} aria-hidden="true" />
        <span>
          <strong>{t("redaction.caution.title")}</strong>
          <small>{t("redaction.caution.description")}</small>
        </span>
      </div>

      {!workspaceOpen ? (
        <PdfEditSafetyNotice
          acknowledged={signatureRiskAcknowledged}
          busy={interfaceBusy}
          editSafety={editSafety}
          onAcknowledgedChange={setSignatureRiskAcknowledged}
          rewriteDescription={t("redaction.signature.rewrite")}
        />
      ) : null}

      <button
        className="primary wide-button"
        disabled={
          !desktopMode ||
          !sourcePath ||
          interfaceBusy ||
          !editSafety.isReady ||
          !certificateRiskAccepted
        }
        onClick={() => void reviewPdf()}
        type="button"
      >
        {busy === "review" || redactionInspectionJob.isActive ? (
          <Loader2 className="spin" size={17} aria-hidden="true" />
        ) : (
          <Highlighter size={17} aria-hidden="true" />
        )}
        {busy === "review" || redactionInspectionJob.isActive
          ? t("redaction.action.opening")
          : t("redaction.action.open")}
      </button>

      {error && !workspaceOpen ? (
        <div className="redaction-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {!workspaceOpen ? reviewJobPanel : null}
      {!workspaceOpen ? exportJobPanel : null}
      {!workspaceOpen ? exportFeedback : null}

      <p className="redaction-note">
        {t("redaction.note")}
      </p>

      {workspaceOpen && inspection && pdfDocument ? (
        <div className="dialog-backdrop redaction-backdrop" role="presentation">
          <section aria-labelledby="redaction-dialog-title" aria-modal="true" className="redaction-dialog" data-dialog-root ref={dialogRef} role="dialog" tabIndex={-1}>
            <header>
              <div>
                <span className="eyebrow">{t("redaction.workspace.eyebrow")}</span>
                <h2 id="redaction-dialog-title">{inspection.fileName}</h2>
              </div>
              <div className="redaction-header-actions">
                <span>
                  {t("redaction.workspace.summary", {
                    pages: redactionPageCount(
                      redactedPageNumbers.length,
                      t,
                      formatNumber
                    ),
                    regions: redactionRegionCount(
                      history.present.length,
                      t,
                      formatNumber
                    )
                  })}
                </span>
                <button className="primary" disabled={!canExport} onClick={() => void exportRedactedPdf()} type="button">
                  {busy === "dialog" || busy === "raster" || redactionJob.isActive ? (
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                  ) : (
                    <Save size={16} aria-hidden="true" />
                  )}
                  {busy === "raster"
                    ? t("redaction.action.rendering")
                    : redactionJob.isActive
                      ? t("redaction.action.verifying")
                      : busy === "dialog"
                        ? t("redaction.action.preparing")
                        : t("redaction.action.save")}
                </button>
                <button
                  aria-label={t("redaction.action.close")}
                  className="icon-button"
                  data-dialog-initial-focus
                  disabled={busy === "dialog" || busy === "raster" || redactionJob.isActive}
                  onClick={closeWorkspace}
                  title={t("redaction.action.close")}
                  type="button"
                >
                  <X size={18} aria-hidden="true" />
                </button>
              </div>
            </header>

            <div className="redaction-workspace">
              <aside className="redaction-search-panel">
                <section>
                  <header>
                    <Search size={17} aria-hidden="true" />
                    <div>
                      <strong>{t("redaction.search.title")}</strong>
                      <small>{t("redaction.search.help")}</small>
                    </div>
                  </header>
                  <div
                    className="redaction-search-modes"
                    role="group"
                    aria-label={t("redaction.search.typeAria")}
                  >
                    <button className={searchMode === "literal" ? "is-active" : undefined} disabled={interfaceBusy} onClick={() => setSearchMode("literal")} type="button">
                      <Search size={15} aria-hidden="true" />
                      {t("redaction.search.mode.text")}
                    </button>
                    <button className={searchMode === "email" ? "is-active" : undefined} disabled={interfaceBusy} onClick={() => setSearchMode("email")} type="button">
                      <Mail size={15} aria-hidden="true" />
                      {t("redaction.search.mode.email")}
                    </button>
                    <button className={searchMode === "pattern" ? "is-active" : undefined} disabled={interfaceBusy} onClick={() => setSearchMode("pattern")} type="button">
                      <Braces size={15} aria-hidden="true" />
                      {t("redaction.search.mode.pattern")}
                    </button>
                  </div>
                  {searchMode !== "email" ? (
                    <label className="redaction-search-query">
                      <span>
                        {searchMode === "pattern"
                          ? t("redaction.search.patternLabel")
                          : t("redaction.search.textLabel")}
                      </span>
                      <input
                        maxLength={512}
                        disabled={interfaceBusy}
                        onChange={(event) => setSearchQuery(event.target.value)}
                        placeholder={
                          searchMode === "pattern"
                            ? t("redaction.search.patternPlaceholder")
                            : t("redaction.search.textPlaceholder")
                        }
                        type="text"
                        value={searchQuery}
                      />
                    </label>
                  ) : (
                    <p className="redaction-search-description">
                      {t("redaction.search.emailHelp")}
                    </p>
                  )}
                  {searchMode === "pattern" ? (
                    <p className="redaction-search-description">
                      {t("redaction.search.patternHelp")}
                    </p>
                  ) : null}
                  <div className="redaction-search-options">
                    <div
                      className="redaction-segmented"
                      role="group"
                      aria-label={t("redaction.search.pagesAria")}
                    >
                      <button className={searchScope === "current" ? "is-active" : undefined} disabled={interfaceBusy} onClick={() => setSearchScope("current")} type="button">{t("redaction.search.thisPage")}</button>
                      <button className={searchScope === "document" ? "is-active" : undefined} disabled={interfaceBusy} onClick={() => setSearchScope("document")} type="button">{t("redaction.search.wholePdf")}</button>
                    </div>
                    {searchMode !== "email" ? (
                      <label>
                        <input checked={matchCase} disabled={interfaceBusy} onChange={(event) => setMatchCase(event.target.checked)} type="checkbox" />
                        {t("redaction.search.matchCase")}
                      </label>
                    ) : null}
                  </div>
                  {busy === "search" ? (
                    <button className="wide-button" onClick={cancelSearch} type="button">
                      <X size={16} aria-hidden="true" />
                      {t("redaction.action.cancelSearch")}
                    </button>
                  ) : (
                    <button className="primary wide-button" disabled={interfaceBusy} onClick={() => void runSearch()} type="button">
                      <Search size={16} aria-hidden="true" />
                      {t("redaction.action.findSuggestions")}
                    </button>
                  )}
                  {searchProgress ? (
                    <p className={busy === "search" ? "redaction-search-progress is-busy" : "redaction-search-progress"}>
                      {busy === "search" ? <Loader2 className="spin" size={14} aria-hidden="true" /> : <Info size={14} aria-hidden="true" />}
                      <span>{searchProgress}</span>
                    </p>
                  ) : null}
                </section>

                <section className="redaction-page-list">
                  <header>
                    <FileText size={17} aria-hidden="true" />
                    <div>
                      <strong>{t("redaction.pages.title")}</strong>
                      <small>
                        {t("redaction.pages.total", {
                          count: formatNumber(inspection.pageCount)
                        })}
                      </small>
                    </div>
                  </header>
                  <div>
                    {inspection.pages.map((page) => {
                      const count = history.present.filter((item) => item.pageNumber === page.pageNumber).length;
                      const suggestionsOnPage = suggestions.filter((item) => item.pageNumber === page.pageNumber).length;
                      return (
                        <button
                          className={page.pageNumber === pageNumber ? "is-active" : undefined}
                          key={page.pageNumber}
                          onClick={() => changePage(page.pageNumber)}
                          type="button"
                        >
                          <span>{formatNumber(page.pageNumber)}</span>
                          <small>
                            {count > 0
                              ? t(
                                  count === 1
                                    ? "redaction.pages.marked.one"
                                    : "redaction.pages.marked.other",
                                  { count: formatNumber(count) }
                                )
                              : suggestionsOnPage > 0
                                ? t(
                                    suggestionsOnPage === 1
                                      ? "redaction.pages.found.one"
                                      : "redaction.pages.found.other",
                                    { count: formatNumber(suggestionsOnPage) }
                                  )
                                : t("redaction.pages.unmarked")}
                          </small>
                          {count > 0 ? <Check size={15} aria-hidden="true" /> : null}
                        </button>
                      );
                    })}
                  </div>
                </section>
              </aside>

              <main className="redaction-canvas-panel">
                <div className="redaction-toolbar">
                  <div className="redaction-tool-group" role="group" aria-label={t("redaction.tool.aria")}>
                    <button className={activeTool === "select" ? "is-active" : undefined} onClick={() => setActiveTool("select")} title={t("redaction.tool.selectTitle")} type="button">
                      <MousePointer2 size={16} aria-hidden="true" /> {t("redaction.tool.select")}
                    </button>
                    <button className={activeTool === "redact" ? "is-active" : undefined} onClick={() => setActiveTool("redact")} title={t("redaction.tool.redactTitle")} type="button">
                      <Highlighter size={16} aria-hidden="true" /> {t("redaction.tool.redact")}
                    </button>
                  </div>
                  <div className="redaction-colours" role="group" aria-label={t("redaction.colour.aria")}>
                    <button aria-label={t("redaction.colour.blackAria")} aria-pressed={redactionColour === "black"} className={redactionColour === "black" ? "is-active is-black" : "is-black"} disabled={interfaceBusy} onClick={() => setSelectedColour("black")} title={t("redaction.colour.black")} type="button" />
                    <button aria-label={t("redaction.colour.whiteAria")} aria-pressed={redactionColour === "white"} className={redactionColour === "white" ? "is-active is-white" : "is-white"} disabled={interfaceBusy} onClick={() => setSelectedColour("white")} title={t("redaction.colour.white")} type="button" />
                  </div>
                  <button aria-label={t("redaction.action.undoAria")} className="icon-button" disabled={history.past.length === 0 || interfaceBusy} onClick={() => {
                    setHistory(undoRedactionHistory);
                    setReviewAcknowledged(false);
                    setExportResult(null);
                    setInteraction(null);
                  }} title={t("common.undo")} type="button"><Undo2 size={17} aria-hidden="true" /></button>
                  <button aria-label={t("redaction.action.redoAria")} className="icon-button" disabled={history.future.length === 0 || interfaceBusy} onClick={() => {
                    setHistory(redoRedactionHistory);
                    setReviewAcknowledged(false);
                    setExportResult(null);
                    setInteraction(null);
                  }} title={t("common.redo")} type="button"><Redo2 size={17} aria-hidden="true" /></button>
                  <button className="redaction-clear-page" disabled={currentRedactions.length === 0 || interfaceBusy} onClick={clearCurrentPage} type="button">
                    <Trash2 size={15} aria-hidden="true" /> {t("redaction.action.clearPage")}
                  </button>
                </div>

                <div className="redaction-page-nav">
                  <button aria-label={t("common.previousPage")} className="icon-button" disabled={pageNumber <= 1} onClick={() => changePage(pageNumber - 1)} title={t("common.previousPage")} type="button"><ChevronLeft size={17} aria-hidden="true" /></button>
                  <label>
                    <span>{t("common.page")}</span>
                    <input max={inspection.pageCount} min={1} onChange={(event) => changePage(Number(event.target.value))} type="number" value={pageNumber} />
                    <span>{t("common.ofCount", { count: formatNumber(inspection.pageCount) })}</span>
                  </label>
                  <button aria-label={t("common.nextPage")} className="icon-button" disabled={pageNumber >= inspection.pageCount} onClick={() => changePage(pageNumber + 1)} title={t("common.nextPage")} type="button"><ChevronRight size={17} aria-hidden="true" /></button>
                  <span>{t("redaction.page.summary", { marked: formatNumber(currentRedactions.length), suggested: formatNumber(currentSuggestions.length) })}</span>
                </div>

                <div className="redaction-preview-host" ref={previewHostRef}>
                  <div className="redaction-page-surface">
                    <PdfPageCanvas document={pdfDocument} pageNumber={pageNumber} targetWidth={previewWidth} variant="page" />
                    <svg
                      aria-label={t("redaction.canvas.aria", { page: formatNumber(pageNumber) })}
                      className={`redaction-overlay is-tool-${activeTool}`}
                      onPointerCancel={() => setInteraction(null)}
                      onPointerDown={startPointer}
                      onPointerMove={continuePointer}
                      onPointerUp={finishPointer}
                      preserveAspectRatio="none"
                      role="application"
                      viewBox="0 0 1000 1000"
                    >
                      {currentSuggestions.flatMap((suggestion) =>
                        suggestion.rects.map((rect, index) => (
                          <rect
                            aria-label={t("redaction.canvas.suggestionAria", { text: suggestion.text })}
                            className={suggestion.selected ? "redaction-suggestion is-selected" : "redaction-suggestion"}
                            data-suggestion-id={suggestion.id}
                            height={rect.height * 1000}
                            key={`${suggestion.id}-${index}`}
                            onPointerDown={(event) => {
                              event.stopPropagation();
                              toggleSuggestion(suggestion.id);
                            }}
                            role="button"
                            width={rect.width * 1000}
                            x={rect.x * 1000}
                            y={rect.y * 1000}
                          />
                        ))
                      )}
                      {visibleRedactions
                        .filter((redaction) => redaction.pageNumber === pageNumber)
                        .map((redaction) => (
                          <RedactionMark key={redaction.id} redaction={redaction} selected={redaction.id === selectedId} />
                        ))}
                    </svg>
                  </div>
                </div>
              </main>

              <aside className="redaction-inspector">
                <section className="redaction-region-panel">
                  <header>
                    <div>
                      <strong>{t("redaction.regions.title")}</strong>
                      <small>
                        {redactionPermanentRegionCount(
                          currentRedactions.length,
                          t,
                          formatNumber
                        )}
                      </small>
                    </div>
                    {selectedId ? (
                      <button aria-label={t("redaction.action.deleteAria")} className="icon-button is-danger" disabled={interfaceBusy} onClick={deleteSelected} title={t("redaction.action.delete")} type="button"><Trash2 size={16} aria-hidden="true" /></button>
                    ) : null}
                  </header>
                  <div className="redaction-region-list">
                    {currentRedactions.length > 0 ? currentRedactions.map((redaction, index) => (
                      <button className={redaction.id === selectedId ? "is-active" : undefined} key={redaction.id} onClick={() => {
                        setSelectedId(redaction.id);
                        setActiveTool("select");
                      }} type="button">
                        <span className={`redaction-fill-swatch is-${redaction.colour}`} />
                        <span>
                          <strong>{t("redaction.regions.item", { number: formatNumber(index + 1) })}</strong>
                          <small>{redaction.label ? compactText(redaction.label) : redaction.source === "search" ? t("redaction.regions.searchAssisted") : t("redaction.regions.manual")}</small>
                        </span>
                      </button>
                    )) : (
                      <div className="redaction-empty"><Highlighter size={21} aria-hidden="true" /><strong>{t("redaction.regions.empty")}</strong></div>
                    )}
                  </div>
                </section>

                {currentSuggestions.length > 0 ? (
                  <section className="redaction-suggestion-panel">
                    <header>
                      <div>
                        <strong>{t("redaction.suggestions.title")}</strong>
                        <small>{t("redaction.suggestions.help")}</small>
                      </div>
                      <button disabled={interfaceBusy} onClick={() => selectCurrentSuggestions(currentSuggestions.some((item) => !item.selected))} type="button">
                        {currentSuggestions.every((item) => item.selected)
                          ? t("common.clear")
                          : t("redaction.action.selectAll")}
                      </button>
                    </header>
                    <div>
                      {currentSuggestions.map((suggestion) => (
                        <label key={suggestion.id}>
                          <input checked={suggestion.selected} disabled={interfaceBusy} onChange={() => toggleSuggestion(suggestion.id)} type="checkbox" />
                          <span title={suggestion.text}>{suggestion.text}</span>
                          <small>
                            {t(
                              suggestion.rects.length === 1
                                ? "redaction.suggestions.area.one"
                                : "redaction.suggestions.area.other",
                              { count: formatNumber(suggestion.rects.length) }
                            )}
                          </small>
                        </label>
                      ))}
                    </div>
                    <button className="primary wide-button" disabled={selectedSuggestionCount === 0 || interfaceBusy} onClick={addSelectedSuggestions} type="button">
                      <Check size={16} aria-hidden="true" />
                      {t("redaction.action.addSelected", {
                        count: formatNumber(selectedSuggestionCount)
                      })}
                    </button>
                  </section>
                ) : null}

                <section className="redaction-export-settings">
                  <header>
                    <strong>{t("redaction.publication.title")}</strong>
                    <small>{t("redaction.publication.lossless")}</small>
                  </header>
                  <label>
                    <span>{t("redaction.publication.resolution")}</span>
                    <select disabled={interfaceBusy} onChange={(event) => {
                      setRasterDpi(Number(event.target.value));
                      setExportNotice(null);
                      setExportResult(null);
                    }} value={rasterDpi}>
                      {rasterOptions.map((dpi) => (
                        <option key={dpi} value={dpi}>
                          {t("redaction.publication.dpi", {
                            dpi: formatNumber(dpi),
                            recommendation:
                              dpi === 180
                                ? t("redaction.publication.recommended")
                                : ""
                          })}
                        </option>
                      ))}
                    </select>
                  </label>
                  <div className="redaction-impact">
                    <ShieldX size={17} aria-hidden="true" />
                    <span>
                      <strong>{t("redaction.publication.verified")}</strong>
                      <small>{t("redaction.publication.description")}</small>
                    </span>
                  </div>
                  <label className="redaction-review-check">
                    <input checked={reviewAcknowledged} disabled={interfaceBusy || history.present.length === 0} onChange={(event) => setReviewAcknowledged(event.target.checked)} type="checkbox" />
                    <span>{t("redaction.publication.acknowledgement")}</span>
                  </label>
                  <OutputProtectionFields
                    disabled={interfaceBusy}
                    onChange={changeOutputProtection}
                    qpdfAvailable={qpdfAvailable}
                    value={outputProtection}
                  />
                </section>

                <PdfEditSafetyNotice
                  acknowledged={signatureRiskAcknowledged}
                  busy={interfaceBusy}
                  editSafety={editSafety}
                  onAcknowledgedChange={setSignatureRiskAcknowledged}
                  rewriteDescription={t("redaction.signature.rewrite")}
                />

                {localiseRedactionWarnings(
                  inspection.warnings,
                  t,
                  formatNumber
                ).map((warning) => (
                  <div className="redaction-warning" key={warning}>
                    <AlertTriangle size={16} aria-hidden="true" />
                    <span>{warning}</span>
                  </div>
                ))}

                {redactedPageNumbers.length > MAX_REDACTED_PAGES ? (
                  <div className="redaction-warning" role="alert"><AlertTriangle size={16} aria-hidden="true" /><span>{t("redaction.error.pageLimit", { count: formatNumber(MAX_REDACTED_PAGES) })}</span></div>
                ) : null}
                {error ? (
                  <div className="redaction-error" role="alert"><AlertCircle size={16} aria-hidden="true" /><span>{error}</span></div>
                ) : null}
                {exportProgress ? (
                  <div className="redaction-progress" role="status"><Loader2 className="spin" size={16} aria-hidden="true" /><span>{exportProgress}</span>{busy === "raster" ? <button onClick={cancelRaster} type="button">{t("common.cancel")}</button> : null}</div>
                ) : null}
                {exportJobPanel}
                {exportFeedback}
              </aside>
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
}

function RedactionMark({
  redaction,
  selected
}: {
  redaction: RedactionDraft;
  selected: boolean;
}) {
  return (
    <g className={selected ? "redaction-mark is-selected" : "redaction-mark"} data-redaction-id={redaction.id}>
      <rect
        fill={redaction.colour === "black" ? "#000000" : "#ffffff"}
        height={redaction.rect.height * 1000}
        width={redaction.rect.width * 1000}
        x={redaction.rect.x * 1000}
        y={redaction.rect.y * 1000}
      />
      <rect
        className="redaction-mark-border"
        fill="none"
        height={redaction.rect.height * 1000}
        width={redaction.rect.width * 1000}
        x={redaction.rect.x * 1000}
        y={redaction.rect.y * 1000}
      />
    </g>
  );
}

async function rasteriseRedactedPages(
  document: PDFDocumentProxy,
  redactions: RedactionDraft[],
  dpi: number,
  cancelRef: { current: boolean },
  onProgress: (current: number, total: number) => void,
  t: ReturnType<typeof useI18n>["t"],
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const grouped = new Map<number, RedactionDraft[]>();
  for (const redaction of redactions) {
    const page = grouped.get(redaction.pageNumber) ?? [];
    page.push(redaction);
    grouped.set(redaction.pageNumber, page);
  }
  const pages = [...grouped.entries()].sort(([left], [right]) => left - right);
  if (pages.length > MAX_REDACTED_PAGES) {
    throw new RedactionUserError(
      t("redaction.error.pageLimit", {
        count: formatNumber(MAX_REDACTED_PAGES)
      })
    );
  }
  let totalPixels = 0;
  const output: Array<{
    pageNumber: number;
    pngDataUrl: string;
    regions: ReturnType<typeof toRedactionRegionInput>[];
  }> = [];

  for (let index = 0; index < pages.length; index += 1) {
    if (cancelRef.current) {
      break;
    }
    const [pageNumber, pageRedactions] = pages[index];
    onProgress(index + 1, pages.length);
    const page = await document.getPage(pageNumber);
    const baseViewport = page.getViewport({ scale: 1 });
    let scale = dpi / 72;
    const desiredWidth = Math.max(1, Math.round(baseViewport.width * scale));
    const desiredHeight = Math.max(1, Math.round(baseViewport.height * scale));
    const dimensionScale = Math.min(
      1,
      MAX_RASTER_DIMENSION / desiredWidth,
      MAX_RASTER_DIMENSION / desiredHeight
    );
    const pixelScale = Math.min(
      1,
      Math.sqrt(MAX_PAGE_RASTER_PIXELS / Math.max(1, desiredWidth * desiredHeight))
    );
    scale *= Math.min(dimensionScale, pixelScale);
    const viewport = page.getViewport({ scale });
    const width = Math.max(32, Math.floor(viewport.width));
    const height = Math.max(32, Math.floor(viewport.height));
    const pixels = width * height;
    totalPixels += pixels;
    if (totalPixels > MAX_TOTAL_RASTER_PIXELS) {
      throw new RedactionUserError(
        t("redaction.error.pixelLimit", {
          count: formatNumber(MAX_TOTAL_RASTER_PIXELS)
        })
      );
    }
    const canvas = window.document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d", { alpha: false });
    if (!context) {
      throw new RedactionUserError(t("redaction.error.rasterPrepare"));
    }
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, width, height);
    await page.render({ background: "#ffffff", canvas, viewport }).promise;
    const pngDataUrl = canvas.toDataURL("image/png");
    canvas.width = 1;
    canvas.height = 1;
    if (!pngDataUrl.startsWith("data:image/png;base64,")) {
      throw new RedactionUserError(
        t("redaction.error.rasterEncode", {
          page: formatNumber(pageNumber)
        })
      );
    }
    output.push({
      pageNumber,
      pngDataUrl,
      regions: pageRedactions.map(toRedactionRegionInput)
    });
    await yieldToInterface();
  }
  return output;
}

function yieldToInterface() {
  return new Promise<void>((resolve) => window.setTimeout(resolve, 0));
}

function suggestedOutputPath(path: string) {
  return /\.pdf$/i.test(path) ? path.replace(/\.pdf$/i, "-redacted.pdf") : `${path}-redacted.pdf`;
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function compactText(value: string) {
  const compact = value.replace(/\s+/gu, " ").trim();
  return compact.length > 72 ? `${compact.slice(0, 69)}...` : compact;
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KiB`;
  }
  return `${formatNumber(bytes / (1024 * 1024), {
    maximumFractionDigits: 1
  })} MiB`;
}

function formatPixelCount(
  pixels: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string,
  t: ReturnType<typeof useI18n>["t"]
) {
  return pixels >= 1_000_000
    ? t("redaction.result.millionPixels", {
        count: formatNumber(pixels / 1_000_000, { maximumFractionDigits: 1 })
      })
    : formatNumber(pixels);
}

function redactionRegionCount(
  count: number,
  t: ReturnType<typeof useI18n>["t"],
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  return t(
    count === 1 ? "redaction.count.region.one" : "redaction.count.region.other",
    { count: formatNumber(count) }
  );
}

function redactionPermanentRegionCount(
  count: number,
  t: ReturnType<typeof useI18n>["t"],
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  return t(
    count === 1
      ? "redaction.count.permanentRegion.one"
      : "redaction.count.permanentRegion.other",
    { count: formatNumber(count) }
  );
}

function redactionPageCount(
  count: number,
  t: ReturnType<typeof useI18n>["t"],
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  return t(
    count === 1 ? "redaction.count.page.one" : "redaction.count.page.other",
    { count: formatNumber(count) }
  );
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
    return t("redaction.error.damaged");
  }
  if (name === "MissingPDFException" || name === "UnexpectedResponseException") {
    return t("redaction.error.read");
  }
  return t("redaction.error.review");
}

class RedactionUserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RedactionUserError";
  }
}
