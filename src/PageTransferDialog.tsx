import {
  type DragEvent,
  type FormEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  Copy,
  FileOutput,
  Files,
  FolderOpen,
  GripVertical,
  KeyRound,
  Loader2,
  MoveRight,
  ShieldAlert,
  X
} from "lucide-react";
import { takeE2eOpenSelection, takeE2eSaveSelection } from "paperworks-e2e-bridge";
import { useDialogFocus } from "./accessibility";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { LazyPdfThumbnail } from "./PdfPageCanvas";
import { PdfJobProgress } from "./PdfJobProgress";
import { useI18n } from "./I18nProvider";
import { localiseOrganiseWarnings } from "./organiseLocalisation";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import {
  canMovePagesBetweenDocuments,
  createPageTransferPlan,
  PAGE_TRANSFER_DRAG_TYPE,
  type PageTransferMode
} from "./pageTransfer";
import {
  createPdfLoadingTask,
  isIncorrectPasswordReason,
  type PDFDocumentProxy,
  type PdfRangeSource
} from "./pdf";
import type { PlannedPage } from "./usePagePlan";
import { usePdfJob } from "./usePdfJob";
import {
  visualSignatureExportPayload,
  type VisualSignatureAsset,
  type VisualSignaturePlacement
} from "./visualSignatures";

type PageImportInspection = {
  certificateSignature: boolean;
  encrypted: boolean;
  pageCount: number;
  selectedPages: number[];
};

type ExportResult = {
  bytesWritten: number;
  outputPath: string;
  pageCount: number;
  warnings: string[];
};

export type PageTransferPdfSource = {
  certificateAcknowledged: boolean;
  certificateSignature: boolean;
  document: PDFDocumentProxy;
  id: string;
  modifiedAtMs: number | null;
  name: string;
  password: string | null;
  path: string;
  size: number;
};

type TransferDestination = {
  document: PDFDocumentProxy;
  inspection: PageImportInspection;
  loadingTask: ReturnType<typeof createPdfLoadingTask>;
  password: string | null;
  source: PdfRangeSource;
};

type PageTransferDialogProps = {
  desktopMode: boolean;
  onClose: () => void;
  onMoveComplete: (pageIds: string[], result: ExportResult) => void;
  open: boolean;
  qpdfAvailable: boolean;
  selectedPages: PlannedPage[];
  signatureAssets: VisualSignatureAsset[];
  signaturePlacements: VisualSignaturePlacement[];
  sourceDocumentName: string;
  sourcePageCount: number;
  sources: PageTransferPdfSource[];
};

const MAX_VISIBLE_SOURCE_PAGES = 80;
const MAX_VISIBLE_DESTINATION_PAGES = 80;

export function PageTransferDialog({
  desktopMode,
  onClose,
  onMoveComplete,
  open: visible,
  qpdfAvailable,
  selectedPages: requestedPages,
  signatureAssets,
  signaturePlacements,
  sourceDocumentName,
  sourcePageCount,
  sources
}: PageTransferDialogProps) {
  const { formatNumber, t } = useI18n();
  const [destinationPath, setDestinationPath] = useState("");
  const [transferPages, setTransferPages] = useState<PlannedPage[]>([]);
  const [transferSourcePageCount, setTransferSourcePageCount] = useState(0);
  const [destinationPassword, setDestinationPassword] = useState("");
  const [destination, setDestination] = useState<TransferDestination | null>(null);
  const [destinationCertificateAcknowledged, setDestinationCertificateAcknowledged] =
    useState(false);
  const [sourceCertificateAcknowledged, setSourceCertificateAcknowledged] = useState(false);
  const [insertionIndex, setInsertionIndex] = useState(0);
  const [mode, setMode] = useState<PageTransferMode>("copy");
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [busy, setBusy] = useState<"choosing" | "loading" | "publishing" | "reviewing" | null>(
    null
  );
  const [reviewCancelBusy, setReviewCancelBusy] = useState(false);
  const [publishCancelBusy, setPublishCancelBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ExportResult | null>(null);
  const destinationTaskRef = useRef<ReturnType<typeof createPdfLoadingTask> | null>(null);
  const pageImportInspectionJob = usePdfJob<PageImportInspection>(
    desktopMode,
    "page-import-inspection",
    "page-transfer"
  );
  const transferExportJob = usePdfJob<ExportResult>(desktopMode, "page-transfer");
  const sourceMap = useMemo(() => new Map(sources.map((source) => [source.id, source])), [sources]);
  const selectedPageIds = useMemo(() => transferPages.map((page) => page.id), [transferPages]);
  const selectedPageIdSet = useMemo(() => new Set(selectedPageIds), [selectedPageIds]);
  const canMove = canMovePagesBetweenDocuments(transferSourcePageCount, transferPages.length);
  const usedSourceIds = useMemo(
    () =>
      new Set(
        transferPages.flatMap((page) => (page.kind === "source" ? [page.sourceId] : []))
      ),
    [transferPages]
  );
  const selectedSources = useMemo(
    () => sources.filter((source) => usedSourceIds.has(source.id)),
    [sources, usedSourceIds]
  );
  const needsSourceCertificateAcknowledgement = selectedSources.some(
    (source) => source.certificateSignature && !source.certificateAcknowledged
  );
  const transferPlan = useMemo(() => {
    if (!destination) {
      return null;
    }
    try {
      return createPageTransferPlan(destination.document.numPages, transferPages, insertionIndex);
    } catch {
      return null;
    }
  }, [destination, insertionIndex, transferPages]);
  const interfaceBusy =
    busy !== null || pageImportInspectionJob.isActive || transferExportJob.isActive;
  const sourcePreviewPages = transferPages.slice(0, MAX_VISIBLE_SOURCE_PAGES);
  const destinationWindow = destination
    ? destinationPreviewWindow(destination.document.numPages, insertionIndex)
    : null;

  useEffect(() => {
    if (!visible) {
      void destinationTaskRef.current?.destroy();
      destinationTaskRef.current = null;
      return;
    }

    void destinationTaskRef.current?.destroy();
    destinationTaskRef.current = null;
    setDestinationPath("");
    setTransferPages(requestedPages.map((page) => ({ ...page })));
    setTransferSourcePageCount(sourcePageCount);
    setDestinationPassword("");
    setDestination(null);
    setDestinationCertificateAcknowledged(false);
    setSourceCertificateAcknowledged(false);
    setInsertionIndex(0);
    setMode("copy");
    setOutputProtection(createOutputProtectionDraft());
    setBusy(null);
    setReviewCancelBusy(false);
    setPublishCancelBusy(false);
    setError(null);
    setResult(null);
    pageImportInspectionJob.clearJob();
    transferExportJob.clearJob();
  }, [visible]);

  useEffect(
    () => () => {
      void destinationTaskRef.current?.destroy();
      destinationTaskRef.current = null;
    },
    []
  );

  useEffect(() => {
    if (!canMove && mode === "move") {
      setMode("copy");
    }
  }, [canMove, mode]);

  const discardDestination = () => {
    void destinationTaskRef.current?.destroy();
    destinationTaskRef.current = null;
    setDestination(null);
    setDestinationCertificateAcknowledged(false);
    setInsertionIndex(0);
    setResult(null);
    pageImportInspectionJob.clearJob();
    transferExportJob.clearJob();
  };

  const chooseDestination = async () => {
    if (interfaceBusy) {
      return;
    }
    setBusy("choosing");
    setError(null);
    try {
      const e2eSelection = takeE2eOpenSelection();
      const selection =
        e2eSelection?.[0] ??
        (await openDialog({
          directory: false,
          filters: [{ name: t("app.dialog.export.filter"), extensions: ["pdf"] }],
          multiple: false,
          title: t("transfer.dialog.chooseDestination")
        }));
      if (typeof selection === "string") {
        if (sources.some((source) => localPathsMatch(source.path, selection))) {
          setError(t("transfer.error.sameDocument"));
          return;
        }
        discardDestination();
        setDestinationPath(selection);
        setDestinationPassword("");
      }
    } catch {
      setError(t("transfer.error.choose"));
    } finally {
      setBusy(null);
    }
  };

  const reviewDestination = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!desktopMode || !destinationPath || interfaceBusy) {
      return;
    }
    setBusy("reviewing");
    setReviewCancelBusy(false);
    setError(null);
    setResult(null);
    discardDestination();
    try {
      const inspection = await pageImportInspectionJob.startJobAndWait({
        inputPassword: destinationPassword || null,
        inputPath: destinationPath,
        pageRange: "all"
      });
      setBusy("loading");
      const source = await invoke<PdfRangeSource>("open_local_pdf", { path: destinationPath });
      const loadingTask = createPdfLoadingTask(source, destinationPassword || null);
      destinationTaskRef.current = loadingTask;
      let passwordRejected = false;
      loadingTask.onPassword = (_updatePassword: (password: string) => void, reason: number) => {
        passwordRejected = true;
        if (isIncorrectPasswordReason(reason)) {
          void loadingTask.destroy();
        }
      };
      const document = await loadingTask.promise;
      if (passwordRejected || document.numPages !== inspection.pageCount) {
        await loadingTask.destroy();
        destinationTaskRef.current = null;
        throw new Error("destination-review-mismatch");
      }
      setDestination({
        document,
        inspection,
        loadingTask,
        password: destinationPassword || null,
        source
      });
      setInsertionIndex(document.numPages);
      pageImportInspectionJob.clearJob();
    } catch {
      discardDestination();
      setError(t("transfer.error.review"));
    } finally {
      setReviewCancelBusy(false);
      setBusy(null);
    }
  };

  const cancelReview = async () => {
    if (!pageImportInspectionJob.isActive || reviewCancelBusy) {
      return;
    }
    setReviewCancelBusy(true);
    try {
      await pageImportInspectionJob.cancelJob();
    } catch {
      setError(t("transfer.error.cancelReview"));
    } finally {
      setReviewCancelBusy(false);
    }
  };

  const cancelPublish = async () => {
    if (!transferExportJob.isActive || publishCancelBusy) {
      return;
    }
    setPublishCancelBusy(true);
    try {
      await transferExportJob.cancelJob();
    } catch {
      setError(t("transfer.error.cancelPublish"));
    } finally {
      setPublishCancelBusy(false);
    }
  };

  const publishTransfer = async () => {
    if (
      !destination ||
      !transferPlan ||
      interfaceBusy ||
      (destination.inspection.certificateSignature && !destinationCertificateAcknowledged) ||
      (needsSourceCertificateAcknowledgement && !sourceCertificateAcknowledged) ||
      !outputProtectionIsValid(outputProtection, qpdfAvailable)
    ) {
      return;
    }
    setBusy("publishing");
    setPublishCancelBusy(false);
    setError(null);
    setResult(null);
    try {
      const outputPath =
        takeE2eSaveSelection() ??
        (await save({
          defaultPath: suggestedTransferPath(destination.source.path),
          filters: [{ name: t("app.dialog.export.filter"), extensions: ["pdf"] }],
          title: t("transfer.dialog.saveDestination")
        }));
      if (!outputPath) {
        return;
      }

      const importedSources = transferPlan.sourceMappings.map((mapping) => {
        const source = sourceMap.get(mapping.sourceId);
        if (!source) {
          throw new Error("missing-transfer-source");
        }
        return {
          acknowledgeCertificateSignature:
            source.certificateAcknowledged || sourceCertificateAcknowledged,
          expectedSourceModifiedAtMs: source.modifiedAtMs,
          expectedSourceSize: source.size,
          id: mapping.destinationSourceId,
          inputPassword: source.password,
          inputPath: source.path
        };
      });
      const transferredPlacements = signaturePlacements
        .filter((placement) => selectedPageIdSet.has(placement.pageId))
        .map((placement) => ({
          ...placement,
          pageId: transferPlan.pageIdMap.get(placement.pageId) ?? placement.pageId
        }));
      const visualSignatures = visualSignatureExportPayload(
        signatureAssets,
        transferredPlacements,
        transferPlan.pages.map((page) => page.id)
      );
      const published = await transferExportJob.startJobAndWait({
        acknowledgePrimaryCertificateSignature: destinationCertificateAcknowledged,
        documentLock: toPdfOutputProtection(outputProtection, qpdfAvailable),
        expectedSourceModifiedAtMs: destination.source.modifiedAtMs,
        expectedSourceSize: destination.source.size,
        importedSources,
        outputPath,
        pages: transferPlan.pages.map((page) =>
          page.kind === "source"
            ? {
                kind: "source",
                rotation: page.rotation,
                sourceId: page.sourceId,
                sourcePage: page.sourcePage
              }
            : {
                heightPt: page.heightPt,
                kind: "blank",
                rotation: page.rotation,
                widthPt: page.widthPt
              }
        ),
        primaryInputPassword: destination.password,
        primaryInputPath: destination.source.path,
        signature: null,
        ...visualSignatures
      });
      setResult(published);
      if (mode === "move") {
        onMoveComplete(selectedPageIds, published);
      }
    } catch {
      setError(t("transfer.error.publish"));
    } finally {
      setPublishCancelBusy(false);
      setBusy(null);
    }
  };

  const setTransferInsertion = (nextIndex: number) => {
    if (!destination || interfaceBusy) {
      return;
    }
    setInsertionIndex(Math.min(destination.document.numPages, Math.max(0, nextIndex)));
    setResult(null);
  };

  const dragTransferPages = (event: DragEvent<HTMLElement>) => {
    event.dataTransfer.effectAllowed = mode === "move" ? "move" : "copy";
    event.dataTransfer.setData(PAGE_TRANSFER_DRAG_TYPE, JSON.stringify(selectedPageIds));
    event.dataTransfer.setData("text/plain", selectedPageIds.join(","));
  };

  const allowTransferDrop = (event: DragEvent<HTMLElement>) => {
    if (!interfaceBusy && event.dataTransfer.types.includes(PAGE_TRANSFER_DRAG_TYPE)) {
      event.preventDefault();
      event.dataTransfer.dropEffect = mode === "move" ? "move" : "copy";
    }
  };

  const dropTransferPages = (event: DragEvent<HTMLElement>, nextIndex: number) => {
    event.preventDefault();
    try {
      const ids = JSON.parse(event.dataTransfer.getData(PAGE_TRANSFER_DRAG_TYPE));
      if (
        Array.isArray(ids) &&
        ids.length === selectedPageIds.length &&
        ids.every((id, index) => id === selectedPageIds[index])
      ) {
        setTransferInsertion(nextIndex);
      }
    } catch {
      // Ignore foreign or malformed drag data.
    }
  };

  const closeDialog = () => {
    if (interfaceBusy) {
      return;
    }
    discardDestination();
    onClose();
  };

  const dialogRef = useDialogFocus<HTMLDivElement>({
    active: visible,
    escapeDisabled: interfaceBusy,
    onEscape: closeDialog
  });

  if (!visible) {
    return null;
  }

  const insertionLabel = destination
    ? insertionIndex === 0
      ? t("transfer.insertion.beginning")
      : insertionIndex === destination.document.numPages
        ? t("transfer.insertion.end")
        : t("transfer.insertion.afterPage", { page: formatNumber(insertionIndex) })
    : t("transfer.insertion.unavailable");
  const outputPageCount = destination ? destination.document.numPages + transferPages.length : 0;
  const localisedWarnings = result ? localiseOrganiseWarnings(result.warnings, t) : [];

  return (
    <div className="dialog-backdrop page-transfer-backdrop" role="presentation">
      <div
        aria-labelledby="page-transfer-title"
        aria-modal="true"
        className="page-transfer-dialog"
        data-dialog-root
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header>
          <div className="dialog-icon" aria-hidden="true">
            <MoveRight size={24} />
          </div>
          <div>
            <span className="eyebrow">{t("transfer.eyebrow")}</span>
            <h2 id="page-transfer-title">{t("transfer.title")}</h2>
            <p>{t("transfer.description")}</p>
          </div>
          <button
            aria-label={t("transfer.close.aria")}
            className="icon-button"
            data-dialog-initial-focus
            disabled={interfaceBusy}
            onClick={closeDialog}
            title={t("common.close")}
            type="button"
          >
            <X size={17} aria-hidden="true" />
          </button>
        </header>

        <div className="page-transfer-mode" role="radiogroup" aria-label={t("transfer.mode.label")}>
          <label className={mode === "copy" ? "is-active" : undefined}>
            <input
              checked={mode === "copy"}
              disabled={interfaceBusy || Boolean(result)}
              name="page-transfer-mode"
              onChange={() => setMode("copy")}
              type="radio"
              value="copy"
            />
            <Copy size={17} aria-hidden="true" />
            <span>
              <strong>{t("transfer.mode.copy")}</strong>
              <small>{t("transfer.mode.copyHelp")}</small>
            </span>
          </label>
          <label className={mode === "move" ? "is-active" : undefined}>
            <input
              checked={mode === "move"}
              disabled={interfaceBusy || Boolean(result) || !canMove}
              name="page-transfer-mode"
              onChange={() => setMode("move")}
              type="radio"
              value="move"
            />
            <MoveRight size={17} aria-hidden="true" />
            <span>
              <strong>{t("transfer.mode.move")}</strong>
              <small>
                {canMove ? t("transfer.mode.moveHelp") : t("transfer.mode.moveUnavailable")}
              </small>
            </span>
          </label>
        </div>

        <div className="page-transfer-workspaces">
          <section className="page-transfer-source" aria-labelledby="page-transfer-source-title">
            <div className="page-transfer-section-heading">
              <div>
                <span>{t("transfer.source.label")}</span>
                <h3 id="page-transfer-source-title">{sourceDocumentName}</h3>
              </div>
              <strong>
                {t(
                  transferPages.length === 1
                    ? "transfer.source.selected.one"
                    : "transfer.source.selected.other",
                  { count: formatNumber(transferPages.length) }
                )}
              </strong>
            </div>
            <div
              aria-label={t("transfer.source.drag.aria", { count: formatNumber(transferPages.length) })}
              className="page-transfer-source-strip"
              draggable={!interfaceBusy && !result}
              onDragStart={dragTransferPages}
              tabIndex={0}
            >
              <span className="page-transfer-grip" aria-hidden="true">
                <GripVertical size={18} />
              </span>
              {sourcePreviewPages.map((page, index) => (
                <TransferThumbnail
                  document={page.kind === "source" ? sourceMap.get(page.sourceId)?.document : null}
                  key={page.id}
                  label={t("transfer.source.page", { page: formatNumber(index + 1) })}
                  page={page}
                />
              ))}
              {transferPages.length > sourcePreviewPages.length ? (
                <span className="page-transfer-omitted">
                  {t("transfer.pages.more", {
                    count: formatNumber(transferPages.length - sourcePreviewPages.length)
                  })}
                </span>
              ) : null}
            </div>
            <p className="page-transfer-drag-hint">{t("transfer.source.drag.help")}</p>
          </section>

          <section className="page-transfer-destination" aria-labelledby="page-transfer-destination-title">
            <div className="page-transfer-section-heading">
              <div>
                <span>{t("transfer.destination.label")}</span>
                <h3 id="page-transfer-destination-title">
                  {destination?.source.name ??
                    (destinationPath ? fileNameFromPath(destinationPath) : t("transfer.destination.none"))}
                </h3>
              </div>
              <button disabled={interfaceBusy || Boolean(result)} onClick={() => void chooseDestination()} type="button">
                {busy === "choosing" ? (
                  <Loader2 className="spin" size={16} aria-hidden="true" />
                ) : (
                  <FolderOpen size={16} aria-hidden="true" />
                )}
                {t("transfer.action.choose")}
              </button>
            </div>

            {destinationPath && !destination ? (
              <form className="page-transfer-review" onSubmit={reviewDestination}>
                <label>
                  {t("transfer.password.label")} <span>{t("transfer.password.optional")}</span>
                  <div className="password-dialog-field">
                    <KeyRound size={16} aria-hidden="true" />
                    <input
                      autoComplete="off"
                      disabled={interfaceBusy}
                      onChange={(event) => {
                        setDestinationPassword(event.target.value);
                        pageImportInspectionJob.clearJob();
                        setError(null);
                      }}
                      spellCheck={false}
                      type="password"
                      value={destinationPassword}
                    />
                  </div>
                </label>
                <button className="primary" disabled={interfaceBusy} type="submit">
                  {busy === "reviewing" || busy === "loading" || pageImportInspectionJob.isActive ? (
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                  ) : (
                    <Files size={16} aria-hidden="true" />
                  )}
                  {t("transfer.action.review")}
                </button>
              </form>
            ) : null}

            {destination && destinationWindow ? (
              <>
                <div className="page-transfer-insertion-control">
                  <label>
                    {t("transfer.insertion.label")}
                    <input
                      disabled={interfaceBusy || Boolean(result)}
                      max={destination.document.numPages}
                      min={0}
                      onChange={(event) => setTransferInsertion(Number(event.target.value))}
                      type="number"
                      value={insertionIndex}
                    />
                  </label>
                  <span>{insertionLabel}</span>
                  <strong>
                    {t("transfer.destination.outputPages", { count: formatNumber(outputPageCount) })}
                  </strong>
                </div>
                <div className="page-transfer-destination-strip">
                  {destinationWindow.start > 0 ? (
                    <span className="page-transfer-omitted">
                      {t("transfer.pages.before", { count: formatNumber(destinationWindow.start) })}
                    </span>
                  ) : null}
                  {destinationWindow.pageNumbers.map((pageNumber) => (
                    <DestinationPageWithSlot
                      destination={destination}
                      disabled={interfaceBusy || Boolean(result)}
                      insertionIndex={insertionIndex}
                      key={pageNumber}
                      onDragOver={allowTransferDrop}
                      onDrop={dropTransferPages}
                      onSelect={setTransferInsertion}
                      pageNumber={pageNumber}
                      selectedPages={sourcePreviewPages}
                      sourceMap={sourceMap}
                    />
                  ))}
                  {destinationWindow.end === destination.document.numPages ? (
                    <>
                      <InsertionSlot
                        disabled={interfaceBusy || Boolean(result)}
                        index={destination.document.numPages}
                        label={t("transfer.insertion.afterLast")}
                        onDragOver={allowTransferDrop}
                        onDrop={dropTransferPages}
                        onSelect={setTransferInsertion}
                        selected={insertionIndex === destination.document.numPages}
                      />
                      {insertionIndex === destination.document.numPages ? (
                        <TransferredPreview pages={sourcePreviewPages} sourceMap={sourceMap} />
                      ) : null}
                    </>
                  ) : null}
                  {destinationWindow.end < destination.document.numPages ? (
                    <span className="page-transfer-omitted">
                      {t("transfer.pages.after", {
                        count: formatNumber(destination.document.numPages - destinationWindow.end)
                      })}
                    </span>
                  ) : null}
                </div>
              </>
            ) : !destinationPath ? (
              <button
                className="page-transfer-empty"
                disabled={interfaceBusy}
                onClick={() => void chooseDestination()}
                type="button"
              >
                <FileOutput size={24} aria-hidden="true" />
                <strong>{t("transfer.destination.emptyTitle")}</strong>
                <span>{t("transfer.destination.emptyDetail")}</span>
              </button>
            ) : null}
          </section>
        </div>

        {destination?.inspection.certificateSignature ? (
          <label className="signature-risk-check">
            <input
              checked={destinationCertificateAcknowledged}
              disabled={interfaceBusy || Boolean(result)}
              onChange={(event) => setDestinationCertificateAcknowledged(event.target.checked)}
              type="checkbox"
            />
            <ShieldAlert size={17} aria-hidden="true" />
            <span>{t("transfer.certificate.destination")}</span>
          </label>
        ) : null}
        {needsSourceCertificateAcknowledgement ? (
          <label className="signature-risk-check">
            <input
              checked={sourceCertificateAcknowledged}
              disabled={interfaceBusy || Boolean(result)}
              onChange={(event) => setSourceCertificateAcknowledged(event.target.checked)}
              type="checkbox"
            />
            <ShieldAlert size={17} aria-hidden="true" />
            <span>{t("transfer.certificate.source")}</span>
          </label>
        ) : null}

        {destination ? (
          <OutputProtectionFields
            disabled={interfaceBusy || Boolean(result)}
            onChange={setOutputProtection}
            qpdfAvailable={qpdfAvailable}
            value={outputProtection}
          />
        ) : null}

        {error ? (
          <p className="dialog-error" role="alert">
            {error}
          </p>
        ) : null}
        {pageImportInspectionJob.job ? (
          <PdfJobProgress
            cancelling={reviewCancelBusy}
            connectionError={pageImportInspectionJob.connectionError}
            job={pageImportInspectionJob.job}
            onCancel={() => void cancelReview()}
            onRetry={() => void reviewDestination()}
            retryDisabled={!destinationPath || interfaceBusy}
          />
        ) : null}
        {transferExportJob.job && !result ? (
          <PdfJobProgress
            cancelling={publishCancelBusy}
            connectionError={transferExportJob.connectionError}
            job={transferExportJob.job}
            onCancel={() => void cancelPublish()}
            onRetry={() => void publishTransfer()}
            retryDisabled={!destination || interfaceBusy}
          />
        ) : null}
        {result ? (
          <section className="page-transfer-success" aria-live="polite">
            <CheckCircle2 size={20} aria-hidden="true" />
            <div>
              <strong>
                {t(
                  mode === "move" ? "transfer.success.move" : "transfer.success.copy",
                  {
                    count: formatNumber(transferPages.length),
                    name: fileNameFromPath(result.outputPath)
                  }
                )}
              </strong>
              <span>{t("transfer.success.verified")}</span>
              {localisedWarnings.length > 0 ? (
                <ul>
                  {localisedWarnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              ) : null}
            </div>
          </section>
        ) : null}

        <p className="page-transfer-safety-note">{t("transfer.safety")}</p>
        <div className="dialog-actions">
          <button disabled={interfaceBusy} onClick={closeDialog} type="button">
            {result ? t("common.close") : t("common.cancel")}
          </button>
          {!result ? (
            <button
              className="primary"
              disabled={
                !destination ||
                !transferPlan ||
                interfaceBusy ||
                (destination.inspection.certificateSignature &&
                  !destinationCertificateAcknowledged) ||
                (needsSourceCertificateAcknowledgement && !sourceCertificateAcknowledged) ||
                !outputProtectionIsValid(outputProtection, qpdfAvailable)
              }
              onClick={() => void publishTransfer()}
              type="button"
            >
              {busy === "publishing" || transferExportJob.isActive ? (
                <Loader2 className="spin" size={16} aria-hidden="true" />
              ) : mode === "move" ? (
                <MoveRight size={16} aria-hidden="true" />
              ) : (
                <Copy size={16} aria-hidden="true" />
              )}
              {mode === "move" ? t("transfer.action.move") : t("transfer.action.copy")}
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function DestinationPageWithSlot({
  destination,
  disabled,
  insertionIndex,
  onDragOver,
  onDrop,
  onSelect,
  pageNumber,
  selectedPages,
  sourceMap
}: {
  destination: TransferDestination;
  disabled: boolean;
  insertionIndex: number;
  onDragOver: (event: DragEvent<HTMLElement>) => void;
  onDrop: (event: DragEvent<HTMLElement>, index: number) => void;
  onSelect: (index: number) => void;
  pageNumber: number;
  selectedPages: PlannedPage[];
  sourceMap: ReadonlyMap<string, PageTransferPdfSource>;
}) {
  const { formatNumber, t } = useI18n();
  const index = pageNumber - 1;
  return (
    <>
      <InsertionSlot
        disabled={disabled}
        index={index}
        label={
          pageNumber === 1
            ? t("transfer.insertion.beforeFirst")
            : t("transfer.insertion.beforePage", { page: formatNumber(pageNumber) })
        }
        onDragOver={onDragOver}
        onDrop={onDrop}
        onSelect={onSelect}
        selected={insertionIndex === index}
      />
      {insertionIndex === index ? (
        <TransferredPreview pages={selectedPages} sourceMap={sourceMap} />
      ) : null}
      <span className="page-transfer-thumbnail destination-page">
        <LazyPdfThumbnail document={destination.document} pageNumber={pageNumber} />
        <strong>{formatNumber(pageNumber)}</strong>
      </span>
    </>
  );
}

function InsertionSlot({
  disabled,
  index,
  label,
  onDragOver,
  onDrop,
  onSelect,
  selected
}: {
  disabled: boolean;
  index: number;
  label: string;
  onDragOver: (event: DragEvent<HTMLElement>) => void;
  onDrop: (event: DragEvent<HTMLElement>, index: number) => void;
  onSelect: (index: number) => void;
  selected: boolean;
}) {
  return (
    <button
      aria-label={label}
      aria-pressed={selected}
      className={`page-transfer-insertion-slot${selected ? " is-selected" : ""}`}
      disabled={disabled}
      onClick={() => onSelect(index)}
      onDragOver={onDragOver}
      onDrop={(event) => onDrop(event, index)}
      title={label}
      type="button"
    >
      <span />
    </button>
  );
}

function TransferredPreview({
  pages,
  sourceMap
}: {
  pages: PlannedPage[];
  sourceMap: ReadonlyMap<string, PageTransferPdfSource>;
}) {
  const { formatNumber, t } = useI18n();
  return (
    <span className="page-transfer-inserted-group">
      {pages.map((page, index) => (
        <TransferThumbnail
          document={page.kind === "source" ? sourceMap.get(page.sourceId)?.document : null}
          key={page.id}
          label={t("transfer.destination.insertedPage", { page: formatNumber(index + 1) })}
          page={page}
        />
      ))}
    </span>
  );
}

function TransferThumbnail({
  document,
  label,
  page
}: {
  document: PDFDocumentProxy | null | undefined;
  label: string;
  page: PlannedPage;
}) {
  return (
    <span aria-label={label} className="page-transfer-thumbnail transferred-page" role="img">
      {page.kind === "source" && document ? (
        <LazyPdfThumbnail
          document={document}
          pageNumber={page.sourcePage}
          rotation={page.rotation}
        />
      ) : (
        <span className="page-transfer-blank" style={{ aspectRatio: `${page.kind === "blank" ? page.widthPt / page.heightPt : 0.707}` }} />
      )}
    </span>
  );
}

function destinationPreviewWindow(pageCount: number, insertionIndex: number) {
  const width = Math.min(pageCount, MAX_VISIBLE_DESTINATION_PAGES);
  const idealStart = insertionIndex - Math.floor(width / 2);
  const start = Math.max(0, Math.min(pageCount - width, idealStart));
  const end = start + width;
  return {
    end,
    pageNumbers: Array.from({ length: width }, (_, index) => start + index + 1),
    start
  };
}

function suggestedTransferPath(path: string) {
  return path.toLocaleLowerCase("en-GB").endsWith(".pdf")
    ? `${path.slice(0, -4)}-with-pages.pdf`
    : `${path}-with-pages.pdf`;
}

function localPathsMatch(first: string, second: string) {
  const normalise = (value: string) => {
    const slashes = value.replace(/\\/gu, "/").replace(/\/+$/gu, "");
    return /^[a-z]:\//iu.test(slashes) ? slashes.toLocaleLowerCase("en-GB") : slashes;
  };
  return normalise(first) === normalise(second);
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/u).pop() || path;
}
