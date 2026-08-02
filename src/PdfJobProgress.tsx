import { useEffect, useState } from "react";
import { AlertCircle, ClipboardCopy, Loader2, RotateCcw, X } from "lucide-react";
import {
  buildPdfJobDiagnostic,
  isInterruptedPdfJob,
  localisePdfJobConnectionError,
  localisePdfJobStage,
  type PdfJobConnectionErrorCode,
  type PdfJobSnapshot
} from "./pdfJobs";
import { useI18n } from "./I18nProvider";

type PdfJobProgressProps = {
  cancelling: boolean;
  connectionError?: PdfJobConnectionErrorCode | null;
  job: PdfJobSnapshot<unknown>;
  onCancel: () => void;
  onRetry?: () => void;
  retryDisabled?: boolean;
};

export function PdfJobProgress({
  cancelling,
  connectionError,
  job,
  onCancel,
  onRetry,
  retryDisabled = false
}: PdfJobProgressProps) {
  const { t } = useI18n();
  const [copyState, setCopyState] = useState<"copied" | "error" | "idle">("idle");
  const progress = Math.max(0, Math.min(100, job.progress));
  const diagnostic = buildPdfJobDiagnostic(job, connectionError);
  const active = job.status === "queued" || job.status === "running";
  const interrupted = isInterruptedPdfJob(job);

  useEffect(() => {
    setCopyState("idle");
  }, [diagnostic]);

  if (!active) {
    if (job.status === "succeeded") {
      return null;
    }
    return (
      <div className="pdf-job-panel is-terminal" aria-live="polite">
        <AlertCircle size={17} aria-hidden="true" />
        <span>
          <strong>
            {interrupted
              ? t("job.interrupted.title")
              : job.status === "failed"
                ? t("job.failed.title")
                : t("job.cancelled.title")}
          </strong>
          <small>
            {interrupted
              ? t("job.interrupted.detail")
              : job.status === "failed"
                ? t("job.failed.detail")
                : t("job.cancelled.detail")}
          </small>
          <details className="pdf-job-diagnostic">
            <summary>{t("job.diagnostic.title")}</summary>
            <textarea
              aria-label={t("job.diagnostic.aria")}
              onFocus={(event) => event.currentTarget.select()}
              readOnly
              rows={7}
              value={diagnostic}
            />
            <button
              onClick={() => void copyDiagnostic(diagnostic, setCopyState)}
              type="button"
            >
              <ClipboardCopy size={14} aria-hidden="true" />
              {copyState === "copied" ? t("job.copied") : t("job.copyDetails")}
            </button>
            {copyState === "error" ? (
              <small className="pdf-job-copy-error">
                {t("job.clipboard.error")}
              </small>
            ) : null}
          </details>
        </span>
        {onRetry ? (
          <button disabled={retryDisabled} onClick={onRetry} type="button">
            <RotateCcw size={15} aria-hidden="true" />
            {t("common.retry")}
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <div className="pdf-job-panel" aria-live="polite">
      <Loader2 className="spin" size={17} aria-hidden="true" />
      <span>
        <strong>
          {job.status === "queued" ? t("job.queued") : localisePdfJobStage(job, t)}
        </strong>
        <span className="pdf-job-progress">
          <progress aria-label={t("job.progress.aria")} max="100" value={progress} />
          <small>{progress}%</small>
        </span>
        {connectionError ? (
          <small className="pdf-job-connection-error">
            {localisePdfJobConnectionError(connectionError, t)}
          </small>
        ) : null}
      </span>
      <button disabled={cancelling} onClick={onCancel} type="button">
        <X size={15} aria-hidden="true" />
        {cancelling ? t("common.cancelling") : t("common.cancel")}
      </button>
    </div>
  );
}

async function copyDiagnostic(
  diagnostic: string,
  setCopyState: (state: "copied" | "error" | "idle") => void
) {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(diagnostic);
    } else {
      copyWithSelection(diagnostic);
    }
    setCopyState("copied");
  } catch {
    try {
      copyWithSelection(diagnostic);
      setCopyState("copied");
    } catch {
      setCopyState("error");
    }
  }
}

function copyWithSelection(value: string) {
  const field = document.createElement("textarea");
  field.value = value;
  field.setAttribute("readonly", "");
  field.style.position = "fixed";
  field.style.opacity = "0";
  document.body.appendChild(field);
  field.select();
  const copied = document.execCommand("copy");
  field.remove();
  if (!copied) {
    throw new Error("Clipboard access is unavailable.");
  }
}
