import { useEffect, useMemo, useRef, useState } from "react";
import { AlertCircle, CheckCircle2, FileSearch, Loader2, X } from "lucide-react";
import { useDialogFocus } from "./accessibility";
import { useI18n } from "./I18nProvider";
import {
  localiseOcrLanguage,
  localiseOcrReviewWarnings
} from "./ocrLocalisation";
import { PdfJobProgress } from "./PdfJobProgress";
import type { PdfJobConnectionErrorCode } from "./pdfJobs";
import { type PdfJobSnapshot } from "./pdfJobs";

export type OcrConfidenceWord = {
  confidence: number;
  height: number;
  left: number;
  text: string;
  top: number;
  width: number;
  wordNumber: number;
};

export type OcrConfidenceResult = {
  averageConfidence?: number | null;
  imageHeight: number;
  imageWidth: number;
  language: string;
  lowConfidenceCount: number;
  lowConfidenceThreshold: number;
  lowConfidenceWords: OcrConfidenceWord[];
  malformedRows: number;
  minimumConfidence?: number | null;
  warnings: string[];
  wordCount: number;
};

type OcrReviewDialogProps = {
  busy: boolean;
  cancelling: boolean;
  connectionError?: PdfJobConnectionErrorCode | null;
  error: string | null;
  existingHintCount: number;
  imageUrl: string | null;
  job: PdfJobSnapshot<OcrConfidenceResult> | null;
  onApplyHints: (words: string[]) => void;
  onCancel: () => void;
  onClose: () => void;
  onRetry: () => void;
  notice: string | null;
  pageName: string;
  result: OcrConfidenceResult | null;
  retryDisabled: boolean;
  visible: boolean;
};

export function OcrReviewDialog({
  busy,
  cancelling,
  connectionError,
  error,
  existingHintCount,
  imageUrl,
  job,
  onApplyHints,
  onCancel,
  onClose,
  onRetry,
  notice,
  pageName,
  result,
  retryDisabled,
  visible
}: OcrReviewDialogProps) {
  const { formatNumber, t } = useI18n();
  const [corrections, setCorrections] = useState<Record<number, string>>({});
  const [selectedWord, setSelectedWord] = useState<number | null>(null);
  const inputRefs = useRef(new Map<number, HTMLInputElement>());
  const dialogRef = useDialogFocus<HTMLElement>({
    active: visible,
    escapeDisabled: busy,
    onEscape: onClose
  });

  useEffect(() => {
    if (!result) {
      setCorrections({});
      setSelectedWord(null);
      return;
    }
    setCorrections(
      Object.fromEntries(result.lowConfidenceWords.map((word) => [word.wordNumber, word.text]))
    );
    setSelectedWord(result.lowConfidenceWords[0]?.wordNumber ?? null);
  }, [result]);

  const correctedWords = useMemo(() => {
    if (!result) {
      return [];
    }
    return Array.from(
      new Set(
        result.lowConfidenceWords.flatMap((word) => {
          const correction = corrections[word.wordNumber]?.trim() ?? "";
          return correction && correction !== word.text ? [correction] : [];
        })
      )
    );
  }, [corrections, result]);
  const reviewWarnings = useMemo(
    () => (result ? localiseOcrReviewWarnings(result, t) : []),
    [result, t]
  );

  if (!visible) {
    return null;
  }

  const selectWord = (wordNumber: number) => {
    setSelectedWord(wordNumber);
    inputRefs.current.get(wordNumber)?.focus();
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        aria-labelledby="ocr-review-title"
        aria-modal="true"
        className="ocr-review-dialog"
        data-dialog-root
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header>
          <div className="dialog-icon" aria-hidden="true">
            <FileSearch size={24} />
          </div>
          <div>
            <span className="eyebrow">{t("ocrReview.eyebrow")}</span>
            <h2 id="ocr-review-title">{t("ocrReview.title")}</h2>
            <p title={pageName}>{pageName}</p>
          </div>
          <button
            aria-label={t("ocrReview.closeAria")}
            className="icon-button"
            data-dialog-initial-focus
            disabled={busy}
            onClick={onClose}
            title={t("common.close")}
            type="button"
          >
            <X size={17} aria-hidden="true" />
          </button>
        </header>

        {busy ? (
          <div className="ocr-review-state" aria-live="polite">
            <Loader2 className="spin" size={28} aria-hidden="true" />
            <strong>{t("ocrReview.busy.title")}</strong>
            <span>{t("ocrReview.busy.detail")}</span>
          </div>
        ) : error ? (
          <div className="ocr-review-state is-error" role="alert">
            <AlertCircle size={28} aria-hidden="true" />
            <strong>{t("ocrReview.failed.title")}</strong>
            <span>{error}</span>
          </div>
        ) : notice ? (
          <div className="ocr-review-state" role="status">
            <AlertCircle size={28} aria-hidden="true" />
            <strong>{t("ocrReview.stopped.title")}</strong>
            <span>{notice}</span>
          </div>
        ) : result ? (
          <>
            <div className="ocr-review-summary" aria-label={t("ocrReview.summary.aria")}>
              <div>
                <span>{t("ocrReview.summary.average")}</span>
                <strong>
                  {result.averageConfidence == null
                    ? t("ocrReview.summary.noText")
                    : `${formatConfidence(result.averageConfidence, formatNumber)}%`}
                </strong>
              </div>
              <div>
                <span>{t("ocrReview.summary.words")}</span>
                <strong>{formatNumber(result.wordCount)}</strong>
              </div>
              <div>
                <span>{t("ocrReview.summary.needsReview")}</span>
                <strong>{formatNumber(result.lowConfidenceCount)}</strong>
              </div>
              <div>
                <span>{t("ocrReview.summary.language")}</span>
                <strong>{localiseOcrLanguage(result.language, result.language, t)}</strong>
              </div>
            </div>

            <div className="ocr-review-workspace">
              <figure>
                <div
                  className="ocr-review-image"
                  style={{ aspectRatio: `${result.imageWidth} / ${result.imageHeight}` }}
                >
                  {imageUrl ? (
                    <img alt={t("ocrReview.imageAlt", { page: pageName })} src={imageUrl} />
                  ) : null}
                  <div className="ocr-confidence-overlay" aria-label={t("ocrReview.overlayAria")}>
                    {result.lowConfidenceWords.map((word) => (
                      <button
                        aria-label={t("ocrReview.wordAria", {
                          confidence: formatConfidence(word.confidence, formatNumber),
                          word: word.text
                        })}
                        className={selectedWord === word.wordNumber ? "is-selected" : undefined}
                        key={word.wordNumber}
                        onClick={() => selectWord(word.wordNumber)}
                        style={{
                          height: `${(word.height / result.imageHeight) * 100}%`,
                          left: `${(word.left / result.imageWidth) * 100}%`,
                          top: `${(word.top / result.imageHeight) * 100}%`,
                          width: `${(word.width / result.imageWidth) * 100}%`
                        }}
                        title={t("ocrReview.wordTitle", {
                          confidence: formatConfidence(word.confidence, formatNumber),
                          word: word.text
                        })}
                        type="button"
                      />
                    ))}
                  </div>
                </div>
                <figcaption>
                  {t("ocrReview.thresholdHelp", {
                    threshold: formatNumber(Math.round(result.lowConfidenceThreshold))
                  })}
                </figcaption>
              </figure>

              <div className="ocr-correction-panel">
                <div className="ocr-correction-heading">
                  <div>
                    <strong>{t("ocrReview.questionable")}</strong>
                    <span>
                      {t(
                        result.lowConfidenceCount === 1
                          ? "ocrReview.detected.one"
                          : "ocrReview.detected.other",
                        { count: formatNumber(result.lowConfidenceCount) }
                      )}
                    </span>
                  </div>
                  {result.lowConfidenceCount === 0 ? (
                    <CheckCircle2 size={20} aria-label={t("ocrReview.noLowConfidence")} />
                  ) : null}
                </div>
                <div className="ocr-correction-list">
                  {result.lowConfidenceWords.length === 0 ? (
                    <div className="ocr-empty-review">
                      <CheckCircle2 size={24} aria-hidden="true" />
                      <strong>{t("ocrReview.empty")}</strong>
                    </div>
                  ) : (
                    result.lowConfidenceWords.map((word) => (
                      <label
                        className={selectedWord === word.wordNumber ? "is-selected" : undefined}
                        key={word.wordNumber}
                      >
                        <span>
                          <strong>{word.text}</strong>
                          <em>{formatConfidence(word.confidence, formatNumber)}%</em>
                        </span>
                        <input
                          aria-label={t("ocrReview.correctionAria", { word: word.text })}
                          maxLength={128}
                          onChange={(event) =>
                            setCorrections((current) => ({
                              ...current,
                              [word.wordNumber]: event.target.value
                            }))
                          }
                          onFocus={() => setSelectedWord(word.wordNumber)}
                          ref={(element) => {
                            if (element) {
                              inputRefs.current.set(word.wordNumber, element);
                            } else {
                              inputRefs.current.delete(word.wordNumber);
                            }
                          }}
                          spellCheck
                          value={corrections[word.wordNumber] ?? word.text}
                        />
                      </label>
                    ))
                  )}
                </div>
              </div>
            </div>

            {reviewWarnings.length > 0 ? (
              <div className="ocr-review-warnings" role="status">
                {reviewWarnings.map((warning) => (
                  <span key={warning}>{warning}</span>
                ))}
              </div>
            ) : null}

            <div className="ocr-review-note">
              {t("ocrReview.note")}
            </div>
          </>
        ) : null}

        {job ? (
          <PdfJobProgress
            cancelling={cancelling}
            connectionError={connectionError}
            job={job}
            onCancel={onCancel}
            onRetry={onRetry}
            retryDisabled={retryDisabled}
          />
        ) : null}

        <footer>
          <span>
            {existingHintCount > 0
              ? t(
                  existingHintCount === 1
                    ? "ocrReview.hints.queued.one"
                    : "ocrReview.hints.queued.other",
                  { count: formatNumber(existingHintCount) }
                )
              : t("ocrReview.hints.none")}
          </span>
          <div>
            <button disabled={busy} onClick={onClose} type="button">
              {t("common.close")}
            </button>
            <button
              className="primary"
              disabled={!result || correctedWords.length === 0}
              onClick={() => onApplyHints(correctedWords)}
              type="button"
            >
              {correctedWords.length > 0
                ? t(
                    correctedWords.length === 1
                      ? "ocrReview.action.one"
                      : "ocrReview.action.other",
                    { count: formatNumber(correctedWords.length) }
                  )
                : t("ocrReview.action.default")}
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
}

function formatConfidence(
  value: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
): string {
  return formatNumber(value, {
    maximumFractionDigits: 1,
    minimumFractionDigits: 1
  });
}
