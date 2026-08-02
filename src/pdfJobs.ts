import type { Translate, TranslationKey } from "./i18n";

export type PdfJobKind =
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

export type PdfJobProtectionPolicy =
  | "inspection-only"
  | "media-input-only"
  | "password-aware-plain-output"
  | "optional-aes-256-output"
  | "preserve-source-encryption"
  | "manage-aes-256";

export const pdfJobProtectionPolicies = {
  annotations: "optional-aes-256-output",
  "annotation-inspection": "inspection-only",
  archive: "password-aware-plain-output",
  batch: "optional-aes-256-output",
  "batch-inspection": "inspection-only",
  "bookmark-inspection": "inspection-only",
  bookmarks: "optional-aes-256-output",
  certificate: "preserve-source-encryption",
  "certificate-validation": "inspection-only",
  compression: "optional-aes-256-output",
  "compression-preview": "inspection-only",
  content: "optional-aes-256-output",
  "content-inspection": "inspection-only",
  "edit-safety-inspection": "inspection-only",
  finishing: "optional-aes-256-output",
  "finishing-inspection": "inspection-only",
  "form-inspection": "inspection-only",
  forms: "optional-aes-256-output",
  health: "inspection-only",
  merge: "optional-aes-256-output",
  "ocr-review": "media-input-only",
  "searchable-ocr": "optional-aes-256-output",
  organise: "optional-aes-256-output",
  "page-import-inspection": "inspection-only",
  "page-transfer": "optional-aes-256-output",
  privacy: "optional-aes-256-output",
  "privacy-inspection": "inspection-only",
  protection: "manage-aes-256",
  redaction: "optional-aes-256-output",
  "redaction-inspection": "inspection-only",
  scan: "optional-aes-256-output",
  "scan-preview": "media-input-only",
  "scanner-capture": "media-input-only",
  split: "optional-aes-256-output"
} as const satisfies Record<PdfJobKind, PdfJobProtectionPolicy>;

const pdfJobProtectionLabels = {
  "inspection-only": "Read-only PDF inspection",
  "media-input-only": "Image or scanner media intake",
  "password-aware-plain-output": "Password-aware PDF/A export with unencrypted output",
  "optional-aes-256-output": "Optional AES-256 protected output",
  "preserve-source-encryption": "Preserves the source PDF encryption state",
  "manage-aes-256": "Adds or removes AES-256 protection"
} as const satisfies Record<PdfJobProtectionPolicy, string>;

export function pdfJobProtectionSummary(kind: PdfJobKind) {
  return pdfJobProtectionLabels[pdfJobProtectionPolicies[kind]];
}

export type PdfJobStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export type PdfJobStageCode =
  | "waiting"
  | "starting"
  | "cancelling"
  | "completed"
  | "cancelled"
  | "failed"
  | "interrupted"
  | "annotations-checking"
  | "annotations-opening"
  | "annotations-preparing"
  | "annotations-writing"
  | "annotations-verifying"
  | "annotations-protecting"
  | "annotations-publishing"
  | "annotation-inspection-checking"
  | "annotation-inspection-opening"
  | "annotation-inspection-inspecting"
  | "annotation-inspection-verifying"
  | "archive-checking"
  | "archive-preparing"
  | "archive-converting"
  | "archive-preflighting"
  | "archive-validating"
  | "archive-verifying"
  | "archive-publishing"
  | "batch-checking"
  | "batch-preparing"
  | "batch-recognising"
  | "batch-cleaning"
  | "batch-compressing"
  | "batch-archiving"
  | "batch-protecting"
  | "batch-verifying"
  | "batch-publishing"
  | "batch-inspection-checking"
  | "batch-inspection-inspecting"
  | "batch-inspection-verifying"
  | "bookmarks-checking"
  | "bookmarks-opening"
  | "bookmarks-preparing-contents"
  | "bookmarks-building"
  | "bookmarks-writing"
  | "bookmarks-protecting"
  | "bookmarks-verifying"
  | "bookmarks-publishing"
  | "bookmark-inspection-checking"
  | "bookmark-inspection-opening"
  | "bookmark-inspection-inspecting"
  | "bookmark-inspection-verifying"
  | "certificate-checking"
  | "certificate-engine"
  | "certificate-opening"
  | "certificate-preparing"
  | "certificate-signing"
  | "certificate-reopening"
  | "certificate-validating"
  | "certificate-rechecking"
  | "certificate-publishing"
  | "certificate-validation-checking"
  | "certificate-validation-opening"
  | "certificate-validation-inspecting"
  | "certificate-validation-engine"
  | "certificate-validation-validating"
  | "certificate-validation-reviewing"
  | "certificate-validation-rechecking"
  | "merge-checking"
  | "merge-preparing"
  | "merge-protecting"
  | "merge-verifying"
  | "merge-publishing"
  | "organise-checking"
  | "organise-opening"
  | "organise-arranging"
  | "organise-flattening"
  | "organise-writing"
  | "organise-verifying"
  | "organise-protecting"
  | "organise-publishing"
  | "compression-checking"
  | "compression-analysing"
  | "compression-writing"
  | "compression-verifying"
  | "compression-protecting"
  | "compression-publishing"
  | "compression-preview-checking"
  | "compression-preview-analysing"
  | "compression-preview-encoding"
  | "compression-preview-verifying"
  | "content-checking"
  | "content-opening"
  | "content-preparing"
  | "content-writing"
  | "content-verifying"
  | "content-protecting"
  | "content-publishing"
  | "content-inspection-checking"
  | "content-inspection-opening"
  | "content-inspection-inspecting"
  | "content-inspection-verifying"
  | "health-checking"
  | "health-opening"
  | "health-inspecting"
  | "health-verifying"
  | "finishing-checking"
  | "finishing-opening"
  | "finishing-preparing"
  | "finishing-applying"
  | "finishing-writing"
  | "finishing-verifying"
  | "finishing-protecting"
  | "finishing-publishing"
  | "finishing-inspection-checking"
  | "finishing-inspection-opening"
  | "finishing-inspection-inspecting"
  | "finishing-inspection-verifying"
  | "forms-checking"
  | "forms-opening"
  | "forms-applying"
  | "forms-writing"
  | "forms-verifying"
  | "forms-protecting"
  | "forms-publishing"
  | "form-inspection-checking"
  | "form-inspection-opening"
  | "form-inspection-inspecting"
  | "form-inspection-verifying"
  | "privacy-checking"
  | "privacy-opening"
  | "privacy-cleaning"
  | "privacy-writing"
  | "privacy-verifying"
  | "privacy-protecting"
  | "privacy-publishing"
  | "privacy-inspection-checking"
  | "privacy-inspection-opening"
  | "privacy-inspection-inspecting"
  | "privacy-inspection-reporting"
  | "privacy-inspection-verifying"
  | "redaction-checking"
  | "redaction-opening"
  | "redaction-applying"
  | "redaction-cleaning"
  | "redaction-writing"
  | "redaction-verifying"
  | "redaction-protecting"
  | "redaction-publishing"
  | "redaction-inspection-checking"
  | "redaction-inspection-opening"
  | "redaction-inspection-inspecting"
  | "redaction-inspection-verifying"
  | "protection-checking"
  | "protection-preparing"
  | "protection-applying"
  | "protection-verifying"
  | "protection-publishing"
  | "split-checking"
  | "split-preparing"
  | "split-protecting"
  | "split-verifying"
  | "split-publishing"
  | "ocr-review-checking"
  | "ocr-review-preparing"
  | "ocr-review-recognising"
  | "ocr-review-verifying"
  | "searchable-ocr-checking"
  | "searchable-ocr-preparing"
  | "searchable-ocr-recognising"
  | "searchable-ocr-verifying"
  | "searchable-ocr-publishing"
  | "scan-checking"
  | "scan-preparing"
  | "scan-writing"
  | "scan-recognising"
  | "scan-protecting"
  | "scan-publishing"
  | "scan-preview-checking"
  | "scan-preview-preparing"
  | "scan-preview-encoding"
  | "scan-preview-verifying"
  | "scanner-capture-checking"
  | "scanner-capture-connecting"
  | "scanner-capture-capturing"
  | "scanner-capture-verifying"
  | "scanner-capture-finalising";

export type PdfJobErrorCode =
  | "annotations-failed"
  | "annotation-inspection-failed"
  | "archive-failed"
  | "batch-failed"
  | "batch-inspection-failed"
  | "bookmarks-failed"
  | "bookmark-inspection-failed"
  | "certificate-failed"
  | "certificate-validation-failed"
  | "certificate-acknowledgement-required"
  | "content-failed"
  | "content-inspection-failed"
  | "finishing-failed"
  | "finishing-inspection-failed"
  | "forms-failed"
  | "form-inspection-failed"
  | "health-failed"
  | "interrupted"
  | "job-failed"
  | "merge-failed"
  | "ocr-engine-unavailable"
  | "ocr-review-failed"
  | "password-rejected"
  | "privacy-failed"
  | "privacy-inspection-failed"
  | "protection-unavailable"
  | "redaction-failed"
  | "redaction-inspection-failed"
  | "safety-limit"
  | "scan-failed"
  | "scanner-capture-failed"
  | "scan-preview-failed"
  | "searchable-ocr-failed"
  | "source-changed";

export type PdfJobConnectionErrorCode =
  | "history-unavailable"
  | "status-unavailable";

export type PdfJobSnapshot<TResult> = {
  createdAtMs: number;
  error?: string | null;
  errorCode?: PdfJobErrorCode | null;
  jobId: string;
  kind: PdfJobKind;
  progress: number;
  result?: TResult | null;
  stage: string;
  stageCode?: PdfJobStageCode | null;
  status: PdfJobStatus;
  updatedAtMs: number;
};

const stageTranslationKeys = {
  waiting: "job.stage.waiting",
  starting: "job.stage.starting",
  cancelling: "job.stage.cancelling",
  completed: "job.stage.completed",
  cancelled: "job.stage.cancelled",
  failed: "job.stage.failed",
  interrupted: "job.stage.interrupted",
  "annotations-checking": "job.stage.annotationsChecking",
  "annotations-opening": "job.stage.annotationsOpening",
  "annotations-preparing": "job.stage.annotationsPreparing",
  "annotations-writing": "job.stage.annotationsWriting",
  "annotations-verifying": "job.stage.annotationsVerifying",
  "annotations-protecting": "job.stage.annotationsProtecting",
  "annotations-publishing": "job.stage.annotationsPublishing",
  "annotation-inspection-checking": "job.stage.annotationInspectionChecking",
  "annotation-inspection-opening": "job.stage.annotationInspectionOpening",
  "annotation-inspection-inspecting": "job.stage.annotationInspectionInspecting",
  "annotation-inspection-verifying": "job.stage.annotationInspectionVerifying",
  "archive-checking": "job.stage.archiveChecking",
  "archive-preparing": "job.stage.archivePreparing",
  "archive-converting": "job.stage.archiveConverting",
  "archive-preflighting": "job.stage.archivePreflighting",
  "archive-validating": "job.stage.archiveValidating",
  "archive-verifying": "job.stage.archiveVerifying",
  "archive-publishing": "job.stage.archivePublishing",
  "batch-checking": "job.stage.batchChecking",
  "batch-preparing": "job.stage.batchPreparing",
  "batch-recognising": "job.stage.batchRecognising",
  "batch-cleaning": "job.stage.batchCleaning",
  "batch-compressing": "job.stage.batchCompressing",
  "batch-archiving": "job.stage.batchArchiving",
  "batch-protecting": "job.stage.batchProtecting",
  "batch-verifying": "job.stage.batchVerifying",
  "batch-publishing": "job.stage.batchPublishing",
  "batch-inspection-checking": "job.stage.batchInspectionChecking",
  "batch-inspection-inspecting": "job.stage.batchInspectionInspecting",
  "batch-inspection-verifying": "job.stage.batchInspectionVerifying",
  "bookmarks-checking": "job.stage.bookmarksChecking",
  "bookmarks-opening": "job.stage.bookmarksOpening",
  "bookmarks-preparing-contents": "job.stage.bookmarksPreparingContents",
  "bookmarks-building": "job.stage.bookmarksBuilding",
  "bookmarks-writing": "job.stage.bookmarksWriting",
  "bookmarks-protecting": "job.stage.bookmarksProtecting",
  "bookmarks-verifying": "job.stage.bookmarksVerifying",
  "bookmarks-publishing": "job.stage.bookmarksPublishing",
  "bookmark-inspection-checking": "job.stage.bookmarkInspectionChecking",
  "bookmark-inspection-opening": "job.stage.bookmarkInspectionOpening",
  "bookmark-inspection-inspecting": "job.stage.bookmarkInspectionInspecting",
  "bookmark-inspection-verifying": "job.stage.bookmarkInspectionVerifying",
  "certificate-checking": "job.stage.certificateChecking",
  "certificate-engine": "job.stage.certificateEngine",
  "certificate-opening": "job.stage.certificateOpening",
  "certificate-preparing": "job.stage.certificatePreparing",
  "certificate-signing": "job.stage.certificateSigning",
  "certificate-reopening": "job.stage.certificateReopening",
  "certificate-validating": "job.stage.certificateValidating",
  "certificate-rechecking": "job.stage.certificateRechecking",
  "certificate-publishing": "job.stage.certificatePublishing",
  "certificate-validation-checking": "job.stage.certificateValidationChecking",
  "certificate-validation-opening": "job.stage.certificateValidationOpening",
  "certificate-validation-inspecting": "job.stage.certificateValidationInspecting",
  "certificate-validation-engine": "job.stage.certificateValidationEngine",
  "certificate-validation-validating": "job.stage.certificateValidationValidating",
  "certificate-validation-reviewing": "job.stage.certificateValidationReviewing",
  "certificate-validation-rechecking": "job.stage.certificateValidationRechecking",
  "merge-checking": "job.stage.mergeChecking",
  "merge-preparing": "job.stage.mergePreparing",
  "merge-protecting": "job.stage.mergeProtecting",
  "merge-verifying": "job.stage.mergeVerifying",
  "merge-publishing": "job.stage.mergePublishing",
  "organise-checking": "job.stage.organiseChecking",
  "organise-opening": "job.stage.organiseOpening",
  "organise-arranging": "job.stage.organiseArranging",
  "organise-flattening": "job.stage.organiseFlattening",
  "organise-writing": "job.stage.organiseWriting",
  "organise-verifying": "job.stage.organiseVerifying",
  "organise-protecting": "job.stage.organiseProtecting",
  "organise-publishing": "job.stage.organisePublishing",
  "compression-checking": "job.stage.compressionChecking",
  "compression-analysing": "job.stage.compressionAnalysing",
  "compression-writing": "job.stage.compressionWriting",
  "compression-verifying": "job.stage.compressionVerifying",
  "compression-protecting": "job.stage.compressionProtecting",
  "compression-publishing": "job.stage.compressionPublishing",
  "compression-preview-checking": "job.stage.compressionPreviewChecking",
  "compression-preview-analysing": "job.stage.compressionPreviewAnalysing",
  "compression-preview-encoding": "job.stage.compressionPreviewEncoding",
  "compression-preview-verifying": "job.stage.compressionPreviewVerifying",
  "content-checking": "job.stage.contentChecking",
  "content-opening": "job.stage.contentOpening",
  "content-preparing": "job.stage.contentPreparing",
  "content-writing": "job.stage.contentWriting",
  "content-verifying": "job.stage.contentVerifying",
  "content-protecting": "job.stage.contentProtecting",
  "content-publishing": "job.stage.contentPublishing",
  "content-inspection-checking": "job.stage.contentInspectionChecking",
  "content-inspection-opening": "job.stage.contentInspectionOpening",
  "content-inspection-inspecting": "job.stage.contentInspectionInspecting",
  "content-inspection-verifying": "job.stage.contentInspectionVerifying",
  "health-checking": "job.stage.healthChecking",
  "health-opening": "job.stage.healthOpening",
  "health-inspecting": "job.stage.healthInspecting",
  "health-verifying": "job.stage.healthVerifying",
  "finishing-checking": "job.stage.finishingChecking",
  "finishing-opening": "job.stage.finishingOpening",
  "finishing-preparing": "job.stage.finishingPreparing",
  "finishing-applying": "job.stage.finishingApplying",
  "finishing-writing": "job.stage.finishingWriting",
  "finishing-verifying": "job.stage.finishingVerifying",
  "finishing-protecting": "job.stage.finishingProtecting",
  "finishing-publishing": "job.stage.finishingPublishing",
  "finishing-inspection-checking": "job.stage.finishingInspectionChecking",
  "finishing-inspection-opening": "job.stage.finishingInspectionOpening",
  "finishing-inspection-inspecting": "job.stage.finishingInspectionInspecting",
  "finishing-inspection-verifying": "job.stage.finishingInspectionVerifying",
  "forms-checking": "job.stage.formsChecking",
  "forms-opening": "job.stage.formsOpening",
  "forms-applying": "job.stage.formsApplying",
  "forms-writing": "job.stage.formsWriting",
  "forms-verifying": "job.stage.formsVerifying",
  "forms-protecting": "job.stage.formsProtecting",
  "forms-publishing": "job.stage.formsPublishing",
  "form-inspection-checking": "job.stage.formInspectionChecking",
  "form-inspection-opening": "job.stage.formInspectionOpening",
  "form-inspection-inspecting": "job.stage.formInspectionInspecting",
  "form-inspection-verifying": "job.stage.formInspectionVerifying",
  "privacy-checking": "job.stage.privacyChecking",
  "privacy-opening": "job.stage.privacyOpening",
  "privacy-cleaning": "job.stage.privacyCleaning",
  "privacy-writing": "job.stage.privacyWriting",
  "privacy-verifying": "job.stage.privacyVerifying",
  "privacy-protecting": "job.stage.privacyProtecting",
  "privacy-publishing": "job.stage.privacyPublishing",
  "privacy-inspection-checking": "job.stage.privacyInspectionChecking",
  "privacy-inspection-opening": "job.stage.privacyInspectionOpening",
  "privacy-inspection-inspecting": "job.stage.privacyInspectionInspecting",
  "privacy-inspection-reporting": "job.stage.privacyInspectionReporting",
  "privacy-inspection-verifying": "job.stage.privacyInspectionVerifying",
  "redaction-checking": "job.stage.redactionChecking",
  "redaction-opening": "job.stage.redactionOpening",
  "redaction-applying": "job.stage.redactionApplying",
  "redaction-cleaning": "job.stage.redactionCleaning",
  "redaction-writing": "job.stage.redactionWriting",
  "redaction-verifying": "job.stage.redactionVerifying",
  "redaction-protecting": "job.stage.redactionProtecting",
  "redaction-publishing": "job.stage.redactionPublishing",
  "redaction-inspection-checking": "job.stage.redactionInspectionChecking",
  "redaction-inspection-opening": "job.stage.redactionInspectionOpening",
  "redaction-inspection-inspecting": "job.stage.redactionInspectionInspecting",
  "redaction-inspection-verifying": "job.stage.redactionInspectionVerifying",
  "protection-checking": "job.stage.protectionChecking",
  "protection-preparing": "job.stage.protectionPreparing",
  "protection-applying": "job.stage.protectionApplying",
  "protection-verifying": "job.stage.protectionVerifying",
  "protection-publishing": "job.stage.protectionPublishing",
  "split-checking": "job.stage.splitChecking",
  "split-preparing": "job.stage.splitPreparing",
  "split-protecting": "job.stage.splitProtecting",
  "split-verifying": "job.stage.splitVerifying",
  "split-publishing": "job.stage.splitPublishing",
  "ocr-review-checking": "job.stage.ocrReviewChecking",
  "ocr-review-preparing": "job.stage.ocrReviewPreparing",
  "ocr-review-recognising": "job.stage.ocrReviewRecognising",
  "ocr-review-verifying": "job.stage.ocrReviewVerifying",
  "searchable-ocr-checking": "job.stage.searchableOcrChecking",
  "searchable-ocr-preparing": "job.stage.searchableOcrPreparing",
  "searchable-ocr-recognising": "job.stage.searchableOcrRecognising",
  "searchable-ocr-verifying": "job.stage.searchableOcrVerifying",
  "searchable-ocr-publishing": "job.stage.searchableOcrPublishing",
  "scan-checking": "job.stage.scanChecking",
  "scan-preparing": "job.stage.scanPreparing",
  "scan-writing": "job.stage.scanWriting",
  "scan-recognising": "job.stage.scanRecognising",
  "scan-protecting": "job.stage.scanProtecting",
  "scan-publishing": "job.stage.scanPublishing",
  "scan-preview-checking": "job.stage.scanPreviewChecking",
  "scan-preview-preparing": "job.stage.scanPreviewPreparing",
  "scan-preview-encoding": "job.stage.scanPreviewEncoding",
  "scan-preview-verifying": "job.stage.scanPreviewVerifying",
  "scanner-capture-checking": "job.stage.scannerCaptureChecking",
  "scanner-capture-connecting": "job.stage.scannerCaptureConnecting",
  "scanner-capture-capturing": "job.stage.scannerCaptureCapturing",
  "scanner-capture-verifying": "job.stage.scannerCaptureVerifying",
  "scanner-capture-finalising": "job.stage.scannerCaptureFinalising"
} as const satisfies Record<PdfJobStageCode, TranslationKey>;

const errorTranslationKeys = {
  "annotations-failed": "job.error.annotationsFailed",
  "annotation-inspection-failed": "job.error.annotationInspectionFailed",
  "archive-failed": "job.error.archiveFailed",
  "batch-failed": "job.error.batchFailed",
  "batch-inspection-failed": "job.error.batchInspectionFailed",
  "bookmarks-failed": "job.error.bookmarksFailed",
  "bookmark-inspection-failed": "job.error.bookmarkInspectionFailed",
  "certificate-failed": "job.error.certificateFailed",
  "certificate-validation-failed": "job.error.certificateValidationFailed",
  "certificate-acknowledgement-required": "job.error.certificateAcknowledgement",
  "content-failed": "job.error.contentFailed",
  "content-inspection-failed": "job.error.contentInspectionFailed",
  "finishing-failed": "job.error.finishingFailed",
  "finishing-inspection-failed": "job.error.finishingInspectionFailed",
  "forms-failed": "job.error.formsFailed",
  "form-inspection-failed": "job.error.formInspectionFailed",
  "health-failed": "job.error.healthFailed",
  interrupted: "job.error.interrupted",
  "job-failed": "job.error.failed",
  "merge-failed": "merge.failed",
  "ocr-engine-unavailable": "job.error.ocrEngineUnavailable",
  "ocr-review-failed": "job.error.ocrReviewFailed",
  "password-rejected": "job.error.passwordRejected",
  "privacy-failed": "job.error.privacyFailed",
  "privacy-inspection-failed": "job.error.privacyInspectionFailed",
  "protection-unavailable": "job.error.protectionUnavailable",
  "redaction-failed": "job.error.redactionFailed",
  "redaction-inspection-failed": "job.error.redactionInspectionFailed",
  "safety-limit": "job.error.safetyLimit",
  "scan-failed": "job.error.scanFailed",
  "scanner-capture-failed": "job.error.scannerCaptureFailed",
  "scan-preview-failed": "job.error.scanPreviewFailed",
  "searchable-ocr-failed": "job.error.searchableOcrFailed",
  "source-changed": "job.error.sourceChanged"
} as const satisfies Record<PdfJobErrorCode, TranslationKey>;

const migratedFailureKeys: Partial<Record<PdfJobKind, TranslationKey>> = {
  annotations: "job.error.annotationsFailed",
  "annotation-inspection": "job.error.annotationInspectionFailed",
  archive: "job.error.archiveFailed",
  batch: "job.error.batchFailed",
  "batch-inspection": "job.error.batchInspectionFailed",
  bookmarks: "job.error.bookmarksFailed",
  "bookmark-inspection": "job.error.bookmarkInspectionFailed",
  certificate: "job.error.certificateFailed",
  "certificate-validation": "job.error.certificateValidationFailed",
  compression: "compression.error.exportFailed",
  "compression-preview": "compression.error.previewFailed",
  content: "job.error.contentFailed",
  "content-inspection": "job.error.contentInspectionFailed",
  finishing: "job.error.finishingFailed",
  "finishing-inspection": "job.error.finishingInspectionFailed",
  forms: "job.error.formsFailed",
  "form-inspection": "job.error.formInspectionFailed",
  health: "job.error.healthFailed",
  merge: "merge.failed",
  "ocr-review": "job.error.ocrReviewFailed",
  organise: "organise.export.failed",
  "page-transfer": "transfer.error.publish",
  privacy: "job.error.privacyFailed",
  "privacy-inspection": "job.error.privacyInspectionFailed",
  protection: "protect.error.failed",
  redaction: "job.error.redactionFailed",
  "redaction-inspection": "job.error.redactionInspectionFailed",
  "searchable-ocr": "job.error.searchableOcrFailed",
  scan: "job.error.scanFailed",
  "scan-preview": "job.error.scanPreviewFailed",
  "scanner-capture": "job.error.scannerCaptureFailed",
  split: "split.error.failed"
};

export function localisePdfJobStage<TResult>(
  job: PdfJobSnapshot<TResult>,
  t: Translate
): string {
  return job.stageCode ? t(stageTranslationKeys[job.stageCode]) : t("job.stage.starting");
}

export function localisePdfJobFailure<TResult>(
  job: PdfJobSnapshot<TResult>,
  t: Translate
): string {
  if (job.errorCode) {
    return t(errorTranslationKeys[job.errorCode]);
  }
  const migratedKey = migratedFailureKeys[job.kind];
  return t(migratedKey ?? "job.error.failed");
}

export function localisePdfJobConnectionError(
  code: PdfJobConnectionErrorCode,
  t: Translate
) {
  return t(
    code === "history-unavailable"
      ? "job.historyConnectionError"
      : "job.connectionError"
  );
}

export function isActivePdfJob<TResult>(
  job: PdfJobSnapshot<TResult> | null
): job is PdfJobSnapshot<TResult> {
  return job?.status === "queued" || job?.status === "running";
}

const interruptedJobId =
  /^interrupted-(annotations|annotation-inspection|archive|batch|batch-inspection|bookmark-inspection|bookmarks|certificate|certificate-validation|compression|compression-preview|content|content-inspection|edit-safety-inspection|finishing|finishing-inspection|form-inspection|forms|health|merge|ocr-review|searchable-ocr|organise|page-import-inspection|page-transfer|privacy|privacy-inspection|protection|redaction|redaction-inspection|scan|scan-preview|scanner-capture|split)-\d+-\d+-\d+$/u;

export function isInterruptedPdfJob<TResult>(
  job: PdfJobSnapshot<TResult> | null
): boolean {
  return Boolean(
      job &&
      job.status === "failed" &&
      (job.stageCode === "interrupted" || job.stage === "Previous job interrupted") &&
      interruptedJobId.test(job.jobId)
  );
}

export function selectRecoverablePdfJob<TResult>(
  jobs: PdfJobSnapshot<TResult>[]
): PdfJobSnapshot<TResult> | undefined {
  return [...jobs]
    .reverse()
    .find((job) => isActivePdfJob(job) || isInterruptedPdfJob(job));
}

export function buildPdfJobDiagnostic<TResult>(
  job: PdfJobSnapshot<TResult>,
  connectionError?: PdfJobConnectionErrorCode | null
) {
  const lines = [
    "Tüfekci Paperworks PDF job diagnostic",
    `Kind: ${boundedDiagnosticValue(job.kind, 64)}`,
    `Protection: ${pdfJobProtectionSummary(job.kind)}`,
    `Job ID: ${boundedDiagnosticValue(job.jobId, 256)}`,
    `Status: ${boundedDiagnosticValue(job.status, 64)}`,
    `Stage: ${boundedDiagnosticValue(job.stage, 512)}`,
    `Stage code: ${boundedDiagnosticValue(job.stageCode || "unavailable", 128)}`,
    `Progress: ${Math.max(0, Math.min(100, Math.round(job.progress)))}%`,
    `Created: ${diagnosticTime(job.createdAtMs)}`,
    `Updated: ${diagnosticTime(job.updatedAtMs)}`,
    `Error: ${boundedDiagnosticValue(job.error || "None reported", 4_096)}`,
    `Error code: ${boundedDiagnosticValue(job.errorCode || "none", 128)}`
  ];
  if (connectionError) {
    lines.push(`Status connection code: ${connectionError}`);
  }
  return lines.join("\n");
}

function boundedDiagnosticValue(value: string, maximumLength: number) {
  return value.slice(0, maximumLength);
}

function diagnosticTime(unixMilliseconds: number) {
  const date = new Date(Number.isFinite(unixMilliseconds) ? unixMilliseconds : 0);
  return Number.isNaN(date.getTime()) ? "Unavailable" : date.toISOString();
}
