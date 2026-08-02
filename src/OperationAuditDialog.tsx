import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  Ban,
  CheckCircle2,
  Download,
  History,
  Loader2,
  RefreshCw,
  ShieldCheck,
  Trash2,
  X
} from "lucide-react";
import { useDialogFocus } from "./accessibility";
import { useI18n } from "./I18nProvider";
import { type Translate } from "./i18n";
import {
  filterOperationAudit,
  formatOperationDuration,
  operationAuditLabel,
  operationAuditOutcomeLabel,
  type ExportOperationAuditResult,
  type OperationAuditOutcome,
  type OperationAuditReport
} from "./operationAudit";

type OperationAuditDialogProps = {
  onClose: () => void;
  visible: boolean;
};

export function OperationAuditDialog({
  onClose,
  visible
}: OperationAuditDialogProps) {
  const { formatDate, formatNumber, t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [clearConfirmation, setClearConfirmation] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<OperationAuditOutcome | "all">("all");
  const [notice, setNotice] = useState<string | null>(null);
  const [report, setReport] = useState<OperationAuditReport | null>(null);
  const dialogRef = useDialogFocus<HTMLElement>({
    active: visible,
    escapeDisabled: busy,
    onEscape: onClose
  });

  const loadAudit = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const loaded = await invoke<OperationAuditReport>("list_operation_audit");
      setReport(loaded);
    } catch (reason) {
      setError(t("activity.error.load"));
    } finally {
      setBusy(false);
    }
  }, [t]);

  useEffect(() => {
    if (!visible) {
      return;
    }
    setClearConfirmation(false);
    setNotice(null);
    void loadAudit();
  }, [loadAudit, visible]);

  const visibleEntries = useMemo(
    () => filterOperationAudit(report?.entries ?? [], filter),
    [filter, report]
  );

  if (!visible) {
    return null;
  }

  const exportAudit = async () => {
    if (!report || report.totalEntries === 0 || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const outputPath = await save({
        defaultPath: `paperworks-activity-${new Date().toISOString().slice(0, 10)}.json`,
        filters: [{ extensions: ["json"], name: t("activity.export.filter") }],
        title: t("activity.export.dialogTitle")
      });
      if (!outputPath) {
        return;
      }
      const result = await invoke<ExportOperationAuditResult>(
        "export_operation_audit",
        {
          request: { outputPath }
        }
      );
      setNotice(
        t(
          result.entryCount === 1
            ? "activity.export.success.one"
            : "activity.export.success.other",
          {
            count: formatNumber(result.entryCount),
            size: formatBytes(result.bytesWritten, formatNumber)
          }
        )
      );
    } catch (reason) {
      setError(t("activity.error.export"));
    } finally {
      setBusy(false);
    }
  };

  const clearAudit = async () => {
    if (!report || report.totalEntries === 0 || busy) {
      return;
    }
    if (!clearConfirmation) {
      setClearConfirmation(true);
      setNotice(t("activity.clear.confirmHelp"));
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const removed = await invoke<number>("clear_operation_audit", {
        request: { confirmed: true }
      });
      setReport({
        capacity: report.capacity,
        entries: [],
        persistenceWarning: null,
        totalEntries: 0
      });
      setFilter("all");
      setClearConfirmation(false);
      setNotice(
        t(
          removed === 1 ? "activity.clear.success.one" : "activity.clear.success.other",
          { count: formatNumber(removed) }
        )
      );
    } catch (reason) {
      setError(t("activity.error.clear"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="dialog-backdrop audit-backdrop" role="presentation">
      <section
        aria-describedby="operation-audit-privacy"
        aria-labelledby="operation-audit-title"
        aria-modal="true"
        className="operation-audit-dialog"
        data-dialog-root
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header>
          <div className="dialog-icon" aria-hidden="true">
            <History size={24} />
          </div>
          <div>
            <span className="eyebrow">{t("activity.eyebrow")}</span>
            <h2 id="operation-audit-title">{t("activity.title")}</h2>
            <p>
              {report
                ? t("activity.summary", {
                    capacity: formatNumber(report.capacity),
                    count: formatNumber(report.totalEntries)
                  })
                : t("activity.loading.short")}
            </p>
          </div>
          <button
            aria-label={t("activity.close.aria")}
            className="icon-button"
            data-dialog-initial-focus
            disabled={busy}
            onClick={onClose}
            title={t("common.close")}
            type="button"
          >
            <X size={18} aria-hidden="true" />
          </button>
        </header>

        <div className="operation-audit-privacy" id="operation-audit-privacy">
          <ShieldCheck size={18} aria-hidden="true" />
          <span>
            {t("activity.privacy")}
          </span>
        </div>

        <div className="operation-audit-toolbar">
          <label>
            {t("activity.filter.label")}
            <select
              disabled={busy}
              onChange={(event) =>
                setFilter(event.target.value as OperationAuditOutcome | "all")
              }
              value={filter}
            >
              <option value="all">{t("activity.filter.all")}</option>
              <option value="succeeded">{t("activity.outcome.succeeded")}</option>
              <option value="failed">{t("activity.outcome.failed")}</option>
              <option value="cancelled">{t("activity.outcome.cancelled")}</option>
            </select>
          </label>
          <button
            className="ghost"
            disabled={busy}
            onClick={() => void loadAudit()}
            type="button"
          >
            {busy ? (
              <Loader2 className="spin" size={16} aria-hidden="true" />
            ) : (
              <RefreshCw size={16} aria-hidden="true" />
            )}
            {t("activity.refresh")}
          </button>
        </div>

        {report?.persistenceWarning ? (
          <div className="operation-audit-message is-warning" role="status">
            <AlertCircle size={17} aria-hidden="true" />
            <span>{localisePersistenceWarning(report.persistenceWarning, t)}</span>
          </div>
        ) : null}
        {error ? (
          <div className="operation-audit-message is-error" role="alert">
            <AlertCircle size={17} aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}
        {notice ? (
          <div className="operation-audit-message is-info" role="status">
            <CheckCircle2 size={17} aria-hidden="true" />
            <span>{notice}</span>
          </div>
        ) : null}

        <div className="operation-audit-content">
          {busy && !report ? (
            <div className="operation-audit-empty" aria-live="polite">
              <Loader2 className="spin" size={24} aria-hidden="true" />
              <strong>{t("activity.loading.title")}</strong>
            </div>
          ) : visibleEntries.length === 0 ? (
            <div className="operation-audit-empty">
              <History size={26} aria-hidden="true" />
              <strong>
                {report?.totalEntries
                  ? t("activity.empty.filtered")
                  : t("activity.empty.none")}
              </strong>
              <span>{t("activity.empty.description")}</span>
            </div>
          ) : (
            <ol className="operation-audit-list" aria-label={t("activity.list.aria")}>
              {visibleEntries.map((entry) => (
                <li className={`is-${entry.outcome}`} key={entry.id}>
                  <span className="operation-audit-outcome" aria-hidden="true">
                    {entry.outcome === "succeeded" ? (
                      <CheckCircle2 size={18} />
                    ) : entry.outcome === "cancelled" ? (
                      <Ban size={18} />
                    ) : (
                      <AlertCircle size={18} />
                    )}
                  </span>
                  <span className="operation-audit-entry-copy">
                    <strong>{operationAuditLabel(entry.operation, t)}</strong>
                    <small>
                      {operationAuditOutcomeLabel(entry.outcome, t)} |{" "}
                      {formatOperationDuration(entry.durationMs, t, formatNumber)}
                    </small>
                  </span>
                  <time dateTime={new Date(entry.completedAtMs).toISOString()}>
                    {formatDate(entry.completedAtMs, {
                      dateStyle: "medium",
                      timeStyle: "short"
                    })}
                  </time>
                </li>
              ))}
            </ol>
          )}
        </div>

        <footer>
          <button
            className={clearConfirmation ? "danger-button" : "ghost"}
            disabled={busy || !report?.totalEntries}
            onClick={() => void clearAudit()}
            type="button"
          >
            <Trash2 size={16} aria-hidden="true" />
            {clearConfirmation ? t("activity.clear.confirm") : t("activity.clear.action")}
          </button>
          <div>
            <button disabled={busy} onClick={onClose} type="button">
              {t("common.close")}
            </button>
            <button
              className="primary"
              disabled={busy || !report?.totalEntries}
              onClick={() => void exportAudit()}
              type="button"
            >
              {busy ? (
                <Loader2 className="spin" size={16} aria-hidden="true" />
              ) : (
                <Download size={16} aria-hidden="true" />
              )}
              {t("activity.export.action")}
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
}

function formatBytes(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
): string {
  if (bytes < 1_024) {
    return `${formatNumber(bytes)} B`;
  }
  return `${formatNumber(bytes / 1_024, { maximumFractionDigits: 1 })} KB`;
}

function localisePersistenceWarning(warning: string, t: Translate): string {
  switch (warning) {
    case "New activity records may not survive an application restart.":
      return t("activity.warning.persistence");
    case "An incomplete activity snapshot was skipped and an earlier valid snapshot was restored.":
      return t("activity.warning.recovered");
    case "Stored activity history could not be read safely and was not loaded.":
      return t("activity.warning.unreadable");
    default:
      return t("activity.warning.generic");
  }
}
