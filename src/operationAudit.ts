import type { Translate, TranslationKey } from "./i18n";

export type OperationAuditKind =
  | "annotations"
  | "annotation-inspection"
  | "archive"
  | "batch"
  | "batch-inspection"
  | "bookmark-inspection"
  | "bookmarks"
  | "certificate"
  | "certificate-validation"
  | "compression"
  | "compression-preview"
  | "content"
  | "content-inspection"
  | "edit-safety-inspection"
  | "finishing"
  | "finishing-inspection"
  | "form-inspection"
  | "forms"
  | "health"
  | "merge"
  | "ocr-review"
  | "searchable-ocr"
  | "organise"
  | "page-import-inspection"
  | "page-transfer"
  | "privacy"
  | "privacy-inspection"
  | "protection"
  | "redaction"
  | "redaction-inspection"
  | "scan"
  | "scan-preview"
  | "scanner-capture"
  | "split";

export type OperationAuditOutcome = "succeeded" | "failed" | "cancelled";

export type OperationAuditEntry = {
  id: string;
  operation: OperationAuditKind;
  outcome: OperationAuditOutcome;
  startedAtMs: number;
  completedAtMs: number;
  durationMs: number;
};

export type OperationAuditReport = {
  entries: OperationAuditEntry[];
  totalEntries: number;
  capacity: number;
  persistenceWarning?: string | null;
};

export type ExportOperationAuditResult = {
  entryCount: number;
  bytesWritten: number;
};

const operationLabelKeys: Record<OperationAuditKind, TranslationKey> = {
  annotations: "activity.operation.annotations",
  "annotation-inspection": "activity.operation.annotationInspection",
  archive: "activity.operation.archive",
  batch: "activity.operation.batch",
  "batch-inspection": "activity.operation.batchInspection",
  "bookmark-inspection": "activity.operation.bookmarkInspection",
  bookmarks: "activity.operation.bookmarks",
  certificate: "activity.operation.certificate",
  "certificate-validation": "activity.operation.certificateValidation",
  compression: "activity.operation.compression",
  "compression-preview": "activity.operation.compressionPreview",
  content: "activity.operation.content",
  "content-inspection": "activity.operation.contentInspection",
  "edit-safety-inspection": "activity.operation.editSafetyInspection",
  finishing: "activity.operation.finishing",
  "finishing-inspection": "activity.operation.finishingInspection",
  "form-inspection": "activity.operation.formInspection",
  forms: "activity.operation.forms",
  health: "activity.operation.health",
  merge: "activity.operation.merge",
  "ocr-review": "activity.operation.ocrReview",
  "searchable-ocr": "activity.operation.searchableOcr",
  organise: "activity.operation.organise",
  "page-import-inspection": "activity.operation.pageImportInspection",
  "page-transfer": "activity.operation.pageTransfer",
  privacy: "activity.operation.privacy",
  "privacy-inspection": "activity.operation.privacyInspection",
  protection: "activity.operation.protection",
  redaction: "activity.operation.redaction",
  "redaction-inspection": "activity.operation.redactionInspection",
  scan: "activity.operation.scan",
  "scan-preview": "activity.operation.scanPreview",
  "scanner-capture": "activity.operation.scannerCapture",
  split: "activity.operation.split"
};

const outcomeLabelKeys: Record<OperationAuditOutcome, TranslationKey> = {
  cancelled: "activity.outcome.cancelled",
  failed: "activity.outcome.failed",
  succeeded: "activity.outcome.succeeded"
};

export function operationAuditLabel(kind: OperationAuditKind, t: Translate): string {
  return t(operationLabelKeys[kind]);
}

export function operationAuditOutcomeLabel(
  outcome: OperationAuditOutcome,
  t: Translate
): string {
  return t(outcomeLabelKeys[outcome]);
}

export function formatOperationDuration(
  durationMs: number,
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
): string {
  const bounded = Math.max(0, Math.round(durationMs));
  if (bounded < 1_000) {
    return t("activity.duration.underSecond");
  }
  if (bounded < 60_000) {
    const seconds = Math.round(bounded / 1_000);
    return t(
      seconds === 1 ? "activity.duration.second.one" : "activity.duration.second.other",
      { count: formatNumber(seconds) }
    );
  }
  if (bounded < 3_600_000) {
    const minutes = Math.round(bounded / 60_000);
    return t(
      minutes === 1 ? "activity.duration.minute.one" : "activity.duration.minute.other",
      { count: formatNumber(minutes) }
    );
  }
  const hours = Math.round((bounded / 3_600_000) * 10) / 10;
  return t(hours === 1 ? "activity.duration.hour.one" : "activity.duration.hour.other", {
    count: formatNumber(hours, { maximumFractionDigits: 1 })
  });
}

export function filterOperationAudit(
  entries: OperationAuditEntry[],
  outcome: OperationAuditOutcome | "all"
): OperationAuditEntry[] {
  return outcome === "all"
    ? entries
    : entries.filter((entry) => entry.outcome === outcome);
}
