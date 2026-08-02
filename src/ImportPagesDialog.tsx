import { type FormEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FilePlus2, FolderOpen, KeyRound, Loader2, ShieldAlert, X } from "lucide-react";
import { useDialogFocus } from "./accessibility";
import { useI18n } from "./I18nProvider";
import {
  createPdfLoadingTask,
  isIncorrectPasswordReason,
  type PDFDocumentProxy,
  type PdfRangeSource
} from "./pdf";
import { PdfJobProgress } from "./PdfJobProgress";
import { localisePdfJobConnectionError } from "./pdfJobs";
import { usePdfJob } from "./usePdfJob";

type PageImportInspection = {
  certificateSignature: boolean;
  encrypted: boolean;
  pageCount: number;
  selectedPages: number[];
};

export type ImportedPdfReady = {
  certificateAcknowledged: boolean;
  certificateSignature: boolean;
  document: PDFDocumentProxy;
  loadingTask: ReturnType<typeof createPdfLoadingTask>;
  modifiedAtMs: number | null;
  name: string;
  password: string | null;
  path: string;
  selectedPages: number[];
  size: number;
};

type ImportPagesDialogProps = {
  desktopMode: boolean;
  onClose: () => void;
  onImport: (source: ImportedPdfReady) => void;
  open: boolean;
};

export function ImportPagesDialog({
  desktopMode,
  onClose,
  onImport,
  open: visible
}: ImportPagesDialogProps) {
  const { formatNumber, t } = useI18n();
  const [sourcePath, setSourcePath] = useState("");
  const [password, setPassword] = useState("");
  const [pageRange, setPageRange] = useState("all");
  const [inspection, setInspection] = useState<PageImportInspection | null>(null);
  const [certificateAcknowledged, setCertificateAcknowledged] = useState(false);
  const [busy, setBusy] = useState<"adding" | "choosing" | "reviewing" | null>(null);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const pageImportInspectionJob = usePdfJob<PageImportInspection>(
    desktopMode,
    "page-import-inspection"
  );
  const interfaceBusy = busy !== null || pageImportInspectionJob.isActive;

  useEffect(() => {
    if (!visible) {
      return;
    }
    setSourcePath("");
    setPassword("");
    setPageRange("all");
    setInspection(null);
    setCertificateAcknowledged(false);
    setReviewCancelBusy(false);
    setBusy(null);
    setError(null);
  }, [visible]);

  useEffect(
    () => () => {
      void loadingTaskRef.current?.destroy();
    },
    []
  );

  const invalidateReview = () => {
    setInspection(null);
    setCertificateAcknowledged(false);
    setError(null);
    pageImportInspectionJob.clearJob();
  };

  const chooseSource = async () => {
    setBusy("choosing");
    setError(null);
    try {
      const selection = await open({
        directory: false,
        filters: [{ name: t("app.dialog.export.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("importPages.dialog.choose")
      });
      if (typeof selection === "string") {
        setSourcePath(selection);
        setPassword("");
        setPageRange("all");
        invalidateReview();
      }
    } catch (reason) {
      void reason;
      setError(t("importPages.error.choose"));
    } finally {
      setBusy(null);
    }
  };

  const reviewPages = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!desktopMode || !sourcePath || !pageRange.trim() || interfaceBusy) {
      return;
    }
    setBusy("reviewing");
    setReviewCancelBusy(false);
    setError(null);
    pageImportInspectionJob.clearJob();
    try {
      const result = await pageImportInspectionJob.startJobAndWait({
        inputPassword: password || null,
        inputPath: sourcePath,
        pageRange: pageRange.trim()
      });
      setInspection(result);
      setCertificateAcknowledged(false);
      pageImportInspectionJob.clearJob();
    } catch (reason) {
      setInspection(null);
      setError(
        reason instanceof Error && reason.message === "The PDF job was cancelled."
          ? t("importPages.error.reviewCancelled")
          : t("importPages.error.review")
      );
    } finally {
      setReviewCancelBusy(false);
      setBusy(null);
    }
  };

  const cancelPageImportReview = async () => {
    if (!pageImportInspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await pageImportInspectionJob.cancelJob();
    } catch (reason) {
      void reason;
      setError(t("importPages.error.cancelReview"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const addPages = async () => {
    if (
      !inspection ||
      busy ||
      (inspection.certificateSignature && !certificateAcknowledged)
    ) {
      return;
    }
    setBusy("adding");
    setError(null);
    try {
      const source = await invoke<PdfRangeSource>("open_local_pdf", { path: sourcePath });
      const task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure: string | null = null;
      task.onPassword = (_updatePassword: (password: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? t("importPages.error.passwordIncorrect")
          : t("importPages.error.passwordRequired");
        void task.destroy();
      };
      let document: PDFDocumentProxy;
      try {
        document = await task.promise;
      } catch (reason) {
        if (passwordFailure) {
          throw new Error(passwordFailure);
        }
        throw reason;
      }
      if (document.numPages !== inspection.pageCount) {
        await task.destroy();
        throw new Error(t("importPages.error.sourceChanged"));
      }
      onImport({
        certificateAcknowledged,
        certificateSignature: inspection.certificateSignature,
        document,
        loadingTask: task,
        modifiedAtMs: source.modifiedAtMs,
        name: source.name,
        password: password || null,
        path: source.path,
        selectedPages: inspection.selectedPages,
        size: source.size
      });
      loadingTaskRef.current = null;
      onClose();
    } catch (reason) {
      const failedTask = loadingTaskRef.current;
      void failedTask?.destroy();
      loadingTaskRef.current = null;
      setError(
        reason instanceof Error &&
          [
            t("importPages.error.passwordIncorrect"),
            t("importPages.error.passwordRequired"),
            t("importPages.error.sourceChanged")
          ].includes(reason.message)
          ? reason.message
          : t("importPages.error.load")
      );
    } finally {
      setBusy(null);
    }
  };

  const closeDialog = () => {
    if (pageImportInspectionJob.isActive) {
      return;
    }
    void loadingTaskRef.current?.destroy();
    loadingTaskRef.current = null;
    pageImportInspectionJob.clearJob();
    onClose();
  };

  const dialogRef = useDialogFocus<HTMLFormElement>({
    active: visible,
    escapeDisabled: interfaceBusy,
    onEscape: closeDialog
  });

  if (!visible) {
    return null;
  }

  const reviewJobPanel = pageImportInspectionJob.job ? (
    <PdfJobProgress
      cancelling={reviewCancelBusy}
      connectionError={pageImportInspectionJob.connectionError}
      job={pageImportInspectionJob.job}
      onCancel={() => void cancelPageImportReview()}
      onRetry={() => void reviewPages()}
      retryDisabled={!desktopMode || !sourcePath || !pageRange.trim() || interfaceBusy}
    />
  ) : null;

  return (
    <div className="dialog-backdrop" role="presentation">
      <form
        aria-labelledby="import-pages-title"
        aria-modal="true"
        className="import-pages-dialog"
        data-dialog-root
        onSubmit={reviewPages}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header>
          <div className="dialog-icon" aria-hidden="true">
            <FilePlus2 size={24} />
          </div>
          <div>
            <span className="eyebrow">{t("importPages.eyebrow")}</span>
            <h2 id="import-pages-title">{t("importPages.title")}</h2>
          </div>
          <button
            aria-label={t("importPages.close.aria")}
            className="icon-button"
            data-dialog-initial-focus
            disabled={busy === "adding" || pageImportInspectionJob.isActive}
            onClick={closeDialog}
            title={t("common.close")}
            type="button"
          >
            <X size={17} aria-hidden="true" />
          </button>
        </header>

        <div className="import-source-row">
          <div>
            <span>{t("importPages.source.label")}</span>
            <strong>{sourcePath ? fileNameFromPath(sourcePath) : t("importPages.source.none")}</strong>
            {sourcePath ? <small>{sourcePath}</small> : null}
          </div>
          <button disabled={interfaceBusy} onClick={() => void chooseSource()} type="button">
            {busy === "choosing" ? (
              <Loader2 className="spin" size={16} aria-hidden="true" />
            ) : (
              <FolderOpen size={16} aria-hidden="true" />
            )}
            {t("importPages.action.choose")}
          </button>
        </div>

        <div className="import-page-fields">
          <label>
            {t("importPages.pages.label")}
            <input
              disabled={interfaceBusy}
              onChange={(event) => {
                setPageRange(event.target.value);
                invalidateReview();
              }}
              placeholder={t("importPages.pages.placeholder")}
              spellCheck={false}
              value={pageRange}
            />
          </label>
          <label>
            {t("importPages.password.label")} <span>{t("importPages.password.optional")}</span>
            <div className="password-dialog-field">
              <KeyRound size={16} aria-hidden="true" />
              <input
                autoComplete="off"
                disabled={interfaceBusy}
                onChange={(event) => {
                  setPassword(event.target.value);
                  invalidateReview();
                }}
                spellCheck={false}
                type="password"
                value={password}
              />
            </div>
          </label>
        </div>

        {inspection ? (
          <section className="import-review" aria-live="polite">
            <div>
              <strong>
                {t(
                  inspection.selectedPages.length === 1
                    ? "importPages.review.ready.one"
                    : "importPages.review.ready.other",
                  { count: formatNumber(inspection.selectedPages.length) }
                )}
              </strong>
              <span>
                {t(
                  inspection.pageCount === 1
                    ? "importPages.review.source.one"
                    : "importPages.review.source.other",
                  { count: formatNumber(inspection.pageCount) }
                )}
                {` | ${t(
                  inspection.encrypted
                    ? "importPages.review.encrypted"
                    : "importPages.review.unencrypted"
                )}`}
              </span>
            </div>
            {inspection.certificateSignature ? (
              <label className="signature-risk-check">
                <input
                  checked={certificateAcknowledged}
                  onChange={(event) => setCertificateAcknowledged(event.target.checked)}
                  type="checkbox"
                />
                <ShieldAlert size={17} aria-hidden="true" />
                <span>
                  {t("importPages.certificate.acknowledgement")}
                </span>
              </label>
            ) : null}
          </section>
        ) : null}

        {error ? <p className="dialog-error" role="alert">{error}</p> : null}
        {reviewJobPanel}
        {!pageImportInspectionJob.job && pageImportInspectionJob.connectionError ? (
          <p className="dialog-error" role="status">
            {localisePdfJobConnectionError(
              pageImportInspectionJob.connectionError,
              t
            )}
          </p>
        ) : null}

        <div className="dialog-actions">
          <button
            disabled={busy === "adding" || pageImportInspectionJob.isActive}
            onClick={closeDialog}
            type="button"
          >
            {t("common.cancel")}
          </button>
          {inspection ? (
            <button
              className="primary"
              disabled={
                interfaceBusy ||
                (inspection.certificateSignature && !certificateAcknowledged)
              }
              onClick={() => void addPages()}
              type="button"
            >
              {busy === "adding" ? (
                <Loader2 className="spin" size={16} aria-hidden="true" />
              ) : (
                <FilePlus2 size={16} aria-hidden="true" />
              )}
              {t(
                inspection.selectedPages.length === 1
                  ? "importPages.action.add.one"
                  : "importPages.action.add.other",
                { count: formatNumber(inspection.selectedPages.length) }
              )}
            </button>
          ) : (
            <button
              className="primary"
              disabled={!desktopMode || !sourcePath || !pageRange.trim() || interfaceBusy}
              type="submit"
            >
              {busy === "reviewing" || pageImportInspectionJob.isActive ? (
                <Loader2 className="spin" size={16} aria-hidden="true" />
              ) : null}
              {t("importPages.action.review")}
            </button>
          )}
        </div>
      </form>
    </div>
  );
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}
