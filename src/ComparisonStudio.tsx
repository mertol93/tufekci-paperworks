import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { PDFPageProxy } from "pdfjs-dist";
import { useDialogFocus } from "./accessibility";
import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Columns2,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  GitCompareArrows,
  Info,
  Loader2,
  X
} from "lucide-react";
import {
  comparePageText,
  comparisonGeometryChanged,
  diffRgbaPixels,
  type ComparisonPageGeometry,
  type TextComparison
} from "./comparison";
import { useI18n } from "./I18nProvider";
import type { Translate } from "./i18n";
import {
  createPdfLoadingTask,
  isIncorrectPasswordReason,
  type PDFDocumentProxy,
  type PdfRangeSource
} from "./pdf";

type ComparisonStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
};

type ComparisonSide = "left" | "right";
type ComparisonStatus =
  | "added"
  | "changed"
  | "error"
  | "limited"
  | "pending"
  | "removed"
  | "same";

type LoadedDocument = {
  document: PDFDocumentProxy;
  name: string;
  path: string;
};

type LoadedComparison = {
  left: LoadedDocument;
  right: LoadedDocument;
};

type PageComparison = {
  error?: string;
  geometryChanged: boolean;
  leftGeometry?: ComparisonPageGeometry;
  pageNumber: number;
  rightGeometry?: ComparisonPageGeometry;
  status: ComparisonStatus;
  text?: TextComparison;
};

type VisualBuffers = {
  height: number;
  left: Uint8ClampedArray;
  right: Uint8ClampedArray;
  width: number;
};

type VisualState = {
  buffers: VisualBuffers | null;
  busy: boolean;
  error: string | null;
};

type ComparisonLoadingTasks = {
  left: ReturnType<typeof createPdfLoadingTask>;
  right: ReturnType<typeof createPdfLoadingTask>;
};

type SourcePickerProps = {
  disabled: boolean;
  label: string;
  onChoose: () => void;
  onPasswordChange: (value: string) => void;
  onShowPasswordChange: () => void;
  password: string;
  path: string | null;
  showPassword: boolean;
};

const MAX_COMPARISON_PAGES = 2_000;
const MAX_TEXT_CHARACTERS = 500_000;
const MAX_TEXT_ITEMS = 100_000;
const MAX_VISUAL_PIXELS = 2_000_000;
const MAX_VISUAL_WIDTH = 1_000;
const MAX_VISUAL_HEIGHT = 1_400;

export function ComparisonStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath
}: ComparisonStudioProps) {
  const { formatNumber, t } = useI18n();
  const [leftPath, setLeftPath] = useState<string | null>(initialSourcePath ?? null);
  const [leftPassword, setLeftPassword] = useState(initialSourcePassword ?? "");
  const [leftPasswordVisible, setLeftPasswordVisible] = useState(false);
  const [rightPath, setRightPath] = useState<string | null>(null);
  const [rightPassword, setRightPassword] = useState("");
  const [rightPasswordVisible, setRightPasswordVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<LoadedComparison | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [pageResults, setPageResults] = useState<PageComparison[]>([]);
  const [analysisProgress, setAnalysisProgress] = useState(0);
  const [analysisRunning, setAnalysisRunning] = useState(false);
  const [selectedPage, setSelectedPage] = useState(1);
  const [pageFilter, setPageFilter] = useState<"all" | "changed">("all");
  const [visualTolerance, setVisualTolerance] = useState(28);
  const [visual, setVisual] = useState<VisualState>({
    buffers: null,
    busy: false,
    error: null
  });
  const loadingTasksRef = useRef<ComparisonLoadingTasks | null>(null);
  const pageListRef = useRef<HTMLDivElement>(null);
  const analysisRunRef = useRef(0);
  const visualRunRef = useRef(0);
  const mountedRef = useRef(true);

  useEffect(() => {
    if (initialSourcePath) {
      setLeftPath(initialSourcePath);
      setLeftPassword(initialSourcePassword ?? "");
      setError(null);
    }
  }, [initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      analysisRunRef.current += 1;
      visualRunRef.current += 1;
      const tasks = loadingTasksRef.current;
      loadingTasksRef.current = null;
      if (tasks) {
        void Promise.allSettled([tasks.left.destroy(), tasks.right.destroy()]);
      }
    };
  }, []);

  const closeComparison = useCallback(() => {
    analysisRunRef.current += 1;
    visualRunRef.current += 1;
    const tasks = loadingTasksRef.current;
    loadingTasksRef.current = null;
    if (tasks) {
      void Promise.allSettled([tasks.left.destroy(), tasks.right.destroy()]);
    }
    setWorkspaceOpen(false);
    setLoaded(null);
    setPageResults([]);
    setAnalysisRunning(false);
    setVisual({ buffers: null, busy: false, error: null });
  }, []);

  useEffect(() => {
    if (!workspaceOpen) {
      return;
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeComparison();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [closeComparison, workspaceOpen]);

  const chooseSource = async (side: ComparisonSide) => {
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("comparison.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title:
          side === "left"
            ? t("comparison.dialog.chooseEarlier")
            : t("comparison.dialog.chooseLater")
      });
      if (typeof selected !== "string") {
        return;
      }
      if (!mountedRef.current) {
        return;
      }
      if (side === "left") {
        setLeftPath(selected);
        setLeftPassword("");
      } else {
        setRightPath(selected);
        setRightPassword("");
      }
    } catch {
      if (mountedRef.current) {
        setError(t("comparison.error.choose"));
      }
    }
  };

  const startComparison = async () => {
    if (!desktopMode || !leftPath || !rightPath || busy) {
      return;
    }

    setBusy(true);
    setError(null);
    const runId = analysisRunRef.current + 1;
    analysisRunRef.current = runId;
    visualRunRef.current += 1;
    const previousTasks = loadingTasksRef.current;
    loadingTasksRef.current = null;
    if (previousTasks) {
      await Promise.allSettled([previousTasks.left.destroy(), previousTasks.right.destroy()]);
    }
    if (!mountedRef.current || analysisRunRef.current !== runId) {
      return;
    }

    let tasks: ComparisonLoadingTasks | null = null;
    try {
      const [leftSource, rightSource] = await Promise.all([
        invoke<PdfRangeSource>("open_local_pdf", { path: leftPath }),
        invoke<PdfRangeSource>("open_local_pdf", { path: rightPath })
      ]);
      if (!mountedRef.current || analysisRunRef.current !== runId) {
        return;
      }
      const leftTask = createPdfLoadingTask(leftSource, leftPassword || null);
      const rightTask = createPdfLoadingTask(rightSource, rightPassword || null);
      tasks = { left: leftTask, right: rightTask };
      loadingTasksRef.current = tasks;

      const [leftDocument, rightDocument] = await Promise.all([
        openComparisonDocument(leftTask, "earlier", t),
        openComparisonDocument(rightTask, "later", t)
      ]);
      if (!mountedRef.current || analysisRunRef.current !== runId) {
        if (loadingTasksRef.current === tasks) {
          loadingTasksRef.current = null;
        }
        await Promise.allSettled([leftTask.destroy(), rightTask.destroy()]);
        return;
      }
      const pageCount = Math.max(leftDocument.numPages, rightDocument.numPages);
      if (pageCount > MAX_COMPARISON_PAGES) {
        throw new ComparisonUserError(
          t("comparison.error.pageLimit", {
            count: formatNumber(MAX_COMPARISON_PAGES)
          })
        );
      }

      const nextLoaded: LoadedComparison = {
        left: { document: leftDocument, name: leftSource.name, path: leftSource.path },
        right: { document: rightDocument, name: rightSource.name, path: rightSource.path }
      };
      const initialResults = createInitialResults(
        leftDocument.numPages,
        rightDocument.numPages
      );
      setLoaded(nextLoaded);
      setPageResults(initialResults);
      setAnalysisProgress(0);
      setAnalysisRunning(true);
      setSelectedPage(1);
      setPageFilter("all");
      setVisualTolerance(28);
      setWorkspaceOpen(true);
      void analyseDocuments(nextLoaded, initialResults, {
        isCurrent: () => mountedRef.current && analysisRunRef.current === runId,
        onProgress: (nextResults, progress) => {
          if (analysisRunRef.current === runId) {
            setPageResults(nextResults);
            setAnalysisProgress(progress);
          }
        },
        onSettled: () => {
          if (analysisRunRef.current === runId) {
            setAnalysisRunning(false);
          }
        },
        t
      });
    } catch (reason) {
      if (loadingTasksRef.current === tasks) {
        loadingTasksRef.current = null;
      }
      if (tasks) {
        await Promise.allSettled([tasks.left.destroy(), tasks.right.destroy()]);
      }
      if (mountedRef.current && analysisRunRef.current === runId) {
        setLoaded(null);
        setWorkspaceOpen(false);
        setError(
          reason instanceof ComparisonUserError
            ? reason.message
            : t("comparison.error.open")
        );
      }
    } finally {
      if (mountedRef.current && analysisRunRef.current === runId) {
        setBusy(false);
      }
    }
  };

  const filteredPages = useMemo(
    () =>
      pageFilter === "all"
        ? pageResults
        : pageResults.filter((page) => page.status !== "same" && page.status !== "pending"),
    [pageFilter, pageResults]
  );
  const selectedResult = pageResults[selectedPage - 1];
  const summary = useMemo(() => summariseResults(pageResults), [pageResults]);
  const visualDifference = useMemo(
    () =>
      visual.buffers
        ? diffRgbaPixels(
            visual.buffers.left,
            visual.buffers.right,
            visualTolerance
          )
        : null,
    [visual.buffers, visualTolerance]
  );

  useEffect(() => {
    if (filteredPages.length > 0 && !filteredPages.some((page) => page.pageNumber === selectedPage)) {
      setSelectedPage(filteredPages[0].pageNumber);
    }
  }, [filteredPages, selectedPage]);

  useEffect(() => {
    pageListRef.current
      ?.querySelector<HTMLElement>('[aria-current="page"]')
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [pageFilter, selectedPage]);

  useEffect(() => {
    if (!loaded || !workspaceOpen) {
      return;
    }
    const runId = visualRunRef.current + 1;
    visualRunRef.current = runId;
    let alive = true;
    const renderTasks: Array<{ cancel: () => void }> = [];
    setVisual({ buffers: null, busy: true, error: null });

    void renderVisualBuffers(loaded, selectedPage, renderTasks)
      .then((buffers) => {
        if (alive && visualRunRef.current === runId) {
          setVisual({ buffers, busy: false, error: null });
        }
      })
      .catch((reason: unknown) => {
        if (alive && visualRunRef.current === runId) {
          setVisual({
            buffers: null,
            busy: false,
            error: t("comparison.error.visual")
          });
        }
      });

    return () => {
      alive = false;
      for (const task of renderTasks) {
        task.cancel();
      }
    };
  }, [loaded, selectedPage, t, workspaceOpen]);

  const canCompare = desktopMode && Boolean(leftPath && rightPath) && !busy;
  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    onEscape: closeComparison
  });

  return (
    <>
      <section className="comparison-studio">
        <div className="comparison-heading">
          <div>
            <h3>{t("comparison.heading.title")}</h3>
            <p>{t("comparison.heading.description")}</p>
          </div>
          <GitCompareArrows size={18} aria-hidden="true" />
        </div>

        <SourcePicker
          disabled={!desktopMode || busy}
          label={t("comparison.source.earlier")}
          onChoose={() => void chooseSource("left")}
          onPasswordChange={setLeftPassword}
          onShowPasswordChange={() => setLeftPasswordVisible((visible) => !visible)}
          password={leftPassword}
          path={leftPath}
          showPassword={leftPasswordVisible}
        />
        <SourcePicker
          disabled={!desktopMode || busy}
          label={t("comparison.source.later")}
          onChoose={() => void chooseSource("right")}
          onPasswordChange={setRightPassword}
          onShowPasswordChange={() => setRightPasswordVisible((visible) => !visible)}
          password={rightPassword}
          path={rightPath}
          showPassword={rightPasswordVisible}
        />

        {!desktopMode ? (
          <div className="engine-state is-info">
            <Info size={16} aria-hidden="true" />
            <span>{t("comparison.desktopOnly")}</span>
          </div>
        ) : null}

        {error ? (
          <div className="engine-state is-missing" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}

        <button
          className="primary wide-button"
          disabled={!canCompare}
          onClick={() => void startComparison()}
          type="button"
        >
          {busy ? (
            <Loader2 className="spin" size={17} aria-hidden="true" />
          ) : (
            <Columns2 size={17} aria-hidden="true" />
          )}
          {busy ? t("comparison.action.opening") : t("comparison.action.compare")}
        </button>
      </section>

      {workspaceOpen && loaded ? (
        <div className="dialog-backdrop comparison-backdrop" role="presentation">
          <section
            aria-labelledby="comparison-title"
            aria-modal="true"
            className="comparison-dialog"
            data-dialog-root
            ref={dialogRef}
            role="dialog"
            tabIndex={-1}
          >
            <header>
              <div className="dialog-icon" aria-hidden="true">
                <GitCompareArrows size={24} />
              </div>
              <div>
                <span className="eyebrow">{t("comparison.dialog.eyebrow")}</span>
                <h2 id="comparison-title">
                  {t("comparison.dialog.title", {
                    left: loaded.left.name,
                    right: loaded.right.name
                  })}
                </h2>
              </div>
              <button
                aria-label={t("comparison.close.aria")}
                className="icon-button"
                data-dialog-initial-focus
                onClick={closeComparison}
                title={t("comparison.close.title")}
                type="button"
              >
                <X size={18} aria-hidden="true" />
              </button>
            </header>

            <div
              className="comparison-summary"
              aria-label={t("comparison.summary.aria")}
            >
              <SummaryMetric
                label={t("comparison.summary.checked")}
                value={t("comparison.summary.checkedValue", {
                  checked: formatNumber(summary.checked),
                  total: formatNumber(pageResults.length)
                })}
              />
              <SummaryMetric
                label={t("comparison.summary.changed")}
                value={formatNumber(summary.changed)}
                tone="changed"
              />
              <SummaryMetric
                label={t("comparison.summary.added")}
                value={formatNumber(summary.added)}
                tone="added"
              />
              <SummaryMetric
                label={t("comparison.summary.removed")}
                value={formatNumber(summary.removed)}
                tone="removed"
              />
              <SummaryMetric
                label={t("comparison.summary.review")}
                value={formatNumber(summary.review)}
                tone="review"
              />
            </div>

            {analysisRunning ? (
              <div className="comparison-analysis-progress" aria-live="polite">
                <Loader2 className="spin" size={16} aria-hidden="true" />
                <span>{t("comparison.analysis.running")}</span>
                <progress max={pageResults.length} value={analysisProgress} />
                <strong>
                  {t("comparison.summary.checkedValue", {
                    checked: formatNumber(analysisProgress),
                    total: formatNumber(pageResults.length)
                  })}
                </strong>
              </div>
            ) : (
              <div className="comparison-analysis-progress is-complete">
                <CheckCircle2 size={16} aria-hidden="true" />
                <span>{t("comparison.analysis.complete")}</span>
                <strong>
                  {t(
                    pageResults.length === 1
                      ? "comparison.analysis.pages.one"
                      : "comparison.analysis.pages.other",
                    { count: formatNumber(pageResults.length) }
                  )}
                </strong>
              </div>
            )}

            <div className="comparison-workspace">
              <aside className="comparison-page-panel">
                <div
                  className="comparison-filter"
                  aria-label={t("comparison.filter.aria")}
                >
                  <button
                    className={pageFilter === "all" ? "is-active" : undefined}
                    onClick={() => setPageFilter("all")}
                    type="button"
                  >
                    {t("comparison.filter.all")}
                  </button>
                  <button
                    className={pageFilter === "changed" ? "is-active" : undefined}
                    onClick={() => setPageFilter("changed")}
                    type="button"
                  >
                    {t("comparison.filter.differences")}
                  </button>
                </div>
                <div
                  className="comparison-page-list"
                  aria-label={t("comparison.pages.aria")}
                  ref={pageListRef}
                >
                  {filteredPages.length > 0 ? (
                    filteredPages.map((page) => (
                      <button
                        aria-current={page.pageNumber === selectedPage ? "page" : undefined}
                        className={`comparison-page-item is-${page.status}${
                          page.pageNumber === selectedPage ? " is-active" : ""
                        }`}
                        key={page.pageNumber}
                        onClick={() => setSelectedPage(page.pageNumber)}
                        type="button"
                      >
                        <span>
                          {t("comparison.page.label", {
                            page: formatNumber(page.pageNumber)
                          })}
                        </span>
                        <small>{statusLabel(page.status, t)}</small>
                      </button>
                    ))
                  ) : (
                    <div className="comparison-empty-filter">
                      <CheckCircle2 size={18} aria-hidden="true" />
                      <span>{t("comparison.filter.empty")}</span>
                    </div>
                  )}
                </div>
              </aside>

              <main className="comparison-detail">
                <div className="comparison-detail-toolbar">
                  <div className="comparison-page-navigation">
                    <button
                      aria-label={t("comparison.navigation.previous")}
                      className="icon-button"
                      disabled={selectedPage <= 1}
                      onClick={() => setSelectedPage((page) => Math.max(1, page - 1))}
                      title={t("comparison.navigation.previous")}
                      type="button"
                    >
                      <ChevronLeft size={18} aria-hidden="true" />
                    </button>
                    <strong>
                      {t("comparison.navigation.page", {
                        page: formatNumber(selectedPage),
                        total: formatNumber(pageResults.length)
                      })}
                    </strong>
                    <button
                      aria-label={t("comparison.navigation.next")}
                      className="icon-button"
                      disabled={selectedPage >= pageResults.length}
                      onClick={() =>
                        setSelectedPage((page) => Math.min(pageResults.length, page + 1))
                      }
                      title={t("comparison.navigation.next")}
                      type="button"
                    >
                      <ChevronRight size={18} aria-hidden="true" />
                    </button>
                  </div>
                  <label className="comparison-tolerance">
                    <span>
                      {t("comparison.visual.tolerance")}{" "}
                      <strong>{formatNumber(visualTolerance)}</strong>
                    </span>
                    <input
                      aria-label={t("comparison.visual.toleranceAria")}
                      max={96}
                      min={0}
                      onChange={(event) => setVisualTolerance(Number(event.target.value))}
                      type="range"
                      value={visualTolerance}
                    />
                  </label>
                </div>

                <div className="comparison-visuals" aria-busy={visual.busy}>
                  {visual.busy ? (
                    <div className="comparison-visual-state">
                      <Loader2 className="spin" size={22} aria-hidden="true" />
                      <span>{t("comparison.visual.rendering")}</span>
                    </div>
                  ) : visual.error ? (
                    <div className="comparison-visual-state is-error" role="alert">
                      <AlertCircle size={22} aria-hidden="true" />
                      <span>{visual.error}</span>
                    </div>
                  ) : visual.buffers && visualDifference ? (
                    <>
                      <ComparisonFigure
                        caption={loaded.left.name}
                        height={visual.buffers.height}
                        label={t("comparison.figure.earlier")}
                        pixels={visual.buffers.left}
                        width={visual.buffers.width}
                      />
                      <ComparisonFigure
                        caption={loaded.right.name}
                        height={visual.buffers.height}
                        label={t("comparison.figure.later")}
                        pixels={visual.buffers.right}
                        width={visual.buffers.width}
                      />
                      <ComparisonFigure
                        caption={t("comparison.figure.changedPixels", {
                          percent: formatNumber(visualDifference.changedPercent)
                        })}
                        height={visual.buffers.height}
                        label={t("comparison.figure.differenceMap")}
                        pixels={visualDifference.pixels}
                        width={visual.buffers.width}
                      />
                    </>
                  ) : null}
                </div>

                {visualDifference ? (
                  <div className="comparison-legend">
                    <span>
                      <i className="is-added" /> {t("comparison.legend.added")}
                    </span>
                    <span>
                      <i className="is-removed" /> {t("comparison.legend.removed")}
                    </span>
                    <span>
                      <i className="is-other" /> {t("comparison.legend.other")}
                    </span>
                  </div>
                ) : null}

                <ComparisonTextDetail result={selectedResult} />
              </main>
            </div>
          </section>
        </div>
      ) : null}
    </>
  );
}

function SourcePicker({
  disabled,
  label,
  onChoose,
  onPasswordChange,
  onShowPasswordChange,
  password,
  path,
  showPassword
}: SourcePickerProps) {
  const { t } = useI18n();
  return (
    <div className="comparison-source-picker">
      <div className="comparison-source-heading">
        <span>{label}</span>
        <button disabled={disabled} onClick={onChoose} type="button">
          <FolderOpen size={16} aria-hidden="true" />
          {path ? t("comparison.source.change") : t("comparison.source.choose")}
        </button>
      </div>
      {path ? (
        <div className="comparison-source-file">
          <FileText size={17} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(path)}</strong>
            <small title={path}>{path}</small>
          </span>
        </div>
      ) : (
        <p>{t("comparison.source.none")}</p>
      )}
      {path ? (
        <label className="comparison-password">
          <span>
            {t("comparison.password.label")}{" "}
            <small>{t("comparison.password.optional")}</small>
          </span>
          <span className="comparison-password-field">
            <input
              autoComplete="off"
              disabled={disabled}
              onChange={(event) => onPasswordChange(event.target.value)}
              placeholder={t("comparison.password.placeholder")}
              type={showPassword ? "text" : "password"}
              value={password}
            />
            <button
              aria-label={
                showPassword
                  ? t("comparison.password.hide")
                  : t("comparison.password.show")
              }
              className="icon-button"
              disabled={disabled}
              onClick={onShowPasswordChange}
              title={
                showPassword
                  ? t("comparison.password.hide")
                  : t("comparison.password.show")
              }
              type="button"
            >
              {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
            </button>
          </span>
        </label>
      ) : null}
    </div>
  );
}

function SummaryMetric({
  label,
  tone,
  value
}: {
  label: string;
  tone?: "added" | "changed" | "removed" | "review";
  value: string;
}) {
  return (
    <span className={tone ? `is-${tone}` : undefined}>
      <strong>{value}</strong>
      <small>{label}</small>
    </span>
  );
}

function ComparisonFigure({
  caption,
  height,
  label,
  pixels,
  width
}: {
  caption: string;
  height: number;
  label: string;
  pixels: Uint8ClampedArray;
  width: number;
}) {
  return (
    <figure>
      <PixelCanvas height={height} label={label} pixels={pixels} width={width} />
      <figcaption>
        <strong>{label}</strong>
        <small title={caption}>{caption}</small>
      </figcaption>
    </figure>
  );
}

function PixelCanvas({
  height,
  label,
  pixels,
  width
}: {
  height: number;
  label: string;
  pixels: Uint8ClampedArray;
  width: number;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) {
      return;
    }
    canvas.width = width;
    canvas.height = height;
    const imageData = context.createImageData(width, height);
    imageData.data.set(pixels);
    context.putImageData(imageData, 0, 0);
  }, [height, pixels, width]);
  return <canvas aria-label={label} ref={canvasRef} />;
}

function ComparisonTextDetail({ result }: { result?: PageComparison }) {
  const { formatNumber, t } = useI18n();
  if (!result || result.status === "pending") {
    return (
      <section className="comparison-text-detail is-pending">
        <Loader2 className="spin" size={18} aria-hidden="true" />
        <span>{t("comparison.text.queued")}</span>
      </section>
    );
  }
  if (result.error) {
    return (
      <section className="comparison-text-detail is-error" role="alert">
        <AlertCircle size={18} aria-hidden="true" />
        <span>{result.error}</span>
      </section>
    );
  }

  return (
    <section className="comparison-text-detail">
      <div className="comparison-text-heading">
        <div>
          <span className={`comparison-status is-${result.status}`}>
            {statusLabel(result.status, t)}
          </span>
          <h3>{t("comparison.text.title")}</h3>
        </div>
        {result.text ? (
          <strong>
            {t("comparison.text.similarity", {
              percent: formatNumber(result.text.similarity)
            })}
          </strong>
        ) : null}
      </div>
      {result.text ? (
        <div className="comparison-text-metrics">
          <span>
            <strong>{formatNumber(result.text.leftWordCount)}</strong>{" "}
            {t("comparison.text.earlierWords")}
          </span>
          <span>
            <strong>+{formatNumber(result.text.addedWords)}</strong>{" "}
            {t("comparison.text.added")}
          </span>
          <span>
            <strong>-{formatNumber(result.text.removedWords)}</strong>{" "}
            {t("comparison.text.removed")}
          </span>
          <span>
            <strong>{formatNumber(result.text.rightWordCount)}</strong>{" "}
            {t("comparison.text.laterWords")}
          </span>
        </div>
      ) : null}
      {result.text?.truncated ? (
        <div className="comparison-inline-warning">
          <Info size={16} aria-hidden="true" />
          <span>{t("comparison.text.limit")}</span>
        </div>
      ) : null}
      {result.geometryChanged ? (
        <div className="comparison-inline-warning">
          <Info size={16} aria-hidden="true" />
          <span>
            {t("comparison.text.geometryChanged", {
              from: formatGeometry(result.leftGeometry, t, formatNumber),
              to: formatGeometry(result.rightGeometry, t, formatNumber)
            })}
          </span>
        </div>
      ) : null}
      {result.text?.exact && !result.geometryChanged ? (
        <p className="comparison-text-match">{t("comparison.text.match")}</p>
      ) : (
        <div className="comparison-snippets">
          <div>
            <strong>{t("comparison.figure.earlier")}</strong>
            <p>{result.text?.leftSnippet || t("comparison.text.none")}</p>
          </div>
          <div>
            <strong>{t("comparison.figure.later")}</strong>
            <p>{result.text?.rightSnippet || t("comparison.text.none")}</p>
          </div>
        </div>
      )}
    </section>
  );
}

async function openComparisonDocument(
  task: ReturnType<typeof createPdfLoadingTask>,
  label: "earlier" | "later",
  t: Translate
) {
  let passwordFailure = "";
  const side = t(
    label === "earlier"
      ? "comparison.side.earlier"
      : "comparison.side.later"
  );
  task.onPassword = (_updatePassword: (password: string) => void, reason: number) => {
    passwordFailure = isIncorrectPasswordReason(reason)
      ? t("comparison.error.passwordIncorrect", { side })
      : t("comparison.error.passwordRequired", { side });
    void task.destroy();
  };
  try {
    return await task.promise;
  } catch (reason) {
    if (passwordFailure) {
      throw new ComparisonUserError(passwordFailure);
    }
    const name = reason instanceof Error ? reason.name : "";
    if (name === "InvalidPDFException") {
      throw new ComparisonUserError(t("comparison.error.damaged", { side }));
    }
    if (name === "ResponseException" || name === "MissingPDFException") {
      throw new ComparisonUserError(t("comparison.error.read", { side }));
    }
    throw new ComparisonUserError(t("comparison.error.openSide", { side }));
  }
}

function createInitialResults(leftPages: number, rightPages: number): PageComparison[] {
  return Array.from({ length: Math.max(leftPages, rightPages) }, (_, index) => ({
    geometryChanged: false,
    pageNumber: index + 1,
    status: "pending"
  }));
}

async function analyseDocuments(
  loaded: LoadedComparison,
  initialResults: PageComparison[],
  callbacks: {
    isCurrent: () => boolean;
    onProgress: (results: PageComparison[], progress: number) => void;
    onSettled: () => void;
    t: Translate;
  }
) {
  const results = [...initialResults];
  try {
    for (let index = 0; index < results.length; index += 1) {
      if (!callbacks.isCurrent()) {
        return;
      }
      results[index] = await comparePage(loaded, index + 1, callbacks.t);
      if (!callbacks.isCurrent()) {
        return;
      }
      if ((index + 1) % 8 === 0 || index === results.length - 1) {
        callbacks.onProgress([...results], index + 1);
      }
    }
  } finally {
    callbacks.onSettled();
  }
}

async function comparePage(
  loaded: LoadedComparison,
  pageNumber: number,
  t: Translate
): Promise<PageComparison> {
  const hasLeft = pageNumber <= loaded.left.document.numPages;
  const hasRight = pageNumber <= loaded.right.document.numPages;
  let leftPage: PDFPageProxy | null = null;
  let rightPage: PDFPageProxy | null = null;
  try {
    [leftPage, rightPage] = await Promise.all([
      hasLeft ? loaded.left.document.getPage(pageNumber) : Promise.resolve(null),
      hasRight ? loaded.right.document.getPage(pageNumber) : Promise.resolve(null)
    ]);
    const leftGeometry = leftPage ? pageGeometry(leftPage) : undefined;
    const rightGeometry = rightPage ? pageGeometry(rightPage) : undefined;
    const [leftText, rightText] = await Promise.all([
      leftPage ? extractPageText(leftPage) : Promise.resolve({ text: "", truncated: false }),
      rightPage ? extractPageText(rightPage) : Promise.resolve({ text: "", truncated: false })
    ]);
    const text = comparePageText(
      leftText.text,
      rightText.text,
      leftText.truncated,
      rightText.truncated
    );
    const geometryChanged = Boolean(
      leftGeometry && rightGeometry && comparisonGeometryChanged(leftGeometry, rightGeometry)
    );
    let status: ComparisonStatus;
    if (!hasLeft) {
      status = "added";
    } else if (!hasRight) {
      status = "removed";
    } else if (text.truncated) {
      status = "limited";
    } else if (!text.exact || geometryChanged) {
      status = "changed";
    } else {
      status = "same";
    }
    return {
      geometryChanged,
      leftGeometry,
      pageNumber,
      rightGeometry,
      status,
      text
    };
  } catch {
    return {
      error: t("comparison.error.page"),
      geometryChanged: false,
      pageNumber,
      status: "error"
    };
  } finally {
    leftPage?.cleanup();
    rightPage?.cleanup();
  }
}

async function extractPageText(page: PDFPageProxy) {
  const reader = page.streamTextContent({ disableNormalization: false }).getReader();
  const parts: string[] = [];
  let length = 0;
  let itemCount = 0;
  let streamFinished = false;
  let truncated = false;

  try {
    reading: while (true) {
      const chunk = await reader.read();
      if (chunk.done) {
        streamFinished = true;
        break;
      }
      for (const item of chunk.value.items) {
        if (itemCount >= MAX_TEXT_ITEMS) {
          truncated = true;
          break reading;
        }
        itemCount += 1;
        if (!("str" in item) || !item.str) {
          continue;
        }
        const separatorLength = parts.length > 0 ? 1 : 0;
        const remaining = MAX_TEXT_CHARACTERS - length - separatorLength;
        if (remaining <= 0) {
          truncated = true;
          break reading;
        }
        if (item.str.length > remaining) {
          parts.push(item.str.slice(0, remaining));
          length += separatorLength + remaining;
          truncated = true;
          break reading;
        }
        parts.push(item.str);
        length += separatorLength + item.str.length;
        if (length >= MAX_TEXT_CHARACTERS) {
          truncated = true;
          break reading;
        }
      }
    }
  } finally {
    if (!streamFinished) {
      await reader.cancel().catch(() => undefined);
    }
  }

  return { text: parts.join(" "), truncated };
}

function pageGeometry(page: PDFPageProxy): ComparisonPageGeometry {
  const viewport = page.getViewport({ scale: 1 });
  return { height: viewport.height, rotation: page.rotate, width: viewport.width };
}

async function renderVisualBuffers(
  loaded: LoadedComparison,
  pageNumber: number,
  renderTasks: Array<{ cancel: () => void }>
): Promise<VisualBuffers> {
  const [leftPage, rightPage] = await Promise.all([
    pageNumber <= loaded.left.document.numPages
      ? loaded.left.document.getPage(pageNumber)
      : Promise.resolve(null),
    pageNumber <= loaded.right.document.numPages
      ? loaded.right.document.getPage(pageNumber)
      : Promise.resolve(null)
  ]);
  const viewports = [leftPage, rightPage]
    .filter((page): page is PDFPageProxy => page !== null)
    .map((page) => page.getViewport({ scale: 1 }));
  if (viewports.length === 0) {
    throw new Error("Neither document contains this page.");
  }
  const sourceWidth = Math.max(...viewports.map((viewport) => viewport.width));
  const sourceHeight = Math.max(...viewports.map((viewport) => viewport.height));
  const scale = Math.max(
    0.05,
    Math.min(
      1.5,
      MAX_VISUAL_WIDTH / sourceWidth,
      MAX_VISUAL_HEIGHT / sourceHeight,
      Math.sqrt(MAX_VISUAL_PIXELS / (sourceWidth * sourceHeight))
    )
  );
  const width = Math.max(1, Math.floor(sourceWidth * scale));
  const height = Math.max(1, Math.floor(sourceHeight * scale));
  const renderScale = Math.min(scale, width / sourceWidth, height / sourceHeight);
  const [left, right] = await Promise.all([
    renderPagePixels(leftPage, width, height, renderScale, renderTasks),
    renderPagePixels(rightPage, width, height, renderScale, renderTasks)
  ]);
  return { height, left, right, width };
}

async function renderPagePixels(
  page: PDFPageProxy | null,
  width: number,
  height: number,
  scale: number,
  renderTasks: Array<{ cancel: () => void }>
) {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    throw new Error("The visual comparison canvas is unavailable.");
  }
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, width, height);
  if (page) {
    const task = page.render({ canvas, viewport: page.getViewport({ scale }) });
    renderTasks.push(task);
    await task.promise;
  }
  return context.getImageData(0, 0, width, height).data;
}

function summariseResults(results: PageComparison[]) {
  return results.reduce(
    (summary, result) => {
      if (result.status !== "pending") {
        summary.checked += 1;
      }
      if (result.status === "changed") {
        summary.changed += 1;
      } else if (result.status === "added") {
        summary.added += 1;
      } else if (result.status === "removed") {
        summary.removed += 1;
      } else if (result.status === "limited" || result.status === "error") {
        summary.review += 1;
      }
      return summary;
    },
    { added: 0, changed: 0, checked: 0, removed: 0, review: 0 }
  );
}

function statusLabel(status: ComparisonStatus, t: Translate) {
  switch (status) {
    case "added":
      return t("comparison.status.added");
    case "changed":
      return t("comparison.status.changed");
    case "error":
      return t("comparison.status.error");
    case "limited":
      return t("comparison.status.limited");
    case "removed":
      return t("comparison.status.removed");
    case "same":
      return t("comparison.status.same");
    default:
      return t("comparison.status.pending");
  }
}

function formatGeometry(
  geometry: ComparisonPageGeometry | undefined,
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (!geometry) {
    return t("comparison.geometry.none");
  }
  return t("comparison.geometry.value", {
    height: formatNumber(geometry.height, { maximumFractionDigits: 1 }),
    rotation: formatNumber(geometry.rotation),
    width: formatNumber(geometry.width, { maximumFractionDigits: 1 })
  });
}

class ComparisonUserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ComparisonUserError";
  }
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/u).pop() || path;
}
