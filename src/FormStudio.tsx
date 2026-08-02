import {
  type ChangeEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useDialogFocus } from "./accessibility";
import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleDot,
  Eye,
  EyeOff,
  FileInput,
  FileText,
  FolderOpen,
  Info,
  ListChecks,
  Loader2,
  LockKeyhole,
  Redo2,
  RotateCcw,
  Save,
  Search,
  ShieldAlert,
  SquareCheckBig,
  Type,
  Undo2,
  X
} from "lucide-react";
import {
  changedFormUpdates,
  createFormHistory,
  initialFormValues,
  redoFormValues,
  undoFormValues,
  updateFormField,
  validateFormDraft,
  type FormField,
  type FormFieldKind,
  type FormHistory
} from "./formDraft";
import {
  localiseFormDraftError,
  localiseFormKind,
  localiseFormWarnings
} from "./formLocalisation";
import { useI18n } from "./I18nProvider";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import { PdfPageCanvas } from "./PdfPageCanvas";
import { localisePdfJobFailure } from "./pdfJobs";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import {
  createPdfLoadingTask,
  isIncorrectPasswordReason,
  type PDFDocumentProxy,
  type PdfRangeSource
} from "./pdf";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

type FormStudioProps = {
  desktopMode: boolean;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  qpdfAvailable: boolean;
};

type PdfFormInspection = {
  certificateSignature: boolean;
  editableFieldCount: number;
  fieldCount: number;
  fields: FormField[];
  fileName: string;
  flattenableFieldCount: number;
  hasXfa: boolean;
  pageCount: number;
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  warnings: string[];
  wasEncrypted: boolean;
};

type ExportPdfFormsResult = {
  bytesWritten: number;
  encryption: "AES-256" | "None";
  flattenedFieldCount: number;
  outputPath: string;
  pageCount: number;
  remainingFieldCount: number;
  updatedFieldCount: number;
  warnings: string[];
};

type FieldFilter = "all" | "changed" | "required" | "readOnly";

export function FormStudio({
  desktopMode,
  initialSourcePassword,
  initialSourcePath,
  qpdfAvailable
}: FormStudioProps) {
  const { formatList, formatNumber, locale, t } = useI18n();
  const [sourcePath, setSourcePath] = useState<string | null>(initialSourcePath ?? null);
  const [password, setPassword] = useState(initialSourcePassword ?? "");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState<"export" | "review" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [inspection, setInspection] = useState<PdfFormInspection | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [history, setHistory] = useState<FormHistory>(() => createFormHistory([]));
  const [selectedFieldId, setSelectedFieldId] = useState<string | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [fieldSearch, setFieldSearch] = useState("");
  const [fieldFilter, setFieldFilter] = useState<FieldFilter>("all");
  const [flatten, setFlatten] = useState(false);
  const [showFieldPassword, setShowFieldPassword] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [exportResult, setExportResult] = useState<ExportPdfFormsResult | null>(null);
  const [jobNotice, setJobNotice] = useState<string | null>(null);
  const [cancelBusy, setCancelBusy] = useState(false);
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const loadingTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const mountedRef = useRef(true);
  const requestRunRef = useRef(0);
  const previewHostRef = useRef<HTMLDivElement>(null);
  const [previewWidth, setPreviewWidth] = useState(640);
  const sourceList = useMemo(
    () =>
      sourcePath
        ? [
            {
              id: "form-source",
              label: fileNameFromPath(sourcePath),
              password,
              path: sourcePath
            }
          ]
        : [],
    [password, sourcePath]
  );
  const editSafety = usePdfEditSafety(desktopMode, sourceList, "forms");
  const formJob = usePdfJob<ExportPdfFormsResult>(desktopMode, "forms");
  const formInspectionJob = usePdfJob<PdfFormInspection>(desktopMode, "form-inspection");
  const operationBusy = busy !== null || formJob.isActive || formInspectionJob.isActive;
  const fields = inspection?.fields ?? [];
  const changedUpdates = useMemo(
    () => changedFormUpdates(fields, history.present),
    [fields, history.present]
  );
  const changedIds = useMemo(
    () => new Set(changedUpdates.map((update) => update.fieldId)),
    [changedUpdates]
  );
  const draftErrors = useMemo(
    () => validateFormDraft(fields, history.present),
    [fields, history.present]
  );
  const selectedField = fields.find((field) => field.fieldId === selectedFieldId) ?? null;
  const selectedValues = selectedField
    ? history.present[selectedField.fieldId] ?? []
    : [];
  const filteredFields = useMemo(() => {
    const query = fieldSearch.trim().toLocaleLowerCase(locale);
    return fields.filter((field) => {
      if (query && !field.name.toLocaleLowerCase(locale).includes(query)) {
        return false;
      }
      if (fieldFilter === "changed") {
        return changedIds.has(field.fieldId);
      }
      if (fieldFilter === "required") {
        return field.required;
      }
      if (fieldFilter === "readOnly") {
        return !field.editable;
      }
      return true;
    });
  }, [changedIds, fieldFilter, fieldSearch, fields, locale]);
  const hasCertificateRisk = Boolean(
    inspection?.certificateSignature || editSafety.signedSources.length > 0
  );
  const certificateRiskAccepted = !hasCertificateRisk || signatureRiskAcknowledged;
  const canExport = Boolean(
    desktopMode &&
      sourcePath &&
      inspection &&
      !inspection.hasXfa &&
      (changedUpdates.length > 0 || (flatten && inspection.flattenableFieldCount > 0)) &&
      Object.keys(draftErrors).length === 0 &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      busy === null &&
      !formJob.isActive
  );
  const resetExportOutcome = () => {
    setExportResult(null);
    setJobNotice(null);
    formJob.clearJob();
  };

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      requestRunRef.current += 1;
      const task = loadingTaskRef.current;
      loadingTaskRef.current = null;
      void task?.destroy();
    };
  }, []);

  const closeWorkspace = useCallback(() => {
    requestRunRef.current += 1;
    const task = loadingTaskRef.current;
    loadingTaskRef.current = null;
    void task?.destroy();
    setWorkspaceOpen(false);
    setInspection(null);
    setPdfDocument(null);
    setHistory(createFormHistory([]));
    setSelectedFieldId(null);
    setPageNumber(1);
    setFieldSearch("");
    setFieldFilter("all");
    setFlatten(false);
    setShowFieldPassword(false);
    setExportResult(null);
    formInspectionJob.clearJob();
  }, [formInspectionJob.clearJob]);

  useEffect(() => {
    if (initialSourcePath) {
      closeWorkspace();
      setSourcePath(initialSourcePath);
      setPassword(initialSourcePassword ?? "");
      setError(null);
    }
  }, [closeWorkspace, initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [password, sourcePath]);

  useEffect(() => {
    const job = formJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setError(null);
      setJobNotice(null);
      setExportResult(job.result);
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
    } else if (job.status === "cancelled") {
      setExportResult(null);
      setJobNotice(t("form.export.cancelled"));
    } else if (job.status === "failed") {
      setExportResult(null);
      setJobNotice(null);
      setError(localisePdfJobFailure(job, t));
    }
  }, [formJob.job?.jobId, formJob.job?.status, t]);

  useEffect(() => {
    if (!workspaceOpen || !previewHostRef.current || !("ResizeObserver" in window)) {
      return;
    }
    const host = previewHostRef.current;
    const update = () => setPreviewWidth(Math.max(280, Math.min(700, host.clientWidth - 28)));
    update();
    const observer = new ResizeObserver(update);
    observer.observe(host);
    return () => observer.disconnect();
  }, [workspaceOpen]);

  useEffect(() => {
    if (!workspaceOpen) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editing = Boolean(target?.closest("input, textarea, select"));
      if (
        !operationBusy &&
        (event.ctrlKey || event.metaKey) &&
        !editing &&
        event.key.toLowerCase() === "z"
      ) {
        event.preventDefault();
        setHistory((current) =>
          event.shiftKey ? redoFormValues(current) : undoFormValues(current)
        );
        resetExportOutcome();
      } else if (
        !operationBusy &&
        (event.ctrlKey || event.metaKey) &&
        !editing &&
        event.key.toLowerCase() === "y"
      ) {
        event.preventDefault();
        setHistory(redoFormValues);
        resetExportOutcome();
      } else if (event.key === "Escape" && !operationBusy) {
        closeWorkspace();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeWorkspace, operationBusy, workspaceOpen]);

  const changeOutputProtection = (value: OutputProtectionDraft) => {
    setOutputProtection(value);
    resetExportOutcome();
  };

  const chooseSource = async () => {
    if (operationBusy) {
      return;
    }
    setError(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("form.dialog.filter"), extensions: ["pdf"] }],
        multiple: false,
        title: t("form.dialog.choose")
      });
      if (typeof selected === "string" && mountedRef.current) {
        closeWorkspace();
        setSourcePath(selected);
        setPassword("");
        setOutputProtection(createOutputProtectionDraft());
        resetExportOutcome();
      }
    } catch {
      if (mountedRef.current) {
        setError(t("form.error.choose"));
      }
    }
  };

  const reviewForm = async () => {
    if (!desktopMode || !sourcePath || operationBusy) {
      return;
    }
    const runId = requestRunRef.current + 1;
    requestRunRef.current = runId;
    setBusy("review");
    setReviewCancelBusy(false);
    setError(null);
    resetExportOutcome();
    formInspectionJob.clearJob();
    const previousTask = loadingTaskRef.current;
    loadingTaskRef.current = null;
    if (previousTask) {
      await previousTask.destroy();
    }
    if (!mountedRef.current || requestRunRef.current !== runId) {
      return;
    }

    let task: ReturnType<typeof createPdfLoadingTask> | null = null;
    try {
      const [report, source] = await Promise.all([
        formInspectionJob.startJobAndWait({
          inputPassword: password || null,
          inputPath: sourcePath
        }),
        invoke<PdfRangeSource>("open_local_pdf", { path: sourcePath })
      ]);
      if (!mountedRef.current || requestRunRef.current !== runId) {
        return;
      }
      if (
        report.sourceSize !== source.size ||
        report.sourceModifiedAtMs !== source.modifiedAtMs
      ) {
        throw new FormUserError(t("form.error.sourceChanged"));
      }
      task = createPdfLoadingTask(source, password || null);
      loadingTaskRef.current = task;
      let passwordFailure = "";
      task.onPassword = (_updatePassword: (value: string) => void, reason: number) => {
        passwordFailure = isIncorrectPasswordReason(reason)
          ? t("form.error.passwordIncorrect")
          : t("form.error.passwordRequired");
        void task?.destroy();
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
      if (!mountedRef.current || requestRunRef.current !== runId) {
        if (loadingTaskRef.current === task) {
          loadingTaskRef.current = null;
        }
        await task.destroy();
        return;
      }
      if (document.numPages !== report.pageCount) {
        throw new FormUserError(t("form.error.pageCountMismatch"));
      }
      const initialField =
        report.fields.find((field) => field.editable) ?? report.fields[0] ?? null;
      setInspection(report);
      setPdfDocument(document);
      setHistory(createFormHistory(report.fields));
      setSelectedFieldId(initialField?.fieldId ?? null);
      setPageNumber(firstFieldPage(initialField) ?? 1);
      setFieldSearch("");
      setFieldFilter("all");
      setFlatten(false);
      setShowFieldPassword(false);
      setWorkspaceOpen(true);
      setSignatureRiskAcknowledged(false);
      formInspectionJob.clearJob();
    } catch (reason) {
      if (loadingTaskRef.current === task) {
        loadingTaskRef.current = null;
      }
      void task?.destroy();
      if (mountedRef.current && requestRunRef.current === runId) {
        setInspection(null);
        setPdfDocument(null);
        setWorkspaceOpen(false);
        setError(
          reason instanceof Error && reason.message === "The PDF job was cancelled."
            ? t("form.review.cancelled")
            : reason instanceof FormUserError
            ? reason.message
            : pdfOpeningError(reason, t)
        );
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setBusy(null);
        setReviewCancelBusy(false);
      }
    }
  };

  const selectField = (field: FormField) => {
    setSelectedFieldId(field.fieldId);
    setShowFieldPassword(false);
    const page = firstFieldPage(field);
    if (page) {
      setPageNumber(page);
    }
  };

  const changePage = (next: number) => {
    if (!inspection) {
      return;
    }
    setPageNumber(Math.max(1, Math.min(inspection.pageCount, Math.round(next) || 1)));
  };

  const setFieldValues = (fieldId: string, values: string[]) => {
    setHistory((current) => updateFormField(current, fieldId, values));
    resetExportOutcome();
  };

  const resetSelectedField = () => {
    if (!selectedField) {
      return;
    }
    setFieldValues(selectedField.fieldId, selectedField.values);
  };

  const resetAllFields = () => {
    setHistory((current) => ({
      future: [],
      past: [...current.past.slice(-99), current.present],
      present: initialFormValues(fields)
    }));
    resetExportOutcome();
  };

  const exportForm = async () => {
    if (!canExport || !sourcePath || !inspection) {
      return;
    }
    const runId = requestRunRef.current;
    setBusy("export");
    setError(null);
    resetExportOutcome();
    try {
      const outputPath = await save({
        defaultPath: suggestedOutputPath(sourcePath, flatten),
        filters: [{ name: t("form.dialog.filter"), extensions: ["pdf"] }],
        title: flatten ? t("form.dialog.saveFlattened") : t("form.dialog.saveCompleted")
      });
      if (typeof outputPath !== "string") {
        return;
      }
      if (!mountedRef.current || requestRunRef.current !== runId) {
        return;
      }
      await formJob.startJob({
        acknowledgeCertificateSignatures: signatureRiskAcknowledged,
        expectedSourceModifiedAtMs: inspection.sourceModifiedAtMs,
        expectedSourceSize: inspection.sourceSize,
        flatten,
        inputPassword: password || null,
        inputPath: sourcePath,
        outputPath,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable),
        updates: changedUpdates
      });
    } catch {
      if (mountedRef.current && requestRunRef.current === runId) {
        setError(t("form.error.export"));
      }
    } finally {
      if (mountedRef.current && requestRunRef.current === runId) {
        setBusy(null);
      }
    }
  };

  const cancelFormExport = async () => {
    if (!formJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await formJob.cancelJob();
    } catch {
      setCancelBusy(false);
      setError(t("form.error.exportCancel"));
    }
  };

  const cancelFormReview = async () => {
    if (!formInspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await formInspectionJob.cancelJob();
    } catch {
      setError(t("form.error.reviewCancel"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const dialogRef = useDialogFocus<HTMLElement>({
    active: workspaceOpen,
    escapeDisabled: operationBusy,
    onEscape: closeWorkspace
  });

  return (
    <>
      <section className="form-studio">
        <div className="form-heading">
          <div>
            <h3>{t("form.heading.title")}</h3>
            <p>{t("form.heading.description")}</p>
          </div>
          <FileInput size={18} aria-hidden="true" />
        </div>
        <button
          className="wide-button"
          disabled={!desktopMode || operationBusy}
          onClick={() => void chooseSource()}
          type="button"
        >
          <FolderOpen size={17} aria-hidden="true" />
          {sourcePath ? t("form.action.chooseAnother") : t("form.action.choose")}
        </button>
        {sourcePath ? (
          <div className="form-source">
            <FileText size={17} aria-hidden="true" />
            <span>
              <strong>{fileNameFromPath(sourcePath)}</strong>
              <small title={sourcePath}>{sourcePath}</small>
            </span>
          </div>
        ) : null}
        {sourcePath ? (
          <label className="form-password">
            <span>
              {t("form.password.label")} <small>{t("common.optional")}</small>
            </span>
            <span>
              <input
                autoComplete="off"
                disabled={operationBusy}
                onChange={(event) => {
                  setPassword(event.target.value);
                  resetExportOutcome();
                }}
                placeholder={t("form.password.placeholder")}
                type={showPassword ? "text" : "password"}
                value={password}
              />
              <button
                aria-label={
                  showPassword ? t("form.password.hide") : t("form.password.show")
                }
                className="icon-button"
                disabled={operationBusy}
                onClick={() => setShowPassword((visible) => !visible)}
                title={
                  showPassword ? t("form.password.hide") : t("form.password.show")
                }
                type="button"
              >
                {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
              </button>
            </span>
          </label>
        ) : null}
        {!desktopMode ? (
          <div className="engine-state is-info">
            <Info size={16} aria-hidden="true" />
            <span>{t("form.desktopOnly")}</span>
          </div>
        ) : null}
        {error && !workspaceOpen ? (
          <div className="engine-state is-missing" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{error}</span>
          </div>
        ) : null}
        <button
          className="primary wide-button"
          disabled={!desktopMode || !sourcePath || operationBusy}
          onClick={() => void reviewForm()}
          type="button"
        >
          {busy === "review" || formInspectionJob.isActive ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <ListChecks size={17} aria-hidden="true" />}
          {busy === "review" || formInspectionJob.isActive
            ? t("form.action.inspecting")
            : t("form.action.open")}
        </button>
        {!workspaceOpen && formInspectionJob.job ? (
          <PdfJobProgress
            cancelling={reviewCancelBusy}
            connectionError={formInspectionJob.connectionError}
            job={formInspectionJob.job}
            onCancel={() => void cancelFormReview()}
            onRetry={() => void reviewForm()}
            retryDisabled={!desktopMode || !sourcePath || operationBusy}
          />
        ) : null}
        {!workspaceOpen && formJob.job ? (
          <PdfJobProgress
            cancelling={cancelBusy}
            connectionError={formJob.connectionError}
            job={formJob.job}
            onCancel={() => void cancelFormExport()}
            onRetry={() => void exportForm()}
            retryDisabled={!canExport}
          />
        ) : null}
        {!workspaceOpen &&
        !formInspectionJob.job &&
        !formJob.isActive &&
        formJob.connectionError ? (
          <div className="engine-state is-info" role="status">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t("job.connectionError")}</span>
          </div>
        ) : null}
        {!workspaceOpen && jobNotice ? (
          <div className="engine-state is-info" role="status">
            <Info size={16} aria-hidden="true" />
            <span>{jobNotice}</span>
          </div>
        ) : null}
        {!workspaceOpen && exportResult ? (
          <FormExportResultPanel
            formatNumber={formatNumber}
            result={exportResult}
            t={t}
          />
        ) : null}
      </section>

      {workspaceOpen && inspection && pdfDocument ? (
        <div className="dialog-backdrop form-backdrop" role="presentation">
          <section
            aria-labelledby="form-dialog-title"
            aria-modal="true"
            className="form-dialog"
            data-dialog-root
            ref={dialogRef}
            role="dialog"
            tabIndex={-1}
          >
            <header>
              <div className="dialog-icon" aria-hidden="true"><FileInput size={24} /></div>
              <div>
                <span className="eyebrow">{t("form.workspace.eyebrow")}</span>
                <h2 id="form-dialog-title">{inspection.fileName}</h2>
              </div>
              <div className="form-header-actions">
                <span>
                  {t("form.workspace.summary", {
                    changed: formatNumber(changedUpdates.length),
                    fields: formatNumber(inspection.fieldCount)
                  })}
                </span>
                <button className="primary" disabled={!canExport} onClick={() => void exportForm()} type="button">
                  {operationBusy ? <Loader2 className="spin" size={16} aria-hidden="true" /> : <Save size={16} aria-hidden="true" />}
                  {formJob.isActive
                    ? t("form.action.exporting")
                    : busy === "export"
                      ? t("form.action.choosing")
                      : t("form.action.save")}
                </button>
                <button aria-label={t("form.action.close")} className="icon-button" data-dialog-initial-focus disabled={operationBusy} onClick={closeWorkspace} title={t("form.action.close")} type="button"><X size={18} aria-hidden="true" /></button>
              </div>
            </header>

            <div className="form-toolbar">
              <div>
                <button aria-label={t("form.action.undo")} className="icon-button" disabled={history.past.length === 0 || operationBusy} onClick={() => { setHistory(undoFormValues); resetExportOutcome(); }} title={t("common.undo")} type="button"><Undo2 size={17} aria-hidden="true" /></button>
                <button aria-label={t("form.action.redo")} className="icon-button" disabled={history.future.length === 0 || operationBusy} onClick={() => { setHistory(redoFormValues); resetExportOutcome(); }} title={t("common.redo")} type="button"><Redo2 size={17} aria-hidden="true" /></button>
                <button disabled={changedUpdates.length === 0 || operationBusy} onClick={resetAllFields} type="button"><RotateCcw size={16} aria-hidden="true" /> {t("form.action.resetAll")}</button>
              </div>
              <label className={flatten ? "form-flatten-toggle is-active" : "form-flatten-toggle"}>
                <input checked={flatten} disabled={inspection.flattenableFieldCount === 0 || operationBusy} onChange={(event) => { setFlatten(event.target.checked); resetExportOutcome(); }} type="checkbox" />
                <span>
                  <strong>{t("form.flatten.label")}</strong>
                  <small>
                    {t(
                      inspection.flattenableFieldCount === 1
                        ? "form.flatten.available.one"
                        : "form.flatten.available.other",
                      { count: formatNumber(inspection.flattenableFieldCount) }
                    )}
                  </small>
                </span>
              </label>
            </div>

            {formJob.job ? (
              <PdfJobProgress
                cancelling={cancelBusy}
                connectionError={formJob.connectionError}
                job={formJob.job}
                onCancel={() => void cancelFormExport()}
                onRetry={() => void exportForm()}
                retryDisabled={!canExport}
              />
            ) : null}
            {!formJob.isActive && formJob.connectionError ? (
              <div className="form-warning" role="status">
                <AlertCircle size={16} aria-hidden="true" />
                <span>{t("job.connectionError")}</span>
              </div>
            ) : null}
            {jobNotice ? (
              <div className="form-warning" role="status">
                <Info size={16} aria-hidden="true" />
                <span>{jobNotice}</span>
              </div>
            ) : null}

            <fieldset
              className="form-workspace form-workspace-fieldset"
              disabled={operationBusy}
            >
              <aside className="form-field-panel">
                <div className="form-field-search">
                  <Search size={15} aria-hidden="true" />
                  <input aria-label={t("form.search.aria")} onChange={(event) => setFieldSearch(event.target.value)} placeholder={t("form.search.placeholder")} type="search" value={fieldSearch} />
                </div>
                <select aria-label={t("form.filter.aria")} onChange={(event) => setFieldFilter(event.target.value as FieldFilter)} value={fieldFilter}>
                  <option value="all">{t("form.filter.all")}</option>
                  <option value="changed">{t("form.filter.changed")}</option>
                  <option value="required">{t("form.filter.required")}</option>
                  <option value="readOnly">{t("form.filter.readOnly")}</option>
                </select>
                <div className="form-field-list">
                  {filteredFields.length > 0 ? filteredFields.map((field, index) => (
                    <button
                      aria-current={field.fieldId === selectedFieldId ? "true" : undefined}
                      className={`${field.fieldId === selectedFieldId ? "is-active" : ""}${draftErrors[field.fieldId] ? " has-error" : ""}`}
                      key={field.fieldId}
                      onClick={() => selectField(field)}
                      type="button"
                    >
                      <FormKindIcon kind={field.kind} />
                      <span>
                        <strong>{field.name}</strong>
                        <small>
                          {localiseFormKind(field.kind, t)}
                          {field.required ? ` | ${t("form.field.required")}` : ""}
                        </small>
                      </span>
                      {draftErrors[field.fieldId] ? <AlertCircle size={14} aria-hidden="true" /> : changedIds.has(field.fieldId) ? <span className="form-changed-dot" title={t("form.field.changed")} /> : <em>{formatNumber(index + 1)}</em>}
                    </button>
                  )) : <div className="form-field-empty"><Search size={22} aria-hidden="true" /><strong>{t("form.search.empty")}</strong></div>}
                </div>
              </aside>

              <main className="form-canvas-panel">
                <div className="form-page-nav">
                  <button aria-label={t("common.previousPage")} className="icon-button" disabled={pageNumber <= 1} onClick={() => changePage(pageNumber - 1)} title={t("common.previousPage")} type="button"><ChevronLeft size={17} aria-hidden="true" /></button>
                  <label><span>{t("common.page")}</span><input max={inspection.pageCount} min={1} onChange={(event) => changePage(Number(event.target.value))} type="number" value={pageNumber} /><span>{t("common.ofCount", { count: formatNumber(inspection.pageCount) })}</span></label>
                  <button aria-label={t("common.nextPage")} className="icon-button" disabled={pageNumber >= inspection.pageCount} onClick={() => changePage(pageNumber + 1)} title={t("common.nextPage")} type="button"><ChevronRight size={17} aria-hidden="true" /></button>
                  <span>
                    {t(
                      fields.filter((field) =>
                        field.widgets.some((widget) => widget.pageNumber === pageNumber)
                      ).length === 1
                        ? "form.page.fields.one"
                        : "form.page.fields.other",
                      {
                        count: formatNumber(
                          fields.filter((field) =>
                            field.widgets.some(
                              (widget) => widget.pageNumber === pageNumber
                            )
                          ).length
                        )
                      }
                    )}
                  </span>
                </div>
                <div className="form-preview-host" ref={previewHostRef}>
                  <div className="form-page-surface">
                    <PdfPageCanvas document={pdfDocument} pageNumber={pageNumber} targetWidth={previewWidth} variant="page" />
                    <svg aria-label={t("form.canvas.aria", { page: formatNumber(pageNumber) })} className="form-overlay" preserveAspectRatio="none" viewBox="0 0 1000 1000">
                      {fields.flatMap((field) => field.widgets
                        .filter((widget) => widget.pageNumber === pageNumber && widget.rect)
                        .map((widget) => (
                          <FormWidgetMark
                            field={field}
                            key={widget.widgetId}
                            onSelect={selectField}
                            selected={field.fieldId === selectedFieldId}
                            t={t}
                            values={history.present[field.fieldId] ?? []}
                            widget={widget}
                          />
                        )))}
                    </svg>
                  </div>
                </div>
              </main>

              <aside className="form-inspector">
                {selectedField ? (
                  <section className="form-field-editor">
                    <header>
                      <div><span className="eyebrow">{localiseFormKind(selectedField.kind, t)}</span><h3>{selectedField.name}</h3></div>
                      <button aria-label={t("form.action.resetField")} className="icon-button" disabled={!changedIds.has(selectedField.fieldId)} onClick={resetSelectedField} title={t("form.action.resetField")} type="button"><RotateCcw size={16} aria-hidden="true" /></button>
                    </header>
                    <FieldEditor
                      error={localiseFormDraftError(
                        draftErrors[selectedField.fieldId],
                        t,
                        formatNumber
                      )}
                      field={selectedField}
                      formatNumber={formatNumber}
                      onChange={(values) => setFieldValues(selectedField.fieldId, values)}
                      onShowPasswordChange={setShowFieldPassword}
                      showPassword={showFieldPassword}
                      t={t}
                      values={selectedValues}
                    />
                    <dl className="form-field-details">
                      <div><dt>{t("form.details.widgets")}</dt><dd>{formatNumber(selectedField.widgets.length)}</dd></div>
                      <div><dt>{t("form.details.pages")}</dt><dd>{fieldPages(selectedField, formatList, formatNumber, t)}</dd></div>
                      <div><dt>{t("form.details.status")}</dt><dd>{selectedField.editable ? t("form.status.editable") : selectedField.readOnly ? t("form.status.readOnly") : t("form.status.preserved")}</dd></div>
                    </dl>
                  </section>
                ) : <section className="form-field-editor is-empty"><ListChecks size={24} aria-hidden="true" /><strong>{t("form.field.noneSelected")}</strong></section>}

                <div className="form-notices">
                  {inspection.hasXfa ? <div className="form-error" role="alert"><ShieldAlert size={17} aria-hidden="true" /><span><strong>{t("form.xfa.title")}</strong><small>{t("form.xfa.description")}</small></span></div> : null}
                  {flatten ? <div className="form-flatten-warning"><Info size={16} aria-hidden="true" /><span>{t("form.flatten.warning")}</span></div> : null}
                  {localiseFormWarnings(inspection.warnings, t, formatNumber).map((warning) => <div className="form-warning" key={warning}><Info size={16} aria-hidden="true" /><span>{warning}</span></div>)}
                  <PdfEditSafetyNotice acknowledged={signatureRiskAcknowledged} busy={operationBusy} editSafety={editSafety} onAcknowledgedChange={setSignatureRiskAcknowledged} rewriteDescription={t("form.signature.rewrite")} />
                  <OutputProtectionFields
                    disabled={operationBusy}
                    onChange={changeOutputProtection}
                    qpdfAvailable={qpdfAvailable}
                    value={outputProtection}
                  />
                  {error ? <div className="form-error" role="alert"><AlertCircle size={16} aria-hidden="true" /><span>{error}</span></div> : null}
                  {exportResult ? <FormExportResultPanel formatNumber={formatNumber} result={exportResult} t={t} /> : null}
                </div>
              </aside>
            </fieldset>
          </section>
        </div>
      ) : null}
    </>
  );
}

function FormExportResultPanel({
  formatNumber,
  result,
  t
}: {
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  result: ExportPdfFormsResult;
  t: ReturnType<typeof useI18n>["t"];
}) {
  return (
    <div className="form-export-result">
      <CheckCircle2 size={18} aria-hidden="true" />
      <span>
        <strong>{t("form.result.title")}</strong>
        <small>
          {t("form.result.summary", {
            encryption:
              result.encryption === "AES-256"
                ? t("common.encryption.protected")
                : t("common.encryption.unprotected"),
            flattened: formatNumber(result.flattenedFieldCount),
            remaining: formatNumber(result.remainingFieldCount),
            size: formatBytes(result.bytesWritten, formatNumber),
            updated: formatNumber(result.updatedFieldCount)
          })}
        </small>
        <small title={result.outputPath}>{fileNameFromPath(result.outputPath)}</small>
        {localiseFormWarnings(result.warnings, t, formatNumber).map((warning) => (
          <small key={warning}>{warning}</small>
        ))}
      </span>
    </div>
  );
}

function FieldEditor({
  error,
  field,
  formatNumber,
  onChange,
  onShowPasswordChange,
  showPassword,
  t,
  values
}: {
  error?: string;
  field: FormField;
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  onChange: (values: string[]) => void;
  onShowPasswordChange: (value: boolean) => void;
  showPassword: boolean;
  t: ReturnType<typeof useI18n>["t"];
  values: string[];
}) {
  if (!field.editable) {
    return (
      <div className="form-readonly-state">
        <LockKeyhole size={18} aria-hidden="true" />
        <span>
          <strong>
            {field.signaturePresent
              ? t("form.readOnly.signed")
              : field.readOnly
                ? t("form.readOnly.readOnly")
                : t("form.readOnly.preserved")}
          </strong>
          <small>
            {field.kind === "button"
              ? t("form.readOnly.button")
              : field.kind === "signature"
                ? t("form.readOnly.signature")
                : t("form.readOnly.unsupported")}
          </small>
        </span>
      </div>
    );
  }

  let control = null;
  if (field.kind === "text") {
    control = field.multiline ? (
      <textarea autoFocus maxLength={field.maxLength ?? 4096} onChange={(event) => onChange([event.target.value])} rows={6} value={values[0] ?? ""} />
    ) : (
      <div className="form-text-input">
        <input autoFocus maxLength={field.maxLength ?? 4096} onChange={(event) => onChange([event.target.value])} type={field.password && !showPassword ? "password" : "text"} value={values[0] ?? ""} />
        {field.password ? <button aria-label={showPassword ? t("form.field.hideValue") : t("form.field.showValue")} className="icon-button" onClick={() => onShowPasswordChange(!showPassword)} title={showPassword ? t("form.field.hideValue") : t("form.field.showValue")} type="button">{showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}</button> : null}
      </div>
    );
  } else if (field.kind === "checkbox") {
    const option = field.options[0] ?? { label: t("form.field.checked"), value: "Yes" };
    control = <label className="form-checkbox-control"><input checked={values.includes(option.value)} onChange={(event) => onChange(event.target.checked ? [option.value] : [])} type="checkbox" /><span><strong>{option.label}</strong><small>{field.required ? t("form.field.required") : t("common.optional")}</small></span></label>;
  } else if (field.kind === "radio") {
    control = <div className="form-option-list">{field.options.map((option) => <label key={option.value}><input checked={values[0] === option.value} name={`form-${field.fieldId}`} onChange={() => onChange([option.value])} type="radio" /><span>{option.label}</span></label>)}</div>;
  } else if (field.kind === "choice" && field.editableChoice) {
    const listId = `form-options-${field.fieldId.replace(/[^a-z0-9]/gi, "-")}`;
    control = <><input autoFocus list={listId} onChange={(event) => onChange(event.target.value ? [event.target.value] : [])} type="text" value={values[0] ?? ""} /><datalist id={listId}>{field.options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</datalist></>;
  } else if (field.kind === "choice" && field.multiSelect) {
    control = <select multiple onChange={(event: ChangeEvent<HTMLSelectElement>) => onChange(Array.from(event.target.selectedOptions, (option) => option.value))} size={Math.min(8, Math.max(3, field.options.length))} value={values}>{field.options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select>;
  } else if (field.kind === "choice") {
    control = <select autoFocus onChange={(event) => onChange(event.target.value ? [event.target.value] : [])} value={values[0] ?? ""}><option value="">{t("form.field.chooseOption")}</option>{field.options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select>;
  }

  return (
    <div className="form-control-editor">
      <label><span>{t("form.field.value")}{field.required ? <em>{t("form.field.required")}</em> : null}</span>{control}</label>
      {field.maxLength !== null ? <small>{t("form.field.characterCount", { count: formatNumber(Array.from(values[0] ?? "").length), maximum: formatNumber(field.maxLength) })}</small> : null}
      {error ? <div className="form-inline-error"><AlertCircle size={14} aria-hidden="true" /><span>{error}</span></div> : null}
    </div>
  );
}

function FormWidgetMark({
  field,
  onSelect,
  selected,
  t,
  values,
  widget
}: {
  field: FormField;
  onSelect: (field: FormField) => void;
  selected: boolean;
  t: ReturnType<typeof useI18n>["t"];
  values: string[];
  widget: FormField["widgets"][number];
}) {
  if (!widget.rect) return null;
  const { x, y, width, height } = widget.rect;
  const left = x * 1000;
  const top = y * 1000;
  const pixelWidth = width * 1000;
  const pixelHeight = height * 1000;
  const display = formDisplayValue(field, values, t);
  const checked = field.kind === "checkbox" || field.kind === "radio" ? values.includes(widget.exportValue ?? field.options[0]?.value ?? "Yes") : false;
  return (
    <g
      aria-label={t("form.canvas.select", { name: field.name })}
      className={`form-widget-mark${selected ? " is-selected" : ""}${field.editable ? "" : " is-readonly"}`}
      onClick={() => onSelect(field)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(field);
        }
      }}
      role="button"
      tabIndex={0}
    >
      <rect fill={changedFill(field, values)} height={pixelHeight} rx={Math.min(5, pixelHeight * 0.12)} stroke={selected ? "#235dd8" : field.editable ? "#7c91b6" : "#9299a5"} strokeDasharray={field.editable ? undefined : "5 4"} strokeWidth={selected ? 3 : 1.5} vectorEffect="non-scaling-stroke" width={pixelWidth} x={left} y={top} />
      {field.kind === "checkbox" && checked ? <path d={`M ${left + pixelWidth * 0.2} ${top + pixelHeight * 0.52} L ${left + pixelWidth * 0.43} ${top + pixelHeight * 0.76} L ${left + pixelWidth * 0.82} ${top + pixelHeight * 0.2}`} fill="none" stroke="#235dd8" strokeLinecap="round" strokeLinejoin="round" strokeWidth={Math.max(2, pixelHeight * 0.12)} /> : null}
      {field.kind === "radio" && checked ? <circle cx={left + pixelWidth / 2} cy={top + pixelHeight / 2} fill="#235dd8" r={Math.min(pixelWidth, pixelHeight) * 0.25} /> : null}
      {!(["checkbox", "radio"] as FormFieldKind[]).includes(field.kind) ? <text dominantBaseline="middle" fill="#2f3a4a" fontFamily="Arial, sans-serif" fontSize={Math.max(10, Math.min(22, pixelHeight * 0.42))} x={left + 5} y={top + pixelHeight / 2}>{truncate(display || field.name, Math.max(8, Math.floor(pixelWidth / 9)))}</text> : null}
    </g>
  );
}

function FormKindIcon({ kind }: { kind: FormFieldKind }) {
  const Icon = kind === "text" ? Type : kind === "checkbox" ? SquareCheckBig : kind === "radio" ? CircleDot : kind === "signature" ? LockKeyhole : ListChecks;
  return <Icon size={15} aria-hidden="true" />;
}

function formDisplayValue(
  field: FormField,
  values: string[],
  t: ReturnType<typeof useI18n>["t"]
) {
  if (field.password) return "\u2022".repeat(values[0]?.length ?? 0);
  if (field.kind === "choice") return values.map((value) => field.options.find((option) => option.value === value)?.label ?? value).join(", ");
  if (field.kind === "signature") {
    return field.signaturePresent
      ? t("form.canvas.signed")
      : t("form.kind.signature");
  }
  if (field.kind === "button") return field.name;
  return values.join(" ");
}

function changedFill(field: FormField, values: string[]) {
  return JSON.stringify(values) === JSON.stringify(field.values) ? "rgba(255,255,255,0.08)" : "rgba(238,244,255,0.94)";
}

function firstFieldPage(field: FormField | null) {
  return field?.widgets.find((widget) => widget.pageNumber !== null)?.pageNumber ?? null;
}

function fieldPages(
  field: FormField,
  formatList: (values: string[], options?: Intl.ListFormatOptions) => string,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string,
  t: ReturnType<typeof useI18n>["t"]
) {
  const pages = [
    ...new Set(
      field.widgets
        .map((widget) => widget.pageNumber)
        .filter((page): page is number => page !== null)
    )
  ];
  return pages.length
    ? formatList(pages.map((page) => formatNumber(page)))
    : t("common.none");
}

function suggestedOutputPath(sourcePath: string, flatten: boolean) {
  return sourcePath.replace(/\.pdf$/i, flatten ? "-flattened.pdf" : "-completed.pdf");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || "Document.pdf";
}

function formatBytes(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KB`;
  }
  return `${formatNumber(bytes / (1024 * 1024), {
    maximumFractionDigits: 1
  })} MB`;
}

function truncate(value: string, limit: number) {
  const normalised = value.replace(/\s+/g, " ").trim();
  return normalised.length > limit ? `${normalised.slice(0, Math.max(1, limit - 3))}...` : normalised;
}

function pdfOpeningError(
  reason: unknown,
  t: ReturnType<typeof useI18n>["t"]
) {
  const name =
    reason && typeof reason === "object" && "name" in reason
      ? String(reason.name)
      : "";
  if (name === "InvalidPDFException") {
    return t("form.error.damaged");
  }
  if (name === "MissingPDFException" || name === "UnexpectedResponseException") {
    return t("form.error.read");
  }
  return t("form.error.review");
}

class FormUserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "FormUserError";
  }
}
