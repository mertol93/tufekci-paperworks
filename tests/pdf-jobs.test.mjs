import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildPdfJobDiagnostic,
  isActivePdfJob,
  isInterruptedPdfJob,
  localisePdfJobConnectionError,
  localisePdfJobFailure,
  localisePdfJobStage,
  selectRecoverablePdfJob
} from "../src/pdfJobs.ts";
import { translate } from "../src/i18n.ts";

function snapshot(status) {
  return {
    createdAtMs: 1,
    error: null,
    jobId: "privacy-1-1",
    kind: "privacy",
    progress: 0,
    result: null,
    stage: "Waiting",
    status,
    updatedAtMs: 1
  };
}

test("treats queued and running PDF jobs as active", () => {
  assert.equal(isActivePdfJob(snapshot("queued")), true);
  assert.equal(isActivePdfJob(snapshot("running")), true);
});

test("treats every terminal PDF job state as inactive", () => {
  assert.equal(isActivePdfJob(snapshot("succeeded")), false);
  assert.equal(isActivePdfJob(snapshot("failed")), false);
  assert.equal(isActivePdfJob(snapshot("cancelled")), false);
  assert.equal(isActivePdfJob(null), false);
});

test("recognises only strict interrupted recovery snapshots", () => {
  const interrupted = {
    ...snapshot("failed"),
    jobId: "interrupted-scan-72-1750000000000-4",
    kind: "scan",
    stage: "Previous job interrupted"
  };
  assert.equal(isInterruptedPdfJob(interrupted), true);
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-annotation-inspection-72-1750000000000-5",
      kind: "annotation-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-bookmark-inspection-72-1750000000000-6",
      kind: "bookmark-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-form-inspection-72-1750000000000-7",
      kind: "form-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-finishing-inspection-72-1750000000000-8",
      kind: "finishing-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-redaction-inspection-72-1750000000000-9",
      kind: "redaction-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-page-import-inspection-72-1750000000000-10",
      kind: "page-import-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-page-transfer-72-1750000000000-11",
      kind: "page-transfer"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-batch-inspection-72-1750000000000-11",
      kind: "batch-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-edit-safety-inspection-72-1750000000000-12",
      kind: "edit-safety-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-health-72-1750000000000-5",
      kind: "health"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-certificate-validation-72-1750000000000-6",
      kind: "certificate-validation"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-compression-preview-72-1750000000000-7",
      kind: "compression-preview"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-privacy-inspection-72-1750000000000-8",
      kind: "privacy-inspection"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-ocr-review-72-1750000000000-9",
      kind: "ocr-review"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-searchable-ocr-72-1750000000000-10",
      kind: "searchable-ocr"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-scan-preview-72-1750000000000-10",
      kind: "scan-preview"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-scanner-capture-72-1750000000000-11",
      kind: "scanner-capture"
    }),
    true
  );
  assert.equal(
    isInterruptedPdfJob({
      ...interrupted,
      jobId: "interrupted-archive-72-1750000000000-12",
      kind: "archive"
    }),
    true
  );
  assert.equal(isInterruptedPdfJob({ ...interrupted, status: "running" }), false);
  assert.equal(isInterruptedPdfJob({ ...interrupted, stage: "PDF job could not complete" }), false);
  assert.equal(isInterruptedPdfJob({ ...interrupted, jobId: "interrupted-scan-private.pdf" }), false);
  assert.equal(isInterruptedPdfJob(null), false);
});

test("selects the newest active or interrupted job and ignores ordinary history", () => {
  const interrupted = {
    ...snapshot("failed"),
    jobId: "interrupted-privacy-72-1750000000000-4",
    stage: "Previous job interrupted"
  };
  const running = {
    ...snapshot("running"),
    jobId: "privacy-72-5"
  };
  assert.equal(
    selectRecoverablePdfJob([snapshot("succeeded"), interrupted])?.jobId,
    interrupted.jobId
  );
  assert.equal(
    selectRecoverablePdfJob([interrupted, running])?.jobId,
    running.jobId
  );
  assert.equal(selectRecoverablePdfJob([snapshot("failed"), snapshot("cancelled")]), undefined);
});

test("builds bounded diagnostics only from the public job snapshot", () => {
  const diagnostic = buildPdfJobDiagnostic(
    {
      createdAtMs: 1_750_000_000_000,
      error: "The prepared output could not be verified.",
      jobId: "job-42",
      kind: "merge",
      progress: 88.6,
      result: {
        outputPath: "C:\\Private\\result.pdf",
        password: "must-not-appear",
        recognisedText: "confidential recognised words"
      },
      stage: "Verifying prepared output",
      status: "failed",
      updatedAtMs: 1_750_000_001_000
    },
    "status-unavailable"
  );

  assert.match(diagnostic, /Kind: merge/u);
  assert.match(diagnostic, /Progress: 89%/u);
  assert.match(diagnostic, /The prepared output could not be verified/u);
  assert.match(diagnostic, /Status connection code: status-unavailable/u);
  assert.doesNotMatch(
    diagnostic,
    /Private|result\\.pdf|must-not-appear|password|confidential recognised words/u
  );
});

test("localises typed job and connection failures without rendering native detail", () => {
  const t = (key, values) => translate("en-GB", key, values);
  assert.equal(
    localisePdfJobFailure(
      {
        ...snapshot("failed"),
        error: "Private merge parser detail",
        errorCode: "merge-failed",
        kind: "merge"
      },
      t
    ),
    "The merge job could not complete."
  );
  assert.equal(
    localisePdfJobFailure(
      {
        ...snapshot("failed"),
        error: "Private legacy merge detail",
        kind: "merge"
      },
      t
    ),
    "The merge job could not complete."
  );
  assert.equal(
    localisePdfJobConnectionError("history-unavailable", t),
    "Recent PDF job history is temporarily unavailable. New operations can still be started."
  );
  assert.equal(
    localisePdfJobStage(
      {
        ...snapshot("running"),
        kind: "merge",
        stage: "Private merge source detail",
        stageCode: "merge-preparing"
      },
      t
    ),
    "Building the combined PDF"
  );
  assert.equal(
    localisePdfJobStage(
      { ...snapshot("running"), stage: "Private legacy stage" },
      t
    ),
    "Starting PDF job"
  );
});

test("keeps clear-job identity stable while native snapshots change", () => {
  const jobHook = readFileSync(new URL("../src/usePdfJob.ts", import.meta.url), "utf8");

  assert.match(jobHook, /const jobRef = useRef<PdfJobSnapshot<TResult> \| null>\(job\);/u);
  assert.match(jobHook, /jobRef\.current = job;/u);
  assert.match(
    jobHook,
    /const clearJob = useCallback\(\(\) => \{\s*if \(isActivePdfJob\(jobRef\.current\)\)/u
  );
  assert.match(jobHook, /setConnectionError\(null\);\s*\}, \[storageKey\]\);/u);
  assert.doesNotMatch(jobHook, /\}, \[job, storageKey\]\);/u);
  assert.doesNotMatch(jobHook, /reason\.message|String\(reason\)|function errorMessage/u);
  assert.match(jobHook, /setConnectionError\("status-unavailable"\)/u);
});

test("routes scan clean-up previews through the shared cancellable job lifecycle", () => {
  const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

  assert.match(
    appSource,
    /usePdfJob<ScanPreviewResult>\(\s*mode === "desktop",\s*"scan-preview"\s*\)/u
  );
  assert.match(appSource, /scanPreviewJob\.cancelJob\(\)/u);
  assert.match(appSource, /<PdfJobProgress[\s\S]*?scanPreviewJob\.connectionError/u);
  assert.doesNotMatch(
    appSource,
    /invoke<ScanPreviewResult>\(\s*"preview_scan_image"/u
  );
});

test("routes connected scanner capture through the shared cancellable job lifecycle", () => {
  const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

  assert.match(
    appSource,
    /usePdfJob<ScannerCaptureResult>\(\s*connectedScanningAvailable,\s*"scanner-capture"\s*\)/u
  );
  assert.match(appSource, /scannerCaptureJob\.cancelJob\(\)/u);
  assert.match(appSource, /scannerCaptureJob\.connectionError/u);
  assert.match(
    appSource,
    /for \(const path of paths\) \{\s*const data = await invoke<ArrayBuffer>\("read_local_document"/u
  );
  assert.match(appSource, /t\("scanner\.capture\.retryOpen"\)/u);
  assert.match(appSource, /t\("scanner\.capture\.discard"\)/u);
  assert.doesNotMatch(
    appSource,
    /invoke<ScannerCaptureResult>\(\s*"capture_scanner_pages"/u
  );
});

test("routes standalone searchable OCR through the shared cancellable job lifecycle", () => {
  const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const studio = readFileSync(
    new URL("../src/SearchableOcrStudio.tsx", import.meta.url),
    "utf8"
  );

  assert.match(
    studio,
    /usePdfJob<SearchableOcrResult>\(desktopMode, "searchable-ocr"\)/u
  );
  assert.match(studio, /ocrJob\.startJob\(/u);
  assert.match(studio, /ocrJob\.cancelJob\(\)/u);
  assert.match(studio, /<PdfJobProgress/u);
  assert.match(studio, /<PdfEditSafetyNotice/u);
  assert.match(studio, /<OutputProtectionFields/u);
  assert.match(appSource, /const ocrWorkflowActive = displayWorkflow\.id === "ocr"/u);
  assert.match(appSource, /<SearchableOcrStudio/u);
  assert.doesNotMatch(appSource, /This operation is not connected yet/u);
});

test("routes annotation review through a typed cancellable inspection job", () => {
  const studio = readFileSync(new URL("../src/AnnotationStudio.tsx", import.meta.url), "utf8");
  const jobHook = readFileSync(new URL("../src/usePdfJob.ts", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<PdfAnnotationInspection>\(\s*desktopMode,\s*"annotation-inspection"\s*\)/u
  );
  assert.match(studio, /annotationInspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /annotationInspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /annotationInspectionJob\.connectionError/u);
  assert.doesNotMatch(
    studio,
    /invoke<PdfAnnotationInspection>\(\s*"inspect_pdf_annotations"/u
  );
  assert.doesNotMatch(nativeLibrary, /annotations::inspect_pdf_annotations/u);
  assert.match(jobHook, /const startJobAndWait = useCallback/u);
  assert.match(jobHook, /settleWaiter\(snapshot\)/u);
});

test("routes page-content review and publication through typed cancellable jobs", () => {
  const studio = readFileSync(new URL("../src/ContentEditStudio.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const nativeJobs = readFileSync(new URL("../src-tauri/src/pdf_jobs.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<ExportPdfContentResult>\(desktopMode, "content"\)/u
  );
  assert.match(
    studio,
    /usePdfJob<PdfContentInspection>\(\s*desktopMode,\s*"content-inspection"\s*\)/u
  );
  assert.match(studio, /inspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /inspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /contentJob\.startJob\(/u);
  assert.match(studio, /contentJob\.cancelJob\(\)/u);
  assert.match(studio, /expectedSourceSha256: inspection\.sourceSha256/u);
  assert.doesNotMatch(studio, /invoke<[^>]+>\(\s*"(?:inspect|export)_pdf_content"/u);
  assert.doesNotMatch(nativeLibrary, /content_editor::(?:inspect|export)_pdf_content/u);
  assert.match(nativeJobs, /StartPdfJobRequest::ContentInspection/u);
  assert.match(nativeJobs, /StartPdfJobRequest::Content\(/u);
});

test("routes bookmark review through a typed cancellable inspection job", () => {
  const studio = readFileSync(new URL("../src/BookmarkStudio.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<PdfBookmarkInspection>\(\s*desktopMode,\s*"bookmark-inspection"\s*\)/u
  );
  assert.match(studio, /bookmarkInspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /bookmarkInspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /bookmarkInspectionJob\.connectionError/u);
  assert.doesNotMatch(
    studio,
    /invoke<PdfBookmarkInspection>\(\s*"inspect_pdf_bookmarks"/u
  );
  assert.doesNotMatch(nativeLibrary, /bookmarks::inspect_pdf_bookmarks/u);
});

test("routes form review through a typed cancellable inspection job", () => {
  const studio = readFileSync(new URL("../src/FormStudio.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<PdfFormInspection>\(\s*desktopMode,\s*"form-inspection"\s*\)/u
  );
  assert.match(studio, /formInspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /formInspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /formInspectionJob\.connectionError/u);
  assert.doesNotMatch(studio, /invoke<PdfFormInspection>\(\s*"inspect_pdf_forms"/u);
  assert.doesNotMatch(nativeLibrary, /forms::inspect_pdf_forms/u);
});

test("routes Page Finish review through a typed cancellable inspection job", () => {
  const studio = readFileSync(new URL("../src/PageFinishStudio.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<PdfFinishingInspection>\(\s*desktopMode,\s*"finishing-inspection"\s*\)/u
  );
  assert.match(studio, /finishingInspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /finishingInspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /finishingInspectionJob\.connectionError/u);
  assert.doesNotMatch(
    studio,
    /invoke<PdfFinishingInspection>\(\s*"inspect_pdf_finishing"/u
  );
  assert.doesNotMatch(nativeLibrary, /page_finish::inspect_pdf_finishing/u);
});

test("routes redaction review through a typed cancellable inspection job", () => {
  const studio = readFileSync(new URL("../src/RedactionStudio.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<PdfRedactionInspection>\(\s*desktopMode,\s*"redaction-inspection"\s*\)/u
  );
  assert.match(studio, /redactionInspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /redactionInspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /redactionInspectionJob\.connectionError/u);
  assert.doesNotMatch(
    studio,
    /invoke<PdfRedactionInspection>\(\s*"inspect_pdf_redaction"/u
  );
  assert.doesNotMatch(nativeLibrary, /redaction::inspect_pdf_redaction/u);
});

test("routes page import review through a typed cancellable inspection job", () => {
  const dialog = readFileSync(new URL("../src/ImportPagesDialog.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    dialog,
    /usePdfJob<PageImportInspection>\(\s*desktopMode,\s*"page-import-inspection"\s*\)/u
  );
  assert.match(dialog, /pageImportInspectionJob\.startJobAndWait\(/u);
  assert.match(dialog, /pageImportInspectionJob\.cancelJob\(\)/u);
  assert.match(dialog, /pageImportInspectionJob\.connectionError/u);
  assert.doesNotMatch(
    dialog,
    /invoke<PageImportInspection>\(\s*"inspect_page_import"/u
  );
  assert.doesNotMatch(nativeLibrary, /combine::inspect_page_import/u);
});

test("routes Batch Recipe source review through its own typed inspection job", () => {
  const studio = readFileSync(new URL("../src/BatchRecipeStudio.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    studio,
    /usePdfJob<InspectBatchSourcesResult>\(\s*desktopMode,\s*"batch-inspection"\s*\)/u
  );
  assert.match(studio, /batchInspectionJob\.startJobAndWait\(/u);
  assert.match(studio, /batchInspectionJob\.cancelJob\(\)/u);
  assert.match(studio, /batchInspectionJob\.connectionError/u);
  assert.doesNotMatch(studio, /invoke<PdfPrivacyInspectionResult>\(\s*"inspect_pdf_privacy"/u);
  assert.doesNotMatch(nativeLibrary, /privacy_inspection::inspect_pdf_privacy/u);
});

test("routes shared edit-safety review through one typed aggregate inspection job", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const hook = readFileSync(new URL("../src/usePdfEditSafety.ts", import.meta.url), "utf8");
  const jobHook = readFileSync(new URL("../src/usePdfJob.ts", import.meta.url), "utf8");
  const notice = readFileSync(new URL("../src/PdfEditSafetyNotice.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  assert.match(
    hook,
    /usePdfJob<PdfEditSafetyInspectionResult>\(\s*desktopMode,\s*"edit-safety-inspection",\s*storageScope\s*\)/u
  );
  assert.match(hook, /editSafetyJob\.startJobAndWait\(/u);
  assert.match(hook, /requestPdfJobCancellation\(/u);
  assert.match(hook, /editSafetyJob\.connectionError/u);
  assert.match(jobHook, /if \(normalisedStorageScope\) \{\s*return;/u);
  assert.match(notice, /<PdfJobProgress/u);
  assert.match(app, /usePdfEditSafety\(/u);
  assert.doesNotMatch(app, /"inspect_pdf_edit_safety"/u);
  assert.doesNotMatch(hook, /"inspect_pdf_edit_safety"/u);
  assert.doesNotMatch(nativeLibrary, /health::inspect_pdf_edit_safety/u);
});

test("routes scan PDF export through the generic typed job lifecycle", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const nativeJobs = readFileSync(
    new URL("../src-tauri/src/pdf_jobs.rs", import.meta.url),
    "utf8"
  );

  assert.match(
    app,
    /usePdfJob<ScanExportResult>\(mode === "desktop", "scan"\)/u
  );
  assert.match(app, /scanExportJob\.startJob\(/u);
  assert.match(app, /scanExportJob\.cancelJob\(\)/u);
  assert.match(app, /scanExportJob\.connectionError/u);
  assert.doesNotMatch(
    app,
    /"(?:cancel|get|list|start)_scan_pdf_job"/u
  );
  assert.doesNotMatch(
    nativeLibrary,
    /pdf_jobs::(?:cancel|get|list|start)_scan_pdf_job/u
  );
  assert.doesNotMatch(
    nativeJobs,
    /pub fn (?:cancel|get|list|start)_scan_pdf_job/u
  );
});

test("keeps encrypted certificate PDF passwords inside the generic private job path", () => {
  const studio = readFileSync(
    new URL("../src/CertificateStudio.tsx", import.meta.url),
    "utf8"
  );
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const signatureStudio = readFileSync(
    new URL("../src/SignatureStudio.tsx", import.meta.url),
    "utf8"
  );
  const nativeCertificate = readFileSync(
    new URL("../src-tauri/src/certificate.rs", import.meta.url),
    "utf8"
  );
  const passwordBridge = readFileSync(
    new URL("../src-tauri/src/pyhanko_password_bridge.py", import.meta.url),
    "utf8"
  );

  assert.match(studio, /usePdfJob<CertificateSignResult>\(desktopMode, "certificate"\)/u);
  assert.match(
    studio,
    /usePdfJob<CertificateValidationReport>\(\s*desktopMode,\s*"certificate-validation"\s*\)/u
  );
  assert.equal(studio.match(/inputPassword: inputPassword \|\| null/gu)?.length, 2);
  assert.match(app, /initialSourcePassword=\{pdf\.openingPassword \?\? undefined\}/u);
  assert.match(signatureStudio, /initialSourcePassword=\{initialSourcePassword\}/u);
  assert.match(nativeCertificate, /\.stdin\(Stdio::piped\(\)\)/u);
  assert.match(nativeCertificate, /TemporaryKind::PyHankoPasswordBridge/u);
  assert.doesNotMatch(nativeCertificate, /"--password"/u);
  assert.match(passwordBridge, /sys\.stdin\.buffer\.readline/u);
  assert.doesNotMatch(passwordBridge, /sys\.argv/u);
});

test("does not register legacy direct wrappers for scheduler-backed work", () => {
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

  for (const command of [
    "certificate::inspect_certificate_signatures",
    "compression::preview_pdf_compression",
    "health::inspect_pdf_edit_safety",
    "health::inspect_pdf_health",
    "pdf_jobs::cancel_scan_pdf_job",
    "pdf_jobs::get_scan_pdf_job",
    "pdf_jobs::list_scan_pdf_jobs",
    "pdf_jobs::start_scan_pdf_job",
    "scan_export::preview_scan_image",
    "scan_export::review_scan_ocr",
    "scanner::capture_scanner_pages"
  ]) {
    assert.doesNotMatch(nativeLibrary, new RegExp(command.replaceAll(".", "\\."), "u"));
  }
});

test("registers only the generic scheduler and the reviewed bounded support commands", () => {
  const nativeLibrary = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const handler = nativeLibrary.match(
    /\.invoke_handler\(tauri::generate_handler!\[\s*([\s\S]*?)\s*\]\)/u
  );
  assert.ok(handler, "the Tauri command handler must remain statically inspectable");
  const commands = handler[1]
    .split(",")
    .map((command) => command.trim())
    .filter(Boolean);

  assert.deepEqual(commands, [
    "app_updates::check_for_update",
    "app_updates::install_update",
    "app_updates::restart_after_update",
    "app_updates::update_readiness",
    "archive::pdf_archive_readiness",
    "certificate::certificate_capabilities",
    "document_io::read_local_document",
    "document_io::open_local_pdf",
    "document_io::read_local_pdf_range",
    "ocr::ocr_readiness",
    "operation_audit::clear_operation_audit",
    "operation_audit::export_operation_audit",
    "operation_audit::list_operation_audit",
    "pdf_jobs::cancel_pdf_job",
    "pdf_jobs::get_pdf_job",
    "pdf_jobs::list_pdf_jobs",
    "pdf_jobs::start_pdf_job",
    "pdf_tools::probe_tools",
    "pdf_tools::scan_presets",
    "pdf_tools::signature_capabilities",
    "protection::protection_capabilities",
    "recovery::clear_recovery_snapshots",
    "recovery::load_recovery_snapshot",
    "recovery::save_recovery_snapshot",
    "runtime_capabilities::runtime_capabilities",
    "scanner::list_scanners",
    "signature_vault::delete_signature_vault",
    "signature_vault::list_signature_vault",
    "signature_vault::store_signature_vault",
    "signature_vault::unlock_signature_vault",
    "temporary_cleanup::temporary_cleanup_status"
  ]);
});
