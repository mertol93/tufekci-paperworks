import { AlertCircle, AlertTriangle, Loader2, RotateCcw } from "lucide-react";
import { PdfJobProgress } from "./PdfJobProgress";
import { type PdfEditSafetyState } from "./usePdfEditSafety";
import { useI18n } from "./I18nProvider";

type PdfEditSafetyNoticeProps = {
  acknowledged: boolean;
  busy?: boolean;
  editSafety: PdfEditSafetyState;
  onAcknowledgedChange: (value: boolean) => void;
  rewriteDescription: string;
};

export function PdfEditSafetyNotice({
  acknowledged,
  busy = false,
  editSafety,
  onAcknowledgedChange,
  rewriteDescription
}: PdfEditSafetyNoticeProps) {
  const { t } = useI18n();
  if (editSafety.job && editSafety.job.status !== "succeeded") {
    return (
      <PdfJobProgress
        cancelling={editSafety.cancelling}
        connectionError={editSafety.connectionError}
        job={editSafety.job}
        onCancel={() => void editSafety.cancelJob()}
        onRetry={editSafety.retry}
        retryDisabled={busy}
      />
    );
  }

  if (editSafety.isChecking) {
    return (
      <div className="pdf-edit-safety is-checking" role="status">
        <Loader2 className="spin" size={17} aria-hidden="true" />
        <span>
          <strong>{t("safety.checking.title")}</strong>
          <small>{t("safety.checking.description")}</small>
        </span>
      </div>
    );
  }

  if (editSafety.errors.length > 0) {
    return (
      <div className="pdf-edit-safety is-error has-action" role="alert">
        <AlertCircle size={17} aria-hidden="true" />
        <span>
          <strong>{t("safety.failed.title")}</strong>
          <small>{t("safety.failed.description")}</small>
          {editSafety.errors.map((check) => (
            <small key={check.id} title={check.path}>
              {t("safety.failed.source", { source: check.label })}
            </small>
          ))}
        </span>
        <button disabled={busy} onClick={editSafety.retry} type="button">
          <RotateCcw size={15} aria-hidden="true" />
          {t("common.retry")}
        </button>
      </div>
    );
  }

  if (editSafety.signedSources.length === 0) {
    return null;
  }

  const sourceNames = editSafety.signedSources.map((source) => source.label).join(", ");
  return (
    <div className="pdf-edit-safety is-warning" role="alert">
      <AlertTriangle size={18} aria-hidden="true" />
      <span>
        <strong>{t("safety.signatureDetected", { sources: sourceNames })}</strong>
        <small>{rewriteDescription}</small>
        <label>
          <input
            checked={acknowledged}
            disabled={busy}
            onChange={(event) => onAcknowledgedChange(event.target.checked)}
            type="checkbox"
          />
          {t("safety.acknowledgement")}
        </label>
      </span>
    </div>
  );
}
