import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { createPortal } from "react-dom";
import {
  AlertCircle,
  CheckCircle2,
  FileStack,
  Loader2,
  Printer,
  ShieldCheck,
  X
} from "lucide-react";
import { requestSystemPrint } from "paperworks-e2e-bridge";
import {
  resolvePrintPages,
  type PrintQuality,
  type PrintRangeErrorCode,
  type PrintRangeMode
} from "./print";
import {
  preparePrintDocument,
  PrintPreparationError,
  type PreparedPrintDocument,
  type PrintPreparationErrorCode,
  type PrintPreparationProgress,
  type PrintableWorkspacePage
} from "./printRenderer";
import type { TranslationKey } from "./i18n";
import { useI18n } from "./I18nProvider";
import type { VisualSignatureAsset, VisualSignaturePlacement } from "./visualSignatures";

type PrintStudioProps = {
  assets: readonly VisualSignatureAsset[];
  currentPage: number;
  documentName: string;
  pages: readonly PrintableWorkspacePage[];
  placements: readonly VisualSignaturePlacement[];
};

type PrintErrorCode = PrintPreparationErrorCode | PrintRangeErrorCode | "dialog-failed";

const rangeErrorKeys: Record<PrintRangeErrorCode, TranslationKey> = {
  empty: "print.error.rangeEmpty",
  invalid: "print.error.rangeInvalid",
  "outside-document": "print.error.rangeOutside",
  reversed: "print.error.rangeReversed",
  "too-long": "print.error.rangeTooLong",
  "too-many-pages": "print.error.tooManyPages"
};

const preparationErrorKeys: Record<PrintPreparationErrorCode, TranslationKey> = {
  "asset-unavailable": "print.error.assetUnavailable",
  cancelled: "print.error.cancelled",
  "job-too-large": "print.error.jobTooLarge",
  "page-too-large": "print.error.pageTooLarge",
  "render-failed": "print.error.renderFailed",
  "source-unavailable": "print.error.sourceUnavailable",
  "too-many-pages": "print.error.tooManyPages"
};

export function PrintStudio({
  assets,
  currentPage,
  documentName,
  pages,
  placements
}: PrintStudioProps) {
  const { formatNumber, t } = useI18n();
  const [rangeMode, setRangeMode] = useState<PrintRangeMode>("all");
  const [customRange, setCustomRange] = useState("");
  const [quality, setQuality] = useState<PrintQuality>("standard");
  const [includeVisualSignatures, setIncludeVisualSignatures] = useState(true);
  const [progress, setProgress] = useState<PrintPreparationProgress | null>(null);
  const [prepared, setPrepared] = useState<PreparedPrintDocument | null>(null);
  const [errorCode, setErrorCode] = useState<PrintErrorCode | null>(null);
  const [dialogOpened, setDialogOpened] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);
  const preparedRef = useRef<PreparedPrintDocument | null>(null);
  const busy = progress !== null;
  const pageSelection = useMemo(
    () => resolvePrintPages(rangeMode, pages.length, currentPage, customRange),
    [currentPage, customRange, pages.length, rangeMode]
  );
  const visibleRangeError = rangeMode === "custom" ? pageSelection.error : null;
  const progressPercent = progress
    ? progress.phase === "checking"
      ? Math.round((progress.completed / Math.max(1, progress.total)) * 15)
      : 15 + Math.round((progress.completed / Math.max(1, progress.total)) * 85)
    : 0;

  const clearPrepared = useCallback(() => {
    preparedRef.current?.dispose();
    preparedRef.current = null;
    setPrepared(null);
  }, []);

  useEffect(() => {
    clearPrepared();
    setErrorCode(null);
    setDialogOpened(false);
  }, [assets, clearPrepared, customRange, includeVisualSignatures, pages, placements, quality, rangeMode]);

  useEffect(
    () => () => {
      abortControllerRef.current?.abort();
      preparedRef.current?.dispose();
    },
    []
  );

  const openPrintDialogue = useCallback(async () => {
    if (!preparedRef.current) {
      return;
    }
    try {
      await waitForPrintLayout();
      requestSystemPrint();
      setDialogOpened(true);
      setErrorCode(null);
    } catch {
      setDialogOpened(false);
      setErrorCode("dialog-failed");
    }
  }, []);

  const prepareAndPrint = async () => {
    if (busy || pageSelection.error) {
      setErrorCode(pageSelection.error);
      return;
    }

    clearPrepared();
    setDialogOpened(false);
    setErrorCode(null);
    const controller = new AbortController();
    abortControllerRef.current = controller;
    setProgress({ completed: 0, phase: "checking", total: pageSelection.pages.length });
    try {
      const result = await preparePrintDocument({
        assets,
        includeVisualSignatures,
        onProgress: setProgress,
        pages,
        placements,
        quality,
        selectedPageNumbers: pageSelection.pages,
        signal: controller.signal
      });
      await preloadPreparedPages(result);
      if (controller.signal.aborted) {
        result.dispose();
        throw new PrintPreparationError("cancelled");
      }
      preparedRef.current = result;
      setPrepared(result);
      setProgress(null);
      await openPrintDialogueAfterCommit(setDialogOpened, setErrorCode);
    } catch (reason) {
      const code =
        reason instanceof PrintPreparationError ? reason.code : ("render-failed" as const);
      setErrorCode(code);
      setDialogOpened(false);
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
      }
      setProgress(null);
    }
  };

  const cancelPreparation = () => {
    abortControllerRef.current?.abort();
  };

  const displayedError = pageSelection.error ?? errorCode;
  const signatureCount = placements.length;

  return (
    <section className="print-studio">
      <div className="print-heading">
        <div className="print-heading-icon">
          <Printer size={20} aria-hidden="true" />
        </div>
        <div>
          <h3>{t("print.title")}</h3>
          <p>{t("print.description")}</p>
        </div>
      </div>

      <div className="print-document-summary">
        <FileStack size={18} aria-hidden="true" />
        <span>
          <strong title={documentName}>{documentName}</strong>
          <small>
            {pages.length === 1
              ? t("print.document.one")
              : t("print.document.other", { count: formatNumber(pages.length) })}
          </small>
        </span>
      </div>

      <fieldset className="print-options" disabled={busy}>
        <legend>{t("print.scope.legend")}</legend>
        <label className={rangeMode === "all" ? "is-selected" : undefined}>
          <input
            checked={rangeMode === "all"}
            name="print-range"
            onChange={() => setRangeMode("all")}
            type="radio"
          />
          <span>{t("print.scope.all")}</span>
        </label>
        <label className={rangeMode === "current" ? "is-selected" : undefined}>
          <input
            checked={rangeMode === "current"}
            name="print-range"
            onChange={() => setRangeMode("current")}
            type="radio"
          />
          <span>{t("print.scope.current", { page: formatNumber(currentPage) })}</span>
        </label>
        <label className={rangeMode === "custom" ? "is-selected" : undefined}>
          <input
            checked={rangeMode === "custom"}
            name="print-range"
            onChange={() => setRangeMode("custom")}
            type="radio"
          />
          <span>{t("print.scope.custom")}</span>
        </label>
      </fieldset>

      {rangeMode === "custom" ? (
        <label className="print-range-field">
          <span>{t("print.range.label")}</span>
          <input
            aria-describedby="print-range-hint"
            aria-invalid={Boolean(visibleRangeError)}
            disabled={busy}
            onChange={(event) => setCustomRange(event.target.value)}
            placeholder={t("print.range.placeholder")}
            spellCheck={false}
            type="text"
            value={customRange}
          />
          <small id="print-range-hint">{t("print.range.hint")}</small>
        </label>
      ) : null}

      <label className="print-quality-field">
        <span>{t("print.quality.label")}</span>
        <select
          disabled={busy}
          onChange={(event) => setQuality(event.target.value as PrintQuality)}
          value={quality}
        >
          <option value="standard">{t("print.quality.standard")}</option>
          <option value="high">{t("print.quality.high")}</option>
        </select>
        <small>
          {quality === "high"
            ? t("print.quality.highDescription")
            : t("print.quality.standardDescription")}
        </small>
      </label>

      {signatureCount > 0 ? (
        <label className="print-signature-option">
          <input
            checked={includeVisualSignatures}
            disabled={busy}
            onChange={(event) => setIncludeVisualSignatures(event.target.checked)}
            type="checkbox"
          />
          <span>
            <strong>{t("print.signatures.label")}</strong>
            <small>
              {signatureCount === 1
                ? t("print.signatures.one")
                : t("print.signatures.other", { count: formatNumber(signatureCount) })}
            </small>
          </span>
        </label>
      ) : null}

      <div className="print-system-note">
        <ShieldCheck size={18} aria-hidden="true" />
        <span>
          <strong>{t("print.system.title")}</strong>
          <small>{t("print.system.description")}</small>
        </span>
      </div>

      {busy && progress ? (
        <div className="print-progress" aria-live="polite">
          <Loader2 className="spin" size={18} aria-hidden="true" />
          <span>
            <strong>
              {progress.phase === "checking"
                ? t("print.progress.checking", {
                    current: formatNumber(progress.completed),
                    total: formatNumber(progress.total)
                  })
                : t("print.progress.rendering", {
                    current: formatNumber(progress.completed),
                    total: formatNumber(progress.total)
                  })}
            </strong>
            <progress aria-label={t("print.progress.aria")} max="100" value={progressPercent} />
          </span>
          <button onClick={cancelPreparation} type="button">
            <X size={15} aria-hidden="true" />
            {t("common.cancel")}
          </button>
        </div>
      ) : null}

      {displayedError ? (
        <div className="print-status is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{localisePrintError(displayedError, t)}</span>
        </div>
      ) : null}

      {dialogOpened ? (
        <div className="print-status is-success" role="status">
          <CheckCircle2 size={17} aria-hidden="true" />
          <span>{t("print.dialog.opened")}</span>
        </div>
      ) : null}

      {prepared ? (
        <section className="print-preview" aria-labelledby="print-preview-title">
          <div className="print-preview-heading">
            <div>
              <strong id="print-preview-title">{t("print.preview.title")}</strong>
              <small>
                {prepared.pages.length === 1
                  ? t("print.ready.one")
                  : t("print.ready.other", { count: formatNumber(prepared.pages.length) })}
              </small>
            </div>
            <button disabled={busy} onClick={() => void openPrintDialogue()} type="button">
              <Printer size={16} aria-hidden="true" />
              {t("print.openAgain")}
            </button>
          </div>
          <div className="print-preview-pages" aria-label={t("print.preview.aria")}>
            {prepared.pages.map((page) => (
              <figure key={`${page.pageNumber}:${page.url}`}>
                <img
                  alt={t("print.preview.pageAlt", { page: formatNumber(page.pageNumber) })}
                  src={page.url}
                />
                <figcaption>{formatNumber(page.pageNumber)}</figcaption>
              </figure>
            ))}
          </div>
        </section>
      ) : null}

      {!prepared && !busy ? (
        <button
          className="primary wide-button"
          disabled={Boolean(pageSelection.error) || pages.length === 0}
          onClick={() => void prepareAndPrint()}
          type="button"
        >
          <Printer size={17} aria-hidden="true" />
          {t("print.prepare")}
        </button>
      ) : null}

      <p className="print-privacy-note">{t("print.privacy")}</p>

      {prepared ? <PrintPortal documentName={documentName} prepared={prepared} /> : null}
    </section>
  );
}

function PrintPortal({
  documentName,
  prepared
}: {
  documentName: string;
  prepared: PreparedPrintDocument;
}) {
  const pageRules = prepared.pages
    .map(
      (page, index) =>
        `@page paperworks-${index + 1} { size: ${page.widthPt}pt ${page.heightPt}pt; margin: 0; }`
    )
    .join("\n");
  return createPortal(
    <div aria-hidden="true" className="paperworks-print-root" data-document-name={documentName}>
      <style>{pageRules}</style>
      {prepared.pages.map((page, index) => (
        <div
          className="paperworks-print-page"
          key={`${page.pageNumber}:${page.url}`}
          style={
            {
              "--paperworks-print-height": `${page.heightPt}pt`,
              "--paperworks-print-width": `${page.widthPt}pt`,
              page: `paperworks-${index + 1}`
            } as CSSProperties
          }
        >
          <img alt="" src={page.url} />
        </div>
      ))}
    </div>,
    document.body
  );
}

function localisePrintError(
  code: PrintErrorCode,
  t: (key: TranslationKey, values?: Readonly<Record<string, number | string>>) => string
) {
  if (code === "dialog-failed") {
    return t("print.error.dialogFailed");
  }
  if (code in rangeErrorKeys) {
    return t(rangeErrorKeys[code as PrintRangeErrorCode]);
  }
  return t(preparationErrorKeys[code as PrintPreparationErrorCode]);
}

async function preloadPreparedPages(prepared: PreparedPrintDocument) {
  try {
    await Promise.all(prepared.pages.map((page) => preloadImage(page.url)));
  } catch (reason) {
    prepared.dispose();
    throw reason;
  }
}

async function preloadImage(url: string) {
  const image = new Image();
  image.src = url;
  if (typeof image.decode === "function") {
    await image.decode();
    return;
  }
  await new Promise<void>((resolve, reject) => {
    image.onload = () => resolve();
    image.onerror = () => reject(new PrintPreparationError("render-failed"));
  });
}

async function openPrintDialogueAfterCommit(
  setDialogOpened: (opened: boolean) => void,
  setErrorCode: (code: PrintErrorCode | null) => void
) {
  await waitForPrintLayout();
  try {
    requestSystemPrint();
    setDialogOpened(true);
    setErrorCode(null);
  } catch {
    setDialogOpened(false);
    setErrorCode("dialog-failed");
  }
}

function waitForPrintLayout() {
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => resolve()));
  });
}
