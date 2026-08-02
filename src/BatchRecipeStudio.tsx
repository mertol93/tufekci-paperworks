import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  ListChecks,
  Loader2,
  Save,
  ScanSearch,
  ShieldCheck,
  Trash2,
  X
} from "lucide-react";
import {
  BATCH_RECIPE_STORAGE_KEY,
  BUILT_IN_BATCH_RECIPES,
  MAX_CUSTOM_BATCH_RECIPES,
  batchRecipeSettingsError,
  buildBatchOutputFileNames,
  cloneBatchRecipeSettings,
  createCustomBatchRecipe,
  parseStoredBatchRecipes,
  serialiseStoredBatchRecipes,
  type BatchPrivacyOptions,
  type BatchRecipe,
  type BatchRecipeInputOrigin,
  type BatchRecipeSettings
} from "./batchRecipes";
import { type PdfArchiveReadiness } from "./ArchiveStudio";
import {
  batchRecipeExceptionKey,
  localiseBatchInputOrigin,
  localiseBatchInspectionError,
  localiseBatchNote,
  localiseBatchRecipeDescription,
  localiseBatchRecipeName,
  localiseBatchRecipeSettingsError,
  localiseBatchSkippedReason,
  localiseBatchSteps,
  localiseBatchWarnings
} from "./batchLocalisation";
import { useI18n } from "./I18nProvider";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { PdfEditSafetyNotice } from "./PdfEditSafetyNotice";
import { PdfJobProgress } from "./PdfJobProgress";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import type { Translate, TranslationKey, TranslationValues } from "./i18n";
import { localisePdfJobFailure } from "./pdfJobs";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";

type BatchRecipeStudioProps = {
  archiveReadiness: PdfArchiveReadiness | null;
  desktopMode: boolean;
  initialSourceOrigin?: BatchRecipeInputOrigin;
  initialSourcePassword?: string;
  initialSourcePath?: string;
  ocrEngineAvailable: boolean;
  ocrLanguages: ReadonlyArray<{ code: string; name: string }>;
  qpdfAvailable: boolean;
};

type PrivacyInspectionStatus = "clear" | "review" | "risk";
type SourceInspectionStatus = "analysing" | "error" | "ready" | "waiting";

type PdfPrivacyInspectionResult = {
  dangerCount: number;
  fileName: string;
  pageCount: number;
  sourceModifiedAtMs: number | null;
  sourceSize: number;
  status: PrivacyInspectionStatus;
  warningCount: number;
};

type BatchSourceInspectionItem = {
  error?: string | null;
  inspection?: PdfPrivacyInspectionResult | null;
  sourceIndex: number;
};

type InspectBatchSourcesResult = {
  failedCount: number;
  inspectedCount: number;
  items: BatchSourceInspectionItem[];
  sourceCount: number;
};

type BatchSource = {
  error?: string;
  id: string;
  inspection?: PdfPrivacyInspectionResult;
  inspectionStatus: SourceInspectionStatus;
  origin: BatchRecipeInputOrigin;
  password: string;
  path: string;
};

type BatchRecipeItemResult = {
  bytesWritten: number;
  imagesRecompressed: number;
  note?: string | null;
  outputPath?: string | null;
  pageCount: number;
  privacyStructuresRemoved: number;
  searchableTextPages: number;
  skippedReason?: string | null;
  sourceFileName: string;
  stepsApplied: string[];
  warnings: string[];
};

type RunBatchRecipeResult = {
  bytesWritten: number;
  encryption: "AES-256" | "None";
  inputCount: number;
  items: BatchRecipeItemResult[];
  outputCount: number;
  outputDirectory: string;
  skippedCount: number;
};

type BatchNotice = {
  kind: "error" | "info" | "success";
  key: TranslationKey;
  values?: TranslationValues;
};

const MAX_BATCH_INPUTS = 50;
const recommendedRecipe = BUILT_IN_BATCH_RECIPES[1];

const privacyOptionRows: Array<{
  key: keyof BatchPrivacyOptions;
  labelKey: TranslationKey;
}> = [
  { key: "removeMetadata", labelKey: "batch.privacy.metadata" },
  { key: "removeActiveContent", labelKey: "batch.privacy.activeContent" },
  { key: "removeAttachments", labelKey: "batch.privacy.attachments" },
  { key: "removeThumbnails", labelKey: "batch.privacy.thumbnails" },
  { key: "removeAnnotationsAndForms", labelKey: "batch.privacy.annotationsForms" }
];

export function BatchRecipeStudio({
  archiveReadiness,
  desktopMode,
  initialSourceOrigin = "document",
  initialSourcePassword,
  initialSourcePath,
  ocrEngineAvailable,
  ocrLanguages,
  qpdfAvailable
}: BatchRecipeStudioProps) {
  const { formatList, formatNumber, t } = useI18n();
  const [sources, setSources] = useState<BatchSource[]>(() =>
    initialSourcePath
      ? [
          createBatchSource(
            initialSourcePath,
            initialSourcePassword ?? "",
            initialSourceOrigin
          )
        ]
      : []
  );
  const [customRecipes, setCustomRecipes] = useState<BatchRecipe[]>(loadStoredRecipes);
  const [selectedRecipeId, setSelectedRecipeId] = useState(recommendedRecipe.id);
  const [settings, setSettings] = useState<BatchRecipeSettings>(() =>
    cloneBatchRecipeSettings(recommendedRecipe)
  );
  const [recipeName, setRecipeName] = useState("");
  const [outputDirectory, setOutputDirectory] = useState<string | null>(null);
  const [showPasswords, setShowPasswords] = useState(false);
  const [analysisBusy, setAnalysisBusy] = useState(false);
  const [analysisCancelling, setAnalysisCancelling] = useState(false);
  const [signatureRiskAcknowledged, setSignatureRiskAcknowledged] = useState(false);
  const [outputProtection, setOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [cancelBusy, setCancelBusy] = useState(false);
  const [notice, setNotice] = useState<BatchNotice | null>(() =>
    initialSourcePath && initialSourceOrigin !== "document"
      ? scanHandoffNotice(initialSourceOrigin)
      : null
  );
  const [result, setResult] = useState<RunBatchRecipeResult | null>(null);
  const lastSeedPath = useRef(initialSourcePath);
  const batchJob = usePdfJob<RunBatchRecipeResult>(desktopMode, "batch");
  const batchInspectionJob = usePdfJob<InspectBatchSourcesResult>(
    desktopMode,
    "batch-inspection"
  );
  const recipes = useMemo(
    () => [...BUILT_IN_BATCH_RECIPES, ...customRecipes],
    [customRecipes]
  );
  const selectedRecipe =
    recipes.find((recipe) => recipe.id === selectedRecipeId) ?? recommendedRecipe;
  const recipeSettingsError = batchRecipeSettingsError(settings);
  const recipeError = localiseBatchRecipeSettingsError(recipeSettingsError, t);
  const ocrLanguageListed = ocrLanguages.some(
    (language) => language.code === settings.ocrLanguage
  );
  const installedOcrLanguageCodes = new Set(ocrLanguages.map((language) => language.code));
  const ocrLanguageAvailable = settings.ocrLanguage
    .split("+")
    .every((code) => installedOcrLanguageCodes.has(code));
  const ocrAvailabilityError: TranslationKey | null = settings.recogniseText
    ? !ocrEngineAvailable
      ? "batch.availability.ocrEngine"
      : !ocrLanguageAvailable
        ? "batch.availability.ocrLanguage"
        : sources.some((source) => source.password) && !qpdfAvailable
          ? "batch.availability.ocrPassword"
          : null
    : null;
  const archiveAvailabilityError: TranslationKey | null = settings.archiveProfile
    ? !archiveReadiness?.ready
      ? "batch.availability.archiveEngines"
      : sources.some((source) => source.password) && !qpdfAvailable
        ? "batch.availability.archivePassword"
        : outputProtection.enabled
          ? "batch.availability.archiveProtection"
          : null
    : null;
  const outputFileNames = useMemo(
    () => buildBatchOutputFileNames(sources.map((source) => source.path), selectedRecipe.outputSuffix),
    [selectedRecipe.outputSuffix, sources]
  );
  const safetyKey = sources
    .map((source) => `${source.id}\u0000${source.path}\u0000${source.password}`)
    .join("\u0001");
  const sourcePathKey = sources.map((source) => `${source.id}\u0000${source.path}`).join("\u0001");
  const safetySources = useMemo(
    () =>
      sources.map((source) => ({
        id: source.id,
        label: fileNameFromPath(source.path),
        password: source.password,
        path: source.path
      })),
    [safetyKey]
  );
  const editSafety = usePdfEditSafety(desktopMode, safetySources, "batch-recipes");
  const certificateRiskAccepted =
    editSafety.signedSources.length === 0 || signatureRiskAcknowledged;
  const inspectedSources = sources.filter(
    (source) => source.inspectionStatus === "ready" && source.inspection
  );
  const analysisReady = sources.length > 0 && inspectedSources.length === sources.length;
  const busy = analysisBusy || batchInspectionJob.isActive || batchJob.isActive;
  const canRun = Boolean(
    desktopMode &&
      outputDirectory &&
      analysisReady &&
      !recipeSettingsError &&
      !ocrAvailabilityError &&
      !archiveAvailabilityError &&
      editSafety.isReady &&
      certificateRiskAccepted &&
      outputProtectionIsValid(outputProtection, qpdfAvailable) &&
      !busy
  );
  const totalPages = inspectedSources.reduce(
    (total, source) => total + (source.inspection?.pageCount ?? 0),
    0
  );
  const totalSourceBytes = inspectedSources.reduce(
    (total, source) => total + (source.inspection?.sourceSize ?? 0),
    0
  );
  const totalRisks = inspectedSources.reduce(
    (total, source) => total + (source.inspection?.dangerCount ?? 0),
    0
  );
  const totalWarnings = inspectedSources.reduce(
    (total, source) => total + (source.inspection?.warningCount ?? 0),
    0
  );
  const selectedRecipeEdited = !sameSettings(settings, selectedRecipe);
  const jobFailure =
    batchJob.job?.status === "failed"
      ? localisePdfJobFailure(batchJob.job, t)
      : null;

  useEffect(() => {
    if (!initialSourcePath || lastSeedPath.current === initialSourcePath) {
      return;
    }
    lastSeedPath.current = initialSourcePath;
    setSources((current) => {
      if (
        current.length >= MAX_BATCH_INPUTS ||
        current.some((source) => samePath(source.path, initialSourcePath))
      ) {
        return current;
      }
      return [
        ...current,
        createBatchSource(
          initialSourcePath,
          initialSourcePassword ?? "",
          initialSourceOrigin
        )
      ];
    });
    setResult(null);
    setNotice(
      initialSourceOrigin === "document" ? null : scanHandoffNotice(initialSourceOrigin)
    );
    batchInspectionJob.clearJob();
  }, [initialSourceOrigin, initialSourcePassword, initialSourcePath]);

  useEffect(() => {
    setSignatureRiskAcknowledged(false);
  }, [safetyKey]);

  useEffect(() => {
    setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
  }, [sourcePathKey]);

  useEffect(() => {
    const job = batchJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setResult(job.result);
      setOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      setNotice({
        kind: "success",
        key:
          job.result.outputCount === 0
            ? "batch.notice.noOutputs"
            : job.result.outputCount === 1
              ? job.result.encryption === "AES-256"
                ? "batch.notice.published.oneProtected"
                : "batch.notice.published.one"
              : job.result.encryption === "AES-256"
                ? "batch.notice.published.otherProtected"
                : "batch.notice.published.other",
        values: { count: formatNumber(job.result.outputCount) }
      });
    } else if (job.status === "cancelled") {
      setResult(null);
      setNotice({
        kind: "info",
        key: "batch.notice.cancelled"
      });
    } else if (job.status === "failed") {
      setResult(null);
      setNotice(null);
    }
  }, [batchJob.job?.jobId, batchJob.job?.status, formatNumber]);

  const chooseSources = async () => {
    setNotice(null);
    try {
      const selected = await open({
        directory: false,
        filters: [{ name: t("batch.dialog.filter"), extensions: ["pdf"] }],
        multiple: true,
        title: t("batch.dialog.chooseSources")
      });
      const selectedPaths = typeof selected === "string" ? [selected] : selected ?? [];
      if (selectedPaths.length === 0) {
        return;
      }
      const next = [...sources];
      let added = 0;
      let omitted = 0;
      for (const path of selectedPaths) {
        if (next.some((source) => samePath(source.path, path))) {
          omitted += 1;
        } else if (next.length >= MAX_BATCH_INPUTS) {
          omitted += 1;
        } else {
          next.push(createBatchSource(path));
          added += 1;
        }
      }
      setSources(next);
      setResult(null);
      batchInspectionJob.clearJob();
      batchJob.clearJob();
      if (omitted > 0) {
        setNotice({
          kind: "info",
          key:
            added === 1
              ? omitted === 1
                ? "batch.notice.added.oneOne"
                : "batch.notice.added.oneOther"
              : omitted === 1
                ? "batch.notice.added.otherOne"
                : "batch.notice.added.otherOther",
          values: {
            added: formatNumber(added),
            omitted: formatNumber(omitted)
          }
        });
      }
    } catch (reason) {
      void reason;
      setNotice({ kind: "error", key: "batch.error.chooseSources" });
    }
  };

  const removeSource = (id: string) => {
    setSources((current) => current.filter((source) => source.id !== id));
    setResult(null);
    setNotice(null);
    batchInspectionJob.clearJob();
    batchJob.clearJob();
  };

  const clearSources = () => {
    setSources([]);
    setOutputDirectory(null);
    setOutputProtection(createOutputProtectionDraft());
    setResult(null);
    setNotice(null);
    batchInspectionJob.clearJob();
    batchJob.clearJob();
  };

  const updatePassword = (id: string, password: string) => {
    setSources((current) =>
      current.map((source) =>
        source.id === id
          ? {
              ...source,
              error: undefined,
              inspection: undefined,
              inspectionStatus: "waiting",
              password
            }
          : source
      )
    );
    setResult(null);
    setNotice(null);
    batchInspectionJob.clearJob();
    batchJob.clearJob();
  };

  const analyseSources = async () => {
    if (!desktopMode || sources.length === 0 || busy) {
      return;
    }
    const sourceSnapshot = sources.map((source) => ({ ...source }));
    setAnalysisBusy(true);
    setAnalysisCancelling(false);
    setResult(null);
    setNotice(null);
    batchInspectionJob.clearJob();
    batchJob.clearJob();
    setSources((current) =>
      current.map((source) => ({
        ...source,
        error: undefined,
        inspection: undefined,
        inspectionStatus: "analysing"
      }))
    );
    try {
      const review = await batchInspectionJob.startJobAndWait({
        sources: sourceSnapshot.map((source) => ({
          inputPassword: source.password || null,
          inputPath: source.path
        }))
      });
      if (
        review.sourceCount !== sourceSnapshot.length ||
        review.items.length !== sourceSnapshot.length
      ) {
        throw new BatchReviewError("incomplete");
      }
      const items = new Map(review.items.map((item) => [item.sourceIndex, item]));
      const sourceIndexes = new Map(
        sourceSnapshot.map((source, sourceIndex) => [source.id, sourceIndex])
      );
      if (items.size !== sourceSnapshot.length) {
        throw new BatchReviewError("duplicate");
      }
      setSources((current) =>
        current.map((source) => {
          const sourceIndex = sourceIndexes.get(source.id);
          if (sourceIndex === undefined) {
            return source;
          }
          const item = items.get(sourceIndex);
          if (!item || item.sourceIndex !== sourceIndex) {
            return {
              ...source,
              error: "incomplete",
              inspection: undefined,
              inspectionStatus: "error"
            };
          }
          return item.inspection
            ? {
                ...source,
                error: undefined,
                inspection: item.inspection,
                inspectionStatus: "ready"
              }
            : {
                ...source,
                error: item.error || "inspect",
                inspection: undefined,
                inspectionStatus: "error"
              };
        })
      );
      batchInspectionJob.clearJob();
      setNotice(
        review.failedCount > 0
          ? {
              kind: "error",
              key:
                review.failedCount === 1
                  ? "batch.inspection.failed.one"
                  : "batch.inspection.failed.other",
              values: { count: formatNumber(review.failedCount) }
            }
          : {
              kind: "success",
              key:
                review.inspectedCount === 1
                  ? "batch.inspection.succeeded.one"
                  : "batch.inspection.succeeded.other",
              values: { count: formatNumber(review.inspectedCount) }
            }
      );
    } catch (reason) {
      const cancelled =
        reason instanceof Error && reason.message === "The PDF job was cancelled.";
      const reviewErrorKey =
        reason instanceof BatchReviewError
          ? reason.code === "duplicate"
            ? "batch.error.reviewDuplicate"
            : "batch.error.reviewIncomplete"
          : "batch.error.inspectSources";
      setSources((current) =>
        current.map((source) =>
          source.inspectionStatus === "analysing"
            ? {
                ...source,
                error: cancelled ? undefined : "request",
                inspection: undefined,
                inspectionStatus: cancelled ? "waiting" : "error"
              }
            : source
        )
      );
      setNotice({
        kind: cancelled ? "info" : "error",
        key: cancelled ? "batch.inspection.cancelled" : reviewErrorKey
      });
    } finally {
      setAnalysisBusy(false);
      setAnalysisCancelling(false);
    }
  };

  const stopAnalysis = async () => {
    if (!batchInspectionJob.isActive || analysisCancelling) {
      return;
    }
    setAnalysisCancelling(true);
    try {
      await batchInspectionJob.cancelJob();
    } catch (reason) {
      void reason;
      setAnalysisCancelling(false);
      setNotice({ kind: "error", key: "batch.error.cancelInspection" });
    }
  };

  const selectRecipe = (id: string) => {
    const recipe = recipes.find((candidate) => candidate.id === id);
    if (!recipe) {
      return;
    }
    setSelectedRecipeId(recipe.id);
    setSettings(cloneBatchRecipeSettings(recipe));
    if (recipe.archiveProfile) {
      setOutputProtection(createOutputProtectionDraft());
    }
    setResult(null);
    setNotice(null);
    batchJob.clearJob();
  };

  const updateSetting = <K extends keyof Omit<BatchRecipeSettings, "privacyOptions">>(
    key: K,
    value: BatchRecipeSettings[K]
  ) => {
    setSettings((current) => ({ ...current, [key]: value }));
    setResult(null);
    setNotice(null);
    batchJob.clearJob();
  };

  const togglePrivacyOption = (key: keyof BatchPrivacyOptions) => {
    setSettings((current) => ({
      ...current,
      privacyOptions: {
        ...current.privacyOptions,
        [key]: !current.privacyOptions[key]
      }
    }));
    setResult(null);
    setNotice(null);
    batchJob.clearJob();
  };

  const toggleSearchableOcr = (enabled: boolean) => {
    setSettings((current) => ({
      ...current,
      recogniseText: enabled,
      straighten: enabled ? current.straighten : false
    }));
    setResult(null);
    setNotice(null);
    batchJob.clearJob();
  };

  const togglePdfArchive = (enabled: boolean) => {
    setSettings((current) => ({
      ...current,
      archiveProfile: enabled ? current.archiveProfile ?? "pdfa-2b" : null
    }));
    if (enabled) {
      setOutputProtection(createOutputProtectionDraft());
    }
    setResult(null);
    setNotice(null);
    batchJob.clearJob();
  };

  const saveCustomRecipe = () => {
    if (customRecipes.length >= MAX_CUSTOM_BATCH_RECIPES) {
      setNotice({
        kind: "error",
        key: "batch.recipe.limit",
        values: { count: formatNumber(MAX_CUSTOM_BATCH_RECIPES) }
      });
      return;
    }
    try {
      const recipe = createCustomBatchRecipe(createRecipeId(), recipeName, settings);
      const next = [...customRecipes, recipe];
      persistRecipes(next);
      setCustomRecipes(next);
      setSelectedRecipeId(recipe.id);
      setRecipeName("");
      setNotice({
        kind: "success",
        key: "batch.recipe.saved",
        values: { name: recipe.name }
      });
    } catch (reason) {
      setNotice({
        kind: "error",
        key: batchRecipeExceptionKey(reason)
      });
    }
  };

  const deleteSelectedRecipe = () => {
    if (!selectedRecipe.custom) {
      return;
    }
    try {
      const next = customRecipes.filter((recipe) => recipe.id !== selectedRecipe.id);
      persistRecipes(next);
      setCustomRecipes(next);
      setSelectedRecipeId(recommendedRecipe.id);
      setSettings(cloneBatchRecipeSettings(recommendedRecipe));
      setNotice({
        kind: "info",
        key: "batch.recipe.removed",
        values: { name: selectedRecipe.name }
      });
    } catch (reason) {
      void reason;
      setNotice({ kind: "error", key: "batch.recipe.error.delete" });
    }
  };

  const chooseOutputDirectory = async () => {
    setNotice(null);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("batch.dialog.output")
      });
      if (typeof selected === "string") {
        setOutputDirectory(selected);
        setResult(null);
        batchJob.clearJob();
      }
    } catch (reason) {
      void reason;
      setNotice({ kind: "error", key: "batch.error.chooseOutput" });
    }
  };

  const runRecipe = async () => {
    if (!canRun || !outputDirectory) {
      return;
    }
    setResult(null);
    setNotice(null);
    try {
      await batchJob.startJob({
        inputs: sources.map((source, index) => ({
          acknowledgeCertificateSignatures: signatureRiskAcknowledged,
          expectedPageCount: source.inspection?.pageCount ?? 0,
          expectedSourceModifiedAtMs: source.inspection?.sourceModifiedAtMs ?? null,
          expectedSourceSize: source.inspection?.sourceSize ?? 0,
          inputPassword: source.password || null,
          inputPath: source.path,
          outputFileName: outputFileNames[index]
        })),
        options: settings,
        outputDirectory,
        outputProtection: toPdfOutputProtection(outputProtection, qpdfAvailable)
      });
    } catch (reason) {
      void reason;
      setNotice({ kind: "error", key: "batch.error.start" });
    }
  };

  const cancelRecipe = async () => {
    if (!batchJob.isActive || cancelBusy) {
      return;
    }
    setCancelBusy(true);
    try {
      await batchJob.cancelJob();
    } catch (reason) {
      void reason;
      setCancelBusy(false);
      setNotice({ kind: "error", key: "batch.error.cancel" });
    }
  };

  return (
    <section className="batch-studio">
      <div className="batch-heading">
        <div>
          <h3>{t("batch.heading.title")}</h3>
          <p>{t("batch.heading.description")}</p>
        </div>
        <ListChecks size={18} aria-hidden="true" />
      </div>

      <div className="batch-source-actions">
        <button disabled={!desktopMode || busy || sources.length >= MAX_BATCH_INPUTS} onClick={chooseSources} type="button">
          <FolderOpen size={16} aria-hidden="true" />
          {t("batch.action.add")}
        </button>
        <button disabled={busy || sources.length === 0} onClick={clearSources} type="button">
          <Trash2 size={15} aria-hidden="true" />
          {t("common.clear")}
        </button>
      </div>

      {sources.length === 0 ? (
        <div className="batch-empty">
          <FileText size={18} aria-hidden="true" />
          <span>
            {t("batch.empty", { count: formatNumber(MAX_BATCH_INPUTS) })}
          </span>
        </div>
      ) : (
        <>
          <div className="batch-source-summary">
            <span><strong>{formatNumber(sources.length)}</strong> {t("batch.summary.pdfs")}</span>
            <span><strong>{analysisReady ? formatNumber(totalPages) : "-"}</strong> {t("batch.summary.pages")}</span>
            <span><strong>{analysisReady ? formatFileSize(totalSourceBytes, formatNumber) : "-"}</strong> {t("batch.summary.source")}</span>
          </div>
          <ol className="batch-source-list">
            {sources.map((source, index) => (
              <li key={source.id}>
                <div className="batch-source-heading">
                  <span className="batch-source-order">{formatNumber(index + 1)}</span>
                  <span>
                    <strong>{fileNameFromPath(source.path)}</strong>
                    {source.origin !== "document" ? (
                      <small className="batch-source-origin">
                        {localiseBatchInputOrigin(source.origin, t)}
                      </small>
                    ) : null}
                    <small title={source.path}>{t("batch.source.local")}</small>
                  </span>
                  <button
                    aria-label={t("batch.action.removeAria", {
                      name: fileNameFromPath(source.path)
                    })}
                    className="icon-button"
                    disabled={busy}
                    onClick={() => removeSource(source.id)}
                    title={t("batch.action.removeTitle")}
                    type="button"
                  >
                    <X size={15} aria-hidden="true" />
                  </button>
                </div>
                <label className="batch-password">
                  <span>{t("batch.source.password")}</span>
                  <input
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => updatePassword(source.id, event.target.value)}
                    spellCheck={false}
                    type={showPasswords ? "text" : "password"}
                    value={source.password}
                  />
                </label>
                <SourceInspection
                  formatNumber={formatNumber}
                  source={source}
                  t={t}
                />
              </li>
            ))}
          </ol>
          <button className="show-passwords" disabled={busy} onClick={() => setShowPasswords((current) => !current)} type="button">
            {showPasswords ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
            {showPasswords
              ? t("batch.action.hidePasswords")
              : t("batch.action.showPasswords")}
          </button>
        </>
      )}

      <button
        className="wide-button"
        disabled={!desktopMode || sources.length === 0 || busy}
        onClick={analyseSources}
        type="button"
      >
        {analysisBusy || batchInspectionJob.isActive ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <ScanSearch size={17} aria-hidden="true" />}
        {analysisBusy || batchInspectionJob.isActive
          ? t("batch.action.inspecting")
          : analysisReady
            ? t("batch.action.inspectAgain")
            : t("batch.action.inspect")}
      </button>

      {batchInspectionJob.job ? (
        <PdfJobProgress
          cancelling={analysisCancelling}
          connectionError={batchInspectionJob.connectionError}
          job={batchInspectionJob.job}
          onCancel={() => void stopAnalysis()}
          onRetry={() => void analyseSources()}
          retryDisabled={!desktopMode || sources.length === 0 || busy}
        />
      ) : null}
      {!batchInspectionJob.job && batchInspectionJob.connectionError ? (
        <p className="batch-availability-error" role="status">
          {t("job.connectionError")}
        </p>
      ) : null}

      {analysisReady ? (
        <div className={`batch-inspection-summary ${totalRisks > 0 ? "is-risk" : totalWarnings > 0 ? "is-review" : "is-clear"}`}>
          {totalRisks > 0 ? <AlertTriangle size={17} aria-hidden="true" /> : <CheckCircle2 size={17} aria-hidden="true" />}
          <span>
            <strong>
              {t("batch.inspection.summary", {
                risks: formatNumber(totalRisks),
                warnings: formatNumber(totalWarnings)
              })}
            </strong>
            <small>{t("batch.inspection.help")}</small>
          </span>
        </div>
      ) : null}

      <section className="batch-recipe-panel">
        <label className="batch-select-field">
          <span>{t("batch.recipe.label")}</span>
          <select disabled={busy} onChange={(event) => selectRecipe(event.target.value)} value={selectedRecipe.id}>
            <optgroup label={t("batch.recipe.group.builtIn")}>
              {BUILT_IN_BATCH_RECIPES.map((recipe) => (
                <option key={recipe.id} value={recipe.id}>
                  {localiseBatchRecipeName(recipe, t)}
                </option>
              ))}
            </optgroup>
            {customRecipes.length > 0 ? (
              <optgroup label={t("batch.recipe.group.saved")}>
                {customRecipes.map((recipe) => (
                  <option key={recipe.id} value={recipe.id}>{recipe.name}</option>
                ))}
              </optgroup>
            ) : null}
          </select>
        </label>
        <p className="batch-recipe-description">
          {localiseBatchRecipeDescription(selectedRecipe, t)}
          {selectedRecipeEdited ? <strong> {t("batch.recipe.edited")}</strong> : null}
        </p>

        <div className="batch-step-toggles">
          <label>
            <input
              checked={settings.recogniseText}
              disabled={busy}
              onChange={(event) => toggleSearchableOcr(event.target.checked)}
              type="checkbox"
            />
            <span><strong>{t("batch.step.ocr")}</strong><small>{t("batch.step.ocrDescription")}</small></span>
          </label>
          <label>
            <input
              checked={settings.cleanPrivacy}
              disabled={busy}
              onChange={(event) => updateSetting("cleanPrivacy", event.target.checked)}
              type="checkbox"
            />
            <span><strong>{t("batch.option.privacy")}</strong><small>{t("batch.option.privacyDescription")}</small></span>
          </label>
          <label>
            <input
              checked={settings.compress}
              disabled={busy}
              onChange={(event) => updateSetting("compress", event.target.checked)}
              type="checkbox"
            />
            <span><strong>{t("batch.option.compress")}</strong><small>{t("batch.option.compressDescription")}</small></span>
          </label>
          <label>
            <input
              checked={settings.archiveProfile !== null}
              disabled={busy}
              onChange={(event) => togglePdfArchive(event.target.checked)}
              type="checkbox"
            />
            <span><strong>{t("batch.option.archive")}</strong><small>{t("batch.option.archiveDescription")}</small></span>
          </label>
        </div>

        {settings.recogniseText ? (
          <fieldset className="batch-privacy-options batch-ocr-options" disabled={busy}>
            <legend>{t("batch.ocr.legend")}</legend>
            <label className="batch-ocr-language">
              <span>{t("batch.ocr.language")}</span>
              <select
                disabled={busy || ocrLanguages.length === 0}
                onChange={(event) => updateSetting("ocrLanguage", event.target.value)}
                value={settings.ocrLanguage}
              >
                {ocrLanguages.length === 0 ? (
                  <option value={settings.ocrLanguage}>{t("batch.ocr.languagesMissing")}</option>
                ) : (
                  <>
                    {!ocrLanguageListed ? (
                      <option value={settings.ocrLanguage}>
                        {settings.ocrLanguage} ({ocrLanguageAvailable
                          ? t("batch.ocr.combined")
                          : t("batch.ocr.notInstalled")})
                      </option>
                    ) : null}
                    {ocrLanguages.map((language) => (
                      <option key={language.code} value={language.code}>{language.name} ({language.code})</option>
                    ))}
                  </>
                )}
              </select>
            </label>
            <label>
              <input
                checked={settings.straighten}
                onChange={(event) => updateSetting("straighten", event.target.checked)}
                type="checkbox"
              />
              <span>{t("batch.ocr.deskew")}</span>
            </label>
          </fieldset>
        ) : null}

        {settings.archiveProfile ? (
          <fieldset className="batch-privacy-options batch-archive-options" disabled={busy}>
            <legend>{t("batch.archive.legend")}</legend>
            <label className="batch-ocr-language">
              <span>{t("batch.archive.profile")}</span>
              <select
                onChange={(event) =>
                  updateSetting(
                    "archiveProfile",
                    event.target.value as NonNullable<BatchRecipeSettings["archiveProfile"]>
                  )
                }
                value={settings.archiveProfile}
              >
                <option value="pdfa-1b">{t("batch.archive.pdfa1b")}</option>
                <option value="pdfa-2b">{t("batch.archive.pdfa2b")}</option>
                <option value="pdfa-3b">{t("batch.archive.pdfa3b")}</option>
              </select>
            </label>
            <small>{t("batch.archive.note")}</small>
          </fieldset>
        ) : null}

        {ocrAvailabilityError ? (
          <div className="batch-warning is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t(ocrAvailabilityError)}</span>
          </div>
        ) : null}

        {archiveAvailabilityError ? (
          <div className="batch-warning is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t(archiveAvailabilityError)}</span>
          </div>
        ) : null}

        {settings.cleanPrivacy ? (
          <fieldset className="batch-privacy-options" disabled={busy}>
            <legend>{t("batch.privacy.legend")}</legend>
            {privacyOptionRows.map((option) => (
              <label className={option.key === "removeAnnotationsAndForms" ? "is-caution" : undefined} key={option.key}>
                <input
                  checked={settings.privacyOptions[option.key]}
                  onChange={() => togglePrivacyOption(option.key)}
                  type="checkbox"
                />
                <span>{t(option.labelKey)}</span>
              </label>
            ))}
          </fieldset>
        ) : null}

        {settings.privacyOptions.removeAnnotationsAndForms && settings.cleanPrivacy ? (
          <div className="batch-warning">
            <AlertTriangle size={16} aria-hidden="true" />
            <span>{t("batch.privacy.caution")}</span>
          </div>
        ) : null}

        {settings.compress ? (
          <label className="batch-quality">
            <span>
              <strong>{t("batch.quality.label")}</strong>
              <b>{formatNumber(settings.jpegQuality / 100, { style: "percent" })}</b>
            </span>
            <input
              disabled={busy}
              max="95"
              min="40"
              onChange={(event) => updateSetting("jpegQuality", Number(event.target.value))}
              step="1"
              type="range"
              value={settings.jpegQuality}
            />
          </label>
        ) : null}

        {recipeError ? (
          <div className="batch-warning is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{recipeError}</span>
          </div>
        ) : null}

        <div className="batch-save-recipe">
          <label>
            <span>{t("batch.recipe.saveAs")}</span>
            <input
              disabled={busy}
              maxLength={60}
              onChange={(event) => setRecipeName(event.target.value)}
              placeholder={t("batch.recipe.placeholder")}
              value={recipeName}
            />
          </label>
          <button disabled={busy || !recipeName.trim() || Boolean(recipeError)} onClick={saveCustomRecipe} type="button">
            <Save size={15} aria-hidden="true" />
            {t("batch.action.save")}
          </button>
        </div>
        {selectedRecipe.custom ? (
          <button className="batch-delete-recipe" disabled={busy} onClick={deleteSelectedRecipe} type="button">
            <Trash2 size={15} aria-hidden="true" />
            {t("batch.action.deleteRecipe", { name: selectedRecipe.name })}
          </button>
        ) : null}
        <small className="batch-storage-note">{t("batch.recipe.storageNote")}</small>
      </section>

      <button className="wide-button" disabled={!desktopMode || busy} onClick={chooseOutputDirectory} type="button">
        <FolderOpen size={17} aria-hidden="true" />
        {outputDirectory
          ? t("batch.action.chooseAnotherOutput")
          : t("batch.action.chooseOutput")}
      </button>
      {outputDirectory ? (
        <div className="batch-output-folder">
          <FolderOpen size={16} aria-hidden="true" />
          <span>
            <strong>{fileNameFromPath(outputDirectory)}</strong>
            <small title={outputDirectory}>{t("batch.output.local")}</small>
          </span>
        </div>
      ) : null}

      <OutputProtectionFields
        disabled={busy || settings.archiveProfile !== null}
        onChange={(value) => {
          setOutputProtection(value);
          setResult(null);
          setNotice(null);
          batchJob.clearJob();
        }}
        qpdfAvailable={qpdfAvailable}
        value={outputProtection}
      />

      <PdfEditSafetyNotice
        acknowledged={signatureRiskAcknowledged}
        busy={busy}
        editSafety={editSafety}
        onAcknowledgedChange={setSignatureRiskAcknowledged}
        rewriteDescription={t("batch.signature.rewrite")}
      />

      <button className="primary wide-button" disabled={!canRun} onClick={runRecipe} type="button">
        {batchJob.isActive ? <Loader2 className="spin" size={17} aria-hidden="true" /> : <ShieldCheck size={17} aria-hidden="true" />}
        {batchJob.isActive
          ? t("batch.action.running")
          : t(
              sources.length === 1
                ? "batch.action.run.one"
                : "batch.action.run.other",
              { count: formatNumber(sources.length) }
            )}
      </button>

      {batchJob.job ? (
        <PdfJobProgress
          cancelling={cancelBusy}
          connectionError={batchJob.connectionError}
          job={batchJob.job}
          onCancel={cancelRecipe}
          onRetry={() => void runRecipe()}
          retryDisabled={!canRun}
        />
      ) : null}

      {!batchJob.isActive && batchJob.connectionError ? (
        <div className="batch-notice is-info" role="status">
          <AlertCircle size={16} aria-hidden="true" />
          <span>{t("job.connectionError")}</span>
        </div>
      ) : null}

      {notice ? (
        <div className={`batch-notice is-${notice.kind}`} role={notice.kind === "error" ? "alert" : "status"}>
          {notice.kind === "success" ? <CheckCircle2 size={17} aria-hidden="true" /> : notice.kind === "error" ? <AlertCircle size={17} aria-hidden="true" /> : <ListChecks size={17} aria-hidden="true" />}
          <span>{t(notice.key, notice.values)}</span>
        </div>
      ) : null}

      {jobFailure ? (
        <div className="batch-notice is-error" role="alert">
          <AlertCircle size={17} aria-hidden="true" />
          <span>{jobFailure}</span>
        </div>
      ) : null}

      {result ? (
        <section className="batch-result" aria-live="polite">
          <header>
            <CheckCircle2 size={18} aria-hidden="true" />
            <span>
              <strong>
                {t("batch.result.summary", {
                  published: formatNumber(result.outputCount),
                  skipped: formatNumber(result.skippedCount)
                })}
              </strong>
              <small>
                {t("batch.result.detail", {
                  encryption:
                    result.encryption === "AES-256"
                      ? t("common.encryption.protected")
                      : t("common.encryption.unprotected"),
                  folder: fileNameFromPath(result.outputDirectory),
                  size: formatFileSize(result.bytesWritten, formatNumber)
                })}
              </small>
            </span>
          </header>
          <ul>
            {result.items.map((item, index) => (
              <li className={item.outputPath ? "is-published" : "is-skipped"} key={`${item.sourceFileName}-${index}`}>
                <span>
                  <strong>{item.sourceFileName}</strong>
                  <small title={item.outputPath ?? undefined}>
                    {item.outputPath
                      ? t("batch.result.output", {
                          name: fileNameFromPath(item.outputPath),
                          size: formatFileSize(item.bytesWritten, formatNumber)
                        })
                      : localiseBatchSkippedReason(item.skippedReason, t)}
                  </small>
                  {item.stepsApplied.length > 0 ? (
                    <small>{formatList(localiseBatchSteps(item.stepsApplied, t))}</small>
                  ) : null}
                  {item.searchableTextPages > 0 ? (
                    <small>
                      {t(
                        item.pageCount === 1
                          ? "batch.result.searchable.one"
                          : "batch.result.searchable.other",
                        {
                          count: formatNumber(item.pageCount),
                          searchable: formatNumber(item.searchableTextPages)
                        }
                      )}
                    </small>
                  ) : null}
                  {item.note ? <small>{localiseBatchNote(item.note, t)}</small> : null}
                  {localiseBatchWarnings(item.warnings, t, formatNumber).map((warning) => (
                    <small className="is-warning" key={warning}>{warning}</small>
                  ))}
                </span>
                {item.outputPath ? <CheckCircle2 size={16} aria-hidden="true" /> : <ListChecks size={16} aria-hidden="true" />}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <p className="batch-note">
        {t("batch.note")}
      </p>
    </section>
  );
}

function SourceInspection({
  formatNumber,
  source,
  t
}: {
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  source: BatchSource;
  t: Translate;
}) {
  if (source.inspectionStatus === "analysing") {
    return <span className="batch-source-status is-working"><Loader2 className="spin" size={14} aria-hidden="true" /> {t("batch.source.inspecting")}</span>;
  }
  if (source.inspectionStatus === "error") {
    return <span className="batch-source-status is-error"><AlertCircle size={14} aria-hidden="true" /> {localiseBatchInspectionError(source.error, t)}</span>;
  }
  if (source.inspectionStatus === "ready" && source.inspection) {
    return (
      <span className={`batch-source-status is-${source.inspection.status}`}>
        {source.inspection.status === "risk" ? <AlertTriangle size={14} aria-hidden="true" /> : <CheckCircle2 size={14} aria-hidden="true" />}
        {t(
          source.inspection.pageCount === 1
            ? "batch.source.ready.one"
            : "batch.source.ready.other",
          {
            count: formatNumber(source.inspection.pageCount),
            risks: formatNumber(source.inspection.dangerCount),
            size: formatFileSize(source.inspection.sourceSize, formatNumber)
          }
        )}
      </span>
    );
  }
  return <span className="batch-source-status"><ScanSearch size={14} aria-hidden="true" /> {t("batch.source.awaiting")}</span>;
}

function createBatchSource(
  path: string,
  password = "",
  origin: BatchRecipeInputOrigin = "document"
): BatchSource {
  return {
    id: createSourceId(),
    inspectionStatus: "waiting",
    origin,
    password,
    path
  };
}

function scanHandoffNotice(origin: BatchRecipeInputOrigin): BatchNotice {
  return {
    kind: "info",
    key:
      origin === "connected-scanner"
        ? "batch.notice.handoff.connectedScanner"
        : origin === "image-scan"
          ? "batch.notice.handoff.imageScan"
          : "batch.notice.handoff.document"
  };
}

function createSourceId() {
  return `source-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`}`;
}

function createRecipeId() {
  return `custom-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`}`;
}

function loadStoredRecipes() {
  try {
    return parseStoredBatchRecipes(window.localStorage.getItem(BATCH_RECIPE_STORAGE_KEY));
  } catch {
    return [];
  }
}

function persistRecipes(recipes: readonly BatchRecipe[]) {
  window.localStorage.setItem(
    BATCH_RECIPE_STORAGE_KEY,
    serialiseStoredBatchRecipes(recipes)
  );
}

function sameSettings(left: BatchRecipeSettings, right: BatchRecipeSettings) {
  return (
    left.archiveProfile === right.archiveProfile &&
    left.cleanPrivacy === right.cleanPrivacy &&
    left.compress === right.compress &&
    left.jpegQuality === right.jpegQuality &&
    left.ocrLanguage === right.ocrLanguage &&
    left.recogniseText === right.recogniseText &&
    left.straighten === right.straighten &&
    privacyOptionRows.every(
      (option) => left.privacyOptions[option.key] === right.privacyOptions[option.key]
    )
  );
}

function samePath(left: string, right: string) {
  return left.normalize("NFC").toLocaleLowerCase("en-GB") === right.normalize("NFC").toLocaleLowerCase("en-GB");
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function formatFileSize(
  bytes: number,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  if (bytes < 1024 * 1024) {
    return `${formatNumber(bytes / 1024, { maximumFractionDigits: 1 })} KiB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${formatNumber(bytes / (1024 * 1024), { maximumFractionDigits: 1 })} MiB`;
  }
  return `${formatNumber(bytes / (1024 * 1024 * 1024), { maximumFractionDigits: 2 })} GiB`;
}

class BatchReviewError extends Error {
  constructor(readonly code: "duplicate" | "incomplete") {
    super(code);
    this.name = "BatchReviewError";
  }
}
