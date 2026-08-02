use crate::annotations::{
    run_pdf_annotation_inspection_job_with_control, run_pdf_annotations_job_with_control,
    validate_export_pdf_annotations_request, validate_inspect_pdf_annotations_request,
    ExportPdfAnnotationsRequest, ExportPdfAnnotationsResult, InspectPdfAnnotationsRequest,
    PdfAnnotationInspection,
};
use crate::archive::{
    run_pdf_archive_job_with_control, validate_pdf_archive_request, PdfArchiveRequest,
    PdfArchiveResult,
};
use crate::batch::{
    run_batch_recipe_job_with_control, run_batch_source_inspection_job_with_control,
    run_searchable_ocr_job_with_control, validate_batch_recipe_request,
    validate_inspect_batch_sources_request, validate_searchable_ocr_request,
    InspectBatchSourcesRequest, InspectBatchSourcesResult, RunBatchRecipeRequest,
    RunBatchRecipeResult, SearchableOcrRequest, SearchableOcrResult,
};
use crate::bookmarks::{
    export_pdf_bookmarks_with_control, run_pdf_bookmark_inspection_job_with_control,
    validate_export_pdf_bookmarks_request, validate_inspect_pdf_bookmarks_request,
    ExportPdfBookmarksRequest, ExportPdfBookmarksResult, InspectPdfBookmarksRequest,
    PdfBookmarkInspection,
};
use crate::certificate::{
    run_certificate_sign_job_with_control, run_certificate_validation_job_with_control,
    validate_certificate_sign_request, validate_inspect_certificate_request,
    CertificateSignRequest, CertificateSignResult, CertificateValidationReport,
    InspectCertificateRequest,
};
use crate::combine::{
    run_page_import_inspection_job_with_control, run_pdf_merge_job_with_control,
    run_pdf_split_job_with_control, validate_combine_pdf_request,
    validate_inspect_page_import_request, validate_split_pdf_request, CombinePdfRequest,
    CombinePdfResult, InspectPageImportRequest, PageImportInspection, SplitPdfRequest,
    SplitPdfResult,
};
use crate::compression::{
    run_pdf_compression_job_with_control, run_pdf_compression_preview_job_with_control,
    validate_export_compressed_pdf_request, validate_preview_pdf_compression_request,
    ExportCompressedPdfRequest, ExportCompressedPdfResult, PdfCompressionPreview,
    PreviewPdfCompressionRequest,
};
use crate::content_editor::{
    run_pdf_content_inspection_job_with_control, run_pdf_content_job_with_control,
    validate_export_pdf_content_request, validate_inspect_pdf_content_request,
    ExportPdfContentRequest, ExportPdfContentResult, InspectPdfContentRequest,
    PdfContentInspection,
};
use crate::export::{
    export_composed_pdf_with_control, validate_composed_pdf_request, ExportComposedPdfRequest,
    ExportPdfResult,
};
use crate::forms::{
    run_pdf_form_inspection_job_with_control, run_pdf_forms_job_with_control,
    validate_export_pdf_forms_request, validate_inspect_pdf_forms_request, ExportPdfFormsRequest,
    ExportPdfFormsResult, InspectPdfFormsRequest, PdfFormInspection,
};
use crate::health::{
    run_pdf_edit_safety_inspection_job_with_control, run_pdf_health_job_with_control,
    validate_inspect_pdf_edit_safety_sources_request, validate_inspect_pdf_health_request,
    InspectPdfEditSafetySourcesRequest, InspectPdfHealthRequest, PdfEditSafetyInspectionResult,
    PdfHealthResult,
};
use crate::job_control::{PdfJobExecutionControl, PDF_JOB_CANCELLED_ERROR};
use crate::job_recovery::{JobRecoveryLease, JobRecoveryStore, RecoveredPdfJob};
use crate::ocr::OcrConfidenceResult;
use crate::operation_audit::OperationAudit;
use crate::page_finish::{
    run_pdf_finishing_inspection_job_with_control, run_pdf_finishing_job_with_control,
    validate_export_pdf_finishing_request, validate_inspect_pdf_finishing_request,
    ExportPdfFinishingRequest, ExportPdfFinishingResult, InspectPdfFinishingRequest,
    PdfFinishingInspection,
};
use crate::privacy::{
    run_pdf_privacy_job_with_control, validate_clean_pdf_privacy_request, CleanPdfPrivacyRequest,
    CleanPdfPrivacyResult,
};
use crate::privacy_inspection::{
    run_pdf_privacy_inspection_job_with_control, validate_inspect_pdf_privacy_request,
    InspectPdfPrivacyRequest, PdfPrivacyInspectionResult,
};
use crate::protection::{
    run_protection_pdf_job_with_control, validate_protection_pdf_job_request,
    ProtectionPdfJobRequest, ProtectionResult,
};
use crate::redaction::{
    run_pdf_redaction_inspection_job_with_control, run_pdf_redaction_job_with_control,
    validate_export_pdf_redaction_request, validate_inspect_pdf_redaction_request,
    ExportPdfRedactionRequest, ExportPdfRedactionResult, InspectPdfRedactionRequest,
    PdfRedactionInspection,
};
use crate::runtime_capabilities::current_capabilities;
use crate::scan_export::{
    run_scan_ocr_review_job_with_control, run_scan_pdf_job_with_control,
    run_scan_preview_job_with_control, validate_preview_scan_image_request,
    validate_review_scan_ocr_request, validate_scan_pdf_request, CreateScanPdfRequest,
    CreateScanPdfResult, PreviewScanImageRequest, PreviewScanImageResult, ReviewScanOcrRequest,
    ScanExecutionControl, SCAN_JOB_CANCELLED_ERROR,
};
use crate::scanner::{
    run_scanner_capture_job_with_control, validate_capture_request, CaptureScannerPagesRequest,
    ScannerCaptureResult,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

const MAX_RUNNING_PDF_JOBS: usize = 2;
const MAX_PENDING_PDF_JOBS: usize = 16;
const MAX_RETAINED_PDF_JOBS: usize = 32;
const INTERRUPTED_JOB_ID_PREFIX: &str = "interrupted-";
const INTERRUPTED_JOB_STAGE: &str = "Previous job interrupted";
const INTERRUPTED_JOB_ERROR: &str = "The previous app process ended before this job reported completion. Its request was not stored. Review the current workflow, then start it again.";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PdfJobKind {
    Annotations,
    #[serde(rename = "annotation-inspection")]
    AnnotationInspection,
    Archive,
    Batch,
    #[serde(rename = "batch-inspection")]
    BatchInspection,
    #[serde(rename = "bookmark-inspection")]
    BookmarkInspection,
    Bookmarks,
    Certificate,
    #[serde(rename = "certificate-validation")]
    CertificateValidation,
    Compression,
    #[serde(rename = "compression-preview")]
    CompressionPreview,
    Content,
    #[serde(rename = "content-inspection")]
    ContentInspection,
    #[serde(rename = "edit-safety-inspection")]
    EditSafetyInspection,
    Finishing,
    #[serde(rename = "finishing-inspection")]
    FinishingInspection,
    #[serde(rename = "form-inspection")]
    FormInspection,
    Forms,
    Health,
    Merge,
    #[serde(rename = "ocr-review")]
    OcrReview,
    #[serde(rename = "searchable-ocr")]
    SearchableOcr,
    Organise,
    #[serde(rename = "page-import-inspection")]
    PageImportInspection,
    #[serde(rename = "page-transfer")]
    PageTransfer,
    Privacy,
    #[serde(rename = "privacy-inspection")]
    PrivacyInspection,
    Protection,
    Redaction,
    #[serde(rename = "redaction-inspection")]
    RedactionInspection,
    Scan,
    #[serde(rename = "scan-preview")]
    ScanPreview,
    #[serde(rename = "scanner-capture")]
    ScannerCapture,
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PdfJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfJobStageCode {
    Waiting,
    Starting,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
    AnnotationsChecking,
    AnnotationsOpening,
    AnnotationsPreparing,
    AnnotationsWriting,
    AnnotationsVerifying,
    AnnotationsProtecting,
    AnnotationsPublishing,
    AnnotationInspectionChecking,
    AnnotationInspectionOpening,
    AnnotationInspectionInspecting,
    AnnotationInspectionVerifying,
    ArchiveChecking,
    ArchivePreparing,
    ArchiveConverting,
    ArchivePreflighting,
    ArchiveValidating,
    ArchiveVerifying,
    ArchivePublishing,
    BatchChecking,
    BatchPreparing,
    BatchRecognising,
    BatchCleaning,
    BatchCompressing,
    BatchArchiving,
    BatchProtecting,
    BatchVerifying,
    BatchPublishing,
    BatchInspectionChecking,
    BatchInspectionInspecting,
    BatchInspectionVerifying,
    BookmarksChecking,
    BookmarksOpening,
    BookmarksPreparingContents,
    BookmarksBuilding,
    BookmarksWriting,
    BookmarksProtecting,
    BookmarksVerifying,
    BookmarksPublishing,
    BookmarkInspectionChecking,
    BookmarkInspectionOpening,
    BookmarkInspectionInspecting,
    BookmarkInspectionVerifying,
    CertificateChecking,
    CertificateEngine,
    CertificateOpening,
    CertificatePreparing,
    CertificateSigning,
    CertificateReopening,
    CertificateValidating,
    CertificateRechecking,
    CertificatePublishing,
    CertificateValidationChecking,
    CertificateValidationOpening,
    CertificateValidationInspecting,
    CertificateValidationEngine,
    CertificateValidationValidating,
    CertificateValidationReviewing,
    CertificateValidationRechecking,
    MergeChecking,
    MergePreparing,
    MergeProtecting,
    MergeVerifying,
    MergePublishing,
    OrganiseChecking,
    OrganiseOpening,
    OrganiseArranging,
    OrganiseFlattening,
    OrganiseWriting,
    OrganiseVerifying,
    OrganiseProtecting,
    OrganisePublishing,
    CompressionChecking,
    CompressionAnalysing,
    CompressionWriting,
    CompressionVerifying,
    CompressionProtecting,
    CompressionPublishing,
    CompressionPreviewChecking,
    CompressionPreviewAnalysing,
    CompressionPreviewEncoding,
    CompressionPreviewVerifying,
    ContentChecking,
    ContentOpening,
    ContentPreparing,
    ContentWriting,
    ContentVerifying,
    ContentProtecting,
    ContentPublishing,
    ContentInspectionChecking,
    ContentInspectionOpening,
    ContentInspectionInspecting,
    ContentInspectionVerifying,
    HealthChecking,
    HealthOpening,
    HealthInspecting,
    HealthVerifying,
    FinishingChecking,
    FinishingOpening,
    FinishingPreparing,
    FinishingApplying,
    FinishingWriting,
    FinishingVerifying,
    FinishingProtecting,
    FinishingPublishing,
    FinishingInspectionChecking,
    FinishingInspectionOpening,
    FinishingInspectionInspecting,
    FinishingInspectionVerifying,
    FormsChecking,
    FormsOpening,
    FormsApplying,
    FormsWriting,
    FormsVerifying,
    FormsProtecting,
    FormsPublishing,
    FormInspectionChecking,
    FormInspectionOpening,
    FormInspectionInspecting,
    FormInspectionVerifying,
    PrivacyChecking,
    PrivacyOpening,
    PrivacyCleaning,
    PrivacyWriting,
    PrivacyVerifying,
    PrivacyProtecting,
    PrivacyPublishing,
    PrivacyInspectionChecking,
    PrivacyInspectionOpening,
    PrivacyInspectionInspecting,
    PrivacyInspectionReporting,
    PrivacyInspectionVerifying,
    RedactionChecking,
    RedactionOpening,
    RedactionApplying,
    RedactionCleaning,
    RedactionWriting,
    RedactionVerifying,
    RedactionProtecting,
    RedactionPublishing,
    RedactionInspectionChecking,
    RedactionInspectionOpening,
    RedactionInspectionInspecting,
    RedactionInspectionVerifying,
    ProtectionChecking,
    ProtectionPreparing,
    ProtectionApplying,
    ProtectionVerifying,
    ProtectionPublishing,
    SplitChecking,
    SplitPreparing,
    SplitProtecting,
    SplitVerifying,
    SplitPublishing,
    OcrReviewChecking,
    OcrReviewPreparing,
    OcrReviewRecognising,
    OcrReviewVerifying,
    SearchableOcrChecking,
    SearchableOcrPreparing,
    SearchableOcrRecognising,
    SearchableOcrVerifying,
    SearchableOcrPublishing,
    ScanChecking,
    ScanPreparing,
    ScanWriting,
    ScanRecognising,
    ScanProtecting,
    ScanPublishing,
    ScanPreviewChecking,
    ScanPreviewPreparing,
    ScanPreviewEncoding,
    ScanPreviewVerifying,
    ScannerCaptureChecking,
    ScannerCaptureConnecting,
    ScannerCaptureCapturing,
    ScannerCaptureVerifying,
    ScannerCaptureFinalising,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdfJobErrorCode {
    AnnotationsFailed,
    AnnotationInspectionFailed,
    ArchiveFailed,
    BatchFailed,
    BatchInspectionFailed,
    BookmarksFailed,
    BookmarkInspectionFailed,
    CertificateFailed,
    CertificateValidationFailed,
    CertificateAcknowledgementRequired,
    ContentFailed,
    ContentInspectionFailed,
    FinishingFailed,
    FinishingInspectionFailed,
    FormsFailed,
    FormInspectionFailed,
    HealthFailed,
    Interrupted,
    MergeFailed,
    OcrEngineUnavailable,
    OcrReviewFailed,
    PasswordRejected,
    PrivacyFailed,
    PrivacyInspectionFailed,
    ProtectionUnavailable,
    RedactionFailed,
    RedactionInspectionFailed,
    SafetyLimit,
    ScanFailed,
    ScannerCaptureFailed,
    ScanPreviewFailed,
    SearchableOcrFailed,
    SourceChanged,
    JobFailed,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "lowercase")]
pub enum StartPdfJobRequest {
    Annotations(ExportPdfAnnotationsRequest),
    #[serde(rename = "annotation-inspection")]
    AnnotationInspection(InspectPdfAnnotationsRequest),
    Archive(PdfArchiveRequest),
    Batch(RunBatchRecipeRequest),
    #[serde(rename = "batch-inspection")]
    BatchInspection(InspectBatchSourcesRequest),
    #[serde(rename = "bookmark-inspection")]
    BookmarkInspection(InspectPdfBookmarksRequest),
    Bookmarks(ExportPdfBookmarksRequest),
    Certificate(Box<CertificateSignRequest>),
    #[serde(rename = "certificate-validation")]
    CertificateValidation(InspectCertificateRequest),
    Compression(ExportCompressedPdfRequest),
    #[serde(rename = "compression-preview")]
    CompressionPreview(PreviewPdfCompressionRequest),
    Content(ExportPdfContentRequest),
    #[serde(rename = "content-inspection")]
    ContentInspection(InspectPdfContentRequest),
    #[serde(rename = "edit-safety-inspection")]
    EditSafetyInspection(InspectPdfEditSafetySourcesRequest),
    Finishing(Box<ExportPdfFinishingRequest>),
    #[serde(rename = "finishing-inspection")]
    FinishingInspection(InspectPdfFinishingRequest),
    #[serde(rename = "form-inspection")]
    FormInspection(InspectPdfFormsRequest),
    Forms(ExportPdfFormsRequest),
    Health(InspectPdfHealthRequest),
    Merge(CombinePdfRequest),
    #[serde(rename = "ocr-review")]
    OcrReview(ReviewScanOcrRequest),
    #[serde(rename = "searchable-ocr")]
    SearchableOcr(SearchableOcrRequest),
    Organise(ExportComposedPdfRequest),
    #[serde(rename = "page-import-inspection")]
    PageImportInspection(InspectPageImportRequest),
    #[serde(rename = "page-transfer")]
    PageTransfer(ExportComposedPdfRequest),
    Privacy(CleanPdfPrivacyRequest),
    #[serde(rename = "privacy-inspection")]
    PrivacyInspection(InspectPdfPrivacyRequest),
    Protection(ProtectionPdfJobRequest),
    Redaction(ExportPdfRedactionRequest),
    #[serde(rename = "redaction-inspection")]
    RedactionInspection(InspectPdfRedactionRequest),
    Scan(CreateScanPdfRequest),
    #[serde(rename = "scan-preview")]
    ScanPreview(PreviewScanImageRequest),
    #[serde(rename = "scanner-capture")]
    ScannerCapture(CaptureScannerPagesRequest),
    Split(SplitPdfRequest),
}

impl StartPdfJobRequest {
    fn kind(&self) -> PdfJobKind {
        match self {
            Self::Annotations(_) => PdfJobKind::Annotations,
            Self::AnnotationInspection(_) => PdfJobKind::AnnotationInspection,
            Self::Archive(_) => PdfJobKind::Archive,
            Self::Batch(_) => PdfJobKind::Batch,
            Self::BatchInspection(_) => PdfJobKind::BatchInspection,
            Self::BookmarkInspection(_) => PdfJobKind::BookmarkInspection,
            Self::Bookmarks(_) => PdfJobKind::Bookmarks,
            Self::Certificate(_) => PdfJobKind::Certificate,
            Self::CertificateValidation(_) => PdfJobKind::CertificateValidation,
            Self::Compression(_) => PdfJobKind::Compression,
            Self::CompressionPreview(_) => PdfJobKind::CompressionPreview,
            Self::Content(_) => PdfJobKind::Content,
            Self::ContentInspection(_) => PdfJobKind::ContentInspection,
            Self::EditSafetyInspection(_) => PdfJobKind::EditSafetyInspection,
            Self::Finishing(_) => PdfJobKind::Finishing,
            Self::FinishingInspection(_) => PdfJobKind::FinishingInspection,
            Self::FormInspection(_) => PdfJobKind::FormInspection,
            Self::Forms(_) => PdfJobKind::Forms,
            Self::Health(_) => PdfJobKind::Health,
            Self::Merge(_) => PdfJobKind::Merge,
            Self::OcrReview(_) => PdfJobKind::OcrReview,
            Self::SearchableOcr(_) => PdfJobKind::SearchableOcr,
            Self::Organise(_) => PdfJobKind::Organise,
            Self::PageImportInspection(_) => PdfJobKind::PageImportInspection,
            Self::PageTransfer(_) => PdfJobKind::PageTransfer,
            Self::Privacy(_) => PdfJobKind::Privacy,
            Self::PrivacyInspection(_) => PdfJobKind::PrivacyInspection,
            Self::Protection(_) => PdfJobKind::Protection,
            Self::Redaction(_) => PdfJobKind::Redaction,
            Self::RedactionInspection(_) => PdfJobKind::RedactionInspection,
            Self::Scan(_) => PdfJobKind::Scan,
            Self::ScanPreview(_) => PdfJobKind::ScanPreview,
            Self::ScannerCapture(_) => PdfJobKind::ScannerCapture,
            Self::Split(_) => PdfJobKind::Split,
        }
    }

    fn requires_external_processes(&self) -> bool {
        match self {
            Self::Archive(_)
            | Self::Batch(_)
            | Self::Certificate(_)
            | Self::CertificateValidation(_)
            | Self::OcrReview(_)
            | Self::Protection(_)
            | Self::SearchableOcr(_) => true,
            Self::Scan(request) => request.requires_desktop_services(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum PdfJobResult {
    Annotations(ExportPdfAnnotationsResult),
    AnnotationInspection(PdfAnnotationInspection),
    Archive(PdfArchiveResult),
    Batch(RunBatchRecipeResult),
    BatchInspection(InspectBatchSourcesResult),
    BookmarkInspection(PdfBookmarkInspection),
    Bookmarks(ExportPdfBookmarksResult),
    Certificate(CertificateSignResult),
    CertificateValidation(CertificateValidationReport),
    Compression(ExportCompressedPdfResult),
    CompressionPreview(PdfCompressionPreview),
    Content(ExportPdfContentResult),
    ContentInspection(PdfContentInspection),
    EditSafetyInspection(PdfEditSafetyInspectionResult),
    Finishing(ExportPdfFinishingResult),
    FinishingInspection(PdfFinishingInspection),
    FormInspection(PdfFormInspection),
    Forms(ExportPdfFormsResult),
    Health(PdfHealthResult),
    Merge(CombinePdfResult),
    OcrReview(OcrConfidenceResult),
    SearchableOcr(SearchableOcrResult),
    Organise(ExportPdfResult),
    PageImportInspection(PageImportInspection),
    PageTransfer(ExportPdfResult),
    Privacy(CleanPdfPrivacyResult),
    PrivacyInspection(PdfPrivacyInspectionResult),
    Protection(ProtectionResult),
    Redaction(ExportPdfRedactionResult),
    RedactionInspection(PdfRedactionInspection),
    Scan(CreateScanPdfResult),
    ScanPreview(PreviewScanImageResult),
    ScannerCapture(ScannerCaptureResult),
    Split(SplitPdfResult),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfJobSnapshot {
    job_id: String,
    kind: PdfJobKind,
    status: PdfJobStatus,
    progress: u8,
    stage: String,
    stage_code: Option<PdfJobStageCode>,
    result: Option<PdfJobResult>,
    error: Option<String>,
    error_code: Option<PdfJobErrorCode>,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Clone)]
struct PdfJobRecord {
    snapshot: PdfJobSnapshot,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct PdfJobStore {
    next_id: u64,
    jobs: HashMap<String, PdfJobRecord>,
    order: VecDeque<String>,
    pending_order: VecDeque<String>,
    pending_requests: HashMap<String, StartPdfJobRequest>,
    recovery_leases: HashMap<String, JobRecoveryLease>,
    running: usize,
}

#[derive(Clone)]
pub struct PdfJobManager {
    inner: Arc<Mutex<PdfJobStore>>,
    max_running: usize,
    audit: Option<OperationAudit>,
    recovery: Option<JobRecoveryStore>,
    scanner_capture_root: Option<PathBuf>,
}

impl Default for PdfJobManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PdfJobStore::default())),
            max_running: MAX_RUNNING_PDF_JOBS,
            audit: None,
            recovery: None,
            scanner_capture_root: None,
        }
    }
}

impl PdfJobManager {
    pub(crate) fn with_services(
        audit: OperationAudit,
        recovery: JobRecoveryStore,
        recovered: Vec<RecoveredPdfJob>,
        scanner_capture_root: PathBuf,
    ) -> Self {
        let mut store = PdfJobStore::default();
        restore_interrupted_jobs(&mut store, recovered);
        Self {
            inner: Arc::new(Mutex::new(store)),
            max_running: MAX_RUNNING_PDF_JOBS,
            audit: Some(audit),
            recovery: Some(recovery),
            scanner_capture_root: Some(scanner_capture_root),
        }
    }
}

#[tauri::command]
pub fn start_pdf_job(
    request: StartPdfJobRequest,
    jobs: State<'_, PdfJobManager>,
) -> Result<PdfJobSnapshot, String> {
    jobs.start(request)
}

#[tauri::command]
pub fn get_pdf_job(
    job_id: String,
    jobs: State<'_, PdfJobManager>,
) -> Result<PdfJobSnapshot, String> {
    jobs.get(&job_id)
}

#[tauri::command]
pub fn list_pdf_jobs(
    kind: Option<PdfJobKind>,
    jobs: State<'_, PdfJobManager>,
) -> Result<Vec<PdfJobSnapshot>, String> {
    jobs.list(kind)
}

#[tauri::command]
pub fn cancel_pdf_job(
    job_id: String,
    jobs: State<'_, PdfJobManager>,
) -> Result<PdfJobSnapshot, String> {
    jobs.cancel(&job_id)
}

impl PdfJobManager {
    fn start(&self, request: StartPdfJobRequest) -> Result<PdfJobSnapshot, String> {
        let capabilities = current_capabilities();
        if request.requires_external_processes() && !capabilities.external_processes() {
            return Err(
                "This PDF operation requires a desktop engine and is unavailable on this platform."
                    .to_string(),
            );
        }
        if matches!(&request, StartPdfJobRequest::ScannerCapture(_))
            && !capabilities.connected_scanning()
        {
            return Err("Connected scanner capture is unavailable on this platform.".to_string());
        }
        match &request {
            StartPdfJobRequest::Annotations(annotation_request) => {
                validate_export_pdf_annotations_request(annotation_request)?;
            }
            StartPdfJobRequest::AnnotationInspection(annotation_request) => {
                validate_inspect_pdf_annotations_request(annotation_request)?;
            }
            StartPdfJobRequest::Archive(archive_request) => {
                validate_pdf_archive_request(archive_request)?;
            }
            StartPdfJobRequest::Batch(batch_request) => {
                validate_batch_recipe_request(batch_request)?;
            }
            StartPdfJobRequest::BatchInspection(batch_request) => {
                validate_inspect_batch_sources_request(batch_request)?;
            }
            StartPdfJobRequest::BookmarkInspection(bookmark_request) => {
                validate_inspect_pdf_bookmarks_request(bookmark_request)?;
            }
            StartPdfJobRequest::Bookmarks(bookmark_request) => {
                validate_export_pdf_bookmarks_request(bookmark_request)?;
            }
            StartPdfJobRequest::Certificate(certificate_request) => {
                validate_certificate_sign_request(certificate_request)?;
            }
            StartPdfJobRequest::CertificateValidation(certificate_request) => {
                validate_inspect_certificate_request(certificate_request)?;
            }
            StartPdfJobRequest::Compression(compression_request) => {
                validate_export_compressed_pdf_request(compression_request)?;
            }
            StartPdfJobRequest::CompressionPreview(compression_request) => {
                validate_preview_pdf_compression_request(compression_request)?;
            }
            StartPdfJobRequest::Content(content_request) => {
                validate_export_pdf_content_request(content_request)?;
            }
            StartPdfJobRequest::ContentInspection(content_request) => {
                validate_inspect_pdf_content_request(content_request)?;
            }
            StartPdfJobRequest::EditSafetyInspection(edit_safety_request) => {
                validate_inspect_pdf_edit_safety_sources_request(edit_safety_request)?;
            }
            StartPdfJobRequest::FormInspection(form_request) => {
                validate_inspect_pdf_forms_request(form_request)?;
            }
            StartPdfJobRequest::Forms(form_request) => {
                validate_export_pdf_forms_request(form_request)?;
            }
            StartPdfJobRequest::Finishing(finishing_request) => {
                validate_export_pdf_finishing_request(finishing_request)?;
            }
            StartPdfJobRequest::FinishingInspection(finishing_request) => {
                validate_inspect_pdf_finishing_request(finishing_request)?;
            }
            StartPdfJobRequest::Health(health_request) => {
                validate_inspect_pdf_health_request(health_request)?;
            }
            StartPdfJobRequest::Merge(merge_request) => {
                validate_combine_pdf_request(merge_request)?;
            }
            StartPdfJobRequest::OcrReview(ocr_review_request) => {
                validate_review_scan_ocr_request(ocr_review_request)?;
            }
            StartPdfJobRequest::SearchableOcr(ocr_request) => {
                validate_searchable_ocr_request(ocr_request)?;
            }
            StartPdfJobRequest::Organise(organise_request) => {
                validate_composed_pdf_request(organise_request)?;
            }
            StartPdfJobRequest::PageImportInspection(import_request) => {
                validate_inspect_page_import_request(import_request)?;
            }
            StartPdfJobRequest::PageTransfer(transfer_request) => {
                validate_composed_pdf_request(transfer_request)?;
            }
            StartPdfJobRequest::Privacy(privacy_request) => {
                validate_clean_pdf_privacy_request(privacy_request)?;
            }
            StartPdfJobRequest::PrivacyInspection(privacy_request) => {
                validate_inspect_pdf_privacy_request(privacy_request)?;
            }
            StartPdfJobRequest::Protection(protection_request) => {
                validate_protection_pdf_job_request(protection_request)?;
            }
            StartPdfJobRequest::Redaction(redaction_request) => {
                validate_export_pdf_redaction_request(redaction_request)?;
            }
            StartPdfJobRequest::RedactionInspection(redaction_request) => {
                validate_inspect_pdf_redaction_request(redaction_request)?;
            }
            StartPdfJobRequest::Scan(scan_request) => {
                validate_scan_pdf_request(scan_request)?;
            }
            StartPdfJobRequest::ScanPreview(scan_preview_request) => {
                validate_preview_scan_image_request(scan_preview_request)?;
            }
            StartPdfJobRequest::ScannerCapture(scanner_capture_request) => {
                validate_capture_request(scanner_capture_request)?;
            }
            StartPdfJobRequest::Split(split_request) => {
                validate_split_pdf_request(split_request)?;
            }
        }
        let kind = request.kind();
        let job_id = {
            let mut store = self.lock()?;
            prune_terminal_jobs(&mut store);
            let pending_jobs = store
                .jobs
                .values()
                .filter(|record| !is_terminal(record.snapshot.status))
                .count();
            if pending_jobs >= MAX_PENDING_PDF_JOBS {
                return Err(format!(
                    "Wait for one of the {MAX_PENDING_PDF_JOBS} queued PDF jobs to finish."
                ));
            }

            store.next_id = store.next_id.saturating_add(1);
            let job_id = format!(
                "{}-{}-{}",
                kind.job_id_prefix(),
                std::process::id(),
                store.next_id
            );
            let now = timestamp_ms();
            let recovery_lease = self
                .recovery
                .as_ref()
                .map(|recovery| recovery.register(kind, now))
                .transpose()?;
            let snapshot = PdfJobSnapshot {
                job_id: job_id.clone(),
                kind,
                status: PdfJobStatus::Queued,
                progress: 0,
                stage: "Waiting for an available worker".to_string(),
                stage_code: Some(PdfJobStageCode::Waiting),
                result: None,
                error: None,
                error_code: None,
                created_at_ms: now,
                updated_at_ms: now,
            };
            store.jobs.insert(
                job_id.clone(),
                PdfJobRecord {
                    snapshot,
                    cancelled: Arc::new(AtomicBool::new(false)),
                },
            );
            if let Some(recovery_lease) = recovery_lease {
                store.recovery_leases.insert(job_id.clone(), recovery_lease);
            }
            store.pending_requests.insert(job_id.clone(), request);
            store.pending_order.push_back(job_id.clone());
            store.order.push_back(job_id.clone());
            job_id
        };

        self.dispatch()?;
        self.get(&job_id)
    }

    fn dispatch(&self) -> Result<(), String> {
        loop {
            let next = {
                let mut store = self.lock()?;
                if store.running >= self.max_running {
                    None
                } else {
                    take_next_job(&mut store)
                }
            };
            let Some((job_id, request, cancelled)) = next else {
                return Ok(());
            };

            let manager = self.clone();
            let thread_name = format!("paperworks-{job_id}");
            let worker_job_id = job_id.clone();
            if let Err(error) = thread::Builder::new()
                .name(thread_name)
                .spawn(move || manager.run_job(worker_job_id, request, cancelled))
            {
                let message = format!("The PDF job worker could not be started: {error}");
                self.finish_failed(&job_id, message)?;
            }
        }
    }

    fn run_job(&self, job_id: String, request: StartPdfJobRequest, cancelled: Arc<AtomicBool>) {
        let progress_manager = self.clone();
        let progress_job_id = job_id.clone();
        let progress: Arc<dyn Fn(u8, String) + Send + Sync> = Arc::new(move |value, stage| {
            let _ = progress_manager.update_progress(&progress_job_id, value, stage);
        });
        let result = match request {
            StartPdfJobRequest::Annotations(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_annotations_job_with_control(request, &control)
                    .map(PdfJobResult::Annotations)
            }
            StartPdfJobRequest::AnnotationInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_annotation_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::AnnotationInspection)
            }
            StartPdfJobRequest::Archive(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_archive_job_with_control(request, &control).map(PdfJobResult::Archive)
            }
            StartPdfJobRequest::Batch(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_batch_recipe_job_with_control(request, &control).map(PdfJobResult::Batch)
            }
            StartPdfJobRequest::BatchInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_batch_source_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::BatchInspection)
            }
            StartPdfJobRequest::BookmarkInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_bookmark_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::BookmarkInspection)
            }
            StartPdfJobRequest::Bookmarks(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                export_pdf_bookmarks_with_control(request, &control).map(PdfJobResult::Bookmarks)
            }
            StartPdfJobRequest::Certificate(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_certificate_sign_job_with_control(*request, &control)
                    .map(PdfJobResult::Certificate)
            }
            StartPdfJobRequest::CertificateValidation(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_certificate_validation_job_with_control(request, &control)
                    .map(PdfJobResult::CertificateValidation)
            }
            StartPdfJobRequest::Compression(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_compression_job_with_control(request, &control)
                    .map(PdfJobResult::Compression)
            }
            StartPdfJobRequest::CompressionPreview(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_compression_preview_job_with_control(request, &control)
                    .map(PdfJobResult::CompressionPreview)
            }
            StartPdfJobRequest::Content(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_content_job_with_control(request, &control).map(PdfJobResult::Content)
            }
            StartPdfJobRequest::ContentInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_content_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::ContentInspection)
            }
            StartPdfJobRequest::EditSafetyInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_edit_safety_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::EditSafetyInspection)
            }
            StartPdfJobRequest::Finishing(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_finishing_job_with_control(*request, &control).map(PdfJobResult::Finishing)
            }
            StartPdfJobRequest::FinishingInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_finishing_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::FinishingInspection)
            }
            StartPdfJobRequest::FormInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_form_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::FormInspection)
            }
            StartPdfJobRequest::Forms(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_forms_job_with_control(request, &control).map(PdfJobResult::Forms)
            }
            StartPdfJobRequest::Health(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_health_job_with_control(request, &control).map(PdfJobResult::Health)
            }
            StartPdfJobRequest::Merge(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_merge_job_with_control(request, &control).map(PdfJobResult::Merge)
            }
            StartPdfJobRequest::OcrReview(request) => {
                let control =
                    ScanExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_scan_ocr_review_job_with_control(request, &control).map(PdfJobResult::OcrReview)
            }
            StartPdfJobRequest::SearchableOcr(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_searchable_ocr_job_with_control(request, &control)
                    .map(PdfJobResult::SearchableOcr)
            }
            StartPdfJobRequest::Organise(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                export_composed_pdf_with_control(request, &control).map(PdfJobResult::Organise)
            }
            StartPdfJobRequest::PageImportInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_page_import_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::PageImportInspection)
            }
            StartPdfJobRequest::PageTransfer(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                export_composed_pdf_with_control(request, &control).map(PdfJobResult::PageTransfer)
            }
            StartPdfJobRequest::Privacy(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_privacy_job_with_control(request, &control).map(PdfJobResult::Privacy)
            }
            StartPdfJobRequest::PrivacyInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_privacy_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::PrivacyInspection)
            }
            StartPdfJobRequest::Protection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_protection_pdf_job_with_control(request, &control).map(PdfJobResult::Protection)
            }
            StartPdfJobRequest::Redaction(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_redaction_job_with_control(request, &control).map(PdfJobResult::Redaction)
            }
            StartPdfJobRequest::RedactionInspection(request) => {
                let control =
                    PdfJobExecutionControl::new(Arc::clone(&cancelled), Arc::clone(&progress));
                run_pdf_redaction_inspection_job_with_control(request, &control)
                    .map(PdfJobResult::RedactionInspection)
            }
            StartPdfJobRequest::Scan(request) => {
                let control = ScanExecutionControl::new(cancelled, progress);
                run_scan_pdf_job_with_control(request, &control).map(PdfJobResult::Scan)
            }
            StartPdfJobRequest::ScanPreview(request) => {
                let control = ScanExecutionControl::new(cancelled, progress);
                run_scan_preview_job_with_control(request, &control).map(PdfJobResult::ScanPreview)
            }
            StartPdfJobRequest::ScannerCapture(request) => {
                let control = PdfJobExecutionControl::new(cancelled, progress);
                self.scanner_capture_root
                    .as_deref()
                    .ok_or_else(|| {
                        "The private scanner capture workspace is unavailable.".to_string()
                    })
                    .and_then(|root| run_scanner_capture_job_with_control(request, root, &control))
                    .map(PdfJobResult::ScannerCapture)
            }
            StartPdfJobRequest::Split(request) => {
                let control = PdfJobExecutionControl::new(cancelled, progress);
                run_pdf_split_job_with_control(request, &control).map(PdfJobResult::Split)
            }
        };

        match result {
            Ok(result) => {
                let _ = self.finish_succeeded(&job_id, result);
            }
            Err(error) if error == PDF_JOB_CANCELLED_ERROR || error == SCAN_JOB_CANCELLED_ERROR => {
                let _ = self.finish_cancelled(&job_id);
            }
            Err(error) => {
                let _ = self.finish_failed(&job_id, error);
            }
        }
    }

    fn get(&self, job_id: &str) -> Result<PdfJobSnapshot, String> {
        self.lock()?
            .jobs
            .get(job_id)
            .map(|record| record.snapshot.clone())
            .ok_or_else(|| "The PDF job is no longer available.".to_string())
    }

    fn list(&self, kind: Option<PdfJobKind>) -> Result<Vec<PdfJobSnapshot>, String> {
        let store = self.lock()?;
        Ok(store
            .order
            .iter()
            .filter_map(|job_id| store.jobs.get(job_id))
            .filter(|record| kind.is_none_or(|expected| record.snapshot.kind == expected))
            .map(|record| record.snapshot.clone())
            .collect())
    }

    fn cancel(&self, job_id: &str) -> Result<PdfJobSnapshot, String> {
        let (snapshot, audit_event, recovery_lease) = {
            let mut store = self.lock()?;
            let record = store
                .jobs
                .get_mut(job_id)
                .ok_or_else(|| "The PDF job is no longer available.".to_string())?;
            let mut audit_event = None;
            let mut remove_recovery_lease = false;
            match record.snapshot.status {
                PdfJobStatus::Queued => {
                    record.cancelled.store(true, Ordering::Release);
                    record.snapshot.status = PdfJobStatus::Cancelled;
                    record.snapshot.stage = "PDF job cancelled before starting".to_string();
                    record.snapshot.stage_code = Some(PdfJobStageCode::Cancelled);
                    record.snapshot.updated_at_ms = timestamp_ms();
                    audit_event = Some((
                        record.snapshot.kind,
                        record.snapshot.created_at_ms,
                        record.snapshot.updated_at_ms,
                    ));
                    store.pending_requests.remove(job_id);
                    store.pending_order.retain(|queued_id| queued_id != job_id);
                    remove_recovery_lease = true;
                }
                PdfJobStatus::Running => {
                    record.cancelled.store(true, Ordering::Release);
                    record.snapshot.stage = "Cancelling PDF job safely".to_string();
                    record.snapshot.stage_code = Some(PdfJobStageCode::Cancelling);
                    record.snapshot.updated_at_ms = timestamp_ms();
                }
                _ => {}
            }
            (
                store
                    .jobs
                    .get(job_id)
                    .expect("the job was checked above")
                    .snapshot
                    .clone(),
                audit_event,
                remove_recovery_lease
                    .then(|| store.recovery_leases.remove(job_id))
                    .flatten(),
            )
        };
        if let Some(recovery_lease) = recovery_lease {
            let _ = recovery_lease.complete();
        }
        if let Some((kind, started_at_ms, completed_at_ms)) = audit_event {
            self.record_audit(
                kind,
                PdfJobStatus::Cancelled,
                started_at_ms,
                completed_at_ms,
            );
        }
        Ok(snapshot)
    }

    fn update_progress(&self, job_id: &str, progress: u8, stage: String) -> Result<(), String> {
        let mut store = self.lock()?;
        let Some(record) = store.jobs.get_mut(job_id) else {
            return Ok(());
        };
        if is_terminal(record.snapshot.status) {
            return Ok(());
        }
        record.snapshot.progress = record.snapshot.progress.max(progress.min(99));
        let cancelled = record.cancelled.load(Ordering::Acquire);
        record.snapshot.stage = if cancelled {
            "Cancelling PDF job safely".to_string()
        } else {
            stage
        };
        record.snapshot.stage_code = if cancelled {
            Some(PdfJobStageCode::Cancelling)
        } else {
            stage_code_for_progress(
                record.snapshot.kind,
                record.snapshot.progress,
                &record.snapshot.stage,
            )
        };
        record.snapshot.updated_at_ms = timestamp_ms();
        Ok(())
    }

    fn finish_succeeded(&self, job_id: &str, result: PdfJobResult) -> Result<(), String> {
        self.finish(
            job_id,
            PdfJobStatus::Succeeded,
            100,
            "PDF job completed".to_string(),
            Some(result),
            None,
        )
    }

    fn finish_cancelled(&self, job_id: &str) -> Result<(), String> {
        self.finish(
            job_id,
            PdfJobStatus::Cancelled,
            0,
            "PDF job cancelled safely".to_string(),
            None,
            None,
        )
    }

    fn finish_failed(&self, job_id: &str, error: String) -> Result<(), String> {
        self.finish(
            job_id,
            PdfJobStatus::Failed,
            0,
            "PDF job could not complete".to_string(),
            None,
            Some(error),
        )
    }

    fn finish(
        &self,
        job_id: &str,
        status: PdfJobStatus,
        progress: u8,
        stage: String,
        result: Option<PdfJobResult>,
        error: Option<String>,
    ) -> Result<(), String> {
        let (audit_event, recovery_lease) = {
            let mut store = self.lock()?;
            let Some(previous_status) = store.jobs.get(job_id).map(|record| record.snapshot.status)
            else {
                return Ok(());
            };
            if is_terminal(previous_status) {
                return Ok(());
            }
            if previous_status == PdfJobStatus::Running {
                store.running = store.running.saturating_sub(1);
            }
            let record = store
                .jobs
                .get_mut(job_id)
                .expect("the job was checked above");
            record.snapshot.status = status;
            if status == PdfJobStatus::Succeeded {
                record.snapshot.progress = progress;
            }
            record.snapshot.stage = stage;
            record.snapshot.stage_code = Some(match status {
                PdfJobStatus::Queued => PdfJobStageCode::Waiting,
                PdfJobStatus::Running => PdfJobStageCode::Starting,
                PdfJobStatus::Succeeded => PdfJobStageCode::Completed,
                PdfJobStatus::Failed => PdfJobStageCode::Failed,
                PdfJobStatus::Cancelled => PdfJobStageCode::Cancelled,
            });
            record.snapshot.result = result;
            record.snapshot.error_code = error
                .as_deref()
                .map(|message| error_code_for(record.snapshot.kind, message));
            record.snapshot.error = error;
            record.snapshot.updated_at_ms = timestamp_ms();
            let audit_event = (
                record.snapshot.kind,
                record.snapshot.created_at_ms,
                record.snapshot.updated_at_ms,
            );
            let recovery_lease = store.recovery_leases.remove(job_id);
            (audit_event, recovery_lease)
        };
        if let Some(recovery_lease) = recovery_lease {
            let _ = recovery_lease.complete();
        }
        self.record_audit(audit_event.0, status, audit_event.1, audit_event.2);
        self.dispatch()
    }

    fn record_audit(
        &self,
        kind: PdfJobKind,
        status: PdfJobStatus,
        started_at_ms: u64,
        completed_at_ms: u64,
    ) {
        if let Some(audit) = &self.audit {
            let _ = audit.record_terminal(kind, status, started_at_ms, completed_at_ms);
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, PdfJobStore>, String> {
        self.inner
            .lock()
            .map_err(|_| "The PDF job queue could not be accessed safely.".to_string())
    }

    #[cfg(test)]
    fn with_max_running(max_running: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PdfJobStore::default())),
            max_running,
            audit: None,
            recovery: None,
            scanner_capture_root: None,
        }
    }

    #[cfg(test)]
    fn with_test_recovery(
        max_running: usize,
        recovery: JobRecoveryStore,
        recovered: Vec<RecoveredPdfJob>,
    ) -> Self {
        let mut store = PdfJobStore::default();
        restore_interrupted_jobs(&mut store, recovered);
        Self {
            inner: Arc::new(Mutex::new(store)),
            max_running,
            audit: None,
            recovery: Some(recovery),
            scanner_capture_root: None,
        }
    }
}

fn restore_interrupted_jobs(store: &mut PdfJobStore, recovered: Vec<RecoveredPdfJob>) {
    for recovered_job in recovered.into_iter().take(MAX_RETAINED_PDF_JOBS) {
        let job_id = format!(
            "{INTERRUPTED_JOB_ID_PREFIX}{}-{}",
            recovered_job.kind.job_id_prefix(),
            recovered_job.entry_id
        );
        let snapshot = PdfJobSnapshot {
            job_id: job_id.clone(),
            kind: recovered_job.kind,
            status: PdfJobStatus::Failed,
            progress: 0,
            stage: INTERRUPTED_JOB_STAGE.to_string(),
            stage_code: Some(PdfJobStageCode::Interrupted),
            result: None,
            error: Some(INTERRUPTED_JOB_ERROR.to_string()),
            error_code: Some(PdfJobErrorCode::Interrupted),
            created_at_ms: recovered_job.started_at_ms,
            updated_at_ms: recovered_job
                .recovered_at_ms
                .max(recovered_job.started_at_ms),
        };
        store.jobs.insert(
            job_id.clone(),
            PdfJobRecord {
                snapshot,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        store.order.push_back(job_id);
    }
}

impl PdfJobKind {
    fn job_id_prefix(self) -> &'static str {
        match self {
            Self::Annotations => "annotations",
            Self::AnnotationInspection => "annotation-inspection",
            Self::Archive => "archive",
            Self::Batch => "batch",
            Self::BatchInspection => "batch-inspection",
            Self::BookmarkInspection => "bookmark-inspection",
            Self::Bookmarks => "bookmarks",
            Self::Certificate => "certificate",
            Self::CertificateValidation => "certificate-validation",
            Self::Compression => "compression",
            Self::CompressionPreview => "compression-preview",
            Self::Content => "content",
            Self::ContentInspection => "content-inspection",
            Self::EditSafetyInspection => "edit-safety-inspection",
            Self::Finishing => "finishing",
            Self::FinishingInspection => "finishing-inspection",
            Self::FormInspection => "form-inspection",
            Self::Forms => "forms",
            Self::Health => "health",
            Self::Merge => "merge",
            Self::OcrReview => "ocr-review",
            Self::SearchableOcr => "searchable-ocr",
            Self::Organise => "organise",
            Self::PageImportInspection => "page-import-inspection",
            Self::PageTransfer => "page-transfer",
            Self::Privacy => "privacy",
            Self::PrivacyInspection => "privacy-inspection",
            Self::Protection => "protection",
            Self::Redaction => "redaction",
            Self::RedactionInspection => "redaction-inspection",
            Self::Scan => "scan",
            Self::ScanPreview => "scan-preview",
            Self::ScannerCapture => "scanner-capture",
            Self::Split => "split",
        }
    }
}

fn stage_code_for_progress(kind: PdfJobKind, progress: u8, stage: &str) -> Option<PdfJobStageCode> {
    let normalised = stage.to_ascii_lowercase();
    Some(match kind {
        PdfJobKind::Annotations => {
            if progress <= 7 {
                PdfJobStageCode::AnnotationsChecking
            } else if progress <= 21 {
                PdfJobStageCode::AnnotationsOpening
            } else if progress <= 59 {
                PdfJobStageCode::AnnotationsPreparing
            } else if progress <= 67 {
                PdfJobStageCode::AnnotationsWriting
            } else if progress <= 75 || (89..=98).contains(&progress) {
                PdfJobStageCode::AnnotationsVerifying
            } else if progress <= 88 {
                PdfJobStageCode::AnnotationsProtecting
            } else {
                PdfJobStageCode::AnnotationsPublishing
            }
        }
        PdfJobKind::AnnotationInspection => {
            if progress <= 17 {
                PdfJobStageCode::AnnotationInspectionChecking
            } else if progress <= 25 {
                PdfJobStageCode::AnnotationInspectionOpening
            } else if progress <= 93 {
                PdfJobStageCode::AnnotationInspectionInspecting
            } else {
                PdfJobStageCode::AnnotationInspectionVerifying
            }
        }
        PdfJobKind::Archive => {
            if normalised.contains("publish") {
                PdfJobStageCode::ArchivePublishing
            } else if progress <= 10 {
                PdfJobStageCode::ArchiveChecking
            } else if normalised.contains("preflight") {
                PdfJobStageCode::ArchivePreflighting
            } else if normalised.contains("verapdf")
                || normalised.contains("validating")
                || normalised.contains("validation")
            {
                PdfJobStageCode::ArchiveValidating
            } else if normalised.contains("creating")
                || normalised.contains("conversion")
                || normalised.contains("ocrmypdf")
                || normalised.contains("recognising")
            {
                PdfJobStageCode::ArchiveConverting
            } else if normalised.contains("verif")
                || normalised.contains("recheck")
                || normalised.contains("finalising")
                || normalised.contains("searchable text")
            {
                PdfJobStageCode::ArchiveVerifying
            } else {
                PdfJobStageCode::ArchivePreparing
            }
        }
        PdfJobKind::Batch => {
            if normalised.contains("publish") {
                PdfJobStageCode::BatchPublishing
            } else if normalised.contains("recheck") || normalised.contains("verif") {
                PdfJobStageCode::BatchVerifying
            } else if normalised.contains("aes-256")
                || normalised.contains("protection")
                || normalised.contains("protecting")
            {
                PdfJobStageCode::BatchProtecting
            } else if normalised.contains("pdf/a")
                || normalised.contains("archive")
                || normalised.contains("verapdf")
                || normalised.contains("converting and validating")
            {
                PdfJobStageCode::BatchArchiving
            } else if normalised.contains("compression") || normalised.contains("compressing") {
                PdfJobStageCode::BatchCompressing
            } else if normalised.contains("privacy") || normalised.contains("cleaning") {
                PdfJobStageCode::BatchCleaning
            } else if normalised.contains("ocr") || normalised.contains("recognising") {
                PdfJobStageCode::BatchRecognising
            } else if progress <= 4 {
                PdfJobStageCode::BatchChecking
            } else {
                PdfJobStageCode::BatchPreparing
            }
        }
        PdfJobKind::BatchInspection => {
            if progress <= 4 {
                PdfJobStageCode::BatchInspectionChecking
            } else if progress <= 96 {
                PdfJobStageCode::BatchInspectionInspecting
            } else {
                PdfJobStageCode::BatchInspectionVerifying
            }
        }
        PdfJobKind::Bookmarks => {
            if normalised.contains("publish") {
                PdfJobStageCode::BookmarksPublishing
            } else if normalised.contains("aes-256") || normalised.contains("protect") {
                PdfJobStageCode::BookmarksProtecting
            } else if normalised.contains("verif") || normalised.contains("recheck") {
                PdfJobStageCode::BookmarksVerifying
            } else if normalised.contains("printed contents") {
                PdfJobStageCode::BookmarksPreparingContents
            } else if normalised.contains("building") || normalised.contains("outline") {
                PdfJobStageCode::BookmarksBuilding
            } else if normalised.contains("writing") || normalised.contains("prepared") {
                PdfJobStageCode::BookmarksWriting
            } else if normalised.contains("opening") || normalised.contains("decrypt") {
                PdfJobStageCode::BookmarksOpening
            } else {
                PdfJobStageCode::BookmarksChecking
            }
        }
        PdfJobKind::BookmarkInspection => {
            if normalised.contains("finalis") || normalised.contains("recheck") {
                PdfJobStageCode::BookmarkInspectionVerifying
            } else if normalised.contains("opening") {
                PdfJobStageCode::BookmarkInspectionOpening
            } else if normalised.contains("inspect") {
                PdfJobStageCode::BookmarkInspectionInspecting
            } else {
                PdfJobStageCode::BookmarkInspectionChecking
            }
        }
        PdfJobKind::Certificate => {
            if progress <= 1 {
                PdfJobStageCode::CertificateChecking
            } else if progress <= 8 {
                PdfJobStageCode::CertificateEngine
            } else if progress <= 16 {
                PdfJobStageCode::CertificateOpening
            } else if progress <= 28 {
                PdfJobStageCode::CertificatePreparing
            } else if progress <= 35 {
                PdfJobStageCode::CertificateSigning
            } else if progress <= 68 {
                PdfJobStageCode::CertificateReopening
            } else if progress <= 78 {
                PdfJobStageCode::CertificateValidating
            } else if progress <= 94 {
                PdfJobStageCode::CertificateRechecking
            } else {
                PdfJobStageCode::CertificatePublishing
            }
        }
        PdfJobKind::CertificateValidation => {
            if progress <= 1 {
                PdfJobStageCode::CertificateValidationChecking
            } else if progress <= 8 {
                PdfJobStageCode::CertificateValidationOpening
            } else if progress <= 24 {
                PdfJobStageCode::CertificateValidationInspecting
            } else if progress <= 42 {
                PdfJobStageCode::CertificateValidationEngine
            } else if progress <= 58 {
                PdfJobStageCode::CertificateValidationValidating
            } else if progress <= 92 {
                PdfJobStageCode::CertificateValidationReviewing
            } else {
                PdfJobStageCode::CertificateValidationRechecking
            }
        }
        PdfJobKind::Organise | PdfJobKind::PageTransfer => {
            if progress <= 5 {
                PdfJobStageCode::OrganiseChecking
            } else if progress <= 49 {
                PdfJobStageCode::OrganiseOpening
            } else if progress <= 75 {
                PdfJobStageCode::OrganiseArranging
            } else if progress <= 82 {
                PdfJobStageCode::OrganiseFlattening
            } else if progress <= 86 {
                PdfJobStageCode::OrganiseWriting
            } else if progress <= 92 || progress == 98 {
                PdfJobStageCode::OrganiseVerifying
            } else if progress <= 97 {
                PdfJobStageCode::OrganiseProtecting
            } else {
                PdfJobStageCode::OrganisePublishing
            }
        }
        PdfJobKind::Compression => {
            if progress <= 18 {
                PdfJobStageCode::CompressionChecking
            } else if progress <= 77 {
                PdfJobStageCode::CompressionAnalysing
            } else if progress <= 83 {
                PdfJobStageCode::CompressionWriting
            } else if progress <= 88 || (95..=98).contains(&progress) {
                PdfJobStageCode::CompressionVerifying
            } else if progress <= 94 {
                PdfJobStageCode::CompressionProtecting
            } else {
                PdfJobStageCode::CompressionPublishing
            }
        }
        PdfJobKind::CompressionPreview => {
            if progress <= 18 {
                PdfJobStageCode::CompressionPreviewChecking
            } else if progress <= 77 {
                PdfJobStageCode::CompressionPreviewAnalysing
            } else if progress <= 95 {
                PdfJobStageCode::CompressionPreviewEncoding
            } else {
                PdfJobStageCode::CompressionPreviewVerifying
            }
        }
        PdfJobKind::Content => {
            if progress <= 9 {
                PdfJobStageCode::ContentChecking
            } else if progress <= 33 {
                PdfJobStageCode::ContentOpening
            } else if progress <= 65 {
                PdfJobStageCode::ContentPreparing
            } else if progress <= 72 {
                PdfJobStageCode::ContentWriting
            } else if progress <= 79 || (89..=98).contains(&progress) {
                PdfJobStageCode::ContentVerifying
            } else if progress <= 88 {
                PdfJobStageCode::ContentProtecting
            } else {
                PdfJobStageCode::ContentPublishing
            }
        }
        PdfJobKind::ContentInspection => {
            if progress <= 19 {
                PdfJobStageCode::ContentInspectionChecking
            } else if progress <= 31 {
                PdfJobStageCode::ContentInspectionOpening
            } else if progress <= 93 {
                PdfJobStageCode::ContentInspectionInspecting
            } else {
                PdfJobStageCode::ContentInspectionVerifying
            }
        }
        PdfJobKind::Health => {
            if progress <= 6 {
                PdfJobStageCode::HealthChecking
            } else if progress <= 14 {
                PdfJobStageCode::HealthOpening
            } else if progress <= 97 {
                PdfJobStageCode::HealthInspecting
            } else {
                PdfJobStageCode::HealthVerifying
            }
        }
        PdfJobKind::Finishing => {
            if progress <= 7 {
                PdfJobStageCode::FinishingChecking
            } else if progress <= 15 {
                PdfJobStageCode::FinishingOpening
            } else if progress <= 35 {
                PdfJobStageCode::FinishingPreparing
            } else if progress <= 67 {
                PdfJobStageCode::FinishingApplying
            } else if progress <= 73 {
                PdfJobStageCode::FinishingWriting
            } else if progress <= 81 || (89..=98).contains(&progress) {
                PdfJobStageCode::FinishingVerifying
            } else if progress <= 88 {
                PdfJobStageCode::FinishingProtecting
            } else {
                PdfJobStageCode::FinishingPublishing
            }
        }
        PdfJobKind::FinishingInspection => {
            if progress <= 17 {
                PdfJobStageCode::FinishingInspectionChecking
            } else if progress <= 29 {
                PdfJobStageCode::FinishingInspectionOpening
            } else if progress <= 93 {
                PdfJobStageCode::FinishingInspectionInspecting
            } else {
                PdfJobStageCode::FinishingInspectionVerifying
            }
        }
        PdfJobKind::Forms => {
            if progress <= 7 {
                PdfJobStageCode::FormsChecking
            } else if progress <= 21 {
                PdfJobStageCode::FormsOpening
            } else if progress <= 65 {
                PdfJobStageCode::FormsApplying
            } else if progress <= 71 {
                PdfJobStageCode::FormsWriting
            } else if progress <= 77 || (89..=98).contains(&progress) {
                PdfJobStageCode::FormsVerifying
            } else if progress <= 88 {
                PdfJobStageCode::FormsProtecting
            } else {
                PdfJobStageCode::FormsPublishing
            }
        }
        PdfJobKind::FormInspection => {
            if progress <= 17 {
                PdfJobStageCode::FormInspectionChecking
            } else if progress <= 25 {
                PdfJobStageCode::FormInspectionOpening
            } else if progress <= 93 {
                PdfJobStageCode::FormInspectionInspecting
            } else {
                PdfJobStageCode::FormInspectionVerifying
            }
        }
        PdfJobKind::Privacy => {
            if progress <= 11 {
                PdfJobStageCode::PrivacyChecking
            } else if progress <= 23 {
                PdfJobStageCode::PrivacyOpening
            } else if progress <= 78 {
                PdfJobStageCode::PrivacyCleaning
            } else if progress <= 83 {
                PdfJobStageCode::PrivacyWriting
            } else if progress <= 88 || (95..=98).contains(&progress) {
                PdfJobStageCode::PrivacyVerifying
            } else if progress <= 94 {
                PdfJobStageCode::PrivacyProtecting
            } else {
                PdfJobStageCode::PrivacyPublishing
            }
        }
        PdfJobKind::PrivacyInspection => {
            if progress <= 6 {
                PdfJobStageCode::PrivacyInspectionChecking
            } else if progress <= 14 {
                PdfJobStageCode::PrivacyInspectionOpening
            } else if progress <= 94 {
                PdfJobStageCode::PrivacyInspectionInspecting
            } else if progress <= 97 {
                PdfJobStageCode::PrivacyInspectionReporting
            } else {
                PdfJobStageCode::PrivacyInspectionVerifying
            }
        }
        PdfJobKind::Redaction => {
            if progress <= 6 {
                PdfJobStageCode::RedactionChecking
            } else if progress <= 13 {
                PdfJobStageCode::RedactionOpening
            } else if progress <= 72 {
                PdfJobStageCode::RedactionApplying
            } else if progress <= 79 {
                PdfJobStageCode::RedactionCleaning
            } else if progress <= 82 {
                PdfJobStageCode::RedactionWriting
            } else if progress <= 89 || (95..=98).contains(&progress) {
                PdfJobStageCode::RedactionVerifying
            } else if progress <= 94 {
                PdfJobStageCode::RedactionProtecting
            } else {
                PdfJobStageCode::RedactionPublishing
            }
        }
        PdfJobKind::RedactionInspection => {
            if progress <= 17 {
                PdfJobStageCode::RedactionInspectionChecking
            } else if progress <= 29 {
                PdfJobStageCode::RedactionInspectionOpening
            } else if progress <= 93 {
                PdfJobStageCode::RedactionInspectionInspecting
            } else {
                PdfJobStageCode::RedactionInspectionVerifying
            }
        }
        PdfJobKind::Protection => {
            if progress <= 17 {
                PdfJobStageCode::ProtectionChecking
            } else if progress <= 24 {
                PdfJobStageCode::ProtectionPreparing
            } else if progress <= 77 {
                PdfJobStageCode::ProtectionApplying
            } else if progress <= 98 {
                PdfJobStageCode::ProtectionVerifying
            } else {
                PdfJobStageCode::ProtectionPublishing
            }
        }
        PdfJobKind::Merge => {
            if progress <= 6 {
                PdfJobStageCode::MergeChecking
            } else if normalised.contains("protect") || normalised.contains("aes-256") {
                PdfJobStageCode::MergeProtecting
            } else if progress <= 84 {
                PdfJobStageCode::MergePreparing
            } else if progress <= 98 {
                PdfJobStageCode::MergeVerifying
            } else {
                PdfJobStageCode::MergePublishing
            }
        }
        PdfJobKind::Split => {
            if progress <= 11 {
                PdfJobStageCode::SplitChecking
            } else if normalised.contains("protect")
                || normalised.contains("aes-256")
                || progress == 92
            {
                PdfJobStageCode::SplitProtecting
            } else if progress <= 91 {
                PdfJobStageCode::SplitPreparing
            } else if progress <= 94 {
                PdfJobStageCode::SplitVerifying
            } else {
                PdfJobStageCode::SplitPublishing
            }
        }
        PdfJobKind::OcrReview => {
            if progress <= 10 {
                PdfJobStageCode::OcrReviewChecking
            } else if progress <= 59 {
                PdfJobStageCode::OcrReviewPreparing
            } else if progress <= 93 {
                PdfJobStageCode::OcrReviewRecognising
            } else {
                PdfJobStageCode::OcrReviewVerifying
            }
        }
        PdfJobKind::SearchableOcr => {
            if progress <= 5 {
                PdfJobStageCode::SearchableOcrChecking
            } else if normalised.contains("publish") {
                PdfJobStageCode::SearchableOcrPublishing
            } else if normalised.contains("recognising text")
                || normalised.contains("local ocr")
                || normalised.contains("ocrmypdf")
            {
                PdfJobStageCode::SearchableOcrRecognising
            } else if normalised.contains("verif")
                || normalised.contains("recheck")
                || normalised.contains("searchable text")
            {
                PdfJobStageCode::SearchableOcrVerifying
            } else {
                PdfJobStageCode::SearchableOcrPreparing
            }
        }
        PdfJobKind::Scan => {
            if progress <= 9 {
                PdfJobStageCode::ScanChecking
            } else if progress <= 65 {
                PdfJobStageCode::ScanPreparing
            } else if progress <= 75 {
                PdfJobStageCode::ScanWriting
            } else if progress <= 91 {
                PdfJobStageCode::ScanRecognising
            } else if progress <= 97 {
                PdfJobStageCode::ScanProtecting
            } else {
                PdfJobStageCode::ScanPublishing
            }
        }
        PdfJobKind::ScanPreview => {
            if progress <= 10 {
                PdfJobStageCode::ScanPreviewChecking
            } else if progress <= 74 {
                PdfJobStageCode::ScanPreviewPreparing
            } else if progress <= 95 {
                PdfJobStageCode::ScanPreviewEncoding
            } else {
                PdfJobStageCode::ScanPreviewVerifying
            }
        }
        PdfJobKind::ScannerCapture => {
            if progress <= 6 {
                PdfJobStageCode::ScannerCaptureChecking
            } else if progress <= 23 {
                PdfJobStageCode::ScannerCaptureConnecting
            } else if progress <= 87 {
                PdfJobStageCode::ScannerCaptureCapturing
            } else if progress <= 98 {
                PdfJobStageCode::ScannerCaptureVerifying
            } else {
                PdfJobStageCode::ScannerCaptureFinalising
            }
        }
        _ => return None,
    })
}

fn error_code_for(kind: PdfJobKind, error: &str) -> PdfJobErrorCode {
    let normalised = error.to_ascii_lowercase();
    if normalised.contains("changed")
        && (normalised.contains("source")
            || normalised.contains("image")
            || normalised.contains("pdf"))
    {
        return PdfJobErrorCode::SourceChanged;
    }
    if normalised.contains("password") || normalised.contains("decrypt") {
        return PdfJobErrorCode::PasswordRejected;
    }
    if kind == PdfJobKind::Certificate {
        return PdfJobErrorCode::CertificateFailed;
    }
    if kind == PdfJobKind::CertificateValidation {
        return PdfJobErrorCode::CertificateValidationFailed;
    }
    if normalised.contains("certificate") || normalised.contains("signed") {
        return PdfJobErrorCode::CertificateAcknowledgementRequired;
    }
    if normalised.contains("qpdf")
        || normalised.contains("aes-256")
        || normalised.contains("protected output")
    {
        return PdfJobErrorCode::ProtectionUnavailable;
    }
    if normalised.contains("512 mb") || normalised.contains("safety limit") {
        return PdfJobErrorCode::SafetyLimit;
    }
    if normalised.contains("ocrmypdf")
        || normalised.contains("tesseract")
        || normalised.contains("ocr engine")
        || normalised.contains("language pack")
    {
        return PdfJobErrorCode::OcrEngineUnavailable;
    }
    match kind {
        PdfJobKind::Annotations => PdfJobErrorCode::AnnotationsFailed,
        PdfJobKind::AnnotationInspection => PdfJobErrorCode::AnnotationInspectionFailed,
        PdfJobKind::Archive => PdfJobErrorCode::ArchiveFailed,
        PdfJobKind::Batch => PdfJobErrorCode::BatchFailed,
        PdfJobKind::BatchInspection => PdfJobErrorCode::BatchInspectionFailed,
        PdfJobKind::Bookmarks => PdfJobErrorCode::BookmarksFailed,
        PdfJobKind::BookmarkInspection => PdfJobErrorCode::BookmarkInspectionFailed,
        PdfJobKind::Content => PdfJobErrorCode::ContentFailed,
        PdfJobKind::ContentInspection => PdfJobErrorCode::ContentInspectionFailed,
        PdfJobKind::Finishing => PdfJobErrorCode::FinishingFailed,
        PdfJobKind::FinishingInspection => PdfJobErrorCode::FinishingInspectionFailed,
        PdfJobKind::Forms => PdfJobErrorCode::FormsFailed,
        PdfJobKind::FormInspection => PdfJobErrorCode::FormInspectionFailed,
        PdfJobKind::Health => PdfJobErrorCode::HealthFailed,
        PdfJobKind::Merge => PdfJobErrorCode::MergeFailed,
        PdfJobKind::OcrReview => PdfJobErrorCode::OcrReviewFailed,
        PdfJobKind::Privacy => PdfJobErrorCode::PrivacyFailed,
        PdfJobKind::PrivacyInspection => PdfJobErrorCode::PrivacyInspectionFailed,
        PdfJobKind::Redaction => PdfJobErrorCode::RedactionFailed,
        PdfJobKind::RedactionInspection => PdfJobErrorCode::RedactionInspectionFailed,
        PdfJobKind::SearchableOcr => PdfJobErrorCode::SearchableOcrFailed,
        PdfJobKind::Scan => PdfJobErrorCode::ScanFailed,
        PdfJobKind::ScanPreview => PdfJobErrorCode::ScanPreviewFailed,
        PdfJobKind::ScannerCapture => PdfJobErrorCode::ScannerCaptureFailed,
        _ => PdfJobErrorCode::JobFailed,
    }
}

fn take_next_job(store: &mut PdfJobStore) -> Option<(String, StartPdfJobRequest, Arc<AtomicBool>)> {
    while let Some(job_id) = store.pending_order.pop_front() {
        let Some(request) = store.pending_requests.remove(&job_id) else {
            continue;
        };
        let Some(record) = store.jobs.get_mut(&job_id) else {
            continue;
        };
        if record.snapshot.status != PdfJobStatus::Queued
            || record.cancelled.load(Ordering::Acquire)
        {
            continue;
        }
        record.snapshot.status = PdfJobStatus::Running;
        record.snapshot.progress = 1;
        record.snapshot.stage_code = Some(PdfJobStageCode::Starting);
        record.snapshot.stage = match record.snapshot.kind {
            PdfJobKind::AnnotationInspection => "Starting annotation review",
            PdfJobKind::BatchInspection => "Starting batch source review",
            PdfJobKind::BookmarkInspection => "Starting bookmark review",
            PdfJobKind::FinishingInspection => "Starting Page Finish review",
            PdfJobKind::FormInspection => "Starting form review",
            PdfJobKind::Archive => "Starting PDF standards workflow",
            PdfJobKind::CertificateValidation => "Starting certificate validation",
            PdfJobKind::CompressionPreview => "Starting compression preview",
            PdfJobKind::ContentInspection => "Starting page-content review",
            PdfJobKind::EditSafetyInspection => "Starting edit-safety inspection",
            PdfJobKind::Health => "Starting document health check",
            PdfJobKind::OcrReview => "Starting OCR confidence review",
            PdfJobKind::SearchableOcr => "Starting searchable OCR",
            PdfJobKind::PageImportInspection => "Starting page import review",
            PdfJobKind::PrivacyInspection => "Starting privacy inspection",
            PdfJobKind::RedactionInspection => "Starting redaction review",
            PdfJobKind::ScanPreview => "Starting scan clean-up preview",
            PdfJobKind::ScannerCapture => "Starting connected-scanner capture",
            _ => "Starting PDF job",
        }
        .to_string();
        record.snapshot.updated_at_ms = timestamp_ms();
        store.running += 1;
        return Some((job_id, request, Arc::clone(&record.cancelled)));
    }
    None
}

fn prune_terminal_jobs(store: &mut PdfJobStore) {
    while store.jobs.len() >= MAX_RETAINED_PDF_JOBS {
        let Some(position) = store.order.iter().position(|job_id| {
            store
                .jobs
                .get(job_id)
                .is_some_and(|record| is_terminal(record.snapshot.status))
        }) else {
            break;
        };
        if let Some(job_id) = store.order.remove(position) {
            store.jobs.remove(&job_id);
            store.pending_requests.remove(&job_id);
            store.pending_order.retain(|queued_id| queued_id != &job_id);
        }
    }
}

fn is_terminal(status: PdfJobStatus) -> bool {
    matches!(
        status,
        PdfJobStatus::Succeeded | PdfJobStatus::Failed | PdfJobStatus::Cancelled
    )
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{PdfArchiveMode, PdfConformanceProfile};
    use crate::health::InspectPdfEditSafetyRequest;
    use crate::job_recovery::JobRecoveryStore;
    use crate::operation_audit::{OperationAudit, OperationAuditOutcome};
    use crate::privacy::{CleanPdfPrivacyRequest, PrivacyCleanOptions};
    use crate::scan_export::test_scan_pdf_request;
    use crate::test_support::create_unique_test_directory;
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use lopdf::{dictionary, Document, Object, Stream};
    use std::fs;
    use std::io::Cursor;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    fn scan_job_with_ocr(recognise_text: bool) -> StartPdfJobRequest {
        serde_json::from_value(serde_json::json!({
            "kind": "scan",
            "request": {
                "inputPaths": ["scan.png"],
                "outputPath": "scan.pdf",
                "paperWidthPt": 595.0,
                "paperHeightPt": 842.0,
                "marginPt": 18.0,
                "dpi": 300,
                "jpegQuality": 88,
                "colourMode": "colour",
                "autoOrient": true,
                "autoCrop": true,
                "correctPerspective": true,
                "removeShadows": false,
                "recogniseText": recognise_text,
                "straighten": recognise_text,
                "ocrLanguage": "eng",
                "ocrUserWords": [],
                "outputProtection": null
            }
        }))
        .unwrap()
    }

    #[test]
    fn mobile_policy_keeps_plain_image_pdf_jobs_but_marks_ocr_as_desktop_only() {
        assert!(!scan_job_with_ocr(false).requires_external_processes());
        assert!(scan_job_with_ocr(true).requires_external_processes());
    }

    #[test]
    fn localisable_job_codes_have_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::SearchableOcrRecognising).unwrap(),
            "\"searchable-ocr-recognising\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::OrganiseArranging).unwrap(),
            "\"organise-arranging\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::CompressionPreviewAnalysing).unwrap(),
            "\"compression-preview-analysing\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::ScannerCaptureFailed).unwrap(),
            "\"scanner-capture-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::PrivacyInspectionReporting).unwrap(),
            "\"privacy-inspection-reporting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::HealthFailed).unwrap(),
            "\"health-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::MergeFailed).unwrap(),
            "\"merge-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::FinishingProtecting).unwrap(),
            "\"finishing-protecting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::FinishingInspectionFailed).unwrap(),
            "\"finishing-inspection-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::AnnotationsProtecting).unwrap(),
            "\"annotations-protecting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::AnnotationInspectionFailed).unwrap(),
            "\"annotation-inspection-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::FormsApplying).unwrap(),
            "\"forms-applying\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::FormInspectionFailed).unwrap(),
            "\"form-inspection-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::ContentInspectionInspecting).unwrap(),
            "\"content-inspection-inspecting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::ContentFailed).unwrap(),
            "\"content-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::RedactionProtecting).unwrap(),
            "\"redaction-protecting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::RedactionInspectionFailed).unwrap(),
            "\"redaction-inspection-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::ArchivePreflighting).unwrap(),
            "\"archive-preflighting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::ArchiveFailed).unwrap(),
            "\"archive-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::BatchCompressing).unwrap(),
            "\"batch-compressing\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::BatchInspectionInspecting).unwrap(),
            "\"batch-inspection-inspecting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::BatchInspectionFailed).unwrap(),
            "\"batch-inspection-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::BookmarksPreparingContents).unwrap(),
            "\"bookmarks-preparing-contents\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::BookmarkInspectionInspecting).unwrap(),
            "\"bookmark-inspection-inspecting\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::BookmarksFailed).unwrap(),
            "\"bookmarks-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::BookmarkInspectionFailed).unwrap(),
            "\"bookmark-inspection-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::CertificateSigning).unwrap(),
            "\"certificate-signing\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobStageCode::CertificateValidationReviewing).unwrap(),
            "\"certificate-validation-reviewing\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::CertificateFailed).unwrap(),
            "\"certificate-failed\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobErrorCode::CertificateValidationFailed).unwrap(),
            "\"certificate-validation-failed\""
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::OcrReview, 72, "Recognising words"),
            Some(PdfJobStageCode::OcrReviewRecognising)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Organise, 65, "Copying planned page 4"),
            Some(PdfJobStageCode::OrganiseArranging)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Organise, 94, "Applying document password"),
            Some(PdfJobStageCode::OrganiseProtecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Organise, 99, "Publishing verified PDF"),
            Some(PdfJobStageCode::OrganisePublishing)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::SearchableOcr, 80, "Verifying searchable text"),
            Some(PdfJobStageCode::SearchableOcrVerifying)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Scan, 93, "Protecting output"),
            Some(PdfJobStageCode::ScanProtecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::ScanPreview, 82, "Encoding preview"),
            Some(PdfJobStageCode::ScanPreviewEncoding)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::ScannerCapture, 54, "Receiving scanner page"),
            Some(PdfJobStageCode::ScannerCaptureCapturing)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Compression, 90, "Applying AES-256"),
            Some(PdfJobStageCode::CompressionProtecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::CompressionPreview,
                84,
                "Encoding compressed image sample"
            ),
            Some(PdfJobStageCode::CompressionPreviewEncoding)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Health, 48, "Inspecting private source details"),
            Some(PdfJobStageCode::HealthInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Finishing, 84, "Applying private protection"),
            Some(PdfJobStageCode::FinishingProtecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::FinishingInspection,
                56,
                "Inspecting private page"
            ),
            Some(PdfJobStageCode::FinishingInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Privacy, 90, "Applying private password"),
            Some(PdfJobStageCode::PrivacyProtecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::PrivacyInspection,
                96,
                "Preparing private report"
            ),
            Some(PdfJobStageCode::PrivacyInspectionReporting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::Annotations,
                82,
                "Applying private output password"
            ),
            Some(PdfJobStageCode::AnnotationsProtecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::AnnotationInspection,
                61,
                "Inspecting private annotation details"
            ),
            Some(PdfJobStageCode::AnnotationInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Forms, 52, "Applying private field values"),
            Some(PdfJobStageCode::FormsApplying)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::FormInspection,
                70,
                "Inspecting private field details"
            ),
            Some(PdfJobStageCode::FormInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Content, 82, "Applying private password"),
            Some(PdfJobStageCode::ContentProtecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::ContentInspection,
                64,
                "Reviewing private page content"
            ),
            Some(PdfJobStageCode::ContentInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Redaction, 92, "Applying private password"),
            Some(PdfJobStageCode::RedactionProtecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::RedactionInspection,
                64,
                "Inspecting private redaction geometry"
            ),
            Some(PdfJobStageCode::RedactionInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Archive, 52, "PDF/X structural preflight"),
            Some(PdfJobStageCode::ArchivePreflighting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Archive, 84, "veraPDF validation"),
            Some(PdfJobStageCode::ArchiveValidating)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Batch, 58, "File 2 of 3, compression"),
            Some(PdfJobStageCode::BatchCompressing)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::BatchInspection,
                48,
                "PDF 2 of 4, inspecting private structures"
            ),
            Some(PdfJobStageCode::BatchInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::Bookmarks,
                41,
                "Building printed contents page 2 of 4"
            ),
            Some(PdfJobStageCode::BookmarksPreparingContents)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Bookmarks, 82, "Applying AES-256 protection"),
            Some(PdfJobStageCode::BookmarksProtecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::BookmarkInspection, 61, "Inspecting bookmark 12"),
            Some(PdfJobStageCode::BookmarkInspectionInspecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Certificate, 35, "Applying private signature"),
            Some(PdfJobStageCode::CertificateSigning)
        );
        assert_eq!(
            stage_code_for_progress(
                PdfJobKind::CertificateValidation,
                92,
                "Reviewing private validation output"
            ),
            Some(PdfJobStageCode::CertificateValidationReviewing)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Protection, 78, "Reopening encrypted output"),
            Some(PdfJobStageCode::ProtectionVerifying)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Split, 82, "Applying AES-256"),
            Some(PdfJobStageCode::SplitProtecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Split, 95, "Publishing verified split PDFs"),
            Some(PdfJobStageCode::SplitPublishing)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Merge, 54, "Copying source page 8"),
            Some(PdfJobStageCode::MergePreparing)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Merge, 88, "Applying AES-256 output protection"),
            Some(PdfJobStageCode::MergeProtecting)
        );
        assert_eq!(
            stage_code_for_progress(PdfJobKind::Merge, 99, "Publishing verified combined PDF"),
            Some(PdfJobStageCode::MergePublishing)
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::SearchableOcr,
                "The requested Tesseract language pack is unavailable"
            ),
            PdfJobErrorCode::OcrEngineUnavailable
        );
        assert_eq!(
            error_code_for(PdfJobKind::Scan, "The source image changed before export"),
            PdfJobErrorCode::SourceChanged
        );
        assert_eq!(
            error_code_for(PdfJobKind::ScannerCapture, "Private adapter failure"),
            PdfJobErrorCode::ScannerCaptureFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Health, "Private source path failed"),
            PdfJobErrorCode::HealthFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Merge, "Private merge path failed"),
            PdfJobErrorCode::MergeFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Finishing, "Private page-finishing failure"),
            PdfJobErrorCode::FinishingFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::FinishingInspection,
                "Private finishing inspector failure"
            ),
            PdfJobErrorCode::FinishingInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Privacy, "Private cleaner failure"),
            PdfJobErrorCode::PrivacyFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::PrivacyInspection, "Private inspector failure"),
            PdfJobErrorCode::PrivacyInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Annotations, "Private annotation failure"),
            PdfJobErrorCode::AnnotationsFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::AnnotationInspection,
                "Private annotation inspector failure"
            ),
            PdfJobErrorCode::AnnotationInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Forms, "Private form failure"),
            PdfJobErrorCode::FormsFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::FormInspection, "Private form inspector failure"),
            PdfJobErrorCode::FormInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Content, "Private content rewrite failure"),
            PdfJobErrorCode::ContentFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::ContentInspection,
                "Private content inspector failure"
            ),
            PdfJobErrorCode::ContentInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Redaction, "Private redaction failure"),
            PdfJobErrorCode::RedactionFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::RedactionInspection,
                "Private redaction inspector failure"
            ),
            PdfJobErrorCode::RedactionInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Archive, "Private standards failure"),
            PdfJobErrorCode::ArchiveFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Batch, "Private batch failure"),
            PdfJobErrorCode::BatchFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::BatchInspection, "Private source review failure"),
            PdfJobErrorCode::BatchInspectionFailed
        );
        assert_eq!(
            error_code_for(PdfJobKind::Bookmarks, "Private bookmark export failure"),
            PdfJobErrorCode::BookmarksFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::BookmarkInspection,
                "Private bookmark review failure"
            ),
            PdfJobErrorCode::BookmarkInspectionFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::Certificate,
                "Private certificate engine failure"
            ),
            PdfJobErrorCode::CertificateFailed
        );
        assert_eq!(
            error_code_for(
                PdfJobKind::CertificateValidation,
                "Private certificate validation failure"
            ),
            PdfJobErrorCode::CertificateValidationFailed
        );
    }

    #[test]
    fn searchable_ocr_uses_an_explicit_strict_scheduler_request() {
        let request: StartPdfJobRequest = serde_json::from_value(serde_json::json!({
            "kind": "searchable-ocr",
            "request": {
                "inputPath": "private-ocr-source.pdf",
                "outputPath": "private-ocr-output.pdf",
                "inputPassword": "private-ocr-password",
                "language": "eng+tur",
                "straighten": true,
                "acknowledgeCertificateSignatures": false,
                "outputProtection": null
            }
        }))
        .unwrap();
        assert_eq!(request.kind(), PdfJobKind::SearchableOcr);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::SearchableOcr).unwrap(),
            "\"searchable-ocr\""
        );

        let unknown = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "searchable-ocr",
            "request": {
                "inputPath": "source.pdf",
                "outputPath": "output.pdf",
                "inputPassword": null,
                "language": "eng",
                "straighten": false,
                "acknowledgeCertificateSignatures": false,
                "outputProtection": null,
                "recognisedDocumentText": "must never be accepted"
            }
        }));
        assert!(unknown.is_err());
    }

    #[test]
    fn bookmark_contents_use_an_explicit_strict_scheduler_request() {
        let request: StartPdfJobRequest = serde_json::from_value(serde_json::json!({
            "kind": "bookmarks",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "bookmarks": [{
                    "bold": false,
                    "colour": [0.0, 0.0, 0.0],
                    "italic": false,
                    "level": 0,
                    "open": true,
                    "pageNumber": 1,
                    "title": "Introduction"
                }],
                "expectedSourceModifiedAtMs": null,
                "expectedSourceSize": 42,
                "inputPassword": null,
                "inputPath": "private-bookmark-source.pdf",
                "outputPath": "private-bookmark-output.pdf",
                "outputProtection": null,
                "printedContents": {
                    "addBookmark": true,
                    "maximumLevel": 2,
                    "title": "Contents"
                }
            }
        }))
        .unwrap();
        assert_eq!(request.kind(), PdfJobKind::Bookmarks);

        let unknown = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "bookmarks",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "bookmarks": [],
                "expectedSourceModifiedAtMs": null,
                "expectedSourceSize": 42,
                "inputPassword": null,
                "inputPath": "source.pdf",
                "outputPath": "output.pdf",
                "outputProtection": null,
                "printedContents": {
                    "addBookmark": true,
                    "maximumLevel": 2,
                    "title": "Contents",
                    "privateTemplate": "must never be accepted"
                }
            }
        }));
        assert!(unknown.is_err());
    }

    #[test]
    fn queued_searchable_ocr_snapshot_excludes_paths_and_passwords() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-ocr-source.pdf");
        let output = directory.path.join("private-ocr-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(searchable_ocr_request(&input, &output, true))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::SearchableOcr);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            "private-ocr-source.pdf",
            "private-ocr-output.pdf",
            "private-ocr-input-password",
            "private-ocr-opening-password",
            "private-ocr-owner-password",
        ] {
            assert!(!serialised.contains(secret));
        }
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("outputPath"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
    }

    #[test]
    fn failed_searchable_ocr_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-ocr-source.pdf");
        let output = directory.path.join("confidential-ocr-output.pdf");
        fs::write(&input, b"private malformed OCR source bytes").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(searchable_ocr_request(&input, &output, false))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Searchable OCR failed a bounded PDF structure or publication check. Review the source and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-ocr-source.pdf"));
        assert!(!serialised.contains("confidential-ocr-output.pdf"));
        assert!(!serialised.contains("private malformed OCR source bytes"));
        assert!(!output.exists());
    }

    #[test]
    fn content_job_variants_use_strict_typed_scheduler_requests() {
        let inspection: StartPdfJobRequest = serde_json::from_value(serde_json::json!({
            "kind": "content-inspection",
            "request": {
                "inputPath": "private-content-source.pdf",
                "inputPassword": "private-content-password"
            }
        }))
        .unwrap();
        assert_eq!(inspection.kind(), PdfJobKind::ContentInspection);

        let publication: StartPdfJobRequest = serde_json::from_value(serde_json::json!({
            "kind": "content",
            "request": {
                "inputPath": "private-content-source.pdf",
                "outputPath": "private-content-output.pdf",
                "inputPassword": "private-content-password",
                "outputProtection": null,
                "acknowledgeCertificateSignatures": false,
                "expectedSourceSize": 42,
                "expectedSourceModifiedAtMs": null,
                "expectedSourceSha256": "a".repeat(64),
                "textEdits": [{
                    "sourceId": format!("text-{}", "b".repeat(64)),
                    "replacementText": "private replacement text"
                }],
                "imageEdits": []
            }
        }))
        .unwrap();
        assert_eq!(publication.kind(), PdfJobKind::Content);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::Content).unwrap(),
            "\"content\""
        );
        assert_eq!(
            serde_json::to_string(&PdfJobKind::ContentInspection).unwrap(),
            "\"content-inspection\""
        );

        let unknown = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "content-inspection",
            "request": {
                "inputPath": "source.pdf",
                "inputPassword": null,
                "unreviewedField": true
            }
        }));
        assert!(unknown.is_err());
    }

    #[test]
    fn content_inspection_recovery_is_secret_free() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("content-inspection-recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::ContentInspection(
                InspectPdfContentRequest {
                    input_path: "private-content-inspection-recovery.pdf".to_string(),
                    input_password: Some("private-content-recovery-password".to_string()),
                },
            ))
            .unwrap();
        assert_eq!(started.kind, PdfJobKind::ContentInspection);
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::ContentInspection)).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0]
            .job_id
            .starts_with("interrupted-content-inspection-"));
        let serialised = serde_json::to_string(&snapshots).unwrap();
        assert!(!serialised.contains("private-content-inspection-recovery.pdf"));
        assert!(!serialised.contains("private-content-recovery-password"));
    }

    #[test]
    fn content_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("content-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::ContentInspection(
                InspectPdfContentRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::ContentInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "content-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["editableTextCount"], 0);
        assert_eq!(value["result"]["readOnlyTextCount"], 1);
    }

    #[test]
    fn queued_content_inspection_snapshot_excludes_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-content-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "content-review-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::ContentInspection(
                InspectPdfContentRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::ContentInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-content-review.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_content_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-content-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::ContentInspection(
                InspectPdfContentRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The page-content review failed a bounded structural safety check. The PDF was not changed."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-content-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn queued_content_snapshot_excludes_paths_passwords_text_and_image_bytes() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-content-source.pdf");
        let output = directory.path.join("private-content-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let private_text = "Private replacement text 47291";
        let private_image = "private-content-pixels-never-serialise";
        let request = content_request(&input, &output, private_text, private_image);
        let started = manager.start(request).unwrap();

        assert_eq!(started.kind, PdfJobKind::Content);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            private_text,
            private_image,
            "private-content-input-password",
            "private-content-opening-password",
            "private-content-owner-password",
            "private-content-source.pdf",
            "private-content-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_content_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-content-source.pdf");
        let output = directory.path.join("confidential-content-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let private_text = "Private replacement text 88412";
        let private_image = "confidential-content-image-pixels";
        let started = manager
            .start(content_request(
                &input,
                &output,
                private_text,
                private_image,
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            private_text,
            private_image,
            "private-content-input-password",
            "private-content-opening-password",
            "private-content-owner-password",
            "confidential-content-source.pdf",
            "confidential-content-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The source PDF changed after review. Review its page content again before exporting."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn restart_restores_a_secret_free_interrupted_terminal_snapshot() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("confidential-source.pdf");
        let output = directory.path.join("confidential-destination.pdf");
        save_fixture(&input);
        let secret = "restart-password-must-not-survive";
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &output,
                Some(secret.to_string()),
                None,
            )))
            .unwrap();
        assert_eq!(started.status, PdfJobStatus::Queued);
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        assert_eq!(recovered.len(), 1);
        let audit = OperationAudit::in_memory();
        let mut restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        restarted.audit = Some(audit.clone());
        let snapshots = restarted.list(Some(PdfJobKind::Privacy)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.progress, 0);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert_eq!(interrupted.error.as_deref(), Some(INTERRUPTED_JOB_ERROR));
        assert!(interrupted.job_id.starts_with("interrupted-privacy-"));
        assert!(interrupted.result.is_none());
        assert!(restarted.get(&started.job_id).is_err());
        assert_eq!(audit.report().unwrap().total_entries, 0);

        let serialised = serde_json::to_string(interrupted).unwrap();
        for forbidden in [
            secret,
            "confidential-source.pdf",
            "confidential-destination.pdf",
            "sourcePath",
            "outputPath",
            "password",
            "result",
        ] {
            if forbidden == "result" {
                assert!(serialised.contains(r#""result":null"#));
            } else {
                assert!(!serialised.contains(forbidden));
            }
        }
    }

    #[test]
    fn restart_restores_an_interrupted_batch_inspection_without_its_requests() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let first = directory.path.join("private-batch-inspection-first.pdf");
        let second = directory.path.join("private-batch-inspection-second.pdf");
        let first_password = "batch-inspection-first-recovery-password";
        let second_password = "batch-inspection-second-recovery-password";
        save_fixture(&first);
        save_fixture(&second);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::BatchInspection(
                InspectBatchSourcesRequest {
                    sources: vec![
                        InspectPdfPrivacyRequest {
                            input_path: first.to_string_lossy().into_owned(),
                            input_password: Some(first_password.to_string()),
                        },
                        InspectPdfPrivacyRequest {
                            input_path: second.to_string_lossy().into_owned(),
                            input_password: Some(second_password.to_string()),
                        },
                    ],
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::BatchInspection)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-batch-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            first_password,
            second_password,
            "private-batch-inspection-first.pdf",
            "private-batch-inspection-second.pdf",
            "inputPath",
            "inputPassword",
            "sources",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_edit_safety_inspection_without_its_requests() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let first = directory.path.join("private-edit-safety-first.pdf");
        let second = directory.path.join("private-edit-safety-second.pdf");
        let first_password = "edit-safety-first-recovery-password";
        let second_password = "edit-safety-second-recovery-password";
        save_fixture(&first);
        save_fixture(&second);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::EditSafetyInspection(
                InspectPdfEditSafetySourcesRequest {
                    sources: vec![
                        InspectPdfEditSafetyRequest {
                            input_path: first.to_string_lossy().into_owned(),
                            input_password: Some(first_password.to_string()),
                        },
                        InspectPdfEditSafetyRequest {
                            input_path: second.to_string_lossy().into_owned(),
                            input_password: Some(second_password.to_string()),
                        },
                    ],
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::EditSafetyInspection))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-edit-safety-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            first_password,
            second_password,
            "private-edit-safety-first.pdf",
            "private-edit-safety-second.pdf",
            "inputPath",
            "inputPassword",
            "sources",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_health_check_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("private-health-recovery.pdf");
        let password = "health-recovery-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::Health(InspectPdfHealthRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: Some(password.to_string()),
            }))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::Health)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted.job_id.starts_with("interrupted-health-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-health-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
    }

    #[test]
    fn restart_restores_an_interrupted_annotation_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-annotation-inspection-recovery.pdf");
        let password = "annotation-inspection-recovery-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::AnnotationInspection(
                InspectPdfAnnotationsRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::AnnotationInspection))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-annotation-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-annotation-inspection-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
    }

    #[test]
    fn restart_restores_an_interrupted_bookmark_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-bookmark-inspection-recovery.pdf");
        let password = "bookmark-inspection-recovery-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::BookmarkInspection(
                InspectPdfBookmarksRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::BookmarkInspection))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-bookmark-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-bookmark-inspection-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
    }

    #[test]
    fn restart_restores_an_interrupted_form_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("private-form-inspection-recovery.pdf");
        let password = "form-inspection-recovery-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::FormInspection(InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: Some(password.to_string()),
            }))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::FormInspection)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-form-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-form-inspection-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
    }

    #[test]
    fn restart_restores_an_interrupted_finishing_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-finishing-inspection-recovery.pdf");
        let password = "finishing-inspection-recovery-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::FinishingInspection(
                InspectPdfFinishingRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::FinishingInspection))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-finishing-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-finishing-inspection-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
    }

    #[test]
    fn restart_restores_an_interrupted_page_import_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-page-import-inspection-recovery.pdf");
        let password = "page-import-inspection-recovery-password";
        let range = "1, 1";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::PageImportInspection(
                InspectPageImportRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    page_range: range.to_string(),
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::PageImportInspection))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-page-import-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains(range));
        assert!(!serialised.contains("private-page-import-inspection-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        assert!(!serialised.contains("pageRange"));
    }

    #[test]
    fn restart_restores_an_interrupted_redaction_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-redaction-inspection-recovery.pdf");
        let password = "redaction-inspection-recovery-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::RedactionInspection(
                InspectPdfRedactionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::RedactionInspection))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-redaction-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-redaction-inspection-recovery.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
    }

    #[test]
    fn restart_restores_an_interrupted_certificate_validation_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("private-validation-recovery.pdf");
        let trust_root = directory.path.join("private-validation-root.pem");
        let password = "private-validation-recovery-password";
        save_fixture(&input);
        fs::write(&trust_root, b"private test trust root").unwrap();
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::CertificateValidation(
                InspectCertificateRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    trust_roots: vec![trust_root.to_string_lossy().into_owned()],
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::CertificateValidation))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-certificate-validation-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            password,
            "private-validation-recovery.pdf",
            "private-validation-root.pem",
            "inputPath",
            "inputPassword",
            "trustRoots",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_compression_preview_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-compression-preview-recovery.pdf");
        let password = "private-compression-preview-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::CompressionPreview(
                PreviewPdfCompressionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    jpeg_quality: 72,
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted
            .list(Some(PdfJobKind::CompressionPreview))
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-compression-preview-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            password,
            "private-compression-preview-recovery.pdf",
            "inputPath",
            "inputPassword",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_privacy_inspection_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory
            .path
            .join("private-privacy-inspection-recovery.pdf");
        let password = "private-privacy-inspection-password";
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager
            .start(StartPdfJobRequest::PrivacyInspection(
                InspectPdfPrivacyRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::PrivacyInspection)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-privacy-inspection-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            password,
            "private-privacy-inspection-recovery.pdf",
            "inputPath",
            "inputPassword",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_ocr_review_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("private-ocr-review-recovery.png");
        RgbImage::from_pixel(40, 30, Rgb([250, 250, 250]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager.start(ocr_review_request(&input)).unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::OcrReview)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted.job_id.starts_with("interrupted-ocr-review-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            "private-ocr-review-recovery.png",
            "inputPath",
            "colourMode",
            "language",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_scan_preview_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("private-scan-preview-recovery.png");
        RgbImage::from_pixel(40, 30, Rgb([250, 250, 250]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager.start(scan_preview_request(&input)).unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::ScanPreview)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted.job_id.starts_with("interrupted-scan-preview-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            "private-scan-preview-recovery.png",
            "inputPath",
            "colourMode",
            "autoCrop",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn restart_restores_an_interrupted_scanner_capture_without_its_request() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let started = manager.start(scanner_capture_request()).unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let restarted = PdfJobManager::with_test_recovery(0, recovery, recovered);
        let snapshots = restarted.list(Some(PdfJobKind::ScannerCapture)).unwrap();
        assert_eq!(snapshots.len(), 1);
        let interrupted = &snapshots[0];
        assert_eq!(interrupted.status, PdfJobStatus::Failed);
        assert_eq!(interrupted.stage, INTERRUPTED_JOB_STAGE);
        assert!(interrupted
            .job_id
            .starts_with("interrupted-scanner-capture-"));
        assert!(restarted.get(&started.job_id).is_err());
        let serialised = serde_json::to_string(interrupted).unwrap();
        for secret in [
            "private:test-scanner",
            "deviceId",
            "paperWidthMm",
            "pageLimit",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn cancellation_failure_and_success_retire_recovery_entries() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);

        let cancelled = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &directory.path.join("cancelled.pdf"),
                None,
                None,
            )))
            .unwrap();
        manager.cancel(&cancelled.job_id).unwrap();

        let failed = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &directory.path.join("failed.pdf"),
                None,
                None,
            )))
            .unwrap();
        manager
            .finish_failed(&failed.job_id, "Safe test failure".to_string())
            .unwrap();
        drop(manager);

        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        assert!(recovered.is_empty());
        let manager = PdfJobManager::with_test_recovery(1, recovery, recovered);
        let succeeded = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &directory.path.join("succeeded.pdf"),
                None,
                None,
            )))
            .unwrap();
        assert_eq!(
            wait_for_terminal(&manager, &succeeded.job_id).status,
            PdfJobStatus::Succeeded
        );
        drop(manager);

        let (_, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        assert!(recovered.is_empty());
    }

    #[test]
    fn job_is_not_accepted_when_its_recovery_marker_cannot_be_created() {
        let directory = TestDirectory::new();
        let recovery_path = directory.path.join("recovery");
        let (recovery, recovered) = JobRecoveryStore::open_directory(&recovery_path).unwrap();
        fs::remove_dir(&recovery_path).unwrap();
        fs::write(&recovery_path, b"not a directory").unwrap();
        let input = directory.path.join("source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_test_recovery(0, recovery, recovered);

        let error = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &directory.path.join("destination.pdf"),
                None,
                None,
            )))
            .unwrap_err();

        assert!(error.contains("recovery lock"));
        assert!(manager.list(None).unwrap().is_empty());
    }

    #[test]
    fn terminal_transitions_write_one_path_free_audit_entry() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-source.pdf");
        save_fixture(&input);
        let audit = OperationAudit::in_memory();
        let mut manager = PdfJobManager::with_max_running(0);
        manager.audit = Some(audit.clone());

        let cancelled_output = directory.path.join("cancelled-output.pdf");
        let cancelled = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &cancelled_output,
                Some("password-never-audited".to_string()),
                None,
            )))
            .unwrap();
        manager.cancel(&cancelled.job_id).unwrap();
        manager.cancel(&cancelled.job_id).unwrap();

        let failed_output = directory.path.join("failed-output.pdf");
        let failed = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &failed_output,
                None,
                None,
            )))
            .unwrap();
        manager
            .finish_failed(&failed.job_id, "private worker error".to_string())
            .unwrap();
        manager
            .finish_failed(&failed.job_id, "second private worker error".to_string())
            .unwrap();

        let report = audit.report().unwrap();
        assert_eq!(report.total_entries, 2);
        assert_eq!(report.entries[0].operation, PdfJobKind::Privacy);
        assert_eq!(report.entries[0].outcome, OperationAuditOutcome::Failed);
        assert_eq!(report.entries[1].outcome, OperationAuditOutcome::Cancelled);
        let encoded = serde_json::to_string(&report).unwrap();
        for forbidden in [
            "private-source.pdf",
            "cancelled-output.pdf",
            "failed-output.pdf",
            "password-never-audited",
            "private worker error",
            "second private worker error",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn privacy_job_reaches_verified_success_and_retains_a_public_result() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("clean.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input, &output, None, None,
            )))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::Privacy);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(completed.result, Some(PdfJobResult::Privacy(_))));
        assert!(completed.error.is_none());
        assert!(output.exists());
        assert_eq!(manager.list(Some(PdfJobKind::Privacy)).unwrap().len(), 1);
        assert!(manager
            .list(Some(PdfJobKind::Compression))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn health_job_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("health-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::Health(InspectPdfHealthRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            }))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::Health);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(completed.result, Some(PdfJobResult::Health(_))));
        assert!(completed.error.is_none());
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(serialised.contains(r#""fileName":"PDF""#));
        assert!(!serialised.contains("health-source.pdf"));
        assert_eq!(manager.list(Some(PdfJobKind::Health)).unwrap().len(), 1);
    }

    #[test]
    fn certificate_validation_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("certificate-validation-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::CertificateValidation(
                InspectCertificateRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some("unused-validation-password".to_string()),
                    trust_roots: Vec::new(),
                },
            ))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::CertificateValidation);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(
            completed.result,
            Some(PdfJobResult::CertificateValidation(_))
        ));
        assert!(completed.error.is_none());
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(serialised.contains(r#""inputPath":"PDF""#));
        assert!(!serialised.contains("certificate-validation-source.pdf"));
        assert!(!serialised.contains("unused-validation-password"));
        assert_eq!(
            manager
                .list(Some(PdfJobKind::CertificateValidation))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn certificate_validation_uses_the_explicit_hyphenated_wire_kind() {
        let request = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "certificate-validation",
            "request": {
                "inputPassword": "wire-password",
                "inputPath": "review.pdf",
                "trustRoots": []
            }
        }))
        .unwrap();

        assert_eq!(request.kind(), PdfJobKind::CertificateValidation);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::CertificateValidation).unwrap(),
            r#""certificate-validation""#
        );
    }

    #[test]
    fn queued_certificate_validation_snapshot_excludes_local_paths() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-validation-source.pdf");
        let trust_root = directory.path.join("private-validation-trust.pem");
        let password = "private-validation-password-never-serialise";
        save_fixture(&input);
        fs::write(&trust_root, b"private trust root bytes").unwrap();
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::CertificateValidation(
                InspectCertificateRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    trust_roots: vec![trust_root.to_string_lossy().into_owned()],
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::CertificateValidation);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            password,
            "private-validation-source.pdf",
            "private-validation-trust.pem",
            "inputPath",
            "inputPassword",
            "trustRoots",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_certificate_validation_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-validation-source.pdf");
        fs::write(
            &input,
            b"%PDF-1.7\nprivate certificate validation document content",
        )
        .unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::CertificateValidation(
                InspectCertificateRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some("failed-validation-password".to_string()),
                    trust_roots: Vec::new(),
                },
            ))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Certificate validation could not complete a bounded integrity and trust review. Review the PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            "failed-validation-password",
            "private certificate validation document content",
            "confidential-validation-source.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn queued_health_snapshot_excludes_the_source_path_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-health-source.pdf");
        save_fixture(&input);
        let password = "health-password-never-serialise";
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::Health(InspectPdfHealthRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: Some(password.to_string()),
            }))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Health);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-health-source.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn queued_archive_snapshot_excludes_paths_and_passwords() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-archive-source.pdf");
        save_fixture(&input);
        let password = "archive-password-never-serialise";
        let output = directory.path.join("private-archive-output.pdf");
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::Archive(PdfArchiveRequest {
                mode: PdfArchiveMode::Convert,
                profile: PdfConformanceProfile::PdfA2b,
                input_path: input.to_string_lossy().into_owned(),
                input_password: Some(password.to_string()),
                output_path: Some(output.to_string_lossy().into_owned()),
                recognise_text: false,
                ocr_language: "eng".to_string(),
                straighten: false,
                acknowledge_certificate_signatures: false,
            }))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Archive);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            password,
            "private-archive-source.pdf",
            "private-archive-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_health_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-health-source.pdf");
        let password = "private-health-input-password";
        fs::write(&input, b"%PDF-1.7\nprivate-health-document-content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::Health(InspectPdfHealthRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: Some(password.to_string()),
            }))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Document Health could not complete a bounded structural check. Review the PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            password,
            "private-health-document-content",
            "confidential-health-source.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn scan_job_uses_the_shared_queue_and_preserves_its_result_shape() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.png");
        let output = directory.path.join("scan.pdf");
        RgbImage::from_pixel(64, 96, Rgb([30, 120, 210]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::Scan(test_scan_pdf_request(
                vec![input.to_string_lossy().into_owned()],
                output.to_string_lossy().into_owned(),
            )))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::Scan);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(completed.result, Some(PdfJobResult::Scan(_))));
        assert!(completed.error.is_none());
        assert!(output.exists());
        assert_eq!(manager.list(Some(PdfJobKind::Scan)).unwrap().len(), 1);
    }

    #[test]
    fn queued_scan_snapshot_excludes_paths_ocr_hints_and_passwords() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-scan-source.png");
        let output = directory.path.join("private-scan-output.pdf");
        RgbImage::from_pixel(64, 96, Rgb([30, 120, 210]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_max_running(0);
        let ocr_hint = "private-scan-recognition-hint";
        let open_password = "private-scan-opening-password";
        let owner_password = "private-scan-owner-password";

        let started = manager
            .start(scan_request(
                &input,
                &output,
                ocr_hint,
                open_password,
                owner_password,
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Scan);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            ocr_hint,
            open_password,
            owner_password,
            "private-scan-source.png",
            "private-scan-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_scan_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-scan-source.png");
        let output = directory.path.join("confidential-scan-output.pdf");
        fs::write(&input, b"private-scan-image-content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let ocr_hint = "private-failed-scan-recognition-hint";
        let open_password = "private-failed-scan-opening-password";
        let owner_password = "private-failed-scan-owner-password";

        let started = manager
            .start(scan_request(
                &input,
                &output,
                ocr_hint,
                open_password,
                owner_password,
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            ocr_hint,
            open_password,
            owner_password,
            "private-scan-image-content",
            "confidential-scan-source.png",
            "confidential-scan-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Scan PDF creation failed a local image or PDF safety check. Review the scan settings and try again."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn batch_inspection_uses_the_shared_queue_and_retains_ordered_typed_results() {
        let directory = TestDirectory::new();
        let input = directory.path.join("batch-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::BatchInspection(
                InspectBatchSourcesRequest {
                    sources: vec![InspectPdfPrivacyRequest {
                        input_path: input.to_string_lossy().into_owned(),
                        input_password: None,
                    }],
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::BatchInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "batch-inspection");
        assert_eq!(value["result"]["sourceCount"], 1);
        assert_eq!(value["result"]["inspectedCount"], 1);
        assert_eq!(value["result"]["failedCount"], 0);
        assert_eq!(value["result"]["items"][0]["sourceIndex"], 0);
        assert_eq!(value["result"]["items"][0]["inspection"]["pageCount"], 1);
    }

    #[test]
    fn queued_batch_inspection_snapshot_excludes_every_source_and_password() {
        let directory = TestDirectory::new();
        let first = directory.path.join("private-batch-review-first.pdf");
        let second = directory.path.join("private-batch-review-second.pdf");
        save_fixture(&first);
        save_fixture(&second);
        let first_password = "first-batch-review-password-never-serialise";
        let second_password = "second-batch-review-password-never-serialise";
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::BatchInspection(
                InspectBatchSourcesRequest {
                    sources: vec![
                        InspectPdfPrivacyRequest {
                            input_path: first.to_string_lossy().into_owned(),
                            input_password: Some(first_password.to_string()),
                        },
                        InspectPdfPrivacyRequest {
                            input_path: second.to_string_lossy().into_owned(),
                            input_password: Some(second_password.to_string()),
                        },
                    ],
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::BatchInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            first_password,
            second_password,
            "private-batch-review-first.pdf",
            "private-batch-review-second.pdf",
            "inputPath",
            "inputPassword",
            "sources",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn batch_inspection_item_failures_are_content_free() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-batch-review.pdf");
        fs::write(&input, b"private malformed batch review content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::BatchInspection(
                InspectBatchSourcesRequest {
                    sources: vec![InspectPdfPrivacyRequest {
                        input_path: input.to_string_lossy().into_owned(),
                        input_password: None,
                    }],
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["result"]["inspectedCount"], 0);
        assert_eq!(value["result"]["failedCount"], 1);
        assert_eq!(
            value["result"]["items"][0]["error"],
            "Privacy Inspection could not complete its bounded structure and page analysis. Review the PDF and try again."
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-batch-review.pdf"));
        assert!(!serialised.contains("private malformed batch review content"));
    }

    #[test]
    fn edit_safety_inspection_uses_the_shared_queue_and_retains_ordered_typed_results() {
        let directory = TestDirectory::new();
        let input = directory.path.join("edit-safety-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::EditSafetyInspection(
                InspectPdfEditSafetySourcesRequest {
                    sources: vec![InspectPdfEditSafetyRequest {
                        input_path: input.to_string_lossy().into_owned(),
                        input_password: None,
                    }],
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::EditSafetyInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "edit-safety-inspection");
        assert_eq!(value["result"]["sourceCount"], 1);
        assert_eq!(value["result"]["inspectedCount"], 1);
        assert_eq!(value["result"]["failedCount"], 0);
        assert_eq!(value["result"]["items"][0]["sourceIndex"], 0);
        assert_eq!(value["result"]["items"][0]["result"]["pageCount"], 1);
    }

    #[test]
    fn queued_edit_safety_snapshot_excludes_every_source_and_password() {
        let directory = TestDirectory::new();
        let first = directory.path.join("private-edit-safety-queued-first.pdf");
        let second = directory.path.join("private-edit-safety-queued-second.pdf");
        save_fixture(&first);
        save_fixture(&second);
        let first_password = "first-edit-safety-password-never-serialise";
        let second_password = "second-edit-safety-password-never-serialise";
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::EditSafetyInspection(
                InspectPdfEditSafetySourcesRequest {
                    sources: vec![
                        InspectPdfEditSafetyRequest {
                            input_path: first.to_string_lossy().into_owned(),
                            input_password: Some(first_password.to_string()),
                        },
                        InspectPdfEditSafetyRequest {
                            input_path: second.to_string_lossy().into_owned(),
                            input_password: Some(second_password.to_string()),
                        },
                    ],
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::EditSafetyInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            first_password,
            second_password,
            "private-edit-safety-queued-first.pdf",
            "private-edit-safety-queued-second.pdf",
            "inputPath",
            "inputPassword",
            "sources",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn edit_safety_inspection_item_failures_are_content_free() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-edit-safety-review.pdf");
        fs::write(&input, b"private malformed edit-safety review content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::EditSafetyInspection(
                InspectPdfEditSafetySourcesRequest {
                    sources: vec![InspectPdfEditSafetyRequest {
                        input_path: input.to_string_lossy().into_owned(),
                        input_password: None,
                    }],
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["result"]["inspectedCount"], 0);
        assert_eq!(value["result"]["failedCount"], 1);
        assert_eq!(
            value["result"]["items"][0]["error"],
            "The edit-safety check could not complete its bounded structural inspection. Review the PDF and try again."
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-edit-safety-review.pdf"));
        assert!(!serialised.contains("private malformed edit-safety review content"));
    }

    #[test]
    fn batch_job_uses_the_shared_queue_and_preserves_its_result_shape() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output_directory = directory.path.join("outputs");
        let output = output_directory.join("source-clean.pdf");
        fs::create_dir(&output_directory).unwrap();
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(batch_request(&input, &output_directory))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::Batch);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(completed.result, Some(PdfJobResult::Batch(_))));
        assert!(completed.error.is_none());
        assert!(output.exists());
        assert_eq!(manager.list(Some(PdfJobKind::Batch)).unwrap().len(), 1);
    }

    #[test]
    fn queued_batch_snapshot_excludes_paths_and_every_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-batch-source.pdf");
        let output_directory = directory.path.join("private-batch-outputs");
        fs::create_dir(&output_directory).unwrap();
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "batch-input-password-never-serialise";
        let open_password = "batch-opening-password-never-serialise";
        let owner_password = "batch-owner-password-never-serialise";

        let started = manager
            .start(batch_request_with_secrets(
                &input,
                &output_directory,
                Some(input_password),
                Some((open_password, owner_password)),
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Batch);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-batch-source.pdf",
            "private-batch-outputs",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert_eq!(fs::read_dir(&output_directory).unwrap().count(), 0);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_batch_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-batch-source.pdf");
        let output_directory = directory.path.join("confidential-batch-outputs");
        fs::create_dir(&output_directory).unwrap();
        fs::write(&input, b"%PDF-1.7\nprivate-batch-document-content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let input_password = "private-batch-input-password";
        let open_password = "private-batch-opening-password";
        let owner_password = "private-batch-owner-password";

        let started = manager
            .start(batch_request_with_secrets(
                &input,
                &output_directory,
                Some(input_password),
                Some((open_password, owner_password)),
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-batch-document-content",
            "confidential-batch-source.pdf",
            "confidential-batch-outputs",
        ] {
            assert!(!serialised.contains(secret));
        }
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The batch recipe failed a structural safety check. Inspect the sources and try again."
            )
        );
        assert_eq!(fs::read_dir(&output_directory).unwrap().count(), 0);
    }

    #[test]
    fn merge_and_split_jobs_use_the_shared_queue_and_preserve_results() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let merged = directory.path.join("combined.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);

        let merge_started = manager
            .start(merge_request(&input, &merged, None, None))
            .unwrap();
        let merge_completed = wait_for_terminal(&manager, &merge_started.job_id);
        assert_eq!(merge_completed.kind, PdfJobKind::Merge);
        assert_eq!(merge_completed.status, PdfJobStatus::Succeeded);
        assert_eq!(merge_completed.progress, 100);
        assert!(matches!(
            merge_completed.result,
            Some(PdfJobResult::Merge(_))
        ));
        assert!(merged.exists());

        let split_started = manager
            .start(split_request(&input, &directory.path, None, None))
            .unwrap();
        let split_completed = wait_for_terminal(&manager, &split_started.job_id);
        assert_eq!(split_completed.kind, PdfJobKind::Split);
        assert_eq!(split_completed.status, PdfJobStatus::Succeeded);
        assert_eq!(split_completed.progress, 100);
        assert!(matches!(
            split_completed.result,
            Some(PdfJobResult::Split(_))
        ));
        assert!(directory.path.join("source-part-01.pdf").exists());
        assert_eq!(manager.list(Some(PdfJobKind::Merge)).unwrap().len(), 1);
        assert_eq!(manager.list(Some(PdfJobKind::Split)).unwrap().len(), 1);
    }

    #[test]
    fn organise_job_uses_the_shared_queue_and_preserves_its_result() {
        let directory = TestDirectory::new();
        let input = directory.path.join("primary.pdf");
        let output = directory.path.join("organised.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);

        let started = manager
            .start(organise_request(&input, &output, None, None))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.kind, PdfJobKind::Organise);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(completed.result, Some(PdfJobResult::Organise(_))));
        assert!(completed.error.is_none());
        assert!(output.exists());
        assert_eq!(manager.list(Some(PdfJobKind::Organise)).unwrap().len(), 1);
    }

    #[test]
    fn page_transfer_uses_a_distinct_job_identity_and_the_verified_publisher() {
        let directory = TestDirectory::new();
        let input = directory.path.join("transfer-source.pdf");
        let output = directory.path.join("transfer-destination.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);

        let started = manager
            .start(page_transfer_request(&input, &output))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.kind, PdfJobKind::PageTransfer);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(
            completed.result,
            Some(PdfJobResult::PageTransfer(_))
        ));
        assert!(completed.error.is_none());
        assert!(output.exists());
        assert!(manager.list(Some(PdfJobKind::Organise)).unwrap().is_empty());
        assert_eq!(
            manager.list(Some(PdfJobKind::PageTransfer)).unwrap().len(),
            1
        );
    }

    #[test]
    fn queued_organise_snapshot_excludes_paths_passwords_and_signature_bytes() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-primary.pdf");
        let output = directory.path.join("private-organised.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "organiser-password-never-serialise";
        let signature_secret = "signature-pixels-never-serialise";
        let signature = format!("data:image/png;base64,{signature_secret}");

        let started = manager
            .start(organise_request(
                &input,
                &output,
                Some(password),
                Some(&signature),
            ))
            .unwrap();

        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains(signature_secret));
        assert!(!serialised.contains("private-primary.pdf"));
        assert!(!serialised.contains("private-organised.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn redaction_job_uses_the_shared_queue_and_preserves_its_result() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("redacted.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);

        let started = manager
            .start(redaction_request(
                &input,
                &output,
                None,
                &test_redaction_png(),
                None,
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.kind, PdfJobKind::Redaction);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(completed.result, Some(PdfJobResult::Redaction(_))));
        assert!(completed.error.is_none());
        assert!(output.exists());
        assert_eq!(manager.list(Some(PdfJobKind::Redaction)).unwrap().len(), 1);
    }

    #[test]
    fn queued_redaction_snapshot_excludes_paths_passwords_and_page_rasters() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-source.pdf");
        let output = directory.path.join("private-redacted.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "redaction-password-never-serialise";
        let open_password = "redaction-opening-password-never-serialise";
        let owner_password = "redaction-owner-password-never-serialise";
        let raster_secret = "reviewed-page-pixels-never-serialise";
        let raster = format!("data:image/png;base64,{raster_secret}");

        let started = manager
            .start(redaction_request(
                &input,
                &output,
                Some(password),
                &raster,
                Some((open_password, owner_password)),
            ))
            .unwrap();

        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains(raster_secret));
        assert!(!serialised.contains("private-source.pdf"));
        assert!(!serialised.contains("private-redacted.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_redaction_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-redaction-source.pdf");
        let output = directory.path.join("confidential-redaction-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let private_raster = "data:image/png;base64,private-redaction-raster-content";

        let started = manager
            .start(redaction_request(
                &input,
                &output,
                Some("private-redaction-password"),
                private_raster,
                None,
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("private-redaction-raster-content"));
        assert!(!serialised.contains("private-redaction-password"));
        assert!(!serialised.contains("confidential-redaction-source.pdf"));
        assert!(!serialised.contains("confidential-redaction-output.pdf"));
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Permanent redaction failed a structural or privacy safety check. Review the redactions and try again."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn queued_protection_snapshot_excludes_paths_and_every_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-protection-source.pdf");
        let output = directory.path.join("private-protected-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "current-password-never-serialise";
        let open_password = "opening-password-never-serialise";
        let owner_password = "owner-password-never-serialise";

        let started = manager
            .start(protection_request(
                &input,
                &output,
                input_password,
                open_password,
                owner_password,
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Protection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(input_password));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains("private-protection-source.pdf"));
        assert!(!serialised.contains("private-protected-output.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn queued_certificate_snapshot_excludes_paths_passphrases_and_signing_details() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-certificate-source.pdf");
        let output = directory.path.join("private-certificate-output.pdf");
        let pkcs12 = directory.path.join("private-client-identity.p12");
        let trust_root = directory.path.join("private-client-root.pem");
        save_fixture(&input);
        fs::write(&pkcs12, b"private certificate bytes").unwrap();
        fs::write(&trust_root, b"private trust root bytes").unwrap();
        let passphrase = "certificate-passphrase-never-serialise";
        let input_password = "certificate-pdf-password-never-serialise";
        let field_name = "PrivateApprovalField";
        let timestamp_url = "https://tsa.example.test/private-client-timestamp";
        let request: StartPdfJobRequest = serde_json::from_value(serde_json::json!({
            "kind": "certificate",
            "request": {
                "embedValidationInfo": true,
                "fieldName": field_name,
                "inputPassword": input_password,
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "pageNumber": null,
                "pkcs12Passphrase": passphrase,
                "pkcs12PassphraseConfirmation": passphrase,
                "pkcs12Path": pkcs12.to_string_lossy(),
                "position": null,
                "timestampUrl": timestamp_url,
                "trustRoots": [trust_root.to_string_lossy()],
                "visible": false
            }
        }))
        .unwrap();
        let manager = PdfJobManager::with_max_running(0);

        let started = manager.start(request).unwrap();

        assert_eq!(started.kind, PdfJobKind::Certificate);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            input_password,
            passphrase,
            field_name,
            timestamp_url,
            "private-certificate-source.pdf",
            "private-certificate-output.pdf",
            "private-client-identity.p12",
            "private-client-root.pem",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn queued_bookmark_snapshot_excludes_paths_passwords_and_document_content() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-bookmark-source.pdf");
        let output = directory.path.join("private-bookmarked-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "bookmark-input-password-never-serialise";
        let open_password = "bookmark-opening-password-never-serialise";
        let owner_password = "bookmark-owner-password-never-serialise";
        let private_title = "Confidential acquisition heading";
        let private_contents_title = "Confidential acquisition contents";

        let started = manager
            .start(bookmark_request(
                &input,
                &output,
                input_password,
                open_password,
                owner_password,
                private_title,
                private_contents_title,
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Bookmarks);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(input_password));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains(private_title));
        assert!(!serialised.contains(private_contents_title));
        assert!(!serialised.contains("private-bookmark-source.pdf"));
        assert!(!serialised.contains("private-bookmarked-output.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn queued_form_snapshot_excludes_paths_passwords_and_field_values() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-form-source.pdf");
        let output = directory.path.join("private-form-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "form-input-password-never-serialise";
        let open_password = "form-opening-password-never-serialise";
        let owner_password = "form-owner-password-never-serialise";
        let private_value = "Private account reference 99281";

        let started = manager
            .start(form_request(
                &input,
                &output,
                input_password,
                open_password,
                owner_password,
                private_value,
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Forms);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(input_password));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains(private_value));
        assert!(!serialised.contains("private-form-source.pdf"));
        assert!(!serialised.contains("private-form-output.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn queued_annotation_snapshot_excludes_paths_passwords_text_and_image_bytes() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-annotation-source.pdf");
        let output = directory.path.join("private-annotation-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "annotation-input-password-never-serialise";
        let open_password = "annotation-opening-password-never-serialise";
        let owner_password = "annotation-owner-password-never-serialise";
        let private_text = "Confidential diagnosis reference 47291";
        let private_image = "private-annotation-pixels-never-serialise";
        let private_id = "private-annotation-identifier";

        let started = manager
            .start(annotation_request(
                &input,
                &output,
                input_password,
                open_password,
                owner_password,
                private_text,
                private_image,
                private_id,
                false,
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Annotations);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(input_password));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains(private_text));
        assert!(!serialised.contains(private_image));
        assert!(!serialised.contains(private_id));
        assert!(!serialised.contains("private-annotation-source.pdf"));
        assert!(!serialised.contains("private-annotation-output.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn annotation_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("annotation-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::AnnotationInspection(
                InspectPdfAnnotationsRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::AnnotationInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "annotation-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["existingAnnotationCount"], 0);
    }

    #[test]
    fn queued_annotation_inspection_snapshot_excludes_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-annotation-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "annotation-review-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::AnnotationInspection(
                InspectPdfAnnotationsRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::AnnotationInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-annotation-review.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_annotation_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-annotation-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::AnnotationInspection(
                InspectPdfAnnotationsRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The annotation review failed a structural safety check. Review the source PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-annotation-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn bookmark_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("bookmark-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::BookmarkInspection(
                InspectPdfBookmarksRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::BookmarkInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "bookmark-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["bookmarkCount"], 0);
    }

    #[test]
    fn queued_bookmark_inspection_snapshot_excludes_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-bookmark-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "bookmark-review-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::BookmarkInspection(
                InspectPdfBookmarksRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::BookmarkInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-bookmark-review.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_bookmark_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-bookmark-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::BookmarkInspection(
                InspectPdfBookmarksRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The bookmark review failed a structural safety check. Review the source PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-bookmark-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn form_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("form-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::FormInspection(InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            }))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::FormInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "form-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["fieldCount"], 0);
    }

    #[test]
    fn queued_form_inspection_snapshot_excludes_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-form-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "form-review-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::FormInspection(InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: Some(password.to_string()),
            }))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::FormInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-form-review.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_form_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-form-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::FormInspection(InspectPdfFormsRequest {
                input_path: input.to_string_lossy().into_owned(),
                input_password: None,
            }))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The form review failed a structural safety check. Review the source PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-form-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn finishing_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("finishing-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::FinishingInspection(
                InspectPdfFinishingRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::FinishingInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "finishing-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["annotationCount"], 0);
    }

    #[test]
    fn queued_finishing_inspection_snapshot_excludes_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-finishing-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "finishing-review-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::FinishingInspection(
                InspectPdfFinishingRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::FinishingInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-finishing-review.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_finishing_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-finishing-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::FinishingInspection(
                InspectPdfFinishingRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The Page Finish review failed a structural safety check. Review the source PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-finishing-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn page_import_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("page-import-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::PageImportInspection(
                InspectPageImportRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: "all".to_string(),
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::PageImportInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "page-import-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["selectedPages"], serde_json::json!([1]));
    }

    #[test]
    fn queued_page_import_inspection_snapshot_excludes_source_password_and_range() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-page-import-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "page-import-review-password-never-serialise";
        let range = "1, 1, 1";
        let started = manager
            .start(StartPdfJobRequest::PageImportInspection(
                InspectPageImportRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    page_range: range.to_string(),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::PageImportInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-page-import-review.pdf"));
        assert!(!serialised.contains(range));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        assert!(!serialised.contains("pageRange"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_page_import_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-page-import-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::PageImportInspection(
                InspectPageImportRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                    page_range: "all".to_string(),
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The page import review failed a structural safety check. Choose the source PDF again and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-page-import-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn redaction_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("redaction-review-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::RedactionInspection(
                InspectPdfRedactionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(started.kind, PdfJobKind::RedactionInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        let value = serde_json::to_value(&completed).unwrap();
        assert_eq!(value["kind"], "redaction-inspection");
        assert_eq!(value["result"]["pageCount"], 1);
        assert_eq!(value["result"]["annotationCount"], 0);
    }

    #[test]
    fn queued_redaction_inspection_snapshot_excludes_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-redaction-review.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let password = "redaction-review-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::RedactionInspection(
                InspectPdfRedactionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::RedactionInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(password));
        assert!(!serialised.contains("private-redaction-review.pdf"));
        assert!(!serialised.contains("inputPath"));
        assert!(!serialised.contains("inputPassword"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
    }

    #[test]
    fn failed_redaction_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-redaction-review.pdf");
        fs::write(&input, b"not a private PDF").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::RedactionInspection(
                InspectPdfRedactionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The redaction review failed a structural safety check. Review the source PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("confidential-redaction-review.pdf"));
        assert!(!serialised.contains("not a private PDF"));
    }

    #[test]
    fn failed_annotation_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-annotation-source.pdf");
        let output = directory.path.join("confidential-annotation-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let private_text = "Private legal note 994-22";
        let private_image = "confidential-image-pixels";
        let private_id = "confidential-annotation-id";
        let request = annotation_request(
            &input,
            &output,
            "private-input-password",
            "private-opening-password",
            "private-owner-password",
            private_text,
            private_image,
            private_id,
            true,
        );
        let started = manager.start(request).unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains(private_text));
        assert!(!serialised.contains(private_image));
        assert!(!serialised.contains(private_id));
        assert!(!serialised.contains("private-input-password"));
        assert!(!serialised.contains("private-opening-password"));
        assert!(!serialised.contains("private-owner-password"));
        assert!(!serialised.contains("confidential-annotation-source.pdf"));
        assert!(!serialised.contains("confidential-annotation-output.pdf"));
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The annotation export failed a structural safety check. Review the annotations and try again."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn queued_finishing_snapshot_excludes_paths_passwords_and_mark_text() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-finishing-source.pdf");
        let output = directory.path.join("private-finishing-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "finishing-input-password-never-serialise";
        let open_password = "finishing-opening-password-never-serialise";
        let owner_password = "finishing-owner-password-never-serialise";
        let watermark = "Confidential litigation watermark 8841";
        let header = "Private client header";
        let footer = "Restricted matter footer";
        let bates_prefix = "CASE-PRIVATE-";

        let started = manager
            .start(finishing_request(
                &input,
                &output,
                input_password,
                open_password,
                owner_password,
                watermark,
                header,
                footer,
                bates_prefix,
                false,
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Finishing);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            watermark,
            header,
            footer,
            bates_prefix,
            "private-finishing-source.pdf",
            "private-finishing-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_finishing_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-finishing-source.pdf");
        let output = directory.path.join("confidential-finishing-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let secrets = [
            "private-input-password",
            "private-opening-password",
            "private-owner-password",
            "Private merger watermark",
            "Secret heading",
            "Sensitive footer",
            "MATTER-771-",
        ];
        let request = finishing_request(
            &input, &output, secrets[0], secrets[1], secrets[2], secrets[3], secrets[4],
            secrets[5], secrets[6], true,
        );
        let started = manager.start(request).unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in secrets {
            assert!(!serialised.contains(secret));
        }
        assert!(!serialised.contains("confidential-finishing-source.pdf"));
        assert!(!serialised.contains("confidential-finishing-output.pdf"));
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Page finishing failed a structural safety check. Review the settings and try again."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn failed_form_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-form-source.pdf");
        let output = directory.path.join("confidential-form-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let private_value = "Medical reference 443-29";
        let request = form_request(
            &input,
            &output,
            "private-input-password",
            "private-opening-password",
            "private-owner-password",
            private_value,
        );
        let started = manager.start(request).unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains(private_value));
        assert!(!serialised.contains("private-input-password"));
        assert!(!serialised.contains("private-opening-password"));
        assert!(!serialised.contains("private-owner-password"));
        assert!(!serialised.contains("confidential-form-source.pdf"));
        assert!(!serialised.contains("confidential-form-output.pdf"));
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The form export failed a structural safety check. Review the form and try again."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn queued_merge_snapshot_never_exposes_its_source_or_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-source.pdf");
        let output = directory.path.join("combined.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let secret = "merge-password-never-serialise";
        let open_password = "merge-opening-password-never-serialise";
        let owner_password = "merge-owner-password-never-serialise";
        let started = manager
            .start(merge_request(
                &input,
                &output,
                Some(secret),
                Some((open_password, owner_password)),
            ))
            .unwrap();

        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(secret));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains("private-source.pdf"));
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_merge_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-merge-source.pdf");
        let output = directory.path.join("confidential-merge-output.pdf");
        fs::write(&input, b"%PDF-1.7\nprivate-merge-document-content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let input_password = "private-merge-input-password";
        let open_password = "private-merge-opening-password";
        let owner_password = "private-merge-owner-password";

        let started = manager
            .start(merge_request(
                &input,
                &output,
                Some(input_password),
                Some((open_password, owner_password)),
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-merge-document-content",
            "confidential-merge-source.pdf",
            "confidential-merge-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        assert_eq!(
            completed.error.as_deref(),
            Some("The merge failed a structural safety check. Review the sources and try again.")
        );
        assert!(!output.exists());
    }

    #[test]
    fn queued_split_snapshot_excludes_paths_and_every_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-split-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "split-input-password-never-serialise";
        let open_password = "split-opening-password-never-serialise";
        let owner_password = "split-owner-password-never-serialise";
        let private_directory = directory.path.to_string_lossy().into_owned();

        let started = manager
            .start(split_request(
                &input,
                &directory.path,
                Some(input_password),
                Some((open_password, owner_password)),
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Split);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-split-source.pdf",
            private_directory.as_str(),
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_split_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-split-source.pdf");
        fs::write(&input, b"%PDF-1.7\nprivate-split-document-content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let input_password = "private-split-input-password";
        let open_password = "private-split-opening-password";
        let owner_password = "private-split-owner-password";
        let private_directory = directory.path.to_string_lossy().into_owned();

        let started = manager
            .start(split_request(
                &input,
                &directory.path,
                Some(input_password),
                Some((open_password, owner_password)),
            ))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-split-document-content",
            "confidential-split-source.pdf",
            private_directory.as_str(),
        ] {
            assert!(!serialised.contains(secret));
        }
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The split failed a structural safety check. Review the page groups and try again."
            )
        );
        assert!(!fs::read_dir(&directory.path).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("-part-")));
    }

    #[test]
    fn queued_cancellation_drops_the_secret_request_without_publishing() {
        let directory = TestDirectory::new();
        let input = directory.path.join("source.pdf");
        let output = directory.path.join("clean.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let secret = "never-serialise-this-password";
        let open_password = "privacy-opening-password-never-serialise";
        let owner_password = "privacy-owner-password-never-serialise";
        let started = manager
            .start(StartPdfJobRequest::Privacy(privacy_request(
                &input,
                &output,
                Some(secret.to_string()),
                Some((open_password, owner_password)),
            )))
            .unwrap();

        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        assert!(!serialised.contains(secret));
        assert!(!serialised.contains(open_password));
        assert!(!serialised.contains(owner_password));
        assert!(!serialised.contains(&input.to_string_lossy().into_owned()));

        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(cancelled.result.is_none());
        assert!(cancelled.error.is_none());
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_privacy_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-privacy-source.pdf");
        let output = directory.path.join("confidential-privacy-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let mut request = privacy_request(
            &input,
            &output,
            Some("private-privacy-password".to_string()),
            None,
        );
        request.expected_source_size = request.expected_source_size.saturating_add(1);

        let started = manager.start(StartPdfJobRequest::Privacy(request)).unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(!serialised.contains("private-privacy-password"));
        assert!(!serialised.contains("confidential-privacy-source.pdf"));
        assert!(!serialised.contains("confidential-privacy-output.pdf"));
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "The source PDF changed after inspection. Inspect it again before privacy cleaning."
            )
        );
        assert!(!output.exists());
    }

    #[test]
    fn compression_preview_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("compression-preview-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::CompressionPreview(
                PreviewPdfCompressionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                    jpeg_quality: 72,
                },
            ))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::CompressionPreview);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(
            completed.result,
            Some(PdfJobResult::CompressionPreview(_))
        ));
        assert!(completed.error.is_none());
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(serialised.contains(r#""fileName":"PDF""#));
        assert!(!serialised.contains("compression-preview-source.pdf"));
        assert_eq!(
            manager
                .list(Some(PdfJobKind::CompressionPreview))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn compression_preview_uses_the_explicit_hyphenated_wire_kind() {
        let request = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "compression-preview",
            "request": {
                "inputPath": "review.pdf",
                "inputPassword": null,
                "jpegQuality": 72
            }
        }))
        .unwrap();

        assert_eq!(request.kind(), PdfJobKind::CompressionPreview);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::CompressionPreview).unwrap(),
            r#""compression-preview""#
        );
    }

    #[test]
    fn queued_compression_preview_snapshot_excludes_the_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory
            .path
            .join("private-compression-preview-source.pdf");
        let password = "compression-preview-password-never-serialise";
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::CompressionPreview(
                PreviewPdfCompressionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    jpeg_quality: 72,
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::CompressionPreview);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            password,
            "private-compression-preview-source.pdf",
            "inputPath",
            "inputPassword",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_compression_preview_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-compression-preview.pdf");
        let password = "private-compression-preview-input-password";
        fs::write(
            &input,
            b"%PDF-1.7\nprivate compression preview document content",
        )
        .unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::CompressionPreview(
                PreviewPdfCompressionRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                    jpeg_quality: 72,
                },
            ))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Compression preview could not complete a bounded image and structure analysis. Review the PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            password,
            "private compression preview document content",
            "confidential-compression-preview.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn privacy_inspection_uses_the_shared_queue_and_retains_its_typed_report() {
        let directory = TestDirectory::new();
        let input = directory.path.join("privacy-inspection-source.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::PrivacyInspection(
                InspectPdfPrivacyRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: None,
                },
            ))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::PrivacyInspection);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(
            completed.result,
            Some(PdfJobResult::PrivacyInspection(_))
        ));
        assert!(completed.error.is_none());
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(serialised.contains(r#""fileName":"PDF""#));
        assert!(!serialised.contains("privacy-inspection-source.pdf"));
        assert_eq!(
            manager
                .list(Some(PdfJobKind::PrivacyInspection))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn privacy_inspection_uses_the_explicit_hyphenated_wire_kind() {
        let request = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "privacy-inspection",
            "request": {
                "inputPath": "review.pdf",
                "inputPassword": null
            }
        }))
        .unwrap();

        assert_eq!(request.kind(), PdfJobKind::PrivacyInspection);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::PrivacyInspection).unwrap(),
            r#""privacy-inspection""#
        );
    }

    #[test]
    fn queued_privacy_inspection_snapshot_excludes_the_source_and_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-privacy-inspection-source.pdf");
        let password = "privacy-inspection-password-never-serialise";
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let started = manager
            .start(StartPdfJobRequest::PrivacyInspection(
                InspectPdfPrivacyRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::PrivacyInspection);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            password,
            "private-privacy-inspection-source.pdf",
            "inputPath",
            "inputPassword",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_privacy_inspection_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-privacy-inspection.pdf");
        let password = "private-privacy-inspection-input-password";
        fs::write(
            &input,
            b"%PDF-1.7\nprivate privacy inspection document content",
        )
        .unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager
            .start(StartPdfJobRequest::PrivacyInspection(
                InspectPdfPrivacyRequest {
                    input_path: input.to_string_lossy().into_owned(),
                    input_password: Some(password.to_string()),
                },
            ))
            .unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Privacy Inspection could not complete its bounded structure and page analysis. Review the PDF and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            password,
            "private privacy inspection document content",
            "confidential-privacy-inspection.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn ocr_review_uses_the_explicit_hyphenated_wire_kind() {
        let request = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "ocr-review",
            "request": {
                "inputPath": "review.png",
                "colourMode": "greyscale",
                "autoOrient": true,
                "autoCrop": true,
                "correctPerspective": false,
                "removeShadows": true,
                "language": "eng"
            }
        }))
        .unwrap();

        assert_eq!(request.kind(), PdfJobKind::OcrReview);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::OcrReview).unwrap(),
            r#""ocr-review""#
        );
    }

    #[test]
    fn queued_ocr_review_snapshot_excludes_the_source_and_settings() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-ocr-review-source.png");
        RgbImage::from_pixel(40, 30, Rgb([250, 250, 250]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_max_running(0);
        let started = manager.start(ocr_review_request(&input)).unwrap();

        assert_eq!(started.kind, PdfJobKind::OcrReview);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            "private-ocr-review-source.png",
            "inputPath",
            "colourMode",
            "autoCrop",
            "language",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_ocr_review_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-ocr-review.png");
        fs::write(&input, b"private OCR review image content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager.start(ocr_review_request(&input)).unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "OCR confidence review could not complete bounded local image preparation and recognition. Review the image and try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            "private OCR review image content",
            "confidential-ocr-review.png",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn scan_preview_uses_the_shared_queue_and_retains_its_volatile_image() {
        let directory = TestDirectory::new();
        let input = directory.path.join("scan-preview-source.png");
        RgbImage::from_pixel(64, 96, Rgb([30, 120, 210]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager.start(scan_preview_request(&input)).unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.kind, PdfJobKind::ScanPreview);
        assert_eq!(completed.status, PdfJobStatus::Succeeded);
        assert_eq!(completed.progress, 100);
        assert!(matches!(
            completed.result,
            Some(PdfJobResult::ScanPreview(_))
        ));
        assert!(completed.error.is_none());
        let serialised = serde_json::to_string(&completed).unwrap();
        assert!(serialised.contains(r#""mimeType":"image/jpeg""#));
        assert!(!serialised.contains("scan-preview-source.png"));
        assert_eq!(
            manager.list(Some(PdfJobKind::ScanPreview)).unwrap().len(),
            1
        );
    }

    #[test]
    fn scan_preview_uses_the_explicit_hyphenated_wire_kind() {
        let request = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "scan-preview",
            "request": {
                "inputPath": "review.png",
                "colourMode": "greyscale",
                "autoOrient": true,
                "autoCrop": true,
                "correctPerspective": false,
                "removeShadows": true
            }
        }))
        .unwrap();

        assert_eq!(request.kind(), PdfJobKind::ScanPreview);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::ScanPreview).unwrap(),
            r#""scan-preview""#
        );
    }

    #[test]
    fn queued_scan_preview_snapshot_excludes_the_source_and_settings() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-scan-preview-source.png");
        RgbImage::from_pixel(40, 30, Rgb([250, 250, 250]))
            .save(&input)
            .unwrap();
        let manager = PdfJobManager::with_max_running(0);
        let started = manager.start(scan_preview_request(&input)).unwrap();

        assert_eq!(started.kind, PdfJobKind::ScanPreview);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            "private-scan-preview-source.png",
            "inputPath",
            "colourMode",
            "autoCrop",
            "removeShadows",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_scan_preview_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-scan-preview.png");
        fs::write(&input, b"private scan preview image content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let started = manager.start(scan_preview_request(&input)).unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Scan clean-up preview could not decode this image locally. Install ImageMagick for HEIC, AVIF or other unsupported formats, then try again."
            )
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            "private scan preview image content",
            "confidential-scan-preview.png",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn scanner_capture_uses_the_explicit_hyphenated_wire_kind() {
        let request = serde_json::from_value::<StartPdfJobRequest>(serde_json::json!({
            "kind": "scanner-capture",
            "request": {
                "deviceId": "test:scanner",
                "source": "feeder",
                "duplex": true,
                "dpi": 300,
                "colourMode": "greyscale",
                "paperWidthMm": 210,
                "paperHeightMm": 297,
                "pageLimit": 25
            }
        }))
        .unwrap();

        assert_eq!(request.kind(), PdfJobKind::ScannerCapture);
        assert_eq!(
            serde_json::to_string(&PdfJobKind::ScannerCapture).unwrap(),
            r#""scanner-capture""#
        );
    }

    #[test]
    fn queued_scanner_capture_snapshot_excludes_device_and_settings() {
        let manager = PdfJobManager::with_max_running(0);
        let started = manager.start(scanner_capture_request()).unwrap();

        assert_eq!(started.kind, PdfJobKind::ScannerCapture);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            "private:test-scanner",
            "deviceId",
            "source",
            "duplex",
            "colourMode",
            "paperWidthMm",
            "paperHeightMm",
            "pageLimit",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
    }

    #[test]
    fn failed_scanner_capture_snapshot_uses_a_content_free_error() {
        let manager = PdfJobManager::with_max_running(1);
        let started = manager.start(scanner_capture_request()).unwrap();

        let completed = wait_for_terminal(&manager, &started.job_id);
        assert_eq!(completed.status, PdfJobStatus::Failed);
        assert_eq!(
            completed.error.as_deref(),
            Some("The private scanner capture workspace is unavailable.")
        );
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            "private:test-scanner",
            "deviceId",
            "paperWidthMm",
            "pageLimit",
        ] {
            assert!(!serialised.contains(secret));
        }
    }

    #[test]
    fn queued_compression_snapshot_excludes_paths_and_every_password() {
        let directory = TestDirectory::new();
        let input = directory.path.join("private-compression-source.pdf");
        let output = directory.path.join("private-compression-output.pdf");
        save_fixture(&input);
        let manager = PdfJobManager::with_max_running(0);
        let input_password = "compression-input-password-never-serialise";
        let open_password = "compression-opening-password-never-serialise";
        let owner_password = "compression-owner-password-never-serialise";

        let started = manager
            .start(StartPdfJobRequest::Compression(compression_request(
                &input,
                &output,
                Some(input_password),
                Some((open_password, owner_password)),
            )))
            .unwrap();

        assert_eq!(started.kind, PdfJobKind::Compression);
        assert_eq!(started.status, PdfJobStatus::Queued);
        let serialised = serde_json::to_string(&started).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-compression-source.pdf",
            "private-compression-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        let cancelled = manager.cancel(&started.job_id).unwrap();
        assert_eq!(cancelled.status, PdfJobStatus::Cancelled);
        assert!(!output.exists());
        let store = manager.lock().unwrap();
        assert!(!store.pending_requests.contains_key(&started.job_id));
        assert!(!store.pending_order.contains(&started.job_id));
    }

    #[test]
    fn failed_compression_snapshot_uses_a_content_free_error() {
        let directory = TestDirectory::new();
        let input = directory.path.join("confidential-compression-source.pdf");
        let output = directory.path.join("confidential-compression-output.pdf");
        fs::write(&input, b"%PDF-1.7\nprivate-compression-document-content").unwrap();
        let manager = PdfJobManager::with_max_running(1);
        let input_password = "private-compression-input-password";
        let open_password = "private-compression-opening-password";
        let owner_password = "private-compression-owner-password";

        let started = manager
            .start(StartPdfJobRequest::Compression(compression_request(
                &input,
                &output,
                Some(input_password),
                Some((open_password, owner_password)),
            )))
            .unwrap();
        let completed = wait_for_terminal(&manager, &started.job_id);

        assert_eq!(completed.status, PdfJobStatus::Failed);
        let serialised = serde_json::to_string(&completed).unwrap();
        for secret in [
            input_password,
            open_password,
            owner_password,
            "private-compression-document-content",
            "confidential-compression-source.pdf",
            "confidential-compression-output.pdf",
        ] {
            assert!(!serialised.contains(secret));
        }
        assert_eq!(
            completed.error.as_deref(),
            Some(
                "Compression failed a structural safety check. Recalculate the preview and try again."
            )
        );
        assert!(!output.exists());
    }

    fn wait_for_terminal(manager: &PdfJobManager, job_id: &str) -> PdfJobSnapshot {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = manager.get(job_id).unwrap();
            if is_terminal(snapshot.status) {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "PDF job timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn privacy_request(
        input: &Path,
        output: &Path,
        input_password: Option<String>,
        output_protection: Option<(&str, &str)>,
    ) -> CleanPdfPrivacyRequest {
        let metadata = fs::metadata(input).unwrap();
        CleanPdfPrivacyRequest {
            acknowledge_certificate_signatures: false,
            expected_source_modified_at_ms: metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .and_then(|value| value.as_millis().try_into().ok()),
            expected_source_size: metadata.len(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password,
            options: PrivacyCleanOptions {
                remove_metadata: true,
                remove_active_content: false,
                remove_attachments: false,
                remove_annotations_and_forms: false,
                remove_thumbnails: false,
            },
            output_protection: output_protection.map(|(open_password, owner_password)| {
                crate::protection::PdfOutputProtection {
                    open_password: open_password.to_string(),
                    owner_password: owner_password.to_string(),
                }
            }),
        }
    }

    fn compression_request(
        input: &Path,
        output: &Path,
        input_password: Option<&str>,
        output_protection: Option<(&str, &str)>,
    ) -> ExportCompressedPdfRequest {
        ExportCompressedPdfRequest {
            acknowledge_certificate_signatures: false,
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            input_password: input_password.map(str::to_string),
            jpeg_quality: 70,
            output_protection: output_protection.map(|(open_password, owner_password)| {
                crate::protection::PdfOutputProtection {
                    open_password: open_password.to_string(),
                    owner_password: owner_password.to_string(),
                }
            }),
        }
    }

    fn batch_request(input: &Path, output_directory: &Path) -> StartPdfJobRequest {
        batch_request_with_secrets(input, output_directory, None, None)
    }

    fn batch_request_with_secrets(
        input: &Path,
        output_directory: &Path,
        input_password: Option<&str>,
        output_protection: Option<(&str, &str)>,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        serde_json::from_value(serde_json::json!({
            "kind": "batch",
            "request": {
                "inputs": [{
                    "acknowledgeCertificateSignatures": false,
                    "expectedPageCount": 1,
                    "expectedSourceModifiedAtMs": metadata
                        .modified()
                        .ok()
                        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                        .and_then(|value| u64::try_from(value.as_millis()).ok()),
                    "expectedSourceSize": metadata.len(),
                    "inputPassword": input_password,
                    "inputPath": input.to_string_lossy(),
                    "outputFileName": "source-clean.pdf"
                }],
                "options": {
                    "cleanPrivacy": true,
                    "compress": false,
                    "jpegQuality": 78,
                    "ocrLanguage": "eng",
                    "privacyOptions": {
                        "removeActiveContent": false,
                        "removeAnnotationsAndForms": false,
                        "removeAttachments": false,
                        "removeMetadata": true,
                        "removeThumbnails": false
                    },
                    "recogniseText": false,
                    "straighten": false
                },
                "outputDirectory": output_directory.to_string_lossy(),
                "outputProtection": output_protection.map(|(open_password, owner_password)| serde_json::json!({
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                }))
            }
        }))
        .unwrap()
    }

    fn protection_request(
        input: &Path,
        output: &Path,
        input_password: &str,
        open_password: &str,
        owner_password: &str,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        serde_json::from_value(serde_json::json!({
            "kind": "protection",
            "request": {
                "operation": "protect",
                "request": {
                    "acknowledgeCertificateSignatures": false,
                    "allowCopying": true,
                    "expectedSourceModifiedAtMs": metadata
                        .modified()
                        .ok()
                        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                        .and_then(|value| u64::try_from(value.as_millis()).ok()),
                    "expectedSourceSize": metadata.len(),
                    "inputPassword": input_password,
                    "inputPath": input.to_string_lossy(),
                    "modificationPermission": "none",
                    "openPassword": open_password,
                    "outputPath": output.to_string_lossy(),
                    "ownerPassword": owner_password,
                    "printPermission": "full"
                }
            }
        }))
        .unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn annotation_request(
        input: &Path,
        output: &Path,
        input_password: &str,
        open_password: &str,
        owner_password: &str,
        private_text: &str,
        private_image: &str,
        private_id: &str,
        invalid_text_area: bool,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        let text_x = if invalid_text_area { 0.95 } else { 0.12 };
        let image_data_url = format!("data:image/png;base64,{private_image}");
        serde_json::from_value(serde_json::json!({
            "kind": "annotations",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "annotations": [{
                    "colour": [0.13, 0.36, 0.84],
                    "end": null,
                    "fillColour": null,
                    "fontSize": 12.0,
                    "id": private_id,
                    "imageDataUrl": null,
                    "kind": "text",
                    "lineWidth": 2.0,
                    "opacity": 0.8,
                    "pageNumber": 1,
                    "points": [],
                    "rect": {
                        "height": 0.12,
                        "width": 0.20,
                        "x": text_x,
                        "y": 0.12
                    },
                    "stamp": null,
                    "start": null,
                    "text": private_text
                }, {
                    "colour": [0.13, 0.36, 0.84],
                    "end": null,
                    "fillColour": null,
                    "fontSize": 12.0,
                    "id": "private-image-annotation",
                    "imageDataUrl": image_data_url,
                    "kind": "image",
                    "lineWidth": 2.0,
                    "opacity": 0.8,
                    "pageNumber": 1,
                    "points": [],
                    "rect": {
                        "height": 0.12,
                        "width": 0.20,
                        "x": 0.45,
                        "y": 0.12
                    },
                    "stamp": null,
                    "start": null,
                    "text": null
                }],
                "expectedSourceModifiedAtMs": metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .and_then(|value| u64::try_from(value.as_millis()).ok()),
                "expectedSourceSize": metadata.len(),
                "inputPassword": input_password,
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "outputProtection": {
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                }
            }
        }))
        .unwrap()
    }

    fn content_request(
        input: &Path,
        output: &Path,
        private_text: &str,
        private_image: &str,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        let modified_at_ms = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .and_then(|value| u64::try_from(value.as_millis()).ok());
        serde_json::from_value(serde_json::json!({
            "kind": "content",
            "request": {
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "inputPassword": "private-content-input-password",
                "outputProtection": {
                    "openPassword": "private-content-opening-password",
                    "ownerPassword": "private-content-owner-password"
                },
                "acknowledgeCertificateSignatures": false,
                "expectedSourceSize": metadata.len(),
                "expectedSourceModifiedAtMs": modified_at_ms,
                "expectedSourceSha256": "a".repeat(64),
                "textEdits": [{
                    "sourceId": format!("text-{}", "b".repeat(64)),
                    "replacementText": private_text
                }],
                "imageEdits": [{
                    "sourceId": format!("image-{}", "c".repeat(64)),
                    "delete": false,
                    "replacementImageDataUrl": format!("data:image/png;base64,{private_image}"),
                    "rect": { "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.2 }
                }]
            }
        }))
        .unwrap()
    }

    fn searchable_ocr_request(
        input: &Path,
        output: &Path,
        protected_output: bool,
    ) -> StartPdfJobRequest {
        serde_json::from_value(serde_json::json!({
            "kind": "searchable-ocr",
            "request": {
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "inputPassword": "private-ocr-input-password",
                "language": "eng",
                "straighten": true,
                "acknowledgeCertificateSignatures": false,
                "outputProtection": protected_output.then(|| serde_json::json!({
                    "openPassword": "private-ocr-opening-password",
                    "ownerPassword": "private-ocr-owner-password"
                }))
            }
        }))
        .unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn finishing_request(
        input: &Path,
        output: &Path,
        input_password: &str,
        open_password: &str,
        owner_password: &str,
        watermark: &str,
        header: &str,
        footer: &str,
        bates_prefix: &str,
        invalid_page_crop: bool,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        let vertical_crop = if invalid_page_crop { 410.0 } else { 1.0 };
        serde_json::from_value(serde_json::json!({
            "kind": "finishing",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "bates": {
                    "colour": [0.1, 0.1, 0.1],
                    "digits": 6,
                    "fontSizePt": 8.0,
                    "marginPt": 18.0,
                    "position": "bottomRight",
                    "prefix": bates_prefix,
                    "startNumber": 1,
                    "suffix": ""
                },
                "crop": {
                    "bottomPt": vertical_crop,
                    "leftPt": 1.0,
                    "rightPt": 1.0,
                    "topPt": vertical_crop
                },
                "expectedSourceModifiedAtMs": metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .and_then(|value| u64::try_from(value.as_millis()).ok()),
                "expectedSourceSize": metadata.len(),
                "headerFooter": {
                    "colour": [0.1, 0.1, 0.1],
                    "fontSizePt": 9.0,
                    "footerAlignment": "centre",
                    "footerText": footer,
                    "headerAlignment": "left",
                    "headerText": header,
                    "marginPt": 18.0
                },
                "inputPassword": input_password,
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "outputProtection": {
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                },
                "pageRange": "all",
                "resize": null,
                "watermark": {
                    "angleDegrees": -35.0,
                    "colour": [0.2, 0.2, 0.2],
                    "fontSizePt": 72.0,
                    "opacity": 0.2,
                    "overContent": true,
                    "text": watermark
                }
            }
        }))
        .unwrap()
    }

    fn bookmark_request(
        input: &Path,
        output: &Path,
        input_password: &str,
        open_password: &str,
        owner_password: &str,
        title: &str,
        contents_title: &str,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        serde_json::from_value(serde_json::json!({
            "kind": "bookmarks",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "bookmarks": [{
                    "bold": false,
                    "colour": [0.0, 0.0, 0.0],
                    "italic": false,
                    "level": 0,
                    "open": true,
                    "pageNumber": 1,
                    "title": title
                }],
                "expectedSourceModifiedAtMs": metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .and_then(|value| u64::try_from(value.as_millis()).ok()),
                "expectedSourceSize": metadata.len(),
                "inputPassword": input_password,
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "outputProtection": {
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                },
                "printedContents": {
                    "addBookmark": true,
                    "maximumLevel": 2,
                    "title": contents_title
                }
            }
        }))
        .unwrap()
    }

    fn form_request(
        input: &Path,
        output: &Path,
        input_password: &str,
        open_password: &str,
        owner_password: &str,
        private_value: &str,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        serde_json::from_value(serde_json::json!({
            "kind": "forms",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "expectedSourceModifiedAtMs": metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .and_then(|value| u64::try_from(value.as_millis()).ok()),
                "expectedSourceSize": metadata.len(),
                "flatten": false,
                "inputPassword": input_password,
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "outputProtection": {
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                },
                "updates": [{
                    "fieldId": "12 0",
                    "values": [private_value]
                }]
            }
        }))
        .unwrap()
    }

    fn merge_request(
        input: &Path,
        output: &Path,
        password: Option<&str>,
        output_protection: Option<(&str, &str)>,
    ) -> StartPdfJobRequest {
        serde_json::from_value(serde_json::json!({
            "kind": "merge",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "outputPath": output.to_string_lossy(),
                "outputProtection": output_protection.map(|(open_password, owner_password)| serde_json::json!({
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                })),
                "preserveBookmarks": true,
                "sources": [{
                    "inputPassword": password,
                    "inputPath": input.to_string_lossy(),
                    "pageRange": "all"
                }]
            }
        }))
        .unwrap()
    }

    fn organise_request(
        input: &Path,
        output: &Path,
        password: Option<&str>,
        signature: Option<&str>,
    ) -> StartPdfJobRequest {
        composed_pdf_request("organise", input, output, password, signature)
    }

    fn page_transfer_request(input: &Path, output: &Path) -> StartPdfJobRequest {
        composed_pdf_request("page-transfer", input, output, None, None)
    }

    fn composed_pdf_request(
        kind: &str,
        input: &Path,
        output: &Path,
        password: Option<&str>,
        signature: Option<&str>,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        serde_json::from_value(serde_json::json!({
            "kind": kind,
            "request": {
                "acknowledgePrimaryCertificateSignature": false,
                "documentLock": null,
                "expectedSourceModifiedAtMs": metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .and_then(|value| u64::try_from(value.as_millis()).ok()),
                "expectedSourceSize": metadata.len(),
                "importedSources": [],
                "outputPath": output.to_string_lossy(),
                "pages": [{
                    "kind": "source",
                    "rotation": 0,
                    "sourceId": "primary",
                    "sourcePage": 1
                }],
                "primaryInputPassword": password,
                "primaryInputPath": input.to_string_lossy(),
                "signature": null,
                "visualSignatureAssets": signature.map(|png_data_url| vec![serde_json::json!({
                    "id": "asset:queue-test",
                    "pngDataUrl": png_data_url
                })]).unwrap_or_default(),
                "visualSignaturePlacements": signature.map(|_| vec![serde_json::json!({
                    "assetId": "asset:queue-test",
                    "id": "placement:queue-test",
                    "leftRatio": 0.65,
                    "pageNumber": 1,
                    "rotationDegrees": 0,
                    "topRatio": 0.8,
                    "widthRatio": 0.28
                })]).unwrap_or_default()
            }
        }))
        .unwrap()
    }

    fn split_request(
        input: &Path,
        output_directory: &Path,
        input_password: Option<&str>,
        output_protection: Option<(&str, &str)>,
    ) -> StartPdfJobRequest {
        serde_json::from_value(serde_json::json!({
            "kind": "split",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "inputPassword": input_password,
                "inputPath": input.to_string_lossy(),
                "outputDirectory": output_directory.to_string_lossy(),
                "pageGroups": ["all"],
                "outputProtection": output_protection.map(|(open_password, owner_password)| serde_json::json!({
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                }))
            }
        }))
        .unwrap()
    }

    fn scan_request(
        input: &Path,
        output: &Path,
        ocr_hint: &str,
        open_password: &str,
        owner_password: &str,
    ) -> StartPdfJobRequest {
        serde_json::from_value(serde_json::json!({
            "kind": "scan",
            "request": {
                "inputPaths": [input.to_string_lossy()],
                "outputPath": output.to_string_lossy(),
                "paperWidthPt": 595,
                "paperHeightPt": 842,
                "marginPt": 18,
                "dpi": 150,
                "jpegQuality": 85,
                "colourMode": "colour",
                "autoOrient": true,
                "autoCrop": false,
                "correctPerspective": false,
                "removeShadows": false,
                "recogniseText": false,
                "straighten": false,
                "ocrLanguage": "eng",
                "ocrUserWords": [ocr_hint],
                "outputProtection": {
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                }
            }
        }))
        .unwrap()
    }

    fn redaction_request(
        input: &Path,
        output: &Path,
        password: Option<&str>,
        png_data_url: &str,
        output_protection: Option<(&str, &str)>,
    ) -> StartPdfJobRequest {
        let metadata = fs::metadata(input).unwrap();
        serde_json::from_value(serde_json::json!({
            "kind": "redaction",
            "request": {
                "acknowledgeCertificateSignatures": false,
                "expectedSourceModifiedAtMs": metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .and_then(|value| u64::try_from(value.as_millis()).ok()),
                "expectedSourceSize": metadata.len(),
                "inputPassword": password,
                "inputPath": input.to_string_lossy(),
                "outputPath": output.to_string_lossy(),
                "outputProtection": output_protection.map(|(open_password, owner_password)| serde_json::json!({
                    "openPassword": open_password,
                    "ownerPassword": owner_password
                })),
                "pages": [{
                    "pageNumber": 1,
                    "pngDataUrl": png_data_url,
                    "regions": [{
                        "colour": "black",
                        "height": 0.2,
                        "width": 0.2,
                        "x": 0.1,
                        "y": 0.1
                    }]
                }]
            }
        }))
        .unwrap()
    }

    fn ocr_review_request(input: &Path) -> StartPdfJobRequest {
        StartPdfJobRequest::OcrReview(ReviewScanOcrRequest {
            input_path: input.to_string_lossy().into_owned(),
            colour_mode: crate::scan_export::ScanColourMode::Colour,
            auto_orient: true,
            auto_crop: false,
            correct_perspective: false,
            remove_shadows: false,
            language: "eng".to_string(),
        })
    }

    fn scan_preview_request(input: &Path) -> StartPdfJobRequest {
        StartPdfJobRequest::ScanPreview(PreviewScanImageRequest {
            input_path: input.to_string_lossy().into_owned(),
            colour_mode: crate::scan_export::ScanColourMode::Colour,
            auto_orient: true,
            auto_crop: false,
            correct_perspective: false,
            remove_shadows: false,
        })
    }

    fn scanner_capture_request() -> StartPdfJobRequest {
        StartPdfJobRequest::ScannerCapture(CaptureScannerPagesRequest {
            device_id: "private:test-scanner".to_string(),
            source: crate::scanner::ScannerSource::Feeder,
            duplex: true,
            dpi: 300,
            colour_mode: crate::scanner::ScannerColourMode::Greyscale,
            paper_width_mm: 210.0,
            paper_height_mm: 297.0,
            page_limit: 25,
        })
    }

    fn test_redaction_png() -> String {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(595, 842, Rgb([24, 24, 24])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(bytes.into_inner())
        )
    }

    fn save_fixture(path: &Path) {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"BT 20 30 Td (Visible page) Tj ET".to_vec(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalogue_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        let info_id = document.add_object(dictionary! {
            "Author" => Object::string_literal("Private Author"),
        });
        document.trailer.set("Root", catalogue_id);
        document.trailer.set("Info", info_id);
        document.save(path).unwrap().sync_all().unwrap();
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = create_unique_test_directory("tufekci-paperworks-pdf-job-test");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
