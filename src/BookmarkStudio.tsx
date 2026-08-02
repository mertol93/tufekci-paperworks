import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { PDFPageProxy } from "pdfjs-dist";
import { useDialogFocus } from "./accessibility";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  Bookmark,
  CheckCircle2,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  IndentDecrease,
  IndentIncrease,
  Info,
  ListTree,
  Loader2,
  Plus,
  Save,
  ScanText,
  Trash2,
  X
} from "lucide-react";
import {
  detectHeadingSuggestions,
  type HeadingLine,
  type HeadingSuggestion
} from "./bookmarkHeadings";
import {
  bookmarkBranchEnd,
  bookmarkHasChildren,
  bookmarkHasPreviousSibling,
  deleteBookmarkBranch,
  indentBookmarkBranch,
  moveBookmarkBranch,
  outdentBookmarkBranch
} from "./bookmarkTree";
import {
  bookmarkPdfOpeningErrorKey,
  localiseBookmarkWarnings,
  localisePrintedContentsValidation
} from "./bookmarkLocalisation";
import {
  createPrintedContentsDraft,
  estimatePrintedContentsPageCount,
  MAX_PRINTED_CONTENTS_TITLE_CHARACTERS,
  printedContentsIsValid,
  printedContentsValidationMessage,
  selectPrintedContentsEntries,
  toPdfPrintedContents,
  type PrintedContentsDraft
} from "./printedContents";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { takeE2eSaveSelection } from "paperworks-e2e-bridge";
import { useI18n } from "./I18nProvider";
import type { TranslationKey } from "./i18n";
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

type BookmarkStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PdfBookmarkEntry = {
  bold: boolean;
  colour: [number, number, number];
  italic: boolean;
  level: number;
  open: boolean;
  pageNumber: number | null;
  title: string;
};

type BookmarkDraft = PdfBookmarkEntry & {
  id: string;
};

type PdfBookmarkInspection = {
  bookmarkCount: number;
  bookmarks: PdfBookmarkEntry[];
  certificateSignature: boolean;
  fileName: string;
  pageCount: number;
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  unresolvedBookmarkCount: number;
  warnings: string[];
  wasEncrypted: boolean;
};

type ExportPdfBookmarksResult = {
  bookmarkCount: number;
  bytesWritten: number;
  contentsPageCount: number;
  encryption: "AES-256" | "None";
  outputPath: string;
  pageCount: number;
  printedEntryCount: number;
  warnings: string[];
};

type SelectableHeading = HeadingSuggestion & {
  id: string;
  selected: boolean;
};

type HeadingReview = {
  limitReached: boolean;
  scannedPages: number;
  suggestions: SelectableHeading[];
};

type HeadingProgress = {
  page: number;
  total: number;
};

type PageFragment = {
  fontSize: number;
  text: string;
  width: number;
  x: number;
  y: number;
};

const MAX_BOOKMARK_LEVEL = 6;
const MAX_HEADING_PAGES = 2_000;
const MAX_HEADING_ITEMS = 250_000;
const MAX_HEADING_ITEMS_PER_PAGE = 25_000;
const MAX_HEADING_LINES = 100_000;

export function BookmarkStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: BookmarkStudioProps) {
  const { formatNumber, t } = useI18n();
  const defaultContentsTitleRef = useRef(t("bookmark.contents.defaultTitle"));
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState<"export" | "review" | null>(null);
  const [error, setError] = useState<TranslationKey | null>(null);
  const [inspection, setInspection] = useState<PdfBookmarkInspection | null>(null);
  const [bookmarks, setBookmarks] = useState<BookmarkDraft[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [exportResult, setExportResult] = useState<ExportPdfBookmarksResult | null>(null);
  const [jobNotice, setJobNotice] = useState<TranslationKey | null>(null);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [printedContents, setPrintedContents] = useState<PrintedContentsDraft>(() =>
    createPrintedContentsDraft(defaultContentsTitleRef.current)
  );
  const [headingBusy, setHeadingBusy] = useState(false);
  const [headingProgress, setHeadingProgress] = useState<HeadingProgress | null>(null);
  const [headingReview, setHeadingReview] = useState<HeadingReview | null>(null);
  const [headingError, setHeadingError] = useState<TranslationKey | null>(null);
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const mountedRef = useRef(true);
  const requestRunRef = useRef(0);
  const headingRunRef = useRef(0);
  const nextIdRef = useRef(1);
  const sourceList = useMemo(
    () =>
      sourcePath
        ? [{ id: "bookmark-source", label: fileNameFromPath(sourcePath), password, path: sourcePath }]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, sourceList, "bookmarks");
  const bookmarkJob = usePdfJob<ExportPdfBookmarksResult>(desktopMode, "bookmarks");
  const bookmarkInspectionJob = usePdfJob<PdfBookmarkInspection>(
    desktopMode,
    "bookmark-inspection"
  );
  const operationBusy =
    busy !== null || bookmarkJob.isActive || bookmarkInspectionJob.isActive;
  const jobFailure =
    bookmarkJob.job?.status === "failed"
      ? localisePdfJobFailure(bookmarkJob.job, t)
      : null;
  const hasCertificateRisk = Boolean(
    inspection?.certificateSignature || editSafety.signedSources.length > 0
  );
  const certificateRiskAccepted = !hasCertificateRisk || signatureRiskAcknowledged;
  const selectedIndex = bookmarks.findIndex((bookmark) => bookmark.id === selectedId);
  const selectedBookmark = selectedIndex >= 0 ? bookmarks[selectedIndex] : null;
  const draftValid = Boolean(
    inspection &&
      bookmarks.every(
        (bookmark, index) =>
          bookmark.title.trim() &&
          bookmark.pageNumber !== null &&
          bookmark.pageNumber >= 1 &&
          bookmark.pageNumber <= inspection.pageCount &&
          bookmark.level >= 0 &&
          bookmark.level <= MAX_BOOKMARK_LEVEL &&
          (index > 0 || bookmark.level === 0) &&
          (index === 0 || bookmark.level <= bookmarks[index - 1].level + 1)
      )
  );
  const printedContentsEntries = useMemo(
    () => selectPrintedContentsEntries(bookmarks, printedContents.maximumLevel),
    [bookmarks, printedContents.maximumLevel]
  );
  const contentsPageCount = estimatePrintedContentsPageCount(printedContentsEntries.length);
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      inspection &&
      draftValid &&
      printedContentsIsValid(printedContents, bookmarks) &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      busy === null &&
      !bookmarkJob.isActive &&
      !headingBusy &&
      !headingReview
  );
  const displayedError = error ? t(error) : jobFailure;
  const localisedInspectionWarnings = inspection
    ? localiseBookmarkWarnings(inspection.warnings, t, formatNumber)
    : [];
  const resetExportOutcome = () => {
    setExportResult(null);
    setJobNotice(null);
    bookmarkJob.clearJob();
  };

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestRunRef.current += 1;
      headingRunRef.current += 1;
      const task = loadingTaskRef.current;
      loadingTaskRef.current = null;
      void task?.destroy();
    };
  }, []);

  useEffect(() => {
    defaultContentsTitleRef.current = t("bookmark.contents.defaultTitle");
  }, [t]);

  const closeWorkspace = useCallback(() => {
    requestRunRef.current += 1;
    headingRunRef.current += 1;
    const task = loadingTaskRef.current;
    loadingTaskRef.current = null;
    void task?.destroy();
    setWorkspaceOpen(false);
    setInspection(null);
    setBookmarks([]);
    setSelectedId(null);
    setPdfDocument(null);
    setHeadingBusy(false);
    setHeadingProgress(null);
    setHeadingReview(null);
    setHeadingError(null);
    setExportResult(null);
    setPrintedContents(createPrintedContentsDraft(defaultContentsTitleRef.current));
    bookmarkInspectionJob.clearJob();
  }, [bookmarkInspectionJob.clearJob]);

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
    const job = bookmarkJob.job;
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
      setJobNotice("bookmark.notice.exportCancelled");
    } else if (job.status === "failed") {
      setExportResult(null);
      setJobNotice(null);
      setError(null);
    }
  }, [bookmarkJob.job?.jobId, bookmarkJob.job?.status]);

  useEffect(() => {
    if (!workspaceOpen) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !headingBusy && !operationBusy) {
        closeWorkspace();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeWorkspace, headingBusy, operationBusy, workspaceOpen]);

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    resetExportOutcome();
  };

  const changePrintedContents = (value: PrintedContentsDraft) => {
    setPrintedContents(value);
    resetExportOutcome();
  };

  const chooseSource = async () => {
    if (operationBusy) {
      return;
    }
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("bookmark.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("bookmark.dialog.choose")
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
      void reason;
      if (mountedRef.current) {
        setError("bookmark.error.choose");
      }
    }
  };

  const reviewBookmarks = async () => {
    if (!desktopMode || !sourcePath || operationBusy) {
      return;
    }
    const runId = requestRunRef.current + 1;
    requestRunRef.current = runId;
    headingRunRef.current += 1;
    setBusy("review");
    setReviewCancelBusy(false);
    setError(null);
    resetExportOutcome();
    bookmarkInspectionJob.clearJob();
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
        bookmarkInspectionJob.startJobAndWait({
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
        throw new BookmarkUserError("bookmark.error.sourceChanged");
      }
      task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure: TranslationKey | null = null;
      task.onPassword = (_updatePassword: (value: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? "bookmark.error.passwordIncorrect"
          : "bookmark.error.passwordRequired";
        void task?.destroy();
      };
      let document: PDFDocumentProxy;
      try {
        document = await task.promise;
      } catch (reason) {
        if (passwordFailure) {
          throw new BookmarkUserError(passwordFailure);
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
        throw new BookmarkUserError("bookmark.error.pageCountMismatch");
      }
      const nextBookmarks = report.bookmarks.map(toDraft);
      setInspection(report);
      setBookmarks(nextBookmarks);
      setSelectedId(nextBookmarks[0]?.id ?? null);
      setPdfDocument(document);
      setWorkspaceOpen(true);
      setSignatureRiskAcknowledged(false);
      setHeadingReview(null);
      setHeadingError(null);
      bookmarkInspectionJob.clearJob();
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
            ? "bookmark.review.cancelled"
            : reason instanceof BookmarkUserError
              ? reason.key
              : bookmarkPdfOpeningErrorKey(reason)
        );
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setBusy(null);
        setReviewCancelBusy(false);
      }
    }
  };

  const toDraft = (entry: PdfBookmarkEntry): BookmarkDraft => ({
    ...entry,
    id: `bookmark-${nextIdRef.current++}`
  });

  const updateSelected = (updates: Partial<PdfBookmarkEntry>) => {
    if (!selectedId) {
      return;
    }
    setBookmarks((current) =>
      current.map((bookmark) =>
        bookmark.id === selectedId ? { ...bookmark, ...updates } : bookmark
      )
    );
    resetExportOutcome();
  };

  const addBookmark = () => {
    if (!inspection) {
      return;
    }
    const insertAt =
      selectedIndex >= 0 ? bookmarkBranchEnd(bookmarks, selectedIndex) : bookmarks.length;
    const level = selectedBookmark?.level ?? 0;
    const bookmark = toDraft({
      bold: false,
      colour: [0, 0, 0],
      italic: false,
      level,
      open: true,
      pageNumber: selectedBookmark?.pageNumber ?? 1,
      title: t("bookmark.default.newTitle")
    });
    setBookmarks((current) => [
      ...current.slice(0, insertAt),
      bookmark,
      ...current.slice(insertAt)
    ]);
    setSelectedId(bookmark.id);
    resetExportOutcome();
  };

  const deleteBranch = () => {
    if (selectedIndex < 0) {
      return;
    }
    const next = deleteBookmarkBranch(bookmarks, selectedIndex);
    setBookmarks(next);
    setSelectedId(next[Math.min(selectedIndex, next.length - 1)]?.id ?? null);
    resetExportOutcome();
  };

  const changeIndent = (direction: -1 | 1) => {
    if (selectedIndex < 0 || !selectedBookmark) {
      return;
    }
    if (direction === -1) {
      if (selectedBookmark.level === 0) {
        return;
      }
    } else if (
      selectedBookmark.level >= MAX_BOOKMARK_LEVEL ||
      !bookmarkHasPreviousSibling(bookmarks, selectedIndex)
    ) {
      return;
    }
    setBookmarks((current) =>
      direction === 1
        ? indentBookmarkBranch(current, selectedIndex, MAX_BOOKMARK_LEVEL)
        : outdentBookmarkBranch(current, selectedIndex)
    );
    resetExportOutcome();
  };

  const moveBranch = (direction: -1 | 1) => {
    if (selectedIndex < 0 || !selectedBookmark) {
      return;
    }
    setBookmarks((current) => moveBookmarkBranch(current, selectedIndex, direction));
    resetExportOutcome();
  };

  const generateFromHeadings = async () => {
    if (!pdfDocument || headingBusy || operationBusy) {
      return;
    }
    const runId = headingRunRef.current + 1;
    headingRunRef.current = runId;
    setHeadingBusy(true);
    setHeadingError(null);
    setHeadingReview(null);
    const headingPageLimit = Math.min(pdfDocument.numPages, MAX_HEADING_PAGES);
    setHeadingProgress({ page: 0, total: headingPageLimit });
    try {
      const result = await scanDocumentHeadings(
        pdfDocument,
        () => mountedRef.current && headingRunRef.current === runId,
        (page) => {
          if (mountedRef.current && headingRunRef.current === runId) {
            setHeadingProgress({ page, total: headingPageLimit });
          }
        }
      );
      if (mountedRef.current && headingRunRef.current === runId) {
        setHeadingReview({
          ...result,
          suggestions: result.suggestions.map((suggestion, index) => ({
            ...suggestion,
            id: `heading-${index + 1}`,
            selected: true
          }))
        });
      }
    } catch (reason) {
      if (
        mountedRef.current &&
        headingRunRef.current === runId &&
        !(reason instanceof HeadingScanCancelled)
      ) {
        setHeadingError("bookmark.heading.error");
      }
    } finally {
      if (mountedRef.current && headingRunRef.current === runId) {
        setHeadingBusy(false);
        setHeadingProgress(null);
      }
    }
  };

  const cancelHeadingScan = () => {
    headingRunRef.current += 1;
    setHeadingBusy(false);
    setHeadingProgress(null);
  };

  const applyHeadingSuggestions = () => {
    if (!headingReview) {
      return;
    }
    let previousLevel = 0;
    const generated = headingReview.suggestions
      .filter((suggestion) => suggestion.selected)
      .map((suggestion, index) => {
        const level = index === 0 ? 0 : Math.min(suggestion.level, previousLevel + 1);
        previousLevel = level;
        return toDraft({
          bold: level === 0,
          colour: [0, 0, 0],
          italic: false,
          level,
          open: true,
          pageNumber: suggestion.pageNumber,
          title: suggestion.title
        });
      });
    setBookmarks(generated);
    setSelectedId(generated[0]?.id ?? null);
    setHeadingReview(null);
    resetExportOutcome();
  };

  const exportBookmarks = async () => {
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
          filters: [{ name: t("bookmark.dialog.filter"), extensions: ["pdf"] }],
          title: t("bookmark.dialog.save")
        }));
      if (typeof outputPath !== "string") {
        return;
      }
      if (!mountedRef.current || requestRunRef.current !== runId) {
        return;
      }
      await bookmarkJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        bookmarks: bookmarks.map(({ id: _id, ...bookmark }) => bookmark),
        expectedSourceModifiedAtMs: inspection.sourceModifiedAtMs,
        expectedSourceSize: inspection.sourceSize,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        printedContents: toPdfPrintedContents(printedContents, bookmarks)
      });
    } catch (reason) {
      void reason;
      if (mountedRef.current && requestRunRef.current === runId) {
        setError("bookmark.error.startExport");
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setBusy(null);
      }
    }
  };

  const cancelBookmarkExport = async () => {
    if (!bookmarkJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await bookmarkJob.cancelJob();
    } catch (reason) {
      void reason;
      setCancelBusy(false);
      setError("bookmark.error.cancelExport");
    }
  };

  const cancelBookmarkReview = async () => {
    if (!bookmarkInspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await bookmarkInspectionJob.cancelJob();
    } catch (reason) {
      void reason;
      setError("bookmark.error.cancelReview");
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    escapeDisabled: headingBusy || operationBusy,
    onEscape: closeWorkspace
  });

  return (
    <>
      <section className="bookmark-studio">
        <div className="bookmark-heading">
          <div>
            <h3>{t("bookmark.heading.title")}</h3>
            <p>{t("bookmark.heading.description")}</p>
          </div>
          <Bookmark size={18} aria-hidden="true" />
        </div>
        <button
          className="wide-button"
          disabled={!desktopMode || operationBusy}
          onClick={() => void chooseSource()}
          type="button"
        >
          <FolderOpen size={17} aria-hidden="true" />
          {sourcePath ? t("bookmark.action.chooseAnother") : t("bookmark.action.choose")}
        </button>
        {sourcePath ? (
          <div className="bookmark-source">
            <FileText size={17} aria-hidden="true" />
            <span>
              <strong>{fileNameFromPath(sourcePath)}</strong>
              <small title={sourcePath}>{t("bookmark.source.local")}</small>
            </span>
          </div>
        ) : null}
        {sourcePath ? (
          <label className="bookmark-password">
            <span>{t("bookmark.password.label")} <small>{t("bookmark.password.optional")}</small></span>
            <span>
              <input
                autoComplete="off"
                disabled={operationBusy}
                onChange={(event) => {
                  setPassword(event.target.value);
                  resetExportOutcome();
                }}
                placeholder={t("bookmark.password.placeholder")}
                type={showPassword ? "text" : "password"}
                value={password}
              />
              <button
                aria-label={
                  showPassword
                    ? t("bookmark.password.hide")
                    : t("bookmark.password.show")
                }
                className="icon-button"
                disabled={operationBusy}
                onClick={() => setShowPassword((visible) => !visible)}
                title={
                  showPassword
                    ? t("bookmark.password.hide")
                    : t("bookmark.password.show")
                }
                type="button"
              >
                {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
              </button>
            </span>
          </label>
        ) : null}
        {!desktopMode ? (
          <div className="engine-state is-info">
            <Info size={16} aria-hidden="true" />
            <span>{t("bookmark.availability.desktopOnly")}</span>
          </div>
        ) : null}
        {displayedError && !workspaceOpen ? (
          <div className="engine-state is-missing" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{displayedError}</span>
          </div>
        ) : null}
        <button
          className="primary wide-button"
          disabled={!desktopMode || !sourcePath || operationBusy}
          onClick={() => void reviewBookmarks()}
          type="button"
        >
          {busy === "review" || bookmarkInspectionJob.isActive ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <ListTree size={17} aria-hidden="true" />}
          {busy === "review" || bookmarkInspectionJob.isActive
            ? t("bookmark.action.reviewing")
            : t("bookmark.action.review")}
        </button>
        {!workspaceOpen && bookmarkInspectionJob.job ? (
          <PdfJobProgress
            cancelling={reviewCancelBusy}
            connectionError={bookmarkInspectionJob.connectionError}
            job={bookmarkInspectionJob.job}
            onCancel={() => void cancelBookmarkReview()}
            onRetry={() => void reviewBookmarks()}
            retryDisabled={!desktopMode || !sourcePath || operationBusy}
          />
        ) : null}
        {!workspaceOpen && bookmarkJob.job ? (
          <PdfJobProgress
            cancelling={cancelBusy}
            connectionError={bookmarkJob.connectionError}
            job={bookmarkJob.job}
            onCancel={() => void cancelBookmarkExport()}
            onRetry={() => void exportBookmarks()}
            retryDisabled={!canExport}
          />
        ) : null}
        {!workspaceOpen &&
        !bookmarkInspectionJob.job &&
        !bookmarkJob.isActive &&
        bookmarkJob.connectionError ? (
          <div className="engine-state is-info" role="status">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t("job.connectionError")}</span>
          </div>
        ) : null}
        {!workspaceOpen && jobNotice ? (
          <div className="engine-state is-info" role="status">
            <Info size={16} aria-hidden="true" />
            <span>{t(jobNotice)}</span>
          </div>
        ) : null}
        {!workspaceOpen && exportResult ? (
          <BookmarkExportResultPanel result={exportResult} />
        ) : null}
      </section>

      {workspaceOpen && inspection && pdfDocument ? (
        <div className="dialog-backdrop bookmark-backdrop" role="presentation">
          <section
            aria-labelledby="bookmark-dialog-title"
            aria-modal="true"
            className="bookmark-dialog"
            data-dialog-root
            ref={dialogRef}
            role="dialog"
            tabIndex={-1}
          >
            <header>
              <div className="dialog-icon" aria-hidden="true"><Bookmark size={24} /></div>
              <div>
                <span className="eyebrow">{t("bookmark.workspace.eyebrow")}</span>
                <h2 id="bookmark-dialog-title">
                  {sourcePath ? fileNameFromPath(sourcePath) : t("bookmark.source.local")}
                </h2>
              </div>
              <button
                aria-label={t("bookmark.workspace.close")}
                className="icon-button"
                data-dialog-initial-focus
                disabled={operationBusy || headingBusy}
                onClick={closeWorkspace}
                title={t("bookmark.workspace.close")}
                type="button"
              >
                <X size={18} aria-hidden="true" />
              </button>
            </header>

            <div className="bookmark-toolbar">
              <span>
                {[
                  t("bookmark.summary.bookmarks", {
                    count: formatNumber(bookmarks.length)
                  }),
                  t("bookmark.summary.sourcePages", {
                    count: formatNumber(inspection.pageCount)
                  }),
                  ...(printedContents.enabled
                    ? [
                        t("bookmark.summary.contentsPages", {
                          count: formatNumber(contentsPageCount)
                        })
                      ]
                    : [])
                ].join(" | ")}
              </span>
              <div>
                <button disabled={headingBusy || operationBusy} onClick={addBookmark} type="button">
                  <Plus size={16} aria-hidden="true" /> {t("bookmark.action.add")}
                </button>
                <button disabled={headingBusy || operationBusy} onClick={() => void generateFromHeadings()} type="button">
                  <ScanText size={16} aria-hidden="true" /> {t("bookmark.action.generate")}
                </button>
                <button className="primary" disabled={!canExport} onClick={() => void exportBookmarks()} type="button">
                  {operationBusy ? <Loader2 className="spin" size={16} aria-hidden="true" /> : <Save size={16} aria-hidden="true" />}
                  {bookmarkJob.isActive
                    ? t("bookmark.action.exporting")
                    : busy === "export"
                      ? t("bookmark.action.choosing")
                      : t("bookmark.action.save")}
                </button>
              </div>
            </div>

            {headingBusy && headingProgress ? (
              <div className="bookmark-heading-progress" aria-live="polite">
                <Loader2 className="spin" size={16} aria-hidden="true" />
                <span>{t("bookmark.heading.analysing")}</span>
                <progress
                  aria-label={t("bookmark.heading.progressAria")}
                  max={headingProgress.total}
                  value={headingProgress.page}
                />
                <strong>
                  {t("bookmark.heading.progress", {
                    page: formatNumber(headingProgress.page),
                    total: formatNumber(headingProgress.total)
                  })}
                </strong>
                <button onClick={cancelHeadingScan} type="button">{t("common.cancel")}</button>
              </div>
            ) : null}

            {bookmarkJob.job ? (
              <PdfJobProgress
                cancelling={cancelBusy}
                connectionError={bookmarkJob.connectionError}
                job={bookmarkJob.job}
                onCancel={() => void cancelBookmarkExport()}
                onRetry={() => void exportBookmarks()}
                retryDisabled={!canExport}
              />
            ) : null}
            {!bookmarkJob.isActive && bookmarkJob.connectionError ? (
              <div className="bookmark-warning" role="status">
                <AlertCircle size={16} aria-hidden="true" />
                <span>{t("job.connectionError")}</span>
              </div>
            ) : null}
            {jobNotice ? (
              <div className="bookmark-warning" role="status">
                <Info size={16} aria-hidden="true" />
                <span>{t(jobNotice)}</span>
              </div>
            ) : null}

            <fieldset
              className="bookmark-workspace bookmark-workspace-fieldset"
              disabled={operationBusy || headingBusy}
            >
              <aside className="bookmark-tree-panel">
                <div className="bookmark-tree-heading">
                  <strong>{t("bookmark.outline.title")}</strong>
                  <small>{t("bookmark.outline.description")}</small>
                </div>
                <div className="bookmark-tree" aria-label={t("bookmark.outline.aria")}>
                  {bookmarks.length > 0 ? bookmarks.map((bookmark, index) => (
                    <button
                      aria-current={bookmark.id === selectedId ? "true" : undefined}
                      className={`${bookmark.id === selectedId ? "is-active" : ""}${bookmark.pageNumber === null ? " is-unresolved" : ""}`}
                      key={bookmark.id}
                      onClick={() => setSelectedId(bookmark.id)}
                      style={{ paddingLeft: `${0.55 + bookmark.level * 0.72}rem` }}
                      type="button"
                    >
                      <Bookmark size={13} aria-hidden="true" />
                      <span>
                        <strong>{bookmark.title || t("bookmark.default.untitled")}</strong>
                        <small>
                          {bookmark.pageNumber
                            ? t("bookmark.item.page", {
                                page: formatNumber(bookmark.pageNumber)
                              })
                            : t("bookmark.item.choosePage")}
                          {" | "}
                          {t("bookmark.item.level", {
                            level: formatNumber(bookmark.level + 1)
                          })}
                        </small>
                      </span>
                      <em>{formatNumber(index + 1)}</em>
                    </button>
                  )) : (
                    <div className="bookmark-empty">
                      <Bookmark size={22} aria-hidden="true" />
                      <strong>{t("bookmark.empty.title")}</strong>
                      <small>{t("bookmark.empty.description")}</small>
                    </div>
                  )}
                </div>
              </aside>

              <main className="bookmark-detail">
                {headingReview ? (
                  <HeadingReviewPanel
                    onApply={applyHeadingSuggestions}
                    onCancel={() => setHeadingReview(null)}
                    onSuggestionsChange={(suggestions) =>
                      setHeadingReview((current) => current ? { ...current, suggestions } : current)
                    }
                    review={headingReview}
                  />
                ) : selectedBookmark ? (
                  <>
                    <div className="bookmark-editor-grid">
                      <div className="bookmark-page-preview">
                        <PdfPageCanvas
                          document={pdfDocument}
                          pageNumber={selectedBookmark.pageNumber ?? 1}
                          targetWidth={360}
                          variant="page"
                        />
                      </div>
                      <div className="bookmark-fields">
                        <div className="bookmark-field-heading">
                          <div>
                            <span className="eyebrow">
                              {t("bookmark.editor.number", {
                                number: formatNumber(selectedIndex + 1)
                              })}
                            </span>
                            <h3>{t("bookmark.editor.title")}</h3>
                          </div>
                          <div className="bookmark-branch-actions">
                            <button aria-label={t("bookmark.action.moveUp")} className="icon-button" onClick={() => moveBranch(-1)} title={t("bookmark.action.moveUp")} type="button"><ArrowUp size={16} aria-hidden="true" /></button>
                            <button aria-label={t("bookmark.action.moveDown")} className="icon-button" onClick={() => moveBranch(1)} title={t("bookmark.action.moveDown")} type="button"><ArrowDown size={16} aria-hidden="true" /></button>
                            <button aria-label={t("bookmark.action.outdent")} className="icon-button" disabled={selectedBookmark.level === 0} onClick={() => changeIndent(-1)} title={t("bookmark.action.outdent")} type="button"><IndentDecrease size={16} aria-hidden="true" /></button>
                            <button aria-label={t("bookmark.action.indent")} className="icon-button" disabled={selectedBookmark.level >= MAX_BOOKMARK_LEVEL || !bookmarkHasPreviousSibling(bookmarks, selectedIndex)} onClick={() => changeIndent(1)} title={t("bookmark.action.indent")} type="button"><IndentIncrease size={16} aria-hidden="true" /></button>
                            <button aria-label={t("bookmark.action.delete")} className="icon-button is-danger" onClick={deleteBranch} title={t("bookmark.action.delete")} type="button"><Trash2 size={16} aria-hidden="true" /></button>
                          </div>
                        </div>
                        <label>
                          <span>{t("bookmark.field.title")}</span>
                          <input maxLength={256} onChange={(event) => updateSelected({ title: event.target.value })} type="text" value={selectedBookmark.title} />
                        </label>
                        <div className="bookmark-field-row">
                          <label>
                            <span>{t("bookmark.field.page")}</span>
                            <input
                              max={inspection.pageCount}
                              min={1}
                              onChange={(event) => updateSelected({ pageNumber: event.target.value ? Number(event.target.value) : null })}
                              type="number"
                              value={selectedBookmark.pageNumber ?? ""}
                            />
                          </label>
                          <label>
                            <span>{t("bookmark.field.level")}</span>
                            <output>{formatNumber(selectedBookmark.level + 1)}</output>
                          </label>
                        </div>
                        <div className="bookmark-style-controls">
                          <button className={selectedBookmark.bold ? "is-active" : undefined} onClick={() => updateSelected({ bold: !selectedBookmark.bold })} type="button"><strong>B</strong> {t("bookmark.style.bold")}</button>
                          <button className={selectedBookmark.italic ? "is-active" : undefined} onClick={() => updateSelected({ italic: !selectedBookmark.italic })} type="button"><i>I</i> {t("bookmark.style.italic")}</button>
                          <label>
                            <span>{t("bookmark.style.colour")}</span>
                            <input aria-label={t("bookmark.style.colourAria")} onChange={(event) => updateSelected({ colour: hexToColour(event.target.value) })} type="color" value={colourToHex(selectedBookmark.colour)} />
                          </label>
                        </div>
                        {bookmarkHasChildren(bookmarks, selectedIndex) ? (
                          <label className="bookmark-open-control">
                            <input checked={selectedBookmark.open} onChange={(event) => updateSelected({ open: event.target.checked })} type="checkbox" />
                            {t("bookmark.field.expandChildren")}
                          </label>
                        ) : null}
                        <div className="bookmark-target-note">
                          <Info size={16} aria-hidden="true" />
                          <span>{t("bookmark.field.fitTarget")}</span>
                        </div>
                      </div>
                    </div>
                  </>
                ) : (
                  <div className="bookmark-detail-empty">
                    <Bookmark size={28} aria-hidden="true" />
                    <strong>{t("bookmark.first.title")}</strong>
                    <button onClick={addBookmark} type="button"><Plus size={16} aria-hidden="true" /> {t("bookmark.action.addBookmark")}</button>
                  </div>
                )}
                {!headingReview ? (
                  <>
                    <PrintedContentsPanel
                      bookmarks={bookmarks}
                      disabled={operationBusy || headingBusy}
                      onChange={changePrintedContents}
                      value={printedContents}
                    />
                    {localisedInspectionWarnings.map((warning) => (
                      <div className="bookmark-warning" key={warning}><Info size={16} aria-hidden="true" /><span>{warning}</span></div>
                    ))}
                    {headingError ? <div className="bookmark-error" role="alert"><AlertCircle size={16} aria-hidden="true" /><span>{t(headingError)}</span></div> : null}
                    <PdfEditSafetyNotice
                      acknowledged={signatureRiskAcknowledged}
                      busy={operationBusy}
                      editSafety={editSafety}
                      onAcknowledgedChange={setSignatureRiskAcknowledged}
                      rewriteDescription={t("bookmark.safety.rewriteDescription")}
                    />
                    <OutputProtectionFields
                      disabled={operationBusy}
                      onChange={changeOutputProtection}
                      qpdfAvailable={qpdfAvailable}
                      value={outputProtection}
                    />
                    {displayedError ? <div className="bookmark-error" role="alert"><AlertCircle size={16} aria-hidden="true" /><span>{displayedError}</span></div> : null}
                    {exportResult ? (
                      <BookmarkExportResultPanel result={exportResult} />
                    ) : null}
                  </>
                ) : null}
              </main>
            </fieldset>
          </section>
        </div>
      ) : null}
    </>
  );
}

function PrintedContentsPanel({
  bookmarks,
  disabled,
  onChange,
  value
}: {
  bookmarks: readonly BookmarkDraft[];
  disabled: boolean;
  onChange: (value: PrintedContentsDraft) => void;
  value: PrintedContentsDraft;
}) {
  const { formatNumber, t } = useI18n();
  const entries = selectPrintedContentsEntries(bookmarks, value.maximumLevel);
  const pageCount = estimatePrintedContentsPageCount(entries.length);
  const validationMessage = localisePrintedContentsValidation(
    printedContentsValidationMessage(value, bookmarks),
    t
  );
  const update = (changes: Partial<PrintedContentsDraft>) =>
    onChange({ ...value, ...changes });

  return (
    <section className={`printed-contents-panel${value.enabled ? " is-enabled" : ""}`}>
      <label className="printed-contents-toggle">
        <input
          checked={value.enabled}
          disabled={disabled}
          onChange={(event) => update({ enabled: event.target.checked })}
          type="checkbox"
        />
        <span>
          <strong>{t("bookmark.contents.toggle")}</strong>
          <small>{t("bookmark.contents.description")}</small>
        </span>
        <ListTree size={19} aria-hidden="true" />
      </label>

      {value.enabled ? (
        <div className="printed-contents-options">
          <div className="printed-contents-fields">
            <label>
              <span>{t("bookmark.contents.titleLabel")}</span>
              <input
                disabled={disabled}
                maxLength={MAX_PRINTED_CONTENTS_TITLE_CHARACTERS}
                onChange={(event) => update({ title: event.target.value })}
                type="text"
                value={value.title}
              />
            </label>
            <label>
              <span>{t("bookmark.contents.levelsLabel")}</span>
              <select
                disabled={disabled}
                onChange={(event) => update({ maximumLevel: Number(event.target.value) })}
                value={value.maximumLevel}
              >
                {Array.from({ length: MAX_BOOKMARK_LEVEL + 1 }, (_, level) => (
                  <option key={level} value={level}>
                    {level === 0
                      ? t("bookmark.contents.levelOneOnly")
                      : t("bookmark.contents.levelRange", {
                          maximum: formatNumber(level + 1)
                        })}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <label className="printed-contents-bookmark-option">
            <input
              checked={value.addBookmark}
              disabled={disabled}
              onChange={(event) => update({ addBookmark: event.target.checked })}
              type="checkbox"
            />
            {t("bookmark.contents.addSidebarBookmark")}
          </label>

          <div className="printed-contents-preview" aria-label={t("bookmark.contents.previewAria")}>
            <header>
              <span>{value.title.trim() || t("bookmark.contents.defaultTitle")}</span>
              <small>{t("bookmark.contents.a4Preview")}</small>
            </header>
            {entries.length > 0 ? (
              <ol>
                {entries.slice(0, 5).map((bookmark) => (
                  <li key={bookmark.id} style={{ paddingLeft: `${bookmark.level * 0.65}rem` }}>
                    <span>{bookmark.title.trim() || t("bookmark.default.untitled")}</span>
                    <i aria-hidden="true" />
                    <strong>
                      {bookmark.pageNumber === null
                        ? "?"
                        : formatNumber(bookmark.pageNumber + pageCount)}
                    </strong>
                  </li>
                ))}
              </ol>
            ) : (
              <p>{t("bookmark.contents.empty")}</p>
            )}
            {entries.length > 5 ? (
              <small>
                {t("bookmark.contents.moreEntries", {
                  count: formatNumber(entries.length - 5)
                })}
              </small>
            ) : null}
          </div>

          {validationMessage ? (
            <div className="bookmark-error" role="alert">
              <AlertCircle size={16} aria-hidden="true" />
              <span>{validationMessage}</span>
            </div>
          ) : (
            <div className="printed-contents-summary" role="status">
              <CheckCircle2 size={16} aria-hidden="true" />
              <span>
                {t("bookmark.contents.summary", {
                  entries: formatNumber(entries.length),
                  pages: formatNumber(pageCount)
                })}
              </span>
            </div>
          )}
          <div className="printed-contents-note">
            <Info size={16} aria-hidden="true" />
            <span>{t("bookmark.contents.accessibilityNote")}</span>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function BookmarkExportResultPanel({ result }: { result: ExportPdfBookmarksResult }) {
  const { formatNumber, t } = useI18n();
  const warnings = localiseBookmarkWarnings(result.warnings, t, formatNumber);
  return (
    <div className="bookmark-export-result">
      <CheckCircle2 size={18} aria-hidden="true" />
      <span>
        <strong>{t("bookmark.result.title")}</strong>
        <small>
          {t("bookmark.result.summary", {
            bookmarks: formatNumber(result.bookmarkCount),
            encryption:
              result.encryption === "AES-256"
                ? t("bookmark.result.protected")
                : t("bookmark.result.unprotected"),
            pages: formatNumber(result.pageCount),
            size: formatBytes(result.bytesWritten, formatNumber)
          })}
        </small>
        {result.contentsPageCount > 0 ? (
          <small>
            {t("bookmark.result.contents", {
              entries: formatNumber(result.printedEntryCount),
              pages: formatNumber(result.contentsPageCount)
            })}
          </small>
        ) : null}
        <small title={result.outputPath}>{fileNameFromPath(result.outputPath)}</small>
        {warnings.map((warning) => (
          <small key={warning}>{warning}</small>
        ))}
      </span>
    </div>
  );
}

function HeadingReviewPanel({
  onApply,
  onCancel,
  onSuggestionsChange,
  review
}: {
  onApply: () => void;
  onCancel: () => void;
  onSuggestionsChange: (suggestions: SelectableHeading[]) => void;
  review: HeadingReview;
}) {
  const { formatNumber, t } = useI18n();
  const selectedCount = review.suggestions.filter((suggestion) => suggestion.selected).length;
  const setAll = (selected: boolean) =>
    onSuggestionsChange(review.suggestions.map((suggestion) => ({ ...suggestion, selected })));
  return (
    <section className="heading-review-panel">
      <header>
        <div>
          <span className="eyebrow">{t("bookmark.heading.reviewEyebrow")}</span>
          <h3>
            {t("bookmark.heading.reviewSummary", {
              headings: formatNumber(review.suggestions.length),
              pages: formatNumber(review.scannedPages)
            })}
          </h3>
        </div>
        <button aria-label={t("bookmark.heading.closeReview")} className="icon-button" onClick={onCancel} title={t("bookmark.heading.closeReview")} type="button"><X size={17} aria-hidden="true" /></button>
      </header>
      {review.limitReached ? (
        <div className="bookmark-warning"><Info size={16} aria-hidden="true" /><span>{t("bookmark.heading.limitReached")}</span></div>
      ) : null}
      <div className="heading-review-actions">
        <span>{t("bookmark.heading.selected", { count: formatNumber(selectedCount) })}</span>
        <div>
          <button onClick={() => setAll(true)} type="button">{t("bookmark.heading.selectAll")}</button>
          <button onClick={() => setAll(false)} type="button">{t("bookmark.heading.selectNone")}</button>
        </div>
      </div>
      <div className="heading-suggestion-list">
        {review.suggestions.length > 0 ? review.suggestions.map((suggestion) => (
          <label key={suggestion.id} style={{ paddingLeft: `${0.65 + suggestion.level * 0.75}rem` }}>
            <input
              checked={suggestion.selected}
              onChange={(event) =>
                onSuggestionsChange(
                  review.suggestions.map((item) =>
                    item.id === suggestion.id ? { ...item, selected: event.target.checked } : item
                  )
                )
              }
              type="checkbox"
            />
            <span>
              <strong>{suggestion.title}</strong>
              <small>
                {t("bookmark.heading.suggestionDetail", {
                  confidence: formatNumber(suggestion.confidence),
                  level: formatNumber(suggestion.level + 1),
                  page: formatNumber(suggestion.pageNumber),
                  size: formatNumber(suggestion.fontSize, { maximumFractionDigits: 1 })
                })}
              </small>
            </span>
          </label>
        )) : (
          <div className="bookmark-detail-empty"><ScanText size={26} aria-hidden="true" /><strong>{t("bookmark.heading.emptyTitle")}</strong><small>{t("bookmark.heading.emptyDescription")}</small></div>
        )}
      </div>
      <footer>
        <button onClick={onCancel} type="button">{t("bookmark.heading.keepDraft")}</button>
        <button className="primary" disabled={selectedCount === 0} onClick={onApply} type="button">
          {t("bookmark.heading.replaceDraft", { count: formatNumber(selectedCount) })}
        </button>
      </footer>
    </section>
  );
}

async function scanDocumentHeadings(
  document: PDFDocumentProxy,
  isCurrent: () => boolean,
  onProgress: (page: number) => void
) {
  const lines: HeadingLine[] = [];
  let itemCount = 0;
  const pageLimitReached = document.numPages > MAX_HEADING_PAGES;
  let limitReached = false;
  let scannedPages = 0;

  const pageLimit = Math.min(document.numPages, MAX_HEADING_PAGES);
  for (let pageNumber = 1; pageNumber <= pageLimit; pageNumber += 1) {
    if (!isCurrent()) {
      throw new HeadingScanCancelled();
    }
    const page = await document.getPage(pageNumber);
    try {
      const result = await extractPageLines(page, pageNumber, () => {
        if (!isCurrent()) {
          throw new HeadingScanCancelled();
        }
        return Math.min(
          MAX_HEADING_ITEMS_PER_PAGE,
          Math.max(0, MAX_HEADING_ITEMS - itemCount)
        );
      });
      itemCount += result.itemCount;
      lines.push(...result.lines.slice(0, Math.max(0, MAX_HEADING_LINES - lines.length)));
      limitReached =
        limitReached ||
        result.limitReached ||
        itemCount >= MAX_HEADING_ITEMS ||
        lines.length >= MAX_HEADING_LINES;
      scannedPages = pageNumber;
      onProgress(pageNumber);
      if (limitReached) {
        break;
      }
    } finally {
      page.cleanup();
    }
  }
  if (!isCurrent()) {
    throw new HeadingScanCancelled();
  }
  return {
    limitReached: limitReached || pageLimitReached,
    scannedPages,
    suggestions: detectHeadingSuggestions(lines)
  };
}

async function extractPageLines(
  page: PDFPageProxy,
  pageNumber: number,
  itemBudget: () => number
) {
  const reader = page.streamTextContent({ disableNormalization: false }).getReader();
  const fragments: PageFragment[] = [];
  let itemCount = 0;
  let finished = false;
  let limitReached = false;
  try {
    reading: while (true) {
      const chunk = await reader.read();
      if (chunk.done) {
        finished = true;
        break;
      }
      for (const item of chunk.value.items) {
        const budget = itemBudget();
        if (itemCount >= budget) {
          limitReached = true;
          break reading;
        }
        itemCount += 1;
        if (!("str" in item) || !item.str.trim()) {
          continue;
        }
        fragments.push({
          fontSize: Math.max(0.1, Math.abs(item.height), Math.hypot(item.transform[2], item.transform[3])),
          text: item.str,
          width: Math.abs(item.width),
          x: item.transform[4],
          y: item.transform[5]
        });
      }
    }
  } finally {
    if (!finished) {
      await reader.cancel().catch(() => undefined);
    }
  }
  return { itemCount, limitReached, lines: groupPageFragments(fragments, pageNumber) };
}

function groupPageFragments(fragments: PageFragment[], pageNumber: number): HeadingLine[] {
  const rows = new Map<number, PageFragment[]>();
  for (const fragment of fragments) {
    const key = Math.round(fragment.y / 2);
    const row = rows.get(key) ?? [];
    row.push(fragment);
    rows.set(key, row);
  }
  const lines: HeadingLine[] = [];
  for (const row of [...rows.values()].sort((left, right) => right[0].y - left[0].y)) {
    const sorted = row.sort((left, right) => left.x - right.x);
    let segment: PageFragment[] = [];
    const flush = () => {
      const text = segment.map((fragment) => fragment.text).join(" ").replace(/\s+/gu, " ").trim();
      if (text) {
        lines.push({
          fontSize: Math.max(...segment.map((fragment) => fragment.fontSize)),
          pageNumber,
          text,
          y: Math.max(...segment.map((fragment) => fragment.y))
        });
      }
      segment = [];
    };
    for (const fragment of sorted) {
      const previous = segment[segment.length - 1];
      if (
        previous &&
        fragment.x - (previous.x + previous.width) > Math.max(56, previous.fontSize * 4.5)
      ) {
        flush();
      }
      segment.push(fragment);
    }
    flush();
  }
  return lines;
}

class HeadingScanCancelled extends Error {
  constructor() {
    super("Heading analysis was cancelled.");
    this.name = "HeadingScanCancelled";
  }
}

function colourToHex(colour: [number, number, number]) {
  return `#${colour.map((value) => Math.round(Math.max(0, Math.min(1, value)) * 255).toString(16).padStart(2, "0")).join("")}`;
}

function hexToColour(value: string): [number, number, number] {
  return [
    Number.parseInt(value.slice(1, 3), 16) / 255,
    Number.parseInt(value.slice(3, 5), 16) / 255,
    Number.parseInt(value.slice(5, 7), 16) / 255
  ];
}

function suggestedOutputPath(sourcePath: string) {
  return sourcePath.replace(/\.pdf$/iu, "-bookmarked.pdf");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/u).pop() || path;
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
  return `${formatNumber(bytes / (1024 * 1024), { maximumFractionDigits: 1 })} MB`;
}

class BookmarkUserError extends Error {
  readonly key: TranslationKey;

  constructor(key: TranslationKey) {
    super(key);
    this.name = "BookmarkUserError";
    this.key = key;
  }
}
