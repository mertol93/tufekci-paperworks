import {
  type ChangeEvent,
  type CSSProperties,
  type DragEvent,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { rovingNavigationIndex } from "./accessibility";
import { takeE2eOpenSelection, takeE2eSaveSelection } from "paperworks-e2e-bridge";
import { PdfPageCanvas, LazyPdfThumbnail } from "./PdfPageCanvas";
import { PdfPasswordDialog } from "./PdfPasswordDialog";
import { pdfOpenErrorTranslationKey, type PdfOpenErrorCode } from "./pdfPassword";
import { AnnotationStudio } from "./AnnotationStudio";
import { ArchiveStudio, type PdfArchiveReadiness } from "./ArchiveStudio";
import { BatchRecipeStudio } from "./BatchRecipeStudio";
import {
  createVerifiedScanBatchSeed,
  type VerifiedScanBatchSeed
} from "./batchRecipes";
import { BookmarkStudio } from "./BookmarkStudio";
import { ComparisonStudio } from "./ComparisonStudio";
import { CompressionStudio } from "./CompressionStudio";
import { ContentEditStudio } from "./ContentEditStudio";
import { FormStudio } from "./FormStudio";
import { ProtectionStudio } from "./ProtectionStudio";
import { PrintStudio } from "./PrintStudio";
import { HealthStudio } from "./HealthStudio";
import { ImportPagesDialog, type ImportedPdfReady } from "./ImportPagesDialog";
import { MergeStudio } from "./MergeStudio";
import { OcrReviewDialog, type OcrConfidenceResult } from "./OcrReviewDialog";
import { describeOcrReadiness, localiseOcrLanguage } from "./ocrLocalisation";
import { SearchableOcrStudio } from "./SearchableOcrStudio";
import { OperationAuditDialog } from "./OperationAuditDialog";
import { OutputProtectionFields } from "./OutputProtectionFields";
import { PageFinishStudio } from "./PageFinishStudio";
import {
  PageTransferDialog,
  type PageTransferPdfSource
} from "./PageTransferDialog";
import { PdfJobProgress } from "./PdfJobProgress";
import {
  localisePdfJobFailure,
  localisePdfJobStage,
  type PdfJobSnapshot
} from "./pdfJobs";
import { describePlannedPage, localiseOrganiseWarnings } from "./organiseLocalisation";
import { PrivacyStudio } from "./PrivacyStudio";
import { RedactionStudio } from "./RedactionStudio";
import {
  recoveryDocumentName,
  type RecoveryMergeSource,
  type RecoverySaveResult,
  type RecoverySplitPlan,
  type RecoverySnapshot
} from "./recovery";
import { SignatureStudio } from "./SignatureStudio";
import {
  describeScannerDiscovery,
  localiseScanPresetDescription,
  localiseScanPresetName,
  localiseScanWarnings,
  type ScannerDiscoveryStatus
} from "./scanLocalisation";
import { VisualSignatureLayer } from "./VisualSignatureLayer";
import { SplitStudio } from "./SplitStudio";
import {
  createOutputProtectionDraft,
  outputProtectionIsValid,
  toPdfOutputProtection,
  type OutputProtectionDraft
} from "./outputProtection";
import {
  createPdfLoadingTask,
  type PDFDocumentProxy,
  type PdfRangeSource,
  type PdfSource
} from "./pdf";
import { useBoundedHistory } from "./useBoundedHistory";
import {
  cloneVisualSignaturePlacements,
  createVisualSignatureId,
  createVisualSignaturePlacement,
  duplicateVisualSignaturePlacement,
  MAX_VISUAL_SIGNATURE_ASSETS,
  MAX_VISUAL_SIGNATURE_PLACEMENTS,
  mergeDetachedVisualSignaturePlacements,
  partitionVisualSignaturePlacements,
  resizeVisualSignaturePlacement,
  rotateVisualSignaturePlacement,
  visualSignatureExportPayload,
  type VisualSignatureAsset,
  type VisualSignaturePlacement
} from "./visualSignatures";
import { type PageRotation, usePagePlan } from "./usePagePlan";
import { usePdfDocument } from "./usePdfDocument";
import { usePdfEditSafety } from "./usePdfEditSafety";
import { usePdfJob } from "./usePdfJob";
import { type PdfSearchErrorCode, usePdfSearch } from "./usePdfSearch";
import {
  isAppleMobileRuntime,
  parseRuntimeCapabilities,
  type RuntimeCapabilities
} from "./runtimeCapabilities";
import {
  canMovePagesByStep,
  movePagesByStep,
  orderedPageSelection,
  reorderPagesAtDrop,
  resolvePageSelection,
  type PageSelectionMode
} from "./pageSelection";
import { UpdateDialog } from "./UpdateDialog";
import { useI18n } from "./I18nProvider";
import {
  SUPPORTED_LOCALES,
  type SupportedLocale,
  type Translate,
  type TranslationKey
} from "./i18n";
import {
  AlertCircle,
  Archive,
  ArrowLeft,
  ArrowRight,
  Bookmark,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Copy,
  Crop,
  Download,
  Eraser,
  FileInput,
  FilePlus2,
  FileSearch,
  FileText,
  Files,
  FolderOpen,
  Gauge,
  GitCompareArrows,
  HeartPulse,
  History,
  Highlighter,
  Info,
  ListChecks,
  Languages,
  Loader2,
  LockKeyhole,
  MousePointer2,
  MoveRight,
  PenLine,
  Plus,
  Printer,
  Redo2,
  RefreshCw,
  RotateCw,
  Save,
  ScanLine,
  Scissors,
  Search,
  ShieldAlert,
  ShieldCheck,
  Shapes,
  Stamp,
  Trash2,
  Type,
  Undo2,
  UploadCloud,
  Wrench,
  X,
  ZoomIn,
  ZoomOut
} from "lucide-react";

type ToolStatus = {
  name: string;
  command: string;
  available: boolean;
  version?: string;
  detail?: string;
};

type AppMode = "desktop" | "browser";

type SelectedDocument = {
  kind: "pdf" | "scan";
  name: string;
  sizeBytes: number;
  fileCount: number;
  previewPages: number;
  sourcePath?: string;
};

type ExportResult = {
  bytesWritten: number;
  outputPath: string;
  pageCount: number;
  warnings: string[];
};

type OperationStatus = {
  action?: "batch-recipe";
  kind: "error" | "info" | "success";
  text: string;
  warnings?: string[];
};

type ScanImage = {
  name: string;
  path?: string;
  url: string;
};

type ScanColourMode = "colour" | "greyscale" | "monochrome";

type ScanExportResult = {
  bytesWritten: number;
  encryption: "AES-256" | "None";
  ocrApplied: boolean;
  ocrHintsApplied: number;
  outputPath: string;
  pageCount: number;
  pagesCropped: number;
  pagesPerspectiveCorrected: number;
  pagesShadowCleaned: number;
  pagesWithoutSearchableText: number[];
  searchableTextPages: number;
  usedImageMagick: boolean;
  warnings: string[];
};

type ScanPreviewResult = {
  bytes: number[] | Uint8Array;
  cropped: boolean;
  height: number;
  mimeType: string;
  pageBoundaryDetected: boolean;
  perspectiveCorrected: boolean;
  shadowRemoved: boolean;
  usedImageMagick: boolean;
  width: number;
};

type ScanPreview = Omit<ScanPreviewResult, "bytes"> & {
  url: string;
};

type ScanJobSnapshot = PdfJobSnapshot<ScanExportResult>;

type ScanPreset = {
  id: string;
  name: string;
  widthMm: number;
  heightMm: number;
  description: string;
};

type ScannerBackend = "image-capture" | "sane" | "wia";
type ScannerSource = "feeder" | "flatbed";

type ScannerDevice = {
  backend: ScannerBackend;
  colourModes: ScanColourMode[];
  detail?: string | null;
  duplex: boolean;
  feeder: boolean;
  flatbed: boolean;
  id: string;
  manufacturer: string;
  model: string;
  name: string;
  supportedDpi: number[];
};

type ScannerDiscovery = {
  available: boolean;
  backend: ScannerBackend;
  backendName: string;
  detail: string;
  devices: ScannerDevice[];
  status: ScannerDiscoveryStatus;
};

type ScannerCaptureResult = {
  captureId: string;
  pageCount: number;
  paths: string[];
  warnings: string[];
};

type OcrLanguage = {
  code: string;
  name: string;
};

type OcrEngineStatus = {
  available: boolean;
  command: string;
  detail?: string | null;
  name: string;
  version?: string | null;
};

type OcrReadinessReport = {
  detail: string;
  languageAvailable: boolean;
  languages: OcrLanguage[];
  ocrMyPdf: OcrEngineStatus;
  ready: boolean;
  selectedLanguage: string;
  tesseract: OcrEngineStatus;
};

type ImportedPdfSource = {
  certificateAcknowledged: boolean;
  certificateSignature: boolean;
  document: PDFDocumentProxy;
  id: string;
  loadingTask: ImportedPdfReady["loadingTask"];
  modifiedAtMs: number | null;
  name: string;
  password: string | null;
  path: string;
  size: number;
};

const fallbackScanPresets: ScanPreset[] = [
  {
    id: "a4",
    name: "A4",
    widthMm: 210,
    heightMm: 297,
    description: "Standard UK document page"
  },
  {
    id: "letter",
    name: "US Letter",
    widthMm: 216,
    heightMm: 279,
    description: "Common North American document page"
  },
  {
    id: "business-card",
    name: "Business card",
    widthMm: 85,
    heightMm: 55,
    description: "Compact card layout"
  },
  {
    id: "id-card",
    name: "ID card",
    widthMm: 85.6,
    heightMm: 54,
    description: "Credit-card sized identity document"
  },
  {
    id: "driving-licence",
    name: "Driving licence",
    widthMm: 85.6,
    heightMm: 54,
    description: "UK photocard driving licence size"
  }
];

const fallbackOcrLanguages: OcrLanguage[] = [{ code: "eng", name: "English" }];

const supportedImageExtensions = [
  ".avif",
  ".bmp",
  ".gif",
  ".heic",
  ".heif",
  ".jpeg",
  ".jpg",
  ".pbm",
  ".pgm",
  ".png",
  ".pnm",
  ".ppm",
  ".tif",
  ".tiff",
  ".webp"
];

const desktopInputExtensions = [
  "pdf",
  ...supportedImageExtensions.map((extension) => extension.slice(1))
];

const workflowDefinitions: Array<{
  descriptionKey: TranslationKey;
  icon: typeof Files;
  id: string;
  stageKey: TranslationKey;
  titleKey: TranslationKey;
}> = [
  {
    id: "organise",
    titleKey: "workflow.organise.title",
    descriptionKey: "workflow.organise.description",
    icon: Files,
    stageKey: "workflow.organise.stage"
  },
  {
    id: "content",
    titleKey: "workflow.content.title",
    descriptionKey: "workflow.content.description",
    icon: FileText,
    stageKey: "workflow.content.stage"
  },
  {
    id: "scan",
    titleKey: "workflow.scan.title",
    descriptionKey: "workflow.scan.description",
    icon: UploadCloud,
    stageKey: "workflow.scan.stage"
  },
  {
    id: "merge",
    titleKey: "workflow.merge.title",
    descriptionKey: "workflow.merge.description",
    icon: FilePlus2,
    stageKey: "workflow.merge.stage"
  },
  {
    id: "split",
    titleKey: "workflow.split.title",
    descriptionKey: "workflow.split.description",
    icon: Scissors,
    stageKey: "workflow.split.stage"
  },
  {
    id: "ocr",
    titleKey: "workflow.ocr.title",
    descriptionKey: "workflow.ocr.description",
    icon: Search,
    stageKey: "workflow.ocr.stage"
  },
  {
    id: "sign",
    titleKey: "workflow.sign.title",
    descriptionKey: "workflow.sign.description",
    icon: PenLine,
    stageKey: "workflow.sign.stage"
  },
  {
    id: "annotate",
    titleKey: "workflow.annotate.title",
    descriptionKey: "workflow.annotate.description",
    icon: Shapes,
    stageKey: "workflow.annotate.stage"
  },
  {
    id: "redact",
    titleKey: "workflow.redact.title",
    descriptionKey: "workflow.redact.description",
    icon: Highlighter,
    stageKey: "workflow.redact.stage"
  },
  {
    id: "forms",
    titleKey: "workflow.forms.title",
    descriptionKey: "workflow.forms.description",
    icon: FileInput,
    stageKey: "workflow.forms.stage"
  },
  {
    id: "finish",
    titleKey: "workflow.finish.title",
    descriptionKey: "workflow.finish.description",
    icon: Crop,
    stageKey: "workflow.finish.stage"
  },
  {
    id: "health",
    titleKey: "workflow.health.title",
    descriptionKey: "workflow.health.description",
    icon: HeartPulse,
    stageKey: "workflow.health.stage"
  },
  {
    id: "archive",
    titleKey: "workflow.archive.title",
    descriptionKey: "workflow.archive.description",
    icon: Archive,
    stageKey: "workflow.archive.stage"
  },
  {
    id: "privacy",
    titleKey: "workflow.privacy.title",
    descriptionKey: "workflow.privacy.description",
    icon: Eraser,
    stageKey: "workflow.privacy.stage"
  },
  {
    id: "compress",
    titleKey: "workflow.compress.title",
    descriptionKey: "workflow.compress.description",
    icon: Gauge,
    stageKey: "workflow.compress.stage"
  },
  {
    id: "batch",
    titleKey: "workflow.batch.title",
    descriptionKey: "workflow.batch.description",
    icon: ListChecks,
    stageKey: "workflow.batch.stage"
  },
  {
    id: "compare",
    titleKey: "workflow.compare.title",
    descriptionKey: "workflow.compare.description",
    icon: GitCompareArrows,
    stageKey: "workflow.compare.stage"
  },
  {
    id: "bookmarks",
    titleKey: "workflow.bookmarks.title",
    descriptionKey: "workflow.bookmarks.description",
    icon: Bookmark,
    stageKey: "workflow.bookmarks.stage"
  },
  {
    id: "print",
    titleKey: "workflow.print.title",
    descriptionKey: "workflow.print.description",
    icon: Printer,
    stageKey: "workflow.print.stage"
  },
  {
    id: "protect",
    titleKey: "workflow.protect.title",
    descriptionKey: "workflow.protect.description",
    icon: ShieldCheck,
    stageKey: "workflow.protect.stage"
  }
];

const editorToolDefinitions = [
  { id: "select", labelKey: "editor.select.label", icon: MousePointer2, implemented: true },
  { id: "text", labelKey: "editor.text.label", icon: Type, implemented: true },
  {
    id: "highlight",
    labelKey: "editor.highlight.label",
    icon: Highlighter,
    implemented: true
  },
  { id: "stamp", labelKey: "editor.stamp.label", icon: Stamp, implemented: true },
  { id: "rotate", labelKey: "editor.rotate.label", icon: RotateCw, implemented: false },
  { id: "delete", labelKey: "editor.delete.label", icon: Trash2, implemented: false }
] satisfies Array<{
  icon: typeof Files;
  id: string;
  implemented: boolean;
  labelKey: TranslationKey;
}>;

const localeLabelKeys: Record<SupportedLocale, TranslationKey> = {
  "de-DE": "locale.de-DE",
  "en-GB": "locale.en-GB",
  "en-US": "locale.en-US",
  "tr-TR": "locale.tr-TR"
};

const checklistKeys: TranslationKey[] = [
  "app.flow.open",
  "app.flow.pick",
  "app.flow.preview",
  "app.flow.export"
];

export function App() {
  const { formatDate, formatList, formatNumber, locale, setLocale, t } = useI18n();
  const workflows = useMemo(
    () =>
      workflowDefinitions.map((workflow) => ({
        ...workflow,
        description: t(workflow.descriptionKey),
        stage: t(workflow.stageKey),
        title: t(workflow.titleKey)
      })),
    [t]
  );
  const editorTools = useMemo(
    () =>
      editorToolDefinitions.map((tool) => ({
        ...tool,
        label: t(tool.labelKey)
      })),
    [t]
  );
  const checklist = useMemo(() => checklistKeys.map((key) => t(key)), [t]);
  const [tools, setTools] = useState<ToolStatus[]>([]);
  const [archiveReadiness, setArchiveReadiness] = useState<PdfArchiveReadiness | null>(null);
  const [archiveReadinessBusy, setArchiveReadinessBusy] = useState(false);
  const [scanPresets, setScanPresets] = useState<ScanPreset[]>(fallbackScanPresets);
  const [ocrLanguages, setOcrLanguages] = useState<OcrLanguage[]>(fallbackOcrLanguages);
  const [mode, setMode] = useState<AppMode>("browser");
  const [runtimeCapabilities, setRuntimeCapabilities] =
    useState<RuntimeCapabilities | null>(null);
  const [activeDocument, setActiveDocument] = useState<SelectedDocument | null>(null);
  const [pdfSource, setPdfSource] = useState<PdfSource | null>(null);
  const [documentReadError, setDocumentReadError] = useState<PdfOpenErrorCode | null>(null);
  const [importPagesOpen, setImportPagesOpen] = useState(false);
  const [pageTransferOpen, setPageTransferOpen] = useState(false);
  const [operationAuditOpen, setOperationAuditOpen] = useState(false);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [importedPdfSources, setImportedPdfSources] = useState<ImportedPdfSource[]>([]);
  const [certificateRewriteAcknowledged, setCertificateRewriteAcknowledged] = useState(false);
  const [scanImages, setScanImages] = useState<ScanImage[]>([]);
  const [scanFromConnectedScanner, setScanFromConnectedScanner] = useState(false);
  const [scanBatchHandoff, setScanBatchHandoff] = useState<VerifiedScanBatchSeed | null>(null);
  const [signatureAssets, setSignatureAssets] = useState<VisualSignatureAsset[]>([]);
  const [selectedSignatureAssetId, setSelectedSignatureAssetId] = useState<string | null>(null);
  const [selectedSignaturePlacementId, setSelectedSignaturePlacementId] = useState<string | null>(
    null
  );
  const signaturePlacementHistory = useBoundedHistory<VisualSignaturePlacement[]>(
    [],
    cloneVisualSignaturePlacements,
    100
  );
  const [documentLocked, setDocumentLocked] = useState(false);
  const [documentLockOpenPassword, setDocumentLockOpenPassword] = useState("");
  const [documentLockOpenPasswordConfirmation, setDocumentLockOpenPasswordConfirmation] =
    useState("");
  const [documentLockOwnerPassword, setDocumentLockOwnerPassword] = useState("");
  const [documentLockOwnerPasswordConfirmation, setDocumentLockOwnerPasswordConfirmation] =
    useState("");
  const [activeWorkflowId, setActiveWorkflowId] = useState(workflowDefinitions[0].id);
  const [activeToolId, setActiveToolId] = useState(editorToolDefinitions[0].id);
  const [selectedPaperId, setSelectedPaperId] = useState(fallbackScanPresets[0].id);
  const [recogniseText, setRecogniseText] = useState(false);
  const [straightenScan, setStraightenScan] = useState(true);
  const [scanColourMode, setScanColourMode] = useState<ScanColourMode>("colour");
  const [scanDpi, setScanDpi] = useState(300);
  const [scanMarginPt, setScanMarginPt] = useState(18);
  const [scanJpegQuality, setScanJpegQuality] = useState(88);
  const [scanAutoCrop, setScanAutoCrop] = useState(true);
  const [scanCorrectPerspective, setScanCorrectPerspective] = useState(true);
  const [scanRemoveShadows, setScanRemoveShadows] = useState(false);
  const [scanPreview, setScanPreview] = useState<ScanPreview | null>(null);
  const [scanPreviewStarting, setScanPreviewStarting] = useState(false);
  const [scanPreviewCancelBusy, setScanPreviewCancelBusy] = useState(false);
  const [scanPreviewError, setScanPreviewError] = useState<string | null>(null);
  const [scanPreviewRetryToken, setScanPreviewRetryToken] = useState(0);
  const [selectedOcrLanguage, setSelectedOcrLanguage] = useState("eng");
  const [ocrReadiness, setOcrReadiness] = useState<OcrReadinessReport | null>(null);
  const [ocrReadinessBusy, setOcrReadinessBusy] = useState(false);
  const [ocrReadinessError, setOcrReadinessError] = useState<string | null>(null);
  const [ocrReviewOpen, setOcrReviewOpen] = useState(false);
  const [ocrReviewBusy, setOcrReviewBusy] = useState(false);
  const [ocrReviewCancelBusy, setOcrReviewCancelBusy] = useState(false);
  const [ocrReviewError, setOcrReviewError] = useState<string | null>(null);
  const [ocrReviewNotice, setOcrReviewNotice] = useState<string | null>(null);
  const [ocrReviewResult, setOcrReviewResult] = useState<OcrConfidenceResult | null>(null);
  const [ocrWordHints, setOcrWordHints] = useState<string[]>([]);
  const [scanOutputProtection, setScanOutputProtection] = useState<OutputProtectionDraft>(() =>
    createOutputProtectionDraft()
  );
  const [scanExportStarting, setScanExportStarting] = useState(false);
  const [scannerDiscovery, setScannerDiscovery] = useState<ScannerDiscovery | null>(null);
  const [scannerDiscoveryBusy, setScannerDiscoveryBusy] = useState(false);
  const [scannerCaptureStarting, setScannerCaptureStarting] = useState(false);
  const [scannerCaptureImportBusy, setScannerCaptureImportBusy] = useState(false);
  const [scannerCaptureCancelBusy, setScannerCaptureCancelBusy] = useState(false);
  const [scannerCaptureImportError, setScannerCaptureImportError] = useState<string | null>(null);
  const [scannerCaptureImportRetryToken, setScannerCaptureImportRetryToken] = useState(0);
  const [selectedScannerId, setSelectedScannerId] = useState("");
  const [scannerSource, setScannerSource] = useState<ScannerSource>("flatbed");
  const [scannerDuplex, setScannerDuplex] = useState(false);
  const [scannerPageLimit, setScannerPageLimit] = useState(25);
  const [scanCancelBusy, setScanCancelBusy] = useState(false);
  const [selectedPage, setSelectedPage] = useState(1);
  const [selectedPageIds, setSelectedPageIds] = useState<string[]>([]);
  const [selectionAnchorId, setSelectionAnchorId] = useState<string | null>(null);
  const [zoom, setZoom] = useState(92);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResultIndex, setSearchResultIndex] = useState(0);
  const [dragActive, setDragActive] = useState(false);
  const [draggedPageId, setDraggedPageId] = useState<string | null>(null);
  const [dropTargetPageId, setDropTargetPageId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [backendUnavailable, setBackendUnavailable] = useState(false);
  const [openingLocalFiles, setOpeningLocalFiles] = useState(false);
  const [exportDialogBusy, setExportDialogBusy] = useState(false);
  const [exportCancelBusy, setExportCancelBusy] = useState(false);
  const [operationStatus, setOperationStatus] = useState<OperationStatus | null>(null);
  const scanExportTerminalPresentedJobIdRef = useRef<string | null>(null);
  const scannerDiscoveryRequestedRef = useRef(false);
  const scannerCaptureDeviceNameRef = useRef<string | null>(null);
  const scannerCaptureTerminalPresentedJobIdRef = useRef<string | null>(null);
  const scannerCaptureProcessedJobIdRef = useRef<string | null>(null);
  const scannerCaptureImportRef = useRef<{
    jobId: string;
    promise: Promise<File[]>;
  } | null>(null);
  const scanPreviewScheduledConfigurationRef = useRef<string | null>(null);
  const scanPreviewStartingConfigurationRef = useRef<string | null>(null);
  const scanPreviewFailedConfigurationRef = useRef<string | null>(null);
  const scanPreviewCurrentConfigurationRef = useRef<string | null>(null);
  const scanPreviewJobConfigurationRef = useRef<{
    configuration: string;
    jobId: string;
  } | null>(null);
  const scanPreviewCancellingJobIdRef = useRef<string | null>(null);
  const scanPreviewPresentedJobIdRef = useRef<string | null>(null);
  const ocrReviewConfigurationRef = useRef<string | null>(null);
  const [recoveryCandidate, setRecoveryCandidate] = useState<RecoverySnapshot | null>(null);
  const [pendingPdfRecovery, setPendingPdfRecovery] = useState<RecoverySnapshot | null>(null);
  const [mergeRecoverySources, setMergeRecoverySources] = useState<RecoveryMergeSource[]>([]);
  const [splitRecoveryPlan, setSplitRecoveryPlan] = useState<RecoverySplitPlan | null>(null);
  const [recoveryChecked, setRecoveryChecked] = useState(false);
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const [recoverySaveState, setRecoverySaveState] = useState<
    "error" | "idle" | "saved" | "saving"
  >("idle");
  const [recoveryLastSavedAt, setRecoveryLastSavedAt] = useState<number | null>(null);
  const fileLoadSequence = useRef(0);
  const importedSourceCounter = useRef(0);
  const importedPdfSourcesRef = useRef<ImportedPdfSource[]>([]);
  const detachedSignaturePlacementsRef = useRef<VisualSignaturePlacement[]>([]);
  const lockedOrganiseJobIdsRef = useRef(new Set<string>());
  const pagePlanFocusIdRef = useRef<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const workflowButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const pdf = usePdfDocument(pdfSource);
  const nativeMode = mode === "desktop";
  const mobileMode = Boolean(nativeMode && runtimeCapabilities?.mobile);
  const appleMobileMode = nativeMode && isAppleMobileRuntime(runtimeCapabilities);
  const runtimeSupportsOcr = Boolean(nativeMode && runtimeCapabilities?.searchableOcr);
  const runtimeSupportsExternalProcesses = Boolean(
    nativeMode && runtimeCapabilities?.externalProcesses
  );
  const runtimeSupportsCertificateSigning = Boolean(
    nativeMode && runtimeCapabilities?.certificateSigning
  );
  const runtimeSupportsArchivalPdf = Boolean(nativeMode && runtimeCapabilities?.archivalPdf);
  const connectedScanningAvailable = Boolean(
    nativeMode && runtimeCapabilities?.connectedScanning
  );
  const activeEditSafetySources = useMemo(
    () =>
      mode === "desktop" &&
      activeDocument?.kind === "pdf" &&
      activeDocument.sourcePath &&
      pdf.document
        ? [
            {
              id: "active-workspace",
              label: activeDocument.name,
              password: pdf.openingPassword ?? undefined,
              path: activeDocument.sourcePath
            }
          ]
        : [],
    [activeDocument, mode, pdf.document, pdf.openingPassword]
  );
  const activeEditSafety = usePdfEditSafety(
    mode === "desktop",
    activeEditSafetySources,
    "organiser"
  );
  const organiseJob = usePdfJob<ExportResult>(mode === "desktop", "organise");
  const scanExportJob = usePdfJob<ScanExportResult>(mode === "desktop", "scan");
  const scanJob = scanExportJob.job;
  const scanBusy = scanExportStarting || scanExportJob.isActive;
  const ocrReviewJob = usePdfJob<OcrConfidenceResult>(runtimeSupportsOcr, "ocr-review");
  const scanPreviewJob = usePdfJob<ScanPreviewResult>(
    mode === "desktop",
    "scan-preview"
  );
  const scannerCaptureJob = usePdfJob<ScannerCaptureResult>(
    connectedScanningAvailable,
    "scanner-capture"
  );
  const scannerCaptureAwaitingImport = Boolean(
    scannerCaptureJob.job?.status === "succeeded" &&
      scannerCaptureProcessedJobIdRef.current !== scannerCaptureJob.job.jobId
  );
  const scannerCaptureBusy =
    scannerCaptureStarting ||
    scannerCaptureImportBusy ||
    scannerCaptureJob.isActive ||
    scannerCaptureAwaitingImport;
  const activePdfRangeSource = pdfSource && "path" in pdfSource ? pdfSource : null;
  const exportBusy = exportDialogBusy || organiseJob.isActive;
  const pagePlan = usePagePlan(
    pdf.document?.fingerprints[0] ?? null,
    pdf.document?.numPages ?? 0
  );
  const importedPdfSourceMap = useMemo(
    () => new Map(importedPdfSources.map((source) => [source.id, source])),
    [importedPdfSources]
  );
  const usedImportedSourceIds = useMemo(
    () =>
      new Set(
        pagePlan.pages.flatMap((page) =>
          page.kind === "source" && page.sourceId !== "primary" ? [page.sourceId] : []
        )
      ),
    [pagePlan.pages]
  );
  const updateMergeRecoverySources = useCallback(
    (sources: RecoveryMergeSource[]) => setMergeRecoverySources(sources),
    []
  );
  const updateSplitRecoveryPlan = useCallback(
    (plan: RecoverySplitPlan | null) => setSplitRecoveryPlan(plan),
    []
  );
  const refreshScanners = useCallback(async () => {
    if (!connectedScanningAvailable || scannerDiscoveryBusy || scannerCaptureBusy) {
      return;
    }
    setScannerDiscoveryBusy(true);
    try {
      const discovery = await invoke<ScannerDiscovery>("list_scanners");
      setScannerDiscovery(discovery);
      setSelectedScannerId((current) =>
        discovery.devices.some((device) => device.id === current)
          ? current
          : (discovery.devices[0]?.id ?? "")
      );
    } catch (reason) {
      void reason;
      setScannerDiscovery(null);
      setOperationStatus({
        kind: "error",
        text: t("scanner.discovery.error")
      });
    } finally {
      setScannerDiscoveryBusy(false);
    }
  }, [connectedScanningAvailable, scannerCaptureBusy, scannerDiscoveryBusy, t]);
  const plannedSearchPages = useMemo(
    () =>
      pagePlan.pages.map((plannedPage) => {
        if (plannedPage.kind === "blank") {
          return { document: null, sourcePage: 0 };
        }
        return {
          document:
            plannedPage.sourceId === "primary"
              ? pdf.document
              : (importedPdfSourceMap.get(plannedPage.sourceId)?.document ?? null),
          sourcePage: plannedPage.sourcePage
        };
      }),
    [importedPdfSourceMap, pagePlan.pages, pdf.document]
  );
  const printableWorkspacePages = useMemo(
    () =>
      pagePlan.pages.map((plannedPage) =>
        plannedPage.kind === "blank"
          ? plannedPage
          : {
              document:
                plannedPage.sourceId === "primary"
                  ? pdf.document
                  : (importedPdfSourceMap.get(plannedPage.sourceId)?.document ?? null),
              id: plannedPage.id,
              kind: plannedPage.kind,
              rotation: plannedPage.rotation,
              sourcePage: plannedPage.sourcePage
            }
      ),
    [importedPdfSourceMap, pagePlan.pages, pdf.document]
  );
  const visiblePdfSearch = usePdfSearch(plannedSearchPages, searchQuery, locale);

  const clearImportedPdfSources = useCallback(() => {
    const sources = importedPdfSourcesRef.current;
    importedPdfSourcesRef.current = [];
    setImportedPdfSources([]);
    sources.forEach((source) => {
      void source.loadingTask.destroy();
    });
  }, []);

  useEffect(() => {
    importedPdfSourcesRef.current = importedPdfSources;
  }, [importedPdfSources]);

  useEffect(() => {
    const openPrintWorkflow = (event: globalThis.KeyboardEvent) => {
      if (
        !pdf.document ||
        event.defaultPrevented ||
        event.altKey ||
        !(event.ctrlKey || event.metaKey) ||
        event.key.toLocaleLowerCase("en-GB") !== "p" ||
        document.querySelector('[role="dialog"]')
      ) {
        return;
      }
      event.preventDefault();
      setActiveWorkflowId("print");
      window.requestAnimationFrame(() => workflowButtonRefs.current.get("print")?.focus());
    };
    window.addEventListener("keydown", openPrintWorkflow);
    return () => window.removeEventListener("keydown", openPrintWorkflow);
  }, [pdf.document]);

  useEffect(
    () => () => {
      importedPdfSourcesRef.current.forEach((source) => {
        void source.loadingTask.destroy();
      });
      importedPdfSourcesRef.current = [];
    },
    []
  );

  useEffect(() => {
    let alive = true;

    const boot = async () => {
      try {
        const runtime = parseRuntimeCapabilities(
          await invoke<unknown>("runtime_capabilities")
        );
        if (!alive) {
          return;
        }
        setRuntimeCapabilities(runtime);
        setBackendUnavailable(false);
        setMode("desktop");

        if (runtime.mobile) {
          setTools([]);
          setArchiveReadiness(null);
          return;
        }

        const [toolResult, archivalResult] = await Promise.all([
          invoke<ToolStatus[]>("probe_tools"),
          invoke<PdfArchiveReadiness>("pdf_archive_readiness").catch(() => null)
        ]);
        if (!alive) {
          return;
        }
        setTools(toolResult);
        setArchiveReadiness(archivalResult);
        if (!archivalResult) {
          setOperationStatus({
            kind: "error",
            text: t("app.archive.checkFailed")
          });
        }
      } catch {
        if (!alive) {
          return;
        }
        setRuntimeCapabilities(null);
        setBackendUnavailable(true);
        setMode("browser");
      } finally {
        if (alive) {
          setLoading(false);
        }
      }
    };
    void boot();

    return () => {
      alive = false;
    };
  }, [t]);

  const refreshArchiveReadiness = useCallback(async () => {
    if (!runtimeSupportsArchivalPdf || archiveReadinessBusy) {
      return;
    }
    setArchiveReadinessBusy(true);
    try {
      const result = await invoke<PdfArchiveReadiness>("pdf_archive_readiness");
      setArchiveReadiness(result);
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: t("app.archive.refreshFailed")
      });
    } finally {
      setArchiveReadinessBusy(false);
    }
  }, [archiveReadinessBusy, runtimeSupportsArchivalPdf, t]);

  useEffect(() => {
    if (mode !== "desktop" || recoveryChecked) {
      return;
    }
    let alive = true;
    invoke<RecoverySnapshot | null>("load_recovery_snapshot")
      .then((snapshot) => {
        if (alive && snapshot) {
          setRecoveryCandidate(snapshot);
        }
      })
      .catch(() => {
        if (alive) {
          setRecoverySaveState("error");
          setOperationStatus({
            kind: "error",
            text: t("app.recovery.error.unavailable")
          });
        }
      })
      .finally(() => {
        if (alive) {
          setRecoveryChecked(true);
        }
      });
    return () => {
      alive = false;
    };
  }, [mode, recoveryChecked, t]);

  useEffect(() => {
    const job = scanExportJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    if (scanExportTerminalPresentedJobIdRef.current === job.jobId) {
      return;
    }
    scanExportTerminalPresentedJobIdRef.current = job.jobId;
    completeScanJob(job);
  }, [scanExportJob.job?.jobId, scanExportJob.job?.status]);

  useEffect(() => {
    const job = organiseJob.job;
    if (!job || job.status === "queued" || job.status === "running") {
      return;
    }
    setExportCancelBusy(false);
    const exportedWithLock = lockedOrganiseJobIdsRef.current.delete(job.jobId);
    if (job.status === "succeeded" && job.result) {
      const pageCount = job.result.pageCount;
      setOperationStatus({
        kind: "success",
        text: t(
          pageCount === 1
            ? "organise.export.success.one"
            : "organise.export.success.other",
          {
            count: formatNumber(pageCount),
            name: fileNameFromPath(job.result.outputPath),
            size: formatFileSize(job.result.bytesWritten, formatNumber)
          }
        ),
        warnings: localiseOrganiseWarnings(job.result.warnings, t)
      });
      if (exportedWithLock) {
        setDocumentLocked(false);
      }
    } else if (job.status === "cancelled") {
      setOperationStatus({
        kind: "info",
        text: t("organise.export.cancelled")
      });
    } else if (job.status === "failed") {
      setOperationStatus({
        kind: "error",
        text: localisePdfJobFailure(job, t)
      });
    } else {
      setOperationStatus({
        kind: "error",
        text: t("organise.export.unverified")
      });
    }
  }, [formatNumber, organiseJob.job?.jobId, organiseJob.job?.status, t]);

  useEffect(() => {
    if (!runtimeSupportsOcr) {
      setOcrReadiness(null);
      setOcrReadinessBusy(false);
      setOcrReadinessError(
        appleMobileMode ? t("ocr.engine.mobileUnavailable") : null
      );
      setOcrLanguages(fallbackOcrLanguages);
      setRecogniseText(false);
      return;
    }
    let alive = true;
    setOcrReadinessBusy(true);
    setOcrReadinessError(null);
    invoke<OcrReadinessReport>("ocr_readiness", {
      request: { language: selectedOcrLanguage }
    })
      .then((result) => {
        if (!alive) {
          return;
        }
        setOcrReadiness(result);
        if (result.languages.length > 0) {
          setOcrLanguages(result.languages);
          setSelectedOcrLanguage((current) =>
            result.languages.some((language) => language.code === current)
              ? current
              : result.languages[0].code
          );
        } else {
          setOcrLanguages([]);
        }
      })
      .catch((reason) => {
        if (alive) {
          void reason;
          setOcrReadiness(null);
          setOcrLanguages([]);
          setOcrReadinessError(t("ocr.engine.checkFailed"));
        }
      })
      .finally(() => {
        if (alive) {
          setOcrReadinessBusy(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [appleMobileMode, runtimeSupportsOcr, selectedOcrLanguage, t]);

  useEffect(() => {
    return () => {
      scanImages.forEach((image) => URL.revokeObjectURL(image.url));
    };
  }, [scanImages]);

  useEffect(() => {
    if (
      selectedSignatureAssetId &&
      !signatureAssets.some((asset) => asset.id === selectedSignatureAssetId)
    ) {
      setSelectedSignatureAssetId(signatureAssets[0]?.id ?? null);
    }
  }, [selectedSignatureAssetId, signatureAssets]);

  useEffect(() => {
    const pageIds = new Set(pagePlan.pages.map((page) => page.id));
    const placements = signaturePlacementHistory.present;
    const partitioned = partitionVisualSignaturePlacements(placements, pageIds);
    if (partitioned.detached.length > 0) {
      detachedSignaturePlacementsRef.current = mergeDetachedVisualSignaturePlacements(
        detachedSignaturePlacementsRef.current,
        partitioned.detached
      );
      signaturePlacementHistory.replace(() => partitioned.attached);
      return;
    }

    const assetIds = new Set(signatureAssets.map((asset) => asset.id));
    const attachedIds = new Set(placements.map((placement) => placement.id));
    const restored = detachedSignaturePlacementsRef.current.filter(
      (placement) =>
        pageIds.has(placement.pageId) &&
        assetIds.has(placement.assetId) &&
        !attachedIds.has(placement.id)
    );
    detachedSignaturePlacementsRef.current = detachedSignaturePlacementsRef.current.filter(
      (placement) => !pageIds.has(placement.pageId) && assetIds.has(placement.assetId)
    );
    if (restored.length > 0) {
      signaturePlacementHistory.replace((current) =>
        [...current, ...restored].slice(-MAX_VISUAL_SIGNATURE_PLACEMENTS)
      );
    }
  }, [
    pagePlan.pages,
    signatureAssets,
    signaturePlacementHistory.present,
    signaturePlacementHistory.replace
  ]);

  useEffect(() => {
    if (
      selectedSignaturePlacementId &&
      !signaturePlacementHistory.present.some(
        (placement) => placement.id === selectedSignaturePlacementId
      )
    ) {
      setSelectedSignaturePlacementId(null);
    }
    if (signaturePlacementHistory.present.length === 0) {
      setDocumentLocked(false);
    }
  }, [selectedSignaturePlacementId, signaturePlacementHistory.present]);

  useEffect(() => {
    if (!documentLocked) {
      setDocumentLockOpenPassword("");
      setDocumentLockOpenPasswordConfirmation("");
      setDocumentLockOwnerPassword("");
      setDocumentLockOwnerPasswordConfirmation("");
    }
  }, [documentLocked]);

  useEffect(() => {
    if (!pdf.document) {
      return;
    }

    const pageCount = pagePlan.pages.length || pdf.document.numPages;
    setActiveDocument((current) =>
      current?.kind === "pdf" ? { ...current, previewPages: pageCount } : current
    );
    setSelectedPage((current) => Math.min(Math.max(1, current), pageCount));
  }, [pagePlan.pages.length, pdf.document]);

  useEffect(() => {
    const orderedIds = pagePlan.pages.map((page) => page.id);
    const requestedFocusIndex = pagePlanFocusIdRef.current
      ? pagePlan.pages.findIndex((page) => page.id === pagePlanFocusIdRef.current)
      : -1;
    const activeIndex = requestedFocusIndex >= 0 ? requestedFocusIndex : selectedPage - 1;
    const activeId = pagePlan.pages[activeIndex]?.id ?? null;
    pagePlanFocusIdRef.current = null;
    if (requestedFocusIndex >= 0 && requestedFocusIndex + 1 !== selectedPage) {
      setSelectedPage(requestedFocusIndex + 1);
    }
    setSelectedPageIds((current) => {
      const valid = orderedPageSelection(orderedIds, current);
      const next = activeId && valid.includes(activeId) ? valid : activeId ? [activeId] : [];
      return sameStringArray(current, next) ? current : next;
    });
    setSelectionAnchorId((current) =>
      current && orderedIds.includes(current) && (!activeId || selectedPageIds.includes(activeId))
        ? current
        : activeId
    );
  }, [pagePlan.pages, selectedPage]);

  useEffect(() => {
    if (!pendingPdfRecovery || pendingPdfRecovery.document.kind !== "pdf" || !pdf.document) {
      return;
    }
    const recoveredPages = pendingPdfRecovery.document.pages;
    const invalidSourcePage = recoveredPages.some((page) => {
      if (page.kind !== "source") {
        return false;
      }
      const sourceDocument =
        page.sourceId === "primary"
          ? pdf.document
          : importedPdfSourceMap.get(page.sourceId)?.document;
      return !sourceDocument || page.sourcePage > sourceDocument.numPages;
    });
    if (invalidSourcePage) {
      setPendingPdfRecovery(null);
      clearImportedPdfSources();
      setOperationStatus({
        kind: "error",
        text: t("app.recovery.error.pageUnavailable")
      });
      void invoke("clear_recovery_snapshots");
      return;
    }

    pagePlan.restore(recoveredPages);
    setSelectedPage(Math.min(pendingPdfRecovery.selectedPage, recoveredPages.length));
    setZoom(pendingPdfRecovery.zoom);
    setActiveWorkflowId(
      workflows.some((workflow) => workflow.id === pendingPdfRecovery.activeWorkflowId)
        ? pendingPdfRecovery.activeWorkflowId
        : "organise"
    );
    setPendingPdfRecovery(null);
    const importedSourceCount = pendingPdfRecovery.document.importedSources.length;
    const recoveryKey =
      recoveredPages.length === 1
        ? importedSourceCount === 1
          ? "app.recovery.pdf.oneOne"
          : "app.recovery.pdf.oneOther"
        : importedSourceCount === 1
          ? "app.recovery.pdf.otherOne"
          : "app.recovery.pdf.otherOther";
    setOperationStatus({
      kind: "success",
      text: t(recoveryKey, {
        name: pendingPdfRecovery.document.name,
        pages: formatNumber(recoveredPages.length),
        sources: formatNumber(importedSourceCount)
      })
    });
  }, [
    clearImportedPdfSources,
    importedPdfSourceMap,
    pagePlan.restore,
    pdf.document,
    pendingPdfRecovery,
    formatNumber,
    t
  ]);

  useEffect(() => {
    const recoveryError = documentReadError ?? pdf.error;
    if (!pendingPdfRecovery || !recoveryError) {
      return;
    }
    setPendingPdfRecovery(null);
    clearImportedPdfSources();
    setOperationStatus({
      kind: "error",
      text: t("app.recovery.error.pdf")
    });
  }, [clearImportedPdfSources, documentReadError, pdf.error, pendingPdfRecovery, t]);

  useEffect(() => {
    setSearchResultIndex((current) =>
      visiblePdfSearch.matches.length === 0
        ? 0
        : Math.min(current, visiblePdfSearch.matches.length - 1)
    );
  }, [visiblePdfSearch.matches.length]);

  useEffect(() => {
    let alive = true;

    invoke<ScanPreset[]>("scan_presets")
      .then((result) => {
        if (alive && result.length > 0) {
          setScanPresets(result);
        }
      })
      .catch(() => {
        setScanPresets(fallbackScanPresets);
      });

    return () => {
      alive = false;
    };
  }, []);

  const activeWorkflow = useMemo(
    () => workflows.find((workflow) => workflow.id === activeWorkflowId) ?? workflows[0],
    [activeWorkflowId, workflows]
  );
  const handleWorkflowKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    workflowId: string
  ) => {
    const currentIndex = workflows.findIndex((workflow) => workflow.id === workflowId);
    const nextIndex = rovingNavigationIndex(currentIndex, workflows.length, event.key);
    if (nextIndex === null) {
      return;
    }

    event.preventDefault();
    const nextWorkflow = workflows[nextIndex];
    setActiveWorkflowId(nextWorkflow.id);
    workflowButtonRefs.current.get(nextWorkflow.id)?.focus();
  };
  const selectedPaper =
    scanPresets.find((preset) => preset.id === selectedPaperId) ?? scanPresets[0];
  const activeDocumentSize = activeDocument
    ? formatFileSize(activeDocument.sizeBytes, formatNumber)
    : "";
  const isScanDocument = activeDocument?.kind === "scan";
  const isPdfDocument = activeDocument?.kind === "pdf";
  const pdfDocument = pdf.document;
  const scanWorkflowActive = activeWorkflow.id === "scan";
  const selectedScanImage = isScanDocument ? scanImages[selectedPage - 1] : undefined;
  const displayWorkflow =
    scanWorkflowActive
      ? workflows.find((workflow) => workflow.id === "scan") ?? activeWorkflow
      : activeWorkflow;
  const signatureWorkflowActive = displayWorkflow.id === "sign";
  const protectionWorkflowActive = displayWorkflow.id === "protect";
  const organiseWorkflowActive = displayWorkflow.id === "organise";
  const contentWorkflowActive = displayWorkflow.id === "content";
  const mergeWorkflowActive = displayWorkflow.id === "merge";
  const splitWorkflowActive = displayWorkflow.id === "split";
  const ocrWorkflowActive = displayWorkflow.id === "ocr";
  const healthWorkflowActive = displayWorkflow.id === "health";
  const archiveWorkflowActive = displayWorkflow.id === "archive";
  const privacyWorkflowActive = displayWorkflow.id === "privacy";
  const compressionWorkflowActive = displayWorkflow.id === "compress";
  const batchWorkflowActive = displayWorkflow.id === "batch";
  const comparisonWorkflowActive = displayWorkflow.id === "compare";
  const bookmarkWorkflowActive = displayWorkflow.id === "bookmarks";
  const annotationWorkflowActive = displayWorkflow.id === "annotate";
  const redactionWorkflowActive = displayWorkflow.id === "redact";
  const formWorkflowActive = displayWorkflow.id === "forms";
  const finishWorkflowActive = displayWorkflow.id === "finish";
  const printWorkflowActive = displayWorkflow.id === "print";
  const ActiveWorkflowIcon = displayWorkflow.icon;
  const selectedScanner = scannerDiscovery?.devices.find(
    (device) => device.id === selectedScannerId
  );
  const scanOperationBusy =
    scanBusy || scannerCaptureBusy || ocrReviewBusy || ocrReviewJob.isActive;
  const scanPreviewConfiguration =
    mode === "desktop" && scanWorkflowActive && selectedScanImage?.path
      ? [
          selectedScanImage.path,
          scanColourMode,
          String(scanAutoCrop),
          String(scanCorrectPerspective),
          String(scanRemoveShadows)
        ].join("\u0000")
      : null;
  scanPreviewCurrentConfigurationRef.current = scanPreviewConfiguration;
  const scanPreviewBusy = scanPreviewStarting || scanPreviewJob.isActive;
  const scanWorkflowBusy = scanOperationBusy || scanPreviewBusy;
  const scanPreviewJobMatchesConfiguration = Boolean(
    scanPreviewConfiguration &&
      scanPreviewJob.job &&
      scanPreviewJobConfigurationRef.current?.jobId === scanPreviewJob.job.jobId &&
      scanPreviewJobConfigurationRef.current.configuration === scanPreviewConfiguration
  );
  const scannerDpiOptions = selectedScanner?.supportedDpi.length
    ? selectedScanner.supportedDpi
    : [150, 300, 600];
  const scannerColourModeOptions = selectedScanner?.colourModes.length
    ? selectedScanner.colourModes
    : (["colour", "greyscale", "monochrome"] as ScanColourMode[]);
  const scannerCanCapture = Boolean(
    connectedScanningAvailable &&
    selectedScanner &&
      ((scannerSource === "flatbed" && selectedScanner.flatbed) ||
        (scannerSource === "feeder" && selectedScanner.feeder)) &&
      (!scannerDuplex || (scannerSource === "feeder" && selectedScanner.duplex)) &&
      !scanWorkflowBusy &&
      !scannerDiscoveryBusy
  );

  useEffect(() => {
    if (
      !connectedScanningAvailable ||
      !scanWorkflowActive ||
      scannerDiscoveryRequestedRef.current
    ) {
      return;
    }
    scannerDiscoveryRequestedRef.current = true;
    void refreshScanners();
  }, [connectedScanningAvailable, refreshScanners, scanWorkflowActive]);

  useEffect(() => {
    if (!selectedScanner) {
      setScannerDuplex(false);
      return;
    }
    setScannerSource((current) => {
      if (
        (current === "flatbed" && selectedScanner.flatbed) ||
        (current === "feeder" && selectedScanner.feeder)
      ) {
        return current;
      }
      return selectedScanner.flatbed ? "flatbed" : "feeder";
    });
    if (!selectedScanner.duplex || !selectedScanner.feeder) {
      setScannerDuplex(false);
    }
    setScanDpi((current) =>
      selectedScanner.supportedDpi.includes(current)
        ? current
        : [...selectedScanner.supportedDpi].sort(
            (left, right) => Math.abs(left - 300) - Math.abs(right - 300)
          )[0] ?? 300
    );
    setScanColourMode((current) =>
      selectedScanner.colourModes.includes(current)
        ? current
        : (selectedScanner.colourModes[0] ?? "colour")
    );
  }, [selectedScanner]);

  useEffect(() => {
    return () => {
      if (scanPreview?.url) {
        URL.revokeObjectURL(scanPreview.url);
      }
    };
  }, [scanPreview]);

  useEffect(() => {
    const job = scanPreviewJob.job;
    const jobIsActive = job?.status === "queued" || job?.status === "running";
    const jobIdentity = scanPreviewJobConfigurationRef.current;
    const jobConfiguration =
      job && jobIdentity?.jobId === job.jobId
        ? jobIdentity.configuration
        : null;

    const requestCancellation = () => {
      if (
        !job ||
        !jobIsActive ||
        scanPreviewCancellingJobIdRef.current === job.jobId
      ) {
        return;
      }
      scanPreviewCancellingJobIdRef.current = job.jobId;
      setScanPreviewCancelBusy(true);
      void scanPreviewJob.cancelJob().catch((reason) => {
        void reason;
        if (scanPreviewCancellingJobIdRef.current === job.jobId) {
          scanPreviewCancellingJobIdRef.current = null;
          setScanPreviewCancelBusy(false);
        }
        if (jobConfiguration === scanPreviewCurrentConfigurationRef.current) {
          setScanPreviewError(t("scan.preview.error.cancel"));
        }
      });
    };

    if (!scanPreviewConfiguration) {
      scanPreviewScheduledConfigurationRef.current = null;
      scanPreviewFailedConfigurationRef.current = null;
      setScanPreview(null);
      setScanPreviewError(null);
      scanPreviewPresentedJobIdRef.current = null;
      if (jobIsActive) {
        requestCancellation();
      } else if (job) {
        scanPreviewJob.clearJob();
        if (scanPreviewJobConfigurationRef.current?.jobId === job.jobId) {
          scanPreviewJobConfigurationRef.current = null;
        }
      }
      return;
    }

    if (scanOperationBusy) {
      scanPreviewScheduledConfigurationRef.current = null;
      if (jobIsActive) {
        requestCancellation();
      }
      return;
    }

    if (scanPreviewStartingConfigurationRef.current) {
      return;
    }

    if (job) {
      if (jobConfiguration === scanPreviewConfiguration) {
        return;
      }
      setScanPreview(null);
      setScanPreviewError(null);
      scanPreviewPresentedJobIdRef.current = null;
      if (jobIsActive) {
        requestCancellation();
        return;
      }
      if (scanPreviewCancellingJobIdRef.current === job.jobId) {
        scanPreviewCancellingJobIdRef.current = null;
        setScanPreviewCancelBusy(false);
      }
      scanPreviewJob.clearJob();
      if (scanPreviewJobConfigurationRef.current?.jobId === job.jobId) {
        scanPreviewJobConfigurationRef.current = null;
      }
      return;
    }

    if (
      scanPreviewFailedConfigurationRef.current === scanPreviewConfiguration ||
      scanPreviewScheduledConfigurationRef.current === scanPreviewConfiguration
    ) {
      return;
    }

    const sourcePath = selectedScanImage?.path;
    if (!sourcePath) {
      return;
    }
    const configuration = scanPreviewConfiguration;
    scanPreviewScheduledConfigurationRef.current = configuration;
    setScanPreview(null);
    setScanPreviewError(null);
    scanPreviewPresentedJobIdRef.current = null;
    const timer = window.setTimeout(() => {
      scanPreviewScheduledConfigurationRef.current = null;
      scanPreviewStartingConfigurationRef.current = configuration;
      setScanPreviewStarting(true);
      void scanPreviewJob
        .startJob({
          autoCrop: scanAutoCrop,
          autoOrient: true,
          colourMode: scanColourMode,
          correctPerspective: scanCorrectPerspective,
          inputPath: sourcePath,
          removeShadows: scanRemoveShadows
        })
        .then((started) => {
          scanPreviewJobConfigurationRef.current = {
            configuration,
            jobId: started.jobId
          };
          scanPreviewFailedConfigurationRef.current = null;
        })
        .catch((reason) => {
          void reason;
          if (scanPreviewCurrentConfigurationRef.current === configuration) {
            scanPreviewFailedConfigurationRef.current = configuration;
            setScanPreviewError(t("scan.preview.error.create"));
          }
        })
        .finally(() => {
          if (scanPreviewStartingConfigurationRef.current === configuration) {
            scanPreviewStartingConfigurationRef.current = null;
            setScanPreviewStarting(false);
          }
        });
    }, 320);

    return () => {
      window.clearTimeout(timer);
      if (scanPreviewScheduledConfigurationRef.current === configuration) {
        scanPreviewScheduledConfigurationRef.current = null;
      }
    };
  }, [
    scanAutoCrop,
    scanColourMode,
    scanCorrectPerspective,
    scanOperationBusy,
    scanPreviewConfiguration,
    scanPreviewJob.job?.jobId,
    scanPreviewJob.job?.status,
    scanPreviewRetryToken,
    scanRemoveShadows,
    scanPreviewStarting,
    selectedScanImage?.path,
    t
  ]);

  useEffect(() => {
    const job = scanPreviewJob.job;
    const jobIsActive = job?.status === "queued" || job?.status === "running";
    if (!job || jobIsActive) {
      return;
    }
    if (scanPreviewCancellingJobIdRef.current === job.jobId) {
      scanPreviewCancellingJobIdRef.current = null;
      setScanPreviewCancelBusy(false);
    }
    const jobIdentity = scanPreviewJobConfigurationRef.current;
    const jobConfiguration =
      jobIdentity?.jobId === job.jobId
        ? jobIdentity.configuration
        : null;
    if (
      !scanPreviewConfiguration ||
      jobConfiguration !== scanPreviewConfiguration ||
      scanPreviewPresentedJobIdRef.current === job.jobId
    ) {
      return;
    }
    scanPreviewPresentedJobIdRef.current = job.jobId;

    if (job.status === "succeeded" && job.result) {
      try {
        const bytes = Uint8Array.from(job.result.bytes);
        if (bytes.length === 0) {
          throw new Error(t("scan.preview.error.open"));
        }
        const url = URL.createObjectURL(
          new Blob([bytes], { type: job.result.mimeType })
        );
        const { bytes: _bytes, ...metadata } = job.result;
        setScanPreview({ ...metadata, url });
        setScanPreviewError(null);
      } catch (reason) {
        void reason;
        setScanPreview(null);
        setScanPreviewError(t("scan.preview.error.open"));
      }
      return;
    }

    setScanPreview(null);
    setScanPreviewError(
      job.status === "failed"
        ? localisePdfJobFailure(job, t)
        : t("job.stage.cancelled")
    );
  }, [
    scanPreviewConfiguration,
    scanPreviewJob.job?.error,
    scanPreviewJob.job?.jobId,
    scanPreviewJob.job?.result,
    scanPreviewJob.job?.status,
    t
  ]);

  const pageSlots = useMemo(() => {
    if (!activeDocument) {
      return [];
    }

    return Array.from({ length: activeDocument.previewPages }, (_, index) => index + 1);
  }, [activeDocument]);

  const readyTools = tools.filter((tool) => tool.available).length;
  const pyhankoAvailable = tools.some(
    (tool) => tool.command === "pyhanko" && tool.available
  );
  const qpdfAvailable = tools.some((tool) => tool.command === "qpdf" && tool.available);
  const ocrAvailable = Boolean(
    ocrReadiness?.ready && ocrReadiness.selectedLanguage === selectedOcrLanguage
  );
  const ocrReviewAvailable = Boolean(
    ocrReadiness?.tesseract.available && ocrReadiness.languageAvailable
  );
  let displayWorkflowStage = displayWorkflow.stage;
  if (displayWorkflow.id === "protect" && qpdfAvailable) {
    displayWorkflowStage = t("app.engine.ready");
  } else if (displayWorkflow.id === "ocr") {
    displayWorkflowStage = ocrReadinessBusy
      ? t("app.stage.checking")
      : ocrAvailable
        ? t("app.engine.ready")
        : t("workflow.ocr.stage");
  } else if (displayWorkflow.id === "archive" && archiveReadiness?.ready) {
    displayWorkflowStage = t("app.engine.ready");
  }
  const pdfError = documentReadError ?? pdf.error;
  const pdfBusy = isPdfDocument && (!pdfSource || pdf.loading) && !pdfError;
  const selectedPlannedPage = isPdfDocument ? pagePlan.pages[selectedPage - 1] : undefined;
  const orderedSelectedPageIds = orderedPageSelection(
    pagePlan.pages.map((page) => page.id),
    selectedPageIds
  );
  const effectiveSelectedPageIds =
    orderedSelectedPageIds.length > 0
      ? orderedSelectedPageIds
      : selectedPlannedPage
        ? [selectedPlannedPage.id]
        : [];
  const selectedPageIdSet = new Set(effectiveSelectedPageIds);
  const pageTransferSelectedPages = pagePlan.pages.filter((page) =>
    selectedPageIdSet.has(page.id)
  );
  const activeTransferSafety = activeEditSafety.checks.find(
    (check) => check.id === "active-workspace"
  )?.result;
  const pageTransferSources: PageTransferPdfSource[] =
    activeDocument?.kind === "pdf" &&
    activePdfRangeSource &&
    activeDocument.sourcePath &&
    pdf.document
      ? [
          {
            certificateAcknowledged: certificateRewriteAcknowledged,
            certificateSignature: Boolean(activeTransferSafety?.certificateSignature),
            document: pdf.document,
            id: "primary",
            modifiedAtMs: activePdfRangeSource.modifiedAtMs,
            name: activeDocument.name,
            password: pdf.openingPassword,
            path: activeDocument.sourcePath,
            size: activePdfRangeSource.size
          },
          ...importedPdfSources.map((source) => ({
            certificateAcknowledged: source.certificateAcknowledged,
            certificateSignature: source.certificateSignature,
            document: source.document,
            id: source.id,
            modifiedAtMs: source.modifiedAtMs,
            name: source.name,
            password: source.password,
            path: source.path,
            size: source.size
          }))
        ]
      : [];
  const pageTransferSourceIds = new Set(pageTransferSources.map((source) => source.id));
  const pageTransferReady =
    pageTransferSelectedPages.length > 0 &&
    pageTransferSelectedPages.every(
      (page) => page.kind === "blank" || pageTransferSourceIds.has(page.sourceId)
    );
  const canMoveSelectedPagesEarlier = canMovePagesByStep(
    pagePlan.pages,
    effectiveSelectedPageIds,
    -1
  );
  const canMoveSelectedPagesLater = canMovePagesByStep(
    pagePlan.pages,
    effectiveSelectedPageIds,
    1
  );
  const selectedImportedSource =
    selectedPlannedPage?.kind === "source" && selectedPlannedPage.sourceId !== "primary"
      ? importedPdfSourceMap.get(selectedPlannedPage.sourceId)
      : undefined;
  const selectedPdfDocument =
    selectedPlannedPage?.kind === "source"
      ? selectedPlannedPage.sourceId === "primary"
        ? pdf.document
        : (selectedImportedSource?.document ?? null)
      : null;
  const signaturePlacements = signaturePlacementHistory.present;
  const signaturePlaced = signaturePlacements.length > 0;
  const selectedSignatureAsset =
    signatureAssets.find((asset) => asset.id === selectedSignatureAssetId) ?? null;
  const selectedSignaturePlacement =
    signaturePlacements.find(
      (placement) =>
        placement.id === selectedSignaturePlacementId &&
        placement.pageId === selectedPlannedPage?.id
    ) ?? null;
  const signaturePlacementAssetsValid = signaturePlacements.every((placement) =>
    signatureAssets.some((asset) => asset.id === placement.assetId)
  );
  const workspaceHasPendingChanges = Boolean(
    activeDocument?.kind === "pdf" &&
      pdf.document &&
      (signaturePlaced ||
        documentLocked ||
        pagePlan.pages.length !== pdf.document.numPages ||
        pagePlan.pages.some(
          (page, index) =>
            page.kind !== "source" ||
            page.sourceId !== "primary" ||
            page.sourcePage !== index + 1 ||
            page.rotation !== 0
        ))
  );
  const documentLockPasswordsValid = Boolean(
    validPdfPassword(documentLockOpenPassword) &&
      documentLockOpenPassword === documentLockOpenPasswordConfirmation &&
      validPdfPassword(documentLockOwnerPassword) &&
      documentLockOwnerPassword === documentLockOwnerPasswordConfirmation &&
      documentLockOwnerPassword !== documentLockOpenPassword
  );
  const displayedPageCount = isPdfDocument ? pagePlan.pages.length : pageSlots.length;
  const selectedSearchMatchIndex = visiblePdfSearch.matches.findIndex(
    (match) => match.pageNumber === selectedPage
  );
  const currentSearchResultIndex =
    selectedSearchMatchIndex >= 0 ? selectedSearchMatchIndex : searchResultIndex;
  const engineSummary =
    mobileMode
      ? t("app.engine.mobileSummary")
      : mode === "desktop" && tools.length > 0
      ? t("app.engine.summary", { ready: formatNumber(readyTools), total: formatNumber(tools.length) })
      : t("app.engine.waiting");
  const activeEditSafetyCheck = activeEditSafety.checks[0];
  const editSafety =
    activeEditSafetyCheck?.status === "ready" ? activeEditSafetyCheck.result : null;
  const editSafetyUnavailable = activeEditSafetyCheck?.status === "error";
  const editSafetyPending = activeEditSafety.isChecking;
  const canExportPdf = Boolean(
    mode === "desktop" &&
      activeDocument?.kind === "pdf" &&
      activeDocument.sourcePath &&
      activePdfRangeSource &&
      activePdfRangeSource.path === activeDocument.sourcePath &&
      pdf.document &&
      pagePlan.pages.length > 0 &&
      !editSafetyPending &&
      (!(editSafety?.certificateSignature || editSafetyUnavailable) ||
        certificateRewriteAcknowledged) &&
      importedPdfSources.every(
        (source) =>
          !usedImportedSourceIds.has(source.id) ||
          !source.certificateSignature ||
          source.certificateAcknowledged
      ) &&
      signaturePlacementAssetsValid &&
      (!documentLocked ||
        (qpdfAvailable && signaturePlaced && documentLockPasswordsValid)) &&
      !exportBusy
  );
  const scanSourcePaths = useMemo(
    () => scanImages.flatMap((image) => (image.path ? [image.path] : [])),
    [scanImages]
  );
  const scanSourceIdentity = scanSourcePaths.join("\u0000");
  useEffect(() => {
    setOcrWordHints([]);
  }, [scanSourceIdentity, selectedOcrLanguage]);
  useEffect(() => {
    setScanOutputProtection((current) => createOutputProtectionDraft(current.enabled));
  }, [scanSourceIdentity]);
  useEffect(() => {
    const job = ocrReviewJob.job;
    if (!job) {
      return;
    }
    setOcrReviewOpen(true);
    if (job.status === "queued" || job.status === "running") {
      setOcrReviewError(null);
      setOcrReviewNotice(null);
      return;
    }
    setOcrReviewBusy(false);
    setOcrReviewCancelBusy(false);
    if (job.status === "succeeded" && job.result) {
      setOcrReviewResult(job.result);
      setOcrReviewError(null);
      setOcrReviewNotice(null);
    } else if (job.status === "cancelled") {
      setOcrReviewResult(null);
      setOcrReviewError(null);
      setOcrReviewNotice(
        t("ocrReview.cancelled")
      );
    } else if (job.status === "failed") {
      setOcrReviewResult(null);
      setOcrReviewNotice(null);
      setOcrReviewError(localisePdfJobFailure(job, t));
    }
  }, [ocrReviewJob.job?.jobId, ocrReviewJob.job?.status, t]);
  const ocrReviewConfiguration = [
    scanAutoCrop,
    scanColourMode,
    scanCorrectPerspective,
    scanRemoveShadows,
    scanSourceIdentity,
    selectedOcrLanguage,
    selectedPage
  ].join("\u0000");
  useEffect(() => {
    if (ocrReviewConfigurationRef.current === null) {
      ocrReviewConfigurationRef.current = ocrReviewConfiguration;
      return;
    }
    if (ocrReviewConfigurationRef.current === ocrReviewConfiguration) {
      return;
    }
    ocrReviewConfigurationRef.current = ocrReviewConfiguration;
    setOcrReviewOpen(false);
    setOcrReviewBusy(false);
    setOcrReviewCancelBusy(false);
    setOcrReviewError(null);
    setOcrReviewNotice(null);
    setOcrReviewResult(null);
    ocrReviewJob.clearJob();
  }, [ocrReviewConfiguration]);
  const recoveryBlockedByImportedPassword = importedPdfSources.some(
    (source) => usedImportedSourceIds.has(source.id) && Boolean(source.password)
  );
  const recoveryDraft = useMemo<Omit<RecoverySnapshot, "savedAtUnixMs"> | null>(() => {
    if (
      mode === "desktop" &&
      activeWorkflowId === "merge" &&
      mergeRecoverySources.length > 0
    ) {
      return {
        activeWorkflowId: "merge",
        document: {
          kind: "merge",
          name: "merge",
          sources: mergeRecoverySources.map((source) => ({ ...source }))
        },
        selectedPage: 1,
        version: 1,
        zoom
      };
    }
    if (
      mode === "desktop" &&
      activeWorkflowId === "split" &&
      splitRecoveryPlan
    ) {
      return {
        activeWorkflowId: "split",
        document: {
          kind: "split",
          name: fileNameFromPath(splitRecoveryPlan.sourcePath),
          pageGroups: splitRecoveryPlan.pageGroups,
          sourcePath: splitRecoveryPlan.sourcePath
        },
        selectedPage: 1,
        version: 1,
        zoom
      };
    }
    if (
      mode === "desktop" &&
      activeDocument?.kind === "pdf" &&
      activeDocument.sourcePath &&
      pdf.document &&
      pagePlan.pages.length > 0 &&
      importedPdfSources.every(
        (source) => !usedImportedSourceIds.has(source.id) || !source.password
      )
    ) {
      return {
        activeWorkflowId,
        document: {
          kind: "pdf",
          importedSources: importedPdfSources
            .filter((source) => usedImportedSourceIds.has(source.id))
            .map((source) => ({
              certificateAcknowledged: source.certificateAcknowledged,
              certificateSignature: source.certificateSignature,
              id: source.id,
              name: source.name,
              sourcePath: source.path
            })),
          name: activeDocument.name,
          pages: pagePlan.pages.map((page) => ({ ...page })),
          sourcePath: activeDocument.sourcePath
        },
        selectedPage,
        version: 1,
        zoom
      };
    }
    if (
      mode === "desktop" &&
      activeDocument?.kind === "scan" &&
      scanImages.length > 0 &&
      scanSourcePaths.length === scanImages.length
    ) {
      return {
        activeWorkflowId,
        document: {
          kind: "scan",
          name: activeDocument.name,
          settings: {
            autoCrop: scanAutoCrop,
            colourMode: scanColourMode,
            correctPerspective: scanCorrectPerspective,
            dpi: scanDpi,
            jpegQuality: scanJpegQuality,
            marginPt: scanMarginPt,
            ocrLanguage: selectedOcrLanguage,
            paperId: selectedPaperId,
            recogniseText,
            removeShadows: scanRemoveShadows,
            straighten: straightenScan
          },
          sourcePaths: scanSourcePaths
        },
        selectedPage,
        version: 1,
        zoom
      };
    }
    return null;
  }, [
    activeDocument,
    activeWorkflowId,
    importedPdfSources,
    mergeRecoverySources,
    mode,
    pagePlan.pages,
    pdf.document,
    recogniseText,
    scanAutoCrop,
    scanColourMode,
    scanCorrectPerspective,
    scanDpi,
    scanImages.length,
    scanJpegQuality,
    scanMarginPt,
    scanRemoveShadows,
    scanSourcePaths,
    selectedOcrLanguage,
    selectedPage,
    selectedPaperId,
    straightenScan,
    splitRecoveryPlan,
    usedImportedSourceIds,
    zoom
  ]);
  const saveRecoveryDraft = useCallback(
    async (announce: boolean) => {
      if (!recoveryDraft) {
        return;
      }
      setRecoverySaveState("saving");
      try {
        const result = await invoke<RecoverySaveResult>("save_recovery_snapshot", {
          snapshot: { ...recoveryDraft, savedAtUnixMs: Date.now() }
        });
        setRecoverySaveState("saved");
        setRecoveryLastSavedAt(result.savedAtUnixMs);
        if (announce) {
          setOperationStatus({
            kind: "success",
            text: t("app.draft.status.saved")
          });
        }
      } catch {
        setRecoverySaveState("error");
        if (announce) {
          setOperationStatus({
            kind: "error",
            text: t("app.draft.status.failed")
          });
        }
      }
    },
    [recoveryDraft, t]
  );

  useEffect(() => {
    setRecoverySaveState("idle");
  }, [recoveryDraft]);

  useEffect(() => {
    if (
      !recoveryDraft ||
      !recoveryChecked ||
      recoveryCandidate ||
      pendingPdfRecovery ||
      recoveryBusy
    ) {
      return;
    }
    const timer = window.setTimeout(() => {
      void saveRecoveryDraft(false);
    }, 900);
    return () => window.clearTimeout(timer);
  }, [
    pendingPdfRecovery,
    recoveryBusy,
    recoveryCandidate,
    recoveryChecked,
    recoveryDraft,
    saveRecoveryDraft
  ]);
  const canCreateScanPdf = Boolean(
    mode === "desktop" &&
      scanExportJob.recoveryComplete &&
      isScanDocument &&
      scanSourcePaths.length === scanImages.length &&
      scanImages.length > 0 &&
      (!recogniseText || ocrAvailable) &&
      outputProtectionIsValid(scanOutputProtection, qpdfAvailable) &&
      !scanWorkflowBusy
  );

  const handleOpenDocument = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    event.target.value = "";

    if (!scanOperationBusy && files.length > 0) {
      loadFiles(files);
    }
  };

  const chooseDocuments = () => {
    if (scanOperationBusy) {
      return;
    }
    if (mode !== "desktop") {
      fileInputRef.current?.click();
      return;
    }

    void chooseDesktopDocuments();
  };

  const chooseDesktopDocuments = async () => {
    setOpeningLocalFiles(true);
    setOperationStatus(null);

    try {
      const selection =
        takeE2eOpenSelection() ??
        (await open({
          directory: false,
          filters: [{ name: t("app.dialog.open.filter"), extensions: desktopInputExtensions }],
          multiple: true,
          title: t("app.dialog.open.title")
        }));
      const paths = Array.isArray(selection) ? selection : selection ? [selection] : [];

      if (paths.length === 0) {
        return;
      }

      const imagePaths = paths.filter((path) => isImagePath(path));
      if (imagePaths.length > 0) {
        const files = await Promise.all(
          imagePaths.map(async (path) => {
            const data = await invoke<ArrayBuffer>("read_local_document", { path });
            return new File([data], fileNameFromPath(path), { type: mimeTypeFromPath(path) });
          })
        );
        loadScanBatch(files, imagePaths);
        return;
      }

      const pdfPath = paths.find((path) => path.toLocaleLowerCase("en-GB").endsWith(".pdf"));
      if (pdfPath) {
        const source = await invoke<PdfRangeSource>("open_local_pdf", { path: pdfPath });
        loadRangedPdf(source);
        return;
      }

      const files = await Promise.all(
        paths.map(async (path) => {
          const data = await invoke<ArrayBuffer>("read_local_document", { path });
          return new File([data], fileNameFromPath(path), { type: mimeTypeFromPath(path) });
        })
      );
      loadFiles(files, paths);
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: t("app.document.openFailedDetail")
      });
    } finally {
      setOpeningLocalFiles(false);
    }
  };

  const handleDrop = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    setDragActive(false);

    if (scanOperationBusy) {
      return;
    }

    const files = Array.from(event.dataTransfer.files).filter((file) => isSupportedInput(file));

    if (files.length > 0) {
      loadFiles(files);
    }
  };

  const loadFiles = (files: File[], sourcePaths?: string[]) => {
    if (recoveryCandidate) {
      setRecoveryCandidate(null);
      void invoke("clear_recovery_snapshots");
    }
    const images = files.flatMap((file, index) =>
      isImageFile(file) ? [{ file, path: sourcePaths?.[index] }] : []
    );

    if (images.length > 0) {
      loadScanBatch(
        images.map((image) => image.file),
        images.map((image) => image.path)
      );
      return;
    }

    const pdfIndex = files.findIndex((file) => isPdfFile(file));
    const pdf = files[pdfIndex];

    if (pdf) {
      loadPdf(pdf, sourcePaths?.[pdfIndex]);
    }
  };

  const preparePdfWorkspace = (name: string, size: number, sourcePath?: string) => {
    const loadId = ++fileLoadSequence.current;

    clearImportedPdfSources();
    setImportPagesOpen(false);
    setPageTransferOpen(false);
    setCertificateRewriteAcknowledged(false);
    setScanImages([]);
    setScanFromConnectedScanner(false);
    setScanBatchHandoff(null);
    setPdfSource(null);
    setDocumentReadError(null);
    setSearchQuery("");
    setSearchResultIndex(0);
    detachedSignaturePlacementsRef.current = [];
    signaturePlacementHistory.reset([]);
    setSelectedSignaturePlacementId(null);
    setDocumentLocked(false);
    setScanOutputProtection(createOutputProtectionDraft());
    setOcrReviewOpen(false);
    setOcrReviewBusy(false);
    setOcrReviewCancelBusy(false);
    setOcrReviewError(null);
    setOcrReviewNotice(null);
    setOcrReviewResult(null);
    ocrReviewJob.clearJob();
    setOperationStatus(null);
    organiseJob.clearJob();
    setExportCancelBusy(false);
    setActiveDocument({
      kind: "pdf",
      name,
      sizeBytes: size,
      fileCount: 1,
      previewPages: 0,
      sourcePath
    });
    setSelectedPage(1);
    return loadId;
  };

  const loadRangedPdf = (source: PdfRangeSource) => {
    const loadId = preparePdfWorkspace(source.name, source.size, source.path);
    if (loadId === fileLoadSequence.current) {
      setPdfSource(source);
    }
  };

  const loadPdf = (file: File, sourcePath?: string) => {
    const loadId = preparePdfWorkspace(file.name, file.size, sourcePath);

    void file
      .arrayBuffer()
      .then((data) => {
        if (loadId === fileLoadSequence.current) {
          setPdfSource({ data, name: file.name, size: file.size });
        }
      })
      .catch((reason: unknown) => {
        if (loadId === fileLoadSequence.current) {
          void reason;
          setDocumentReadError("unreadable");
        }
      });
  };

  const loadScanBatch = (
    files: File[],
    sourcePaths?: Array<string | undefined>,
    fromConnectedScanner = false
  ) => {
    fileLoadSequence.current += 1;
    const totalSize = files.reduce((size, file) => size + file.size, 0);
    const firstName = files[0]?.name ?? t("scan.batch.defaultName");
    const batchName =
      files.length === 1
        ? firstName
        : t("scan.batch.multipleName", { count: formatNumber(files.length) });

    clearImportedPdfSources();
    setImportPagesOpen(false);
    setPageTransferOpen(false);
    setCertificateRewriteAcknowledged(false);
    setScanFromConnectedScanner(fromConnectedScanner);
    setScanBatchHandoff(null);
    setScanImages(
      files.map((file, index) => ({
        name: file.name,
        path: sourcePaths?.[index],
        url: URL.createObjectURL(file)
      }))
    );
    setPdfSource(null);
    setDocumentReadError(null);
    setSearchOpen(false);
    setSearchQuery("");
    setSearchResultIndex(0);
    detachedSignaturePlacementsRef.current = [];
    signaturePlacementHistory.reset([]);
    setSelectedSignaturePlacementId(null);
    setDocumentLocked(false);
    setScanOutputProtection(createOutputProtectionDraft());
    setScanPreview(null);
    setScanPreviewStarting(false);
    setScanPreviewCancelBusy(false);
    setScanPreviewError(null);
    scanPreviewScheduledConfigurationRef.current = null;
    scanPreviewStartingConfigurationRef.current = null;
    scanPreviewFailedConfigurationRef.current = null;
    scanPreviewJobConfigurationRef.current = null;
    scanPreviewCancellingJobIdRef.current = null;
    scanPreviewPresentedJobIdRef.current = null;
    scanPreviewJob.clearJob();
    setScannerCaptureStarting(false);
    setScannerCaptureImportBusy(false);
    setScannerCaptureCancelBusy(false);
    setScannerCaptureImportError(null);
    scannerCaptureDeviceNameRef.current = null;
    scannerCaptureTerminalPresentedJobIdRef.current = null;
    scannerCaptureProcessedJobIdRef.current = null;
    scannerCaptureImportRef.current = null;
    scannerCaptureJob.clearJob();
    setOperationStatus(null);
    setActiveDocument({
      kind: "scan",
      name: batchName,
      sizeBytes: totalSize,
      fileCount: files.length,
      previewPages: Math.min(24, Math.max(1, files.length))
    });
    setActiveWorkflowId("scan");
    setSelectedPage(1);
  };

  useEffect(() => {
    const job = scannerCaptureJob.job;
    if (
      !job ||
      job.status !== "succeeded" ||
      !job.result ||
      scannerCaptureProcessedJobIdRef.current === job.jobId
    ) {
      return;
    }

    let importEntry = scannerCaptureImportRef.current;
    if (!importEntry || importEntry.jobId !== job.jobId) {
      importEntry = {
        jobId: job.jobId,
        promise: readScannerCaptureFiles(job.result.paths)
      };
      scannerCaptureImportRef.current = importEntry;
    }
    const result = job.result;
    let active = true;
    setScannerCaptureImportBusy(true);
    setScannerCaptureImportError(null);

    void importEntry.promise
      .then((files) => {
        if (!active || scannerCaptureProcessedJobIdRef.current === job.jobId) {
          return;
        }
        scannerCaptureProcessedJobIdRef.current = job.jobId;
        loadScanBatch(files, result.paths, true);
        const scannerName = scannerCaptureDeviceNameRef.current;
        setOperationStatus({
          kind: "success",
          text: t(
            result.pageCount === 1
              ? "scanner.capture.success.one"
              : "scanner.capture.success.other",
            {
              count: formatNumber(result.pageCount),
              scanner: scannerName || t("scanner.capture.unknown")
            }
          ),
          warnings:
            result.warnings.length > 0
              ? [
                  t(
                    result.warnings.length === 1
                      ? "scanner.capture.warning.one"
                      : "scanner.capture.warning.other",
                    { count: formatNumber(result.warnings.length) }
                  )
                ]
              : []
        });
        scannerCaptureImportRef.current = null;
        scannerCaptureJob.clearJob();
      })
      .catch((reason) => {
        if (!active) {
          return;
        }
        void reason;
        const message = t("scanner.capture.openFailed");
        setScannerCaptureImportError(message);
        setOperationStatus({ kind: "error", text: message });
      })
      .finally(() => {
        if (active) {
          setScannerCaptureImportBusy(false);
        }
      });

    return () => {
      active = false;
    };
  }, [
    scannerCaptureImportRetryToken,
    scannerCaptureJob.job?.jobId,
    scannerCaptureJob.job?.result,
    scannerCaptureJob.job?.status,
    t
  ]);

  useEffect(() => {
    const job = scannerCaptureJob.job;
    if (
      !job ||
      job.status === "queued" ||
      job.status === "running" ||
      job.status === "succeeded" ||
      scannerCaptureTerminalPresentedJobIdRef.current === job.jobId
    ) {
      return;
    }
    scannerCaptureTerminalPresentedJobIdRef.current = job.jobId;
    setScannerCaptureCancelBusy(false);
    setOperationStatus(
      job.status === "cancelled"
        ? {
            kind: "info",
            text: t("scanner.capture.cancelled")
          }
        : {
            kind: "error",
            text: localisePdfJobFailure(job, t)
          }
    );
  }, [
    scannerCaptureJob.job?.error,
    scannerCaptureJob.job?.jobId,
    scannerCaptureJob.job?.status,
    t
  ]);

  const captureFromScanner = async () => {
    if (
      !connectedScanningAvailable ||
      !selectedScanner ||
      !selectedPaper ||
      !scannerCanCapture
    ) {
      return;
    }
    setScannerCaptureStarting(true);
    setScannerCaptureCancelBusy(false);
    setScannerCaptureImportError(null);
    setOperationStatus(null);
    scannerCaptureDeviceNameRef.current = selectedScanner.name;
    scannerCaptureTerminalPresentedJobIdRef.current = null;
    scannerCaptureProcessedJobIdRef.current = null;
    scannerCaptureImportRef.current = null;
    scannerCaptureJob.clearJob();
    try {
      await scannerCaptureJob.startJob({
        colourMode: scanColourMode,
        deviceId: selectedScanner.id,
        dpi: scanDpi,
        duplex: scannerSource === "feeder" && scannerDuplex,
        pageLimit: scannerSource === "flatbed" ? 1 : scannerPageLimit,
        paperHeightMm: selectedPaper.heightMm,
        paperWidthMm: selectedPaper.widthMm,
        source: scannerSource
      });
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: t("scanner.capture.failed")
      });
    } finally {
      setScannerCaptureStarting(false);
    }
  };

  const cancelScannerCapture = async () => {
    if (!scannerCaptureJob.isActive || scannerCaptureCancelBusy) {
      return;
    }
    setScannerCaptureCancelBusy(true);
    try {
      await scannerCaptureJob.cancelJob();
    } catch (reason) {
      void reason;
      setScannerCaptureCancelBusy(false);
      setOperationStatus({
        kind: "error",
        text: t("scanner.capture.cancelFailed")
      });
    }
  };

  const retryScannerCaptureImport = () => {
    const job = scannerCaptureJob.job;
    if (
      !job ||
      job.status !== "succeeded" ||
      !job.result ||
      scannerCaptureImportBusy
    ) {
      return;
    }
    scannerCaptureImportRef.current = null;
    scannerCaptureProcessedJobIdRef.current = null;
    setScannerCaptureImportError(null);
    setScannerCaptureImportRetryToken((current) => current + 1);
  };

  const discardScannerCaptureImport = () => {
    const job = scannerCaptureJob.job;
    if (!job || job.status !== "succeeded" || scannerCaptureImportBusy) {
      return;
    }
    scannerCaptureProcessedJobIdRef.current = job.jobId;
    scannerCaptureImportRef.current = null;
    scannerCaptureDeviceNameRef.current = null;
    setScannerCaptureImportError(null);
    scannerCaptureJob.clearJob();
    setOperationStatus({
      kind: "info",
      text: t("scanner.capture.discarded")
    });
  };

  const restoreRecovery = async () => {
    const snapshot = recoveryCandidate;
    if (!snapshot || mode !== "desktop" || recoveryBusy) {
      return;
    }
    setRecoveryBusy(true);
    setOperationStatus(null);
    let recoveryFailureMessage = t("app.recovery.error.restore");
    try {
      if (snapshot.document.kind === "pdf") {
        const restoredSources: ImportedPdfSource[] = [];
        try {
          for (const source of snapshot.document.importedSources) {
            const pdfSource = await invoke<PdfRangeSource>("open_local_pdf", {
              path: source.sourcePath
            });
            const loadingTask = createPdfLoadingTask(pdfSource);
            let passwordRequired = false;
            loadingTask.onPassword = () => {
              passwordRequired = true;
              void loadingTask.destroy();
            };
            try {
              const document = await loadingTask.promise;
              restoredSources.push({
                certificateAcknowledged: source.certificateAcknowledged,
                certificateSignature: source.certificateSignature,
                document,
                id: source.id,
                loadingTask,
                modifiedAtMs: pdfSource.modifiedAtMs,
                name: pdfSource.name,
                password: null,
                path: pdfSource.path,
                size: pdfSource.size
              });
            } catch (reason) {
              if (passwordRequired) {
                recoveryFailureMessage = t("app.recovery.error.importedProtected", {
                  name: source.name
                });
              }
              throw reason;
            }
          }
        } catch (reason) {
          restoredSources.forEach((source) => {
            void source.loadingTask.destroy();
          });
          throw reason;
        }
        const source = await invoke<PdfRangeSource>("open_local_pdf", {
          path: snapshot.document.sourcePath
        });
        setPendingPdfRecovery(snapshot);
        setRecoveryCandidate(null);
        loadRangedPdf(source);
        importedPdfSourcesRef.current = restoredSources;
        setImportedPdfSources(restoredSources);
      } else if (snapshot.document.kind === "scan") {
        const files = await Promise.all(
          snapshot.document.sourcePaths.map(async (path) => {
            const data = await invoke<ArrayBuffer>("read_local_document", { path });
            return new File([data], fileNameFromPath(path), { type: mimeTypeFromPath(path) });
          })
        );
        loadScanBatch(files, snapshot.document.sourcePaths);
        setScanAutoCrop(snapshot.document.settings.autoCrop ?? false);
        setScanColourMode(snapshot.document.settings.colourMode);
        setScanCorrectPerspective(snapshot.document.settings.correctPerspective ?? false);
        setScanDpi(snapshot.document.settings.dpi);
        setScanJpegQuality(snapshot.document.settings.jpegQuality);
        setScanMarginPt(snapshot.document.settings.marginPt);
        setSelectedOcrLanguage(snapshot.document.settings.ocrLanguage);
        setSelectedPaperId(snapshot.document.settings.paperId);
        setRecogniseText(snapshot.document.settings.recogniseText);
        setScanRemoveShadows(snapshot.document.settings.removeShadows ?? false);
        setStraightenScan(snapshot.document.settings.straighten);
        setSelectedPage(snapshot.selectedPage);
        setZoom(snapshot.zoom);
        setActiveWorkflowId(
          workflows.some((workflow) => workflow.id === snapshot.activeWorkflowId)
            ? snapshot.activeWorkflowId
            : "scan"
        );
        setRecoveryCandidate(null);
        setOperationStatus({
          kind: "success",
          text: t(
            files.length === 1 ? "app.recovery.scan.one" : "app.recovery.scan.other",
            {
              count: formatNumber(files.length),
              name: snapshot.document.name
            }
          )
        });
      } else if (snapshot.document.kind === "merge") {
        const restoredSources: RecoveryMergeSource[] = [];
        for (const source of snapshot.document.sources) {
          const opened = await invoke<PdfRangeSource>("open_local_pdf", {
            path: source.sourcePath
          });
          restoredSources.push({
            id: source.id,
            pageRange: source.pageRange,
            sourcePath: opened.path
          });
        }
        setMergeRecoverySources(restoredSources);
        setSplitRecoveryPlan(null);
        setSelectedPage(1);
        setZoom(snapshot.zoom);
        setActiveWorkflowId("merge");
        setRecoveryCandidate(null);
        setOperationStatus({
          kind: "success",
          text: t(
            restoredSources.length === 1
              ? "app.recovery.merge.one"
              : "app.recovery.merge.other",
            { count: formatNumber(restoredSources.length) }
          )
        });
      } else {
        const opened = await invoke<PdfRangeSource>("open_local_pdf", {
          path: snapshot.document.sourcePath
        });
        setSplitRecoveryPlan({
          pageGroups: snapshot.document.pageGroups,
          sourcePath: opened.path
        });
        setMergeRecoverySources([]);
        setSelectedPage(1);
        setZoom(snapshot.zoom);
        setActiveWorkflowId("split");
        setRecoveryCandidate(null);
        setOperationStatus({
          kind: "success",
          text: t("app.recovery.split", { name: fileNameFromPath(opened.path) })
        });
      }
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: recoveryFailureMessage
      });
    } finally {
      setRecoveryBusy(false);
    }
  };

  const discardRecovery = async () => {
    setRecoveryCandidate(null);
    try {
      await invoke("clear_recovery_snapshots");
      setRecoverySaveState("idle");
      setRecoveryLastSavedAt(null);
    } catch {
      setRecoverySaveState("error");
      setOperationStatus({
        kind: "error",
        text: t("app.recovery.error.clear")
      });
    }
  };

  const resetWorkspace = () => {
    fileLoadSequence.current += 1;
    clearImportedPdfSources();
    setImportPagesOpen(false);
    setPageTransferOpen(false);
    setCertificateRewriteAcknowledged(false);
    setActiveDocument(null);
    setScanImages([]);
    setScanFromConnectedScanner(false);
    setScanBatchHandoff(null);
    setPdfSource(null);
    setDocumentReadError(null);
    setSearchOpen(false);
    setSearchQuery("");
    setSearchResultIndex(0);
    detachedSignaturePlacementsRef.current = [];
    signaturePlacementHistory.reset([]);
    setSelectedSignaturePlacementId(null);
    setDocumentLocked(false);
    setScanOutputProtection(createOutputProtectionDraft());
    setOperationStatus(null);
    setSelectedPage(1);
    setActiveWorkflowId(workflows[0].id);
    setActiveToolId(editorTools[0].id);
    setRecoveryCandidate(null);
    setPendingPdfRecovery(null);
    setMergeRecoverySources([]);
    setSplitRecoveryPlan(null);
    setRecoverySaveState("idle");
    setRecoveryLastSavedAt(null);
    if (mode === "desktop") {
      void invoke("clear_recovery_snapshots");
    }
  };

  const closeSearch = () => {
    setSearchOpen(false);
    setSearchQuery("");
    setSearchResultIndex(0);
  };

  const navigateSearch = (direction: -1 | 1) => {
    if (visiblePdfSearch.matches.length === 0) {
      return;
    }

    const currentMatch = visiblePdfSearch.matches.findIndex(
      (match) => match.pageNumber === selectedPage
    );
    const nextIndex =
      currentMatch === -1
        ? direction === 1
          ? 0
          : visiblePdfSearch.matches.length - 1
        : (currentMatch + direction + visiblePdfSearch.matches.length) %
          visiblePdfSearch.matches.length;

    setSearchResultIndex(nextIndex);
    setSelectedPage(visiblePdfSearch.matches[nextIndex].pageNumber);
  };

  const selectPlannedPage = (pageNumber: number, mode: PageSelectionMode) => {
    const clickedId = pagePlan.pages[pageNumber - 1]?.id;
    if (!clickedId) {
      return;
    }
    const next = resolvePageSelection(
      pagePlan.pages.map((page) => page.id),
      effectiveSelectedPageIds,
      selectedPlannedPage?.id ?? null,
      selectionAnchorId,
      clickedId,
      mode
    );
    if (!next) {
      return;
    }
    setSelectedPageIds(next.selectedIds);
    setSelectionAnchorId(next.anchorId);
    setSelectedPage(pagePlan.pages.findIndex((page) => page.id === next.activeId) + 1);
  };

  const toggleAllPageSelection = () => {
    if (!selectedPlannedPage) {
      return;
    }
    if (effectiveSelectedPageIds.length === pagePlan.pages.length) {
      setSelectedPageIds([selectedPlannedPage.id]);
      setSelectionAnchorId(selectedPlannedPage.id);
      return;
    }
    setSelectedPageIds(pagePlan.pages.map((page) => page.id));
    setSelectionAnchorId(selectedPlannedPage.id);
  };

  const undoPageOperation = () => {
    pagePlanFocusIdRef.current = selectedPlannedPage?.id ?? null;
    pagePlan.undo();
  };

  const redoPageOperation = () => {
    pagePlanFocusIdRef.current = selectedPlannedPage?.id ?? null;
    pagePlan.redo();
  };

  const handleEditorTool = (toolId: string) => {
    if (["text", "highlight", "stamp"].includes(toolId)) {
      setActiveWorkflowId("annotate");
      setActiveToolId("select");
      return;
    }

    if (editSafetyPending && (toolId === "rotate" || toolId === "delete")) {
      return;
    }

    if (toolId === "rotate" && selectedPlannedPage) {
      pagePlan.rotateMany(effectiveSelectedPageIds);
      setActiveToolId("select");
      return;
    }
    if (
      toolId === "delete" &&
      selectedPlannedPage &&
      effectiveSelectedPageIds.length < pagePlan.pages.length
    ) {
      const selected = new Set(effectiveSelectedPageIds);
      const firstSelectedIndex = pagePlan.pages.findIndex((page) => selected.has(page.id));
      const remainingPages = pagePlan.pages.filter((page) => !selected.has(page.id));
      const nextIndex = Math.min(Math.max(0, firstSelectedIndex), remainingPages.length - 1);
      const nextActivePage = remainingPages[nextIndex];
      pagePlan.removeMany(effectiveSelectedPageIds);
      setSelectedPageIds([nextActivePage.id]);
      setSelectionAnchorId(nextActivePage.id);
      setSelectedPage(nextIndex + 1);
      setActiveToolId("select");
      return;
    }

    setActiveToolId(toolId);
  };

  const duplicateSelectedPage = () => {
    if (!selectedPlannedPage || editSafetyPending) {
      return;
    }

    pagePlan.duplicateMany(effectiveSelectedPageIds);
  };

  const insertBlankPage = () => {
    if (!pdf.document || !selectedPaper || editSafetyPending) {
      return;
    }

    pagePlan.insertBlank(
      selectedPage - 1,
      millimetresToPoints(selectedPaper.widthMm),
      millimetresToPoints(selectedPaper.heightMm),
      selectedPaper.name
    );
    setSelectedPage((current) => current + 1);
  };

  const importPdfPages = (source: ImportedPdfReady) => {
    importedSourceCounter.current += 1;
    const sourceId = `import-${Date.now()}-${importedSourceCounter.current}`;
    const importedSource: ImportedPdfSource = {
      certificateAcknowledged: source.certificateAcknowledged,
      certificateSignature: source.certificateSignature,
      document: source.document,
      id: sourceId,
      loadingTask: source.loadingTask,
      modifiedAtMs: source.modifiedAtMs,
      name: source.name,
      password: source.password,
      path: source.path,
      size: source.size
    };
    setImportedPdfSources((current) => {
      const next = [...current, importedSource];
      importedPdfSourcesRef.current = next;
      return next;
    });
    pagePlan.insertSourcePages(selectedPage - 1, sourceId, source.selectedPages);
    setSelectedPage((current) => current + source.selectedPages.length);
    setActiveWorkflowId("organise");
    setOperationStatus({
      kind: "success",
      text: t(
        source.selectedPages.length === 1
          ? "organise.import.success.one"
          : "organise.import.success.other",
        {
          count: formatNumber(source.selectedPages.length),
          name: source.name
        }
      )
    });
  };

  const completePageTransferMove = (pageIds: string[], result: ExportResult) => {
    const selected = new Set(pageIds);
    const firstSelectedIndex = pagePlan.pages.findIndex((page) => selected.has(page.id));
    const remainingPages = pagePlan.pages.filter((page) => !selected.has(page.id));
    if (firstSelectedIndex < 0 || remainingPages.length === 0) {
      return;
    }

    const nextIndex = Math.min(firstSelectedIndex, remainingPages.length - 1);
    const nextActivePage = remainingPages[nextIndex];
    pagePlan.removeMany(pageIds);
    setSelectedPageIds([nextActivePage.id]);
    setSelectionAnchorId(nextActivePage.id);
    setSelectedPage(nextIndex + 1);
    setOperationStatus({
      kind: "success",
      text: t("transfer.source.moveApplied", {
        count: formatNumber(pageIds.length),
        name: fileNameFromPath(result.outputPath)
      })
    });
  };

  const moveSelectedPage = (direction: -1 | 1) => {
    if (editSafetyPending) {
      return;
    }
    const moved = movePagesByStep(pagePlan.pages, effectiveSelectedPageIds, direction);
    if (moved === pagePlan.pages) {
      return;
    }

    pagePlan.moveManyByStep(effectiveSelectedPageIds, direction);
    const activeId = selectedPlannedPage?.id;
    if (activeId) {
      setSelectedPage(moved.findIndex((page) => page.id === activeId) + 1);
    }
  };

  const dropPlannedPage = (targetPageId: string) => {
    if (editSafetyPending) {
      setDraggedPageId(null);
      setDropTargetPageId(null);
      return;
    }
    if (!draggedPageId || draggedPageId === targetPageId) {
      setDraggedPageId(null);
      setDropTargetPageId(null);
      return;
    }

    const draggedSelection = selectedPageIdSet.has(draggedPageId)
      ? effectiveSelectedPageIds
      : [draggedPageId];
    const reordered = reorderPagesAtDrop(
      pagePlan.pages,
      draggedSelection,
      draggedPageId,
      targetPageId
    );
    if (reordered !== pagePlan.pages) {
      pagePlan.moveManyAtDrop(draggedSelection, draggedPageId, targetPageId);
      setSelectedPage(reordered.findIndex((page) => page.id === draggedPageId) + 1);
    }

    setDraggedPageId(null);
    setDropTargetPageId(null);
  };

  const currentVisualPageAspect = () => {
    const frame = document.querySelector<HTMLElement>(".pdf-page-frame");
    const bounds = frame?.getBoundingClientRect();
    return bounds && bounds.width > 0 && bounds.height > 0 ? bounds.width / bounds.height : 1;
  };

  const addSignatureAsset = (asset: VisualSignatureAsset) => {
    if (signatureAssets.length >= MAX_VISUAL_SIGNATURE_ASSETS) {
      setOperationStatus({
        kind: "error",
        text: t("signature.error.assetLimit", { count: MAX_VISUAL_SIGNATURE_ASSETS })
      });
      return;
    }
    setSignatureAssets((current) => [...current, asset]);
    setSelectedSignatureAssetId(asset.id);
  };

  const removeSignatureAsset = (assetId: string) => {
    if (
      [...signaturePlacements, ...detachedSignaturePlacementsRef.current].some(
        (placement) => placement.assetId === assetId && placement.locked
      )
    ) {
      setOperationStatus({
        kind: "info",
        text: t("signature.error.lockedAsset")
      });
      return;
    }
    setSignatureAssets((current) => current.filter((asset) => asset.id !== assetId));
    detachedSignaturePlacementsRef.current = detachedSignaturePlacementsRef.current.filter(
      (placement) => placement.assetId !== assetId
    );
    if (signaturePlacements.some((placement) => placement.assetId === assetId)) {
      signaturePlacementHistory.commit((current) =>
        current.filter((placement) => placement.assetId !== assetId)
      );
    }
    if (selectedSignatureAssetId === assetId) {
      setSelectedSignatureAssetId(
        signatureAssets.find((asset) => asset.id !== assetId)?.id ?? null
      );
    }
  };

  const placeSignatureAsset = (
    assetId = selectedSignatureAssetId,
    centre?: { x: number; y: number },
    pageAspect = currentVisualPageAspect()
  ) => {
    if (!assetId || !selectedPlannedPage) return;
    if (signaturePlacements.length >= MAX_VISUAL_SIGNATURE_PLACEMENTS) {
      setOperationStatus({
        kind: "error",
        text: t("signature.error.placementLimit", {
          count: MAX_VISUAL_SIGNATURE_PLACEMENTS
        })
      });
      return;
    }
    const asset = signatureAssets.find((candidate) => candidate.id === assetId);
    if (!asset) return;
    const placement = createVisualSignaturePlacement(
      createVisualSignatureId("placement"),
      asset,
      selectedPlannedPage.id,
      pageAspect,
      "right",
      centre
    );
    signaturePlacementHistory.commit((current) => [...current, placement]);
    setSelectedSignatureAssetId(asset.id);
    setSelectedSignaturePlacementId(placement.id);
  };

  const changeSignaturePlacement = (next: VisualSignaturePlacement) => {
    signaturePlacementHistory.commit((current) =>
      current.map((placement) =>
        placement.id === next.id && !placement.locked ? next : placement
      )
    );
  };

  const deleteSignaturePlacement = (placementId: string) => {
    const placement = signaturePlacements.find((candidate) => candidate.id === placementId);
    if (!placement || placement.locked) return;
    signaturePlacementHistory.commit((current) =>
      current.filter((candidate) => candidate.id !== placementId)
    );
    setSelectedSignaturePlacementId(null);
  };

  const duplicateSignaturePlacement = (
    placementId: string,
    pageAspect = currentVisualPageAspect()
  ) => {
    if (signaturePlacements.length >= MAX_VISUAL_SIGNATURE_PLACEMENTS) return;
    const placement = signaturePlacements.find((candidate) => candidate.id === placementId);
    const asset = placement
      ? signatureAssets.find((candidate) => candidate.id === placement.assetId)
      : null;
    if (!placement || !asset) return;
    const duplicate = duplicateVisualSignaturePlacement(
      placement,
      createVisualSignatureId("placement"),
      asset,
      pageAspect
    );
    signaturePlacementHistory.commit((current) => [...current, duplicate]);
    setSelectedSignaturePlacementId(duplicate.id);
    setSelectedSignatureAssetId(asset.id);
  };

  const lockSignaturePlacement = (placementId: string, locked: boolean) => {
    signaturePlacementHistory.commit((current) =>
      current.map((placement) =>
        placement.id === placementId ? { ...placement, locked } : placement
      )
    );
  };

  const resizeSignaturePlacement = (placementId: string, widthRatio: number) => {
    const placement = signaturePlacements.find((candidate) => candidate.id === placementId);
    const asset = placement
      ? signatureAssets.find((candidate) => candidate.id === placement.assetId)
      : null;
    if (!placement || !asset || placement.locked) return;
    changeSignaturePlacement(
      resizeVisualSignaturePlacement(
        placement,
        asset,
        widthRatio,
        currentVisualPageAspect()
      )
    );
  };

  const rotateSignaturePlacement = (placementId: string, rotationDegrees: number) => {
    const placement = signaturePlacements.find((candidate) => candidate.id === placementId);
    const asset = placement
      ? signatureAssets.find((candidate) => candidate.id === placement.assetId)
      : null;
    if (!placement || !asset || placement.locked) return;
    changeSignaturePlacement(
      rotateVisualSignaturePlacement(
        placement,
        asset,
        rotationDegrees,
        currentVisualPageAspect()
      )
    );
  };

  const exportCurrentPdf = async () => {
    if (!canExportPdf || !activeDocument?.sourcePath || !activePdfRangeSource) {
      return;
    }

    setExportDialogBusy(true);
    setExportCancelBusy(false);
    setOperationStatus(null);

    try {
      const destination =
        takeE2eSaveSelection() ??
        (await save({
          defaultPath: suggestedExportPath(activeDocument.sourcePath, signaturePlaced),
          filters: [{ name: t("app.dialog.export.filter"), extensions: ["pdf"] }],
          title: signaturePlaced
            ? t("app.dialog.export.signedTitle")
            : t("app.dialog.export.organisedTitle")
        }));

      if (!destination) {
        return;
      }

      const visualSignatures = visualSignatureExportPayload(
        signatureAssets,
        signaturePlacements,
        pagePlan.pages.map((page) => page.id)
      );

      const job = await organiseJob.startJob({
        acknowledgePrimaryCertificateSignature: certificateRewriteAcknowledged,
        documentLock: documentLocked
          ? {
              openPassword: documentLockOpenPassword,
              ownerPassword: documentLockOwnerPassword
            }
          : null,
        importedSources: importedPdfSources
          .filter((source) => usedImportedSourceIds.has(source.id))
          .map((source) => ({
            acknowledgeCertificateSignature: source.certificateAcknowledged,
            expectedSourceModifiedAtMs: source.modifiedAtMs,
            expectedSourceSize: source.size,
            id: source.id,
            inputPassword: source.password,
            inputPath: source.path
          })),
        outputPath: destination,
        pages: pagePlan.pages.map((page) =>
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
        primaryInputPassword: pdf.openingPassword,
        primaryInputPath: activeDocument.sourcePath,
        expectedSourceModifiedAtMs: activePdfRangeSource.modifiedAtMs,
        expectedSourceSize: activePdfRangeSource.size,
        signature: null,
        ...visualSignatures
      });
      if (documentLocked) {
        lockedOrganiseJobIdsRef.current.add(job.jobId);
      }
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: t("organise.export.startFailed")
      });
    } finally {
      setExportDialogBusy(false);
    }
  };

  const cancelOrganisedExport = async () => {
    if (!organiseJob.isActive || exportCancelBusy) {
      return;
    }
    setExportCancelBusy(true);
    try {
      await organiseJob.cancelJob();
    } catch (reason) {
      void reason;
      setExportCancelBusy(false);
      setOperationStatus({
        kind: "error",
        text: t("organise.export.cancelFailed")
      });
    }
  };

  function completeScanJob(snapshot: ScanJobSnapshot) {
    setScanCancelBusy(false);
    if (snapshot.status === "succeeded" && snapshot.result) {
      const result = snapshot.result;
      setScanBatchHandoff(
        createVerifiedScanBatchSeed(result.outputPath, scanFromConnectedScanner)
      );
      setScanOutputProtection((current) => createOutputProtectionDraft(current.enabled));
      const cleanUpSummary = [
        result.pagesCropped > 0
          ? t(
              result.pagesCropped === 1
                ? "scan.result.cropped.one"
                : "scan.result.cropped.other",
              { count: formatNumber(result.pagesCropped) }
            )
          : null,
        result.pagesPerspectiveCorrected > 0
          ? t(
              result.pagesPerspectiveCorrected === 1
                ? "scan.result.perspective.one"
                : "scan.result.perspective.other",
              { count: formatNumber(result.pagesPerspectiveCorrected) }
            )
          : null,
        result.pagesShadowCleaned > 0
          ? t(
              result.pagesShadowCleaned === 1
                ? "scan.result.lighting.one"
                : "scan.result.lighting.other",
              { count: formatNumber(result.pagesShadowCleaned) }
            )
          : null
      ].filter((item): item is string => Boolean(item));
      const details = [
        cleanUpSummary.length > 0
          ? t("scan.result.cleanup", {
              items: formatList(cleanUpSummary, { type: "conjunction" })
            })
          : null,
        result.ocrApplied
          ? t("scan.result.searchable", {
              searchable: formatNumber(result.searchableTextPages),
              total: formatNumber(result.pageCount)
            })
          : null,
        result.ocrHintsApplied > 0
          ? t(
              result.ocrHintsApplied === 1
                ? "scan.hints.applied.one"
                : "scan.hints.applied.other",
              { count: formatNumber(result.ocrHintsApplied) }
            )
          : null
      ].filter((item): item is string => Boolean(item));
      setOperationStatus({
        action: "batch-recipe",
        kind: "success",
        text: t(
          result.pageCount === 1
            ? "scan.result.success.one"
            : "scan.result.success.other",
          {
            count: formatNumber(result.pageCount),
            details: details.length > 0 ? ` ${details.join(" ")}` : "",
            encryption: result.encryption === "None" ? t("common.none") : result.encryption,
            fileName: fileNameFromPath(result.outputPath),
            fileSize: formatFileSize(result.bytesWritten, formatNumber),
            kind: result.ocrApplied ? t("scan.result.searchableKind") : ""
          }
        ),
        warnings: localiseScanWarnings(result, t)
      });
    } else if (snapshot.status === "cancelled") {
      setOperationStatus({
        kind: "info",
        text: t("scan.export.cancelled")
      });
    } else {
      setOperationStatus({
        kind: "error",
        text: localisePdfJobFailure(snapshot, t)
      });
    }
  }

  const continueScanInBatchRecipes = () => {
    if (!scanBatchHandoff) {
      return;
    }
    setActiveWorkflowId("batch");
    setOperationStatus({
      kind: "info",
      text: t("scan.handoff")
    });
  };

  const cancelScanPreview = async () => {
    const job = scanPreviewJob.job;
    if (
      !job ||
      (job.status !== "queued" && job.status !== "running") ||
      scanPreviewCancelBusy
    ) {
      return;
    }
    scanPreviewCancellingJobIdRef.current = job.jobId;
    setScanPreviewCancelBusy(true);
    try {
      await scanPreviewJob.cancelJob();
    } catch (reason) {
      if (scanPreviewCancellingJobIdRef.current === job.jobId) {
        scanPreviewCancellingJobIdRef.current = null;
        setScanPreviewCancelBusy(false);
      }
      setScanPreviewError(t("scan.preview.error.cancel"));
    }
  };

  const retryScanPreview = () => {
    if (scanPreviewBusy) {
      return;
    }
    scanPreviewFailedConfigurationRef.current = null;
    scanPreviewPresentedJobIdRef.current = null;
    scanPreviewJobConfigurationRef.current = null;
    setScanPreview(null);
    setScanPreviewError(null);
    scanPreviewJob.clearJob();
    setScanPreviewRetryToken((current) => current + 1);
  };

  const reviewSelectedScanOcr = async () => {
    const sourcePath = selectedScanImage?.path;
    if (
      mode !== "desktop" ||
      !sourcePath ||
      !scanPreview ||
      !ocrReviewAvailable ||
      scanOperationBusy
    ) {
      return;
    }
    setOcrReviewOpen(true);
    setOcrReviewBusy(true);
    setOcrReviewCancelBusy(false);
    setOcrReviewError(null);
    setOcrReviewNotice(null);
    setOcrReviewResult(null);
    ocrReviewJob.clearJob();
    try {
      await ocrReviewJob.startJob({
        autoCrop: scanAutoCrop,
        autoOrient: true,
        colourMode: scanColourMode,
        correctPerspective: scanCorrectPerspective,
        inputPath: sourcePath,
        language: selectedOcrLanguage,
        removeShadows: scanRemoveShadows
      });
    } catch (reason) {
      setOcrReviewError(t("job.error.ocrReviewFailed"));
    } finally {
      setOcrReviewBusy(false);
    }
  };

  const closeOcrReview = () => {
    if (ocrReviewJob.isActive) {
      return;
    }
    setOcrReviewOpen(false);
    setOcrReviewBusy(false);
    setOcrReviewCancelBusy(false);
    setOcrReviewNotice(null);
    ocrReviewJob.clearJob();
  };

  const cancelOcrReview = async () => {
    if (!ocrReviewJob.isActive || ocrReviewCancelBusy) {
      return;
    }
    setOcrReviewCancelBusy(true);
    try {
      await ocrReviewJob.cancelJob();
    } catch (reason) {
      setOcrReviewCancelBusy(false);
      void reason;
      setOcrReviewError(t("ocr.error.cancel"));
    }
  };

  const applyOcrWordHints = (words: string[]) => {
    const normalised = words.map((word) => word.trim()).filter(Boolean);
    setOcrWordHints((current) => Array.from(new Set([...current, ...normalised])).slice(0, 250));
    setOcrReviewOpen(false);
    setOcrReviewNotice(null);
    ocrReviewJob.clearJob();
    setOperationStatus({
      kind: "info",
      text: t(
        normalised.length === 1 ? "scan.hints.saved.one" : "scan.hints.saved.other",
        { count: formatNumber(normalised.length) }
      )
    });
  };

  const createScanPdf = async () => {
    if (!canCreateScanPdf || !selectedPaper) {
      return;
    }

    setScanExportStarting(true);
    setOperationStatus(null);

    try {
      const destination = await save({
        defaultPath: suggestedScanPath(scanSourcePaths[0]),
        filters: [{ name: t("ocr.filter.pdfDocuments"), extensions: ["pdf"] }],
        title:
          recogniseText && ocrAvailable
            ? t("scan.dialog.createSearchable")
            : t("scan.dialog.create")
      });
      if (!destination) {
        return;
      }

      setScanBatchHandoff(null);
      scanExportJob.clearJob();
      scanExportTerminalPresentedJobIdRef.current = null;
      await scanExportJob.startJob({
        autoOrient: true,
        autoCrop: scanAutoCrop,
        colourMode: scanColourMode,
        correctPerspective: scanCorrectPerspective,
        dpi: scanDpi,
        inputPaths: scanSourcePaths,
        jpegQuality: scanJpegQuality,
        marginPt: scanMarginPt,
        ocrLanguage: selectedOcrLanguage,
        ocrUserWords: ocrWordHints,
        outputPath: destination,
        outputProtection: toPdfOutputProtection(scanOutputProtection, qpdfAvailable),
        paperHeightPt: millimetresToPoints(selectedPaper.heightMm),
        paperWidthPt: millimetresToPoints(selectedPaper.widthMm),
        recogniseText,
        removeShadows: scanRemoveShadows,
        straighten: straightenScan
      });
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: t("scan.export.failed")
      });
    } finally {
      setScanExportStarting(false);
    }
  };

  const cancelScanPdf = async () => {
    if (!scanExportJob.isActive || scanCancelBusy) {
      return;
    }
    setScanCancelBusy(true);
    try {
      await scanExportJob.cancelJob();
    } catch (reason) {
      void reason;
      setOperationStatus({
        kind: "error",
        text: t("scan.export.cancelFailed")
      });
      setScanCancelBusy(false);
    }
  };

  const previewStyle = {
    "--preview-scale": `${zoom / 100}`,
    "--paper-ratio": selectedPaper
      ? `${selectedPaper.widthMm} / ${selectedPaper.heightMm}`
      : "210 / 297"
  } as CSSProperties;

  const acceptedFileTypes = [
    "application/pdf",
    "image/*",
    ".avif",
    ".bmp",
    ".gif",
    ".heic",
    ".heif",
    ".pbm",
    ".pgm",
    ".pnm",
    ".ppm",
    ".tif",
    ".tiff",
    ".webp"
  ].join(",");

  return (
    <main className="app-shell">
      <a
        className="skip-link"
        href="#document-editor"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById("document-editor")?.focus();
        }}
      >
        {t("app.skipToEditor")}
      </a>
      <header className="app-header">
        <div className="brand-block">
          <div className="brand-mark" aria-hidden="true">
            <FileText size={23} />
          </div>
          <div>
            <span className="eyebrow">{t("app.brand.eyebrow")}</span>
            <h1>Tüfekci Paperworks</h1>
          </div>
        </div>

        <div className="top-actions">
          <button
            className="primary"
            disabled={openingLocalFiles || scanOperationBusy}
            onClick={chooseDocuments}
            type="button"
          >
            {openingLocalFiles ? (
              <Loader2 className="spin" size={17} aria-hidden="true" />
            ) : (
              <FolderOpen size={17} aria-hidden="true" />
            )}
            {openingLocalFiles ? t("app.open.opening") : t("app.open.label")}
          </button>
          <input
            accept={acceptedFileTypes}
            aria-hidden="true"
            className="visually-hidden"
            multiple
            onChange={handleOpenDocument}
            ref={fileInputRef}
            tabIndex={-1}
            type="file"
          />
          <button
            onClick={() => setUpdateDialogOpen(true)}
            title={t("app.updates.title")}
            type="button"
          >
            <RefreshCw size={17} aria-hidden="true" />
            {t("app.updates.label")}
          </button>
          <button
            disabled={mode !== "desktop"}
            onClick={() => setOperationAuditOpen(true)}
            title={t("app.activity.title")}
            type="button"
          >
            <History size={17} aria-hidden="true" />
            {t("app.activity.label")}
          </button>
          <label className="locale-picker">
            <Languages size={17} aria-hidden="true" />
            <span className="visually-hidden">{t("locale.selector.label")}</span>
            <select
              aria-label={t("locale.selector.label")}
              onChange={(event) => setLocale(event.target.value as SupportedLocale)}
              value={locale}
            >
              {SUPPORTED_LOCALES.map((supportedLocale) => (
                <option key={supportedLocale} value={supportedLocale}>
                  {t(localeLabelKeys[supportedLocale])}
                </option>
              ))}
            </select>
          </label>
          <button
            disabled={mode !== "desktop" || !recoveryDraft || recoverySaveState === "saving"}
            onClick={() => void saveRecoveryDraft(true)}
            title={
              recoveryBlockedByImportedPassword
                ? t("app.draft.blockedTitle")
                : recoverySaveState === "error"
                ? t("app.draft.errorTitle")
                : recoveryLastSavedAt
                  ? t("app.draft.lastTitle", {
                      time: formatDate(recoveryLastSavedAt, {
                        dateStyle: "medium",
                        timeStyle: "short"
                      })
                    })
                  : t("app.draft.title")
            }
            type="button"
          >
            {recoverySaveState === "saving" ? (
              <Loader2 className="spin" size={17} aria-hidden="true" />
            ) : (
              <Save size={17} aria-hidden="true" />
            )}
            {recoverySaveState === "saving"
              ? t("app.draft.saving")
              : recoverySaveState === "saved"
                ? t("app.draft.saved")
                : t("app.draft.save")}
          </button>
          <button
            disabled={!canExportPdf}
            onClick={exportCurrentPdf}
            title={describeExportAvailability({
              activeDocument,
              certificateRewriteAcknowledged,
              certificateRiskAcknowledgementRequired: Boolean(
                editSafety?.certificateSignature || editSafetyUnavailable
              ),
              desktopMode: mode === "desktop",
              documentLockPasswordsValid,
              documentLocked,
              editSafetyPending,
              exportBusy,
              qpdfAvailable,
              signaturePlaced,
              t
            })}
            type="button"
          >
            {exportBusy ? (
              <Loader2 className="spin" size={17} aria-hidden="true" />
            ) : (
              <Download size={17} aria-hidden="true" />
            )}
            {organiseJob.isActive
              ? t("app.export.exporting")
              : exportDialogBusy
                ? t("app.export.choosing")
                : t("app.export.label")}
          </button>
        </div>
      </header>

      {recoveryCandidate ? (
        <section className="recovery-banner" aria-live="polite">
          <div className="recovery-icon">
            <History size={20} aria-hidden="true" />
          </div>
          <div className="recovery-copy">
            <strong>{t("app.recovery.title")}</strong>
            <span>
              {recoveryCandidate.document.kind === "merge"
                ? t(
                    recoveryCandidate.document.sources.length === 1
                      ? "app.recovery.name.merge.one"
                      : "app.recovery.name.merge.other",
                    { count: formatNumber(recoveryCandidate.document.sources.length) }
                  )
                : recoveryCandidate.document.kind === "split"
                  ? t("app.recovery.name.split", {
                      name: fileNameFromPath(recoveryCandidate.document.sourcePath)
                    })
                  : recoveryDocumentName(recoveryCandidate)} |{" "}
              {t("app.recovery.saved", {
                time: formatDate(recoveryCandidate.savedAtUnixMs, {
                  dateStyle: "medium",
                  timeStyle: "short"
                })
              })}
            </span>
            <small>{t("app.recovery.privacy")}</small>
          </div>
          <div className="recovery-actions">
            <button disabled={recoveryBusy} onClick={() => void discardRecovery()} type="button">
              {t("app.recovery.discard")}
            </button>
            <button
              className="primary"
              disabled={recoveryBusy}
              onClick={() => void restoreRecovery()}
              type="button"
            >
              {recoveryBusy ? <Loader2 className="spin" size={16} aria-hidden="true" /> : <History size={16} aria-hidden="true" />}
              {recoveryBusy ? t("app.recovery.restoring") : t("app.recovery.continue")}
            </button>
          </div>
        </section>
      ) : null}

      {organiseJob.job && organiseJob.job.status !== "succeeded" ? (
        <div className="organise-job-banner">
          <PdfJobProgress
            cancelling={exportCancelBusy}
            connectionError={organiseJob.connectionError}
            job={organiseJob.job}
            onCancel={cancelOrganisedExport}
            onRetry={() => void exportCurrentPdf()}
            retryDisabled={!canExportPdf}
          />
        </div>
      ) : null}

      {!organiseJob.isActive && organiseJob.connectionError ? (
        <section className="operation-banner is-info" role="status">
          <Info size={18} aria-hidden="true" />
          <div>
            <strong>{t("app.operation.statusUnavailable")}</strong>
            <span>{t("app.operation.statusUnavailableDetail")}</span>
          </div>
        </section>
      ) : null}

      {operationStatus ? (
        <section className={`operation-banner is-${operationStatus.kind}`} role="status">
          {operationStatus.kind === "success" ? (
            <CheckCircle2 size={18} aria-hidden="true" />
          ) : operationStatus.kind === "info" ? (
            <Info size={18} aria-hidden="true" />
          ) : (
            <AlertCircle size={18} aria-hidden="true" />
          )}
          <div>
            <strong>
              {operationStatus.kind === "success"
                ? t("app.operation.completed")
                : operationStatus.kind === "info"
                  ? t("app.operation.stopped")
                  : t("app.operation.failed")}
            </strong>
            <span>{operationStatus.text}</span>
            {operationStatus.warnings?.map((warning) => (
              <small key={warning}>{warning}</small>
            ))}
          </div>
          <div className="operation-banner-actions">
            {operationStatus.action === "batch-recipe" && scanBatchHandoff ? (
              <button onClick={continueScanInBatchRecipes} type="button">
                <ListChecks size={16} aria-hidden="true" />
                {t("workflow.batch.title")}
              </button>
            ) : null}
            <button
              aria-label={t("common.dismissMessage")}
              className="icon-button"
              onClick={() => setOperationStatus(null)}
              type="button"
            >
              <X size={16} aria-hidden="true" />
            </button>
          </div>
        </section>
      ) : null}

      {scanBusy ? (
        <section className="scan-job-banner" aria-live="polite">
          <Loader2 className="spin" size={18} aria-hidden="true" />
          <div>
            <strong>{t("scan.job.title")}</strong>
            <span>
              {scanExportStarting
                ? t("scan.job.preparing")
                : scanJob
                  ? localisePdfJobStage(scanJob, t)
                  : t("scan.job.starting")}
            </span>
            <div className="scan-job-progress">
              <progress
                aria-label={t("scan.job.progressAria")}
                max="100"
                value={
                  scanExportStarting
                    ? 0
                    : Math.max(0, Math.min(100, scanJob?.progress ?? 0))
                }
              />
              <small>
                {formatNumber(
                  scanExportStarting
                    ? 0
                    : Math.max(0, Math.min(100, scanJob?.progress ?? 0))
                )}%
              </small>
            </div>
            {!scanExportStarting && scanExportJob.connectionError ? (
              <small className="pdf-job-connection-error">
                {t("job.connectionError")}
              </small>
            ) : null}
          </div>
          {scanExportJob.isActive ? (
            <button
              className="ghost"
              disabled={scanCancelBusy}
              onClick={() => void cancelScanPdf()}
              type="button"
            >
              {scanCancelBusy ? (
                <Loader2 className="spin" size={15} aria-hidden="true" />
              ) : (
                <X size={15} aria-hidden="true" />
              )}
              {scanCancelBusy ? t("common.cancelling") : t("common.cancel")}
            </button>
          ) : null}
        </section>
      ) : null}

      {!scanBusy &&
      scanJob &&
      (scanJob.status === "failed" || scanJob.status === "cancelled") ? (
        <div className="organise-job-banner">
          <PdfJobProgress
            cancelling={scanCancelBusy}
            connectionError={scanExportJob.connectionError}
            job={scanJob}
            onCancel={() => void cancelScanPdf()}
            onRetry={() => void createScanPdf()}
            retryDisabled={!canCreateScanPdf}
          />
        </div>
      ) : null}

      {activeEditSafety.job && activeEditSafety.job.status !== "succeeded" ? (
        <div className="organise-job-banner">
          <PdfJobProgress
            cancelling={activeEditSafety.cancelling}
            connectionError={activeEditSafety.connectionError}
            job={activeEditSafety.job}
            onCancel={() => void activeEditSafety.cancelJob()}
            onRetry={activeEditSafety.retry}
            retryDisabled={exportBusy}
          />
        </div>
      ) : editSafetyPending ? (
        <section className="document-safety-banner is-checking" aria-live="polite">
          <Loader2 className="spin" size={18} aria-hidden="true" />
          <div>
            <strong>{t("safety.checking.title")}</strong>
            <span>{t("safety.checking.description")}</span>
          </div>
        </section>
      ) : editSafety?.certificateSignature || editSafetyUnavailable ? (
        <section
          className={`document-safety-banner ${editSafety?.certificateSignature ? "is-signature" : "is-unavailable"}`}
          role="alert"
        >
          {editSafety?.certificateSignature ? (
            <ShieldAlert size={19} aria-hidden="true" />
          ) : (
            <AlertCircle size={19} aria-hidden="true" />
          )}
          <div>
            <strong>
              {editSafety?.certificateSignature
                ? t("safety.organiser.signatureTitle")
                : t("safety.organiser.unavailableTitle")}
            </strong>
            <span>
              {editSafety?.certificateSignature
                ? t("safety.organiser.signatureDescription")
                : t("safety.organiser.unavailableDescription")}
            </span>
            {editSafety?.certificateSignature && (editSafety.formFields || editSafety.xfa) ? (
              <small>{t("safety.organiser.formWarning")}</small>
            ) : null}
            {editSafety?.certificateSignature || editSafetyUnavailable ? (
              <label className="document-safety-ack">
                <input
                  checked={certificateRewriteAcknowledged}
                  onChange={(event) =>
                    setCertificateRewriteAcknowledged(event.target.checked)
                  }
                  type="checkbox"
                />
                {editSafety?.certificateSignature
                  ? t("safety.organiser.signatureAcknowledgement")
                  : t("safety.organiser.unavailableAcknowledgement")}
              </label>
            ) : null}
          </div>
          <button
            onClick={() =>
              editSafetyUnavailable
                ? activeEditSafety.retry()
                : setActiveWorkflowId("health")
            }
            type="button"
          >
            {editSafetyUnavailable ? (
              <RefreshCw size={16} aria-hidden="true" />
            ) : (
              <FileSearch size={16} aria-hidden="true" />
            )}
            {editSafetyUnavailable
              ? t("safety.organiser.retry")
              : t("safety.organiser.reviewHealth")}
          </button>
        </section>
      ) : null}

      <section className="workspace-grid">
        <nav className="workflow-panel" aria-label={t("app.workflow.navigation")}>
          <div className="panel-heading">
            <span>{t("app.workflow.title")}</span>
          </div>

          <div
            aria-label={t("app.workflow.choose")}
            aria-orientation="vertical"
            className="workflow-list"
            role="tablist"
          >
            {workflows.map((workflow) => {
              const Icon = workflow.icon;
              const active = workflow.id === activeWorkflowId;

              return (
                <button
                  aria-controls="workflow-details"
                  aria-label={workflow.title}
                  aria-selected={active}
                  className={active ? "workflow-item is-active" : "workflow-item"}
                  id={`workflow-tab-${workflow.id}`}
                  key={workflow.id}
                  onClick={() => setActiveWorkflowId(workflow.id)}
                  onKeyDown={(event) => handleWorkflowKeyDown(event, workflow.id)}
                  ref={(element) => {
                    if (element) {
                      workflowButtonRefs.current.set(workflow.id, element);
                    } else {
                      workflowButtonRefs.current.delete(workflow.id);
                    }
                  }}
                  role="tab"
                  tabIndex={active ? 0 : -1}
                  type="button"
                >
                  <span className="workflow-icon">
                    <Icon size={18} aria-hidden="true" />
                  </span>
                  <span>
                    <strong>{workflow.title}</strong>
                    <small>{workflow.description}</small>
                  </span>
                  <em>
                    {workflow.id === "protect" && qpdfAvailable
                      ? t("app.engine.ready")
                      : workflow.id === "archive" && archiveReadiness?.ready
                        ? t("app.engine.ready")
                        : workflow.stage}
                  </em>
                </button>
              );
            })}
          </div>

          <div className="system-card">
            <div className="system-title">
              <Wrench size={17} aria-hidden="true" />
              <span>{engineSummary}</span>
            </div>
            {loading ? (
              <p>{t("app.engine.checking")}</p>
            ) : mobileMode ? (
              <div className="engine-list">
                <div className="engine-row">
                  <span>{t("app.engine.mobileCore")}</span>
                  <strong className="ready">{t("app.engine.ready")}</strong>
                </div>
                <div className="engine-row">
                  <span>{t("app.engine.desktopServices")}</span>
                  <strong className="missing">{t("app.engine.notIncluded")}</strong>
                </div>
                <p className="engine-mobile-detail">{t("app.engine.mobileDetail")}</p>
              </div>
            ) : mode === "desktop" ? (
              <div className="engine-list">
                {tools.map((tool) => (
                  <div className="engine-row" key={tool.command}>
                    <span>{tool.name}</span>
                    <strong className={tool.available ? "ready" : "missing"}>
                      {tool.available ? t("app.engine.ready") : t("app.engine.missing")}
                    </strong>
                  </div>
                ))}
              </div>
            ) : (
              <p>
                {backendUnavailable
                  ? t("app.engine.browserError")
                  : t("app.engine.browserHint")}
              </p>
            )}
          </div>
        </nav>

        <section
          aria-label={t("app.document.editorAria")}
          className="document-area"
          id="document-editor"
          tabIndex={-1}
        >
          <div className="editor-controls">
            <div className="document-toolbar">
            <div className="tool-group" aria-label={t("app.editor.toolsAria")}>
              {editorTools.map((tool) => {
                const Icon = tool.icon;
                const active = tool.id === activeToolId;
                const pageOperationReady =
                  Boolean(pdf.document && selectedPlannedPage) &&
                  (tool.id === "rotate" || tool.id === "delete");
                const implemented = tool.implemented || pageOperationReady;
                const disabled =
                  !implemented ||
                  (pageOperationReady && editSafetyPending) ||
                  (tool.id === "delete" &&
                    effectiveSelectedPageIds.length >= pagePlan.pages.length);

                return (
                  <button
                    className={active ? "tool-button is-active" : "tool-button"}
                    disabled={disabled}
                    key={tool.id}
                    onClick={() => handleEditorTool(tool.id)}
                    type="button"
                    title={
                      implemented
                        ? tool.label
                        : t("app.editor.requiresPage", { tool: tool.label })
                    }
                  >
                    <Icon size={17} aria-hidden="true" />
                    <span>{tool.label}</span>
                  </button>
                );
              })}
            </div>

            <div className="view-actions">
              <button
                aria-label={t("print.title")}
                className={printWorkflowActive ? "icon-button is-active" : "icon-button"}
                disabled={!pdf.document}
                onClick={() => setActiveWorkflowId("print")}
                title={t("print.toolbarTitle")}
                type="button"
              >
                <Printer size={17} aria-hidden="true" />
              </button>
              <button
                className={searchOpen ? "icon-button is-active" : "icon-button"}
                disabled={!pdf.document}
                onClick={() => setSearchOpen((current) => !current)}
                type="button"
                aria-label={t("search.document")}
                title={t("search.document")}
              >
                <Search size={17} aria-hidden="true" />
              </button>
              <button
                className="icon-button"
                disabled={!pagePlan.canUndo || editSafetyPending}
                onClick={undoPageOperation}
                type="button"
                aria-label={t("app.editor.undoPageAria")}
              >
                <Undo2 size={17} aria-hidden="true" />
              </button>
              <button
                className="icon-button"
                disabled={!pagePlan.canRedo || editSafetyPending}
                onClick={redoPageOperation}
                type="button"
                aria-label={t("app.editor.redoPageAria")}
              >
                <Redo2 size={17} aria-hidden="true" />
              </button>
              <button
                className="icon-button"
                onClick={() => setZoom((value) => Math.max(60, value - 10))}
                type="button"
                aria-label={t("app.editor.zoomOutAria")}
              >
                <ZoomOut size={17} aria-hidden="true" />
              </button>
              <span className="zoom-value">{zoom}%</span>
              <button
                className="icon-button"
                onClick={() => setZoom((value) => Math.min(140, value + 10))}
                type="button"
                aria-label={t("app.editor.zoomInAria")}
              >
                <ZoomIn size={17} aria-hidden="true" />
              </button>
            </div>
            </div>

            {searchOpen && pdf.document ? (
              <div className="document-search" role="search">
                <label>
                  <Search size={16} aria-hidden="true" />
                  <input
                    aria-label={t("search.input")}
                    autoFocus
                    onChange={(event) => {
                      setSearchQuery(event.target.value);
                      setSearchResultIndex(0);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        navigateSearch(event.shiftKey ? -1 : 1);
                      } else if (event.key === "Escape") {
                        closeSearch();
                      }
                    }}
                    placeholder={t("search.placeholder")}
                    type="search"
                    value={searchQuery}
                  />
                </label>
                <span aria-atomic="true" className="search-status" aria-live="polite">
                  {describeSearchStatus(
                    searchQuery,
                    visiblePdfSearch,
                    currentSearchResultIndex,
                    formatNumber,
                    t
                  )}
                </span>
                <div className="search-actions">
                  <button
                    className="icon-button"
                    disabled={visiblePdfSearch.matches.length === 0}
                    onClick={() => navigateSearch(-1)}
                    type="button"
                    aria-label={t("search.previous")}
                    title={t("search.previous")}
                  >
                    <ChevronUp size={16} aria-hidden="true" />
                  </button>
                  <button
                    className="icon-button"
                    disabled={visiblePdfSearch.matches.length === 0}
                    onClick={() => navigateSearch(1)}
                    type="button"
                    aria-label={t("search.next")}
                    title={t("search.next")}
                  >
                    <ChevronDown size={16} aria-hidden="true" />
                  </button>
                  <button
                    className="icon-button"
                    onClick={closeSearch}
                    type="button"
                    aria-label={t("search.close")}
                    title={t("search.close")}
                  >
                    <X size={16} aria-hidden="true" />
                  </button>
                </div>
              </div>
            ) : null}
          </div>

          <div className="main-stage">
            <div
              className={displayedPageCount > 0 ? "page-strip" : "page-strip is-empty"}
              aria-label={t("app.pages.thumbnailsAria")}
            >
              <div className="strip-heading">
                <span className="strip-title">
                  <span>{t("app.pages.title")}</span>
                  {isPdfDocument && effectiveSelectedPageIds.length > 0 ? (
                    <small aria-live="polite">
                      {effectiveSelectedPageIds.length === 1
                        ? t("organise.selection.one")
                        : t("organise.selection.other", {
                            count: formatNumber(effectiveSelectedPageIds.length)
                          })}
                    </small>
                  ) : null}
                </span>
                <span className="strip-actions">
                  {isPdfDocument && pagePlan.pages.length > 1 ? (
                    <button
                      aria-label={
                        effectiveSelectedPageIds.length === pagePlan.pages.length
                          ? t("organise.selection.onlyActive")
                          : t("organise.selection.selectAll")
                      }
                      aria-pressed={effectiveSelectedPageIds.length === pagePlan.pages.length}
                      className="icon-button"
                      disabled={editSafetyPending}
                      onClick={toggleAllPageSelection}
                      title={
                        effectiveSelectedPageIds.length === pagePlan.pages.length
                          ? t("organise.selection.onlyActive")
                          : t("organise.selection.selectAll")
                      }
                      type="button"
                    >
                      <ListChecks size={16} aria-hidden="true" />
                    </button>
                  ) : null}
                  <button
                    className="icon-button"
                    disabled={!pdf.document || editSafetyPending}
                    onClick={insertBlankPage}
                    type="button"
                    aria-label={t("app.pages.addAria")}
                    title={t("app.pages.addTitle")}
                  >
                    <Plus size={16} aria-hidden="true" />
                  </button>
                </span>
              </div>

              {activeDocument && displayedPageCount > 0 ? (
                <div className="thumbnail-list">
                      {isPdfDocument && pdfDocument
                    ? pagePlan.pages.map((plannedPage, index) => {
                        const pageNumber = index + 1;
                        const importedSource =
                          plannedPage.kind === "source" && plannedPage.sourceId !== "primary"
                            ? importedPdfSourceMap.get(plannedPage.sourceId)
                            : undefined;
                        const pageDocument =
                          plannedPage.kind === "source"
                            ? plannedPage.sourceId === "primary"
                              ? pdfDocument
                              : importedSource?.document
                            : null;
                        const classes = [
                          "thumbnail",
                          selectedPageIdSet.has(plannedPage.id) ? "is-selected" : "",
                          pageNumber === selectedPage ? "is-active" : "",
                          draggedPageId && selectedPageIdSet.has(plannedPage.id)
                            ? "is-dragging"
                            : "",
                          plannedPage.id === dropTargetPageId &&
                          !selectedPageIdSet.has(plannedPage.id)
                            ? "is-drop-target"
                            : ""
                        ]
                          .filter(Boolean)
                          .join(" ");

                        return (
                          <div className="thumbnail-entry" key={plannedPage.id}>
                            <button
                              aria-label={
                                plannedPage.kind === "source"
                                  ? t("app.pages.sourceThumbnailAria", {
                                      page: formatNumber(pageNumber),
                                      source: formatNumber(plannedPage.sourcePage)
                                    })
                                  : t("app.pages.blankThumbnailAria", {
                                      page: formatNumber(pageNumber)
                                    })
                              }
                              aria-pressed={selectedPageIdSet.has(plannedPage.id)}
                              className={classes}
                              data-page-rotation={plannedPage.rotation}
                              draggable={!editSafetyPending}
                              onClick={(event) =>
                                selectPlannedPage(
                                  pageNumber,
                                  pageSelectionModeFromModifiers(event)
                                )
                              }
                              onDragEnd={() => {
                                setDraggedPageId(null);
                                setDropTargetPageId(null);
                              }}
                              onDragEnter={() => setDropTargetPageId(plannedPage.id)}
                              onDragOver={(event) => {
                                event.preventDefault();
                                event.dataTransfer.dropEffect = "move";
                              }}
                              onDragStart={(event) => {
                                const dragSelection = selectedPageIdSet.has(plannedPage.id)
                                  ? effectiveSelectedPageIds
                                  : [plannedPage.id];
                                if (!selectedPageIdSet.has(plannedPage.id)) {
                                  selectPlannedPage(pageNumber, "single");
                                }
                                setDraggedPageId(plannedPage.id);
                                event.dataTransfer.effectAllowed = "move";
                                event.dataTransfer.setData("text/plain", plannedPage.id);
                                event.dataTransfer.setData(
                                  "application/x-tufekci-paperworks-pages",
                                  JSON.stringify(dragSelection)
                                );
                              }}
                              onDrop={(event) => {
                                event.preventDefault();
                                event.stopPropagation();
                                dropPlannedPage(plannedPage.id);
                              }}
                              onKeyDown={(event) => {
                                if (
                                  (event.ctrlKey || event.metaKey) &&
                                  event.key.toLocaleLowerCase("en-GB") === "a"
                                ) {
                                  event.preventDefault();
                                  setSelectedPageIds(pagePlan.pages.map((page) => page.id));
                                  setSelectionAnchorId(plannedPage.id);
                                } else if (
                                  event.key === "Escape" &&
                                  effectiveSelectedPageIds.length > 1
                                ) {
                                  event.preventDefault();
                                  selectPlannedPage(pageNumber, "single");
                                } else if (
                                  event.key === " " &&
                                  (event.ctrlKey || event.metaKey || event.shiftKey)
                                ) {
                                  event.preventDefault();
                                  selectPlannedPage(
                                    pageNumber,
                                    event.shiftKey
                                      ? event.ctrlKey || event.metaKey
                                        ? "extend-range"
                                        : "range"
                                      : "toggle"
                                  );
                                }
                              }}
                              title={t("organise.selection.modifierHint")}
                              type="button"
                            >
                              {plannedPage.kind === "source" && pageDocument ? (
                                <LazyPdfThumbnail
                                  document={pageDocument}
                                  pageNumber={plannedPage.sourcePage}
                                  rotation={plannedPage.rotation}
                                />
                              ) : plannedPage.kind === "source" ? (
                                <span className="thumbnail-sheet is-unavailable">
                                  <AlertCircle size={18} aria-hidden="true" />
                                </span>
                              ) : (
                                <span
                                  className="thumbnail-sheet is-blank"
                                  aria-label={t("app.pages.blankAria")}
                                >
                                  <Plus size={18} aria-hidden="true" />
                                </span>
                              )}
                              {importedSource ? (
                                <span className="thumbnail-source" title={importedSource.name}>
                                  {t("app.pages.imported")}
                                </span>
                              ) : null}
                              <strong>{pageNumber}</strong>
                            </button>
                            <button
                              aria-label={t(
                                selectedPageIdSet.has(plannedPage.id)
                                  ? "organise.selection.removePage"
                                  : "organise.selection.addPage",
                                { page: formatNumber(pageNumber) }
                              )}
                              aria-pressed={selectedPageIdSet.has(plannedPage.id)}
                              className="thumbnail-select-toggle"
                              disabled={editSafetyPending}
                              onClick={() => selectPlannedPage(pageNumber, "toggle")}
                              title={t(
                                selectedPageIdSet.has(plannedPage.id)
                                  ? "organise.selection.removePage"
                                  : "organise.selection.addPage",
                                { page: formatNumber(pageNumber) }
                              )}
                              type="button"
                            >
                              {selectedPageIdSet.has(plannedPage.id) ? (
                                <CheckCircle2 size={15} aria-hidden="true" />
                              ) : (
                                <Plus size={15} aria-hidden="true" />
                              )}
                            </button>
                          </div>
                        );
                      })
                    : pageSlots.map((page) => (
                        <button
                          aria-label={t("app.pages.scanThumbnailAria", {
                            name:
                              scanImages[page - 1]?.name ??
                              t("app.scan.imageFallback", { page: formatNumber(page) }),
                            page: formatNumber(page)
                          })}
                          className={page === selectedPage ? "thumbnail is-selected" : "thumbnail"}
                          key={page}
                          onClick={() => setSelectedPage(page)}
                          type="button"
                        >
                          {scanImages[page - 1] ? (
                            <span className="thumbnail-photo">
                              <img src={scanImages[page - 1].url} alt="" />
                            </span>
                          ) : (
                            <span className="thumbnail-sheet">
                              <span />
                              <span />
                              <span />
                            </span>
                          )}
                          <strong>{page}</strong>
                        </button>
                      ))}
                </div>
              ) : activeDocument ? (
                <div className="strip-empty">
                  {pdfError ? (
                    <AlertCircle size={25} aria-hidden="true" />
                  ) : (
                    <Loader2 className="spin" size={25} aria-hidden="true" />
                  )}
                  <span>
                    {pdfError ? t("app.pages.unavailable") : t("app.pages.reading")}
                  </span>
                </div>
              ) : (
                <div className="strip-empty">
                  <Files size={25} aria-hidden="true" />
                  <span>{t("app.pages.none")}</span>
                </div>
              )}
            </div>

            <section
              className={dragActive ? "preview-zone is-dragging" : "preview-zone"}
              onDragEnter={() => setDragActive(true)}
              onDragLeave={() => setDragActive(false)}
              onDragOver={(event) => event.preventDefault()}
              onDrop={handleDrop}
              aria-label={t("app.document.previewAria")}
            >
              {activeDocument ? (
                <>
                  <div className="document-status">
                    <div>
                      <strong>{activeDocument.name}</strong>
                      <span>
                        {activeDocument.kind === "scan"
                          ? t(
                              activeDocument.fileCount === 1
                                ? "app.document.scanSummary.one"
                                : "app.document.scanSummary.other",
                              {
                                count: formatNumber(activeDocument.fileCount),
                                paper: localiseScanPresetName(
                                  selectedPaper.id,
                                  selectedPaper.name,
                                  t
                                ),
                                size: activeDocumentSize
                              }
                            )
                          : pdfBusy
                            ? `${activeDocumentSize} | ${t("app.pages.reading")}`
                            : pdfError
                              ? `${activeDocumentSize} | ${t("app.document.pagesUnavailable")}`
                              : `${activeDocumentSize} | ${t("app.document.pageStatus", {
                                  current: formatNumber(selectedPage),
                                  total: formatNumber(activeDocument.previewPages)
                                })}`}
                      </span>
                    </div>
                    <span className="status-pill">
                      {pdfError ? (
                        <AlertCircle size={15} aria-hidden="true" />
                      ) : pdfBusy ? (
                        <Loader2 className="spin" size={15} aria-hidden="true" />
                      ) : documentLocked ? (
                        <LockKeyhole size={15} aria-hidden="true" />
                      ) : (
                        <CheckCircle2 size={15} aria-hidden="true" />
                      )}
                      {pdfError
                        ? t("app.document.couldNotOpen")
                        : pdfBusy
                          ? t("app.document.opening")
                          : documentLocked
                            ? t("app.document.locked")
                            : t("app.document.ready")}
                    </span>
                  </div>

                  <div className="preview-scroll">
                    {isScanDocument ? (
                      <article className="paper-preview" style={previewStyle}>
                        <header>
                          <span>Tüfekci Paperworks</span>
                          <strong>{displayWorkflow.title}</strong>
                        </header>
                        <div className="scan-photo">
                          {selectedScanImage ? (
                            <img src={selectedScanImage.url} alt={selectedScanImage.name} />
                          ) : (
                            <FileSearch size={38} aria-hidden="true" />
                          )}
                          <span>
                            {selectedScanImage?.name ??
                              t("app.scan.imageFallback", {
                                page: formatNumber(selectedPage)
                              })}
                          </span>
                        </div>
                        <div className="ocr-lines">
                          <span />
                          <span />
                          <span />
                        </div>
                        <footer>
                          {localiseScanPresetName(selectedPaper.id, selectedPaper.name, t)}
                        </footer>
                      </article>
                    ) : pdf.document && selectedPlannedPage ? (
                      <div className="pdf-page-frame">
                        {selectedPlannedPage.kind === "source" && selectedPdfDocument ? (
                          <PdfPageCanvas
                            document={selectedPdfDocument}
                            pageNumber={selectedPlannedPage.sourcePage}
                            rotation={selectedPlannedPage.rotation}
                            scale={zoom / 100}
                            variant="page"
                          />
                        ) : selectedPlannedPage.kind === "source" ? (
                          <div className="pdf-page-unavailable" role="alert">
                            <AlertCircle size={24} aria-hidden="true" />
                            <strong>{t("app.preview.importedUnavailable")}</strong>
                            <span>{t("app.preview.reimport")}</span>
                          </div>
                        ) : (
                          <BlankPdfPage
                            ariaLabel={t("app.pages.blankAria")}
                            heightPt={selectedPlannedPage.heightPt}
                            label={t("app.pages.blankLabel")}
                            rotation={selectedPlannedPage.rotation}
                            widthPt={selectedPlannedPage.widthPt}
                            zoom={zoom / 100}
                          />
                        )}
                        <VisualSignatureLayer
                          assets={signatureAssets}
                          editable={signatureWorkflowActive}
                          onAdd={(assetId, centre, pageAspect) =>
                            placeSignatureAsset(assetId, centre, pageAspect)
                          }
                          onChange={changeSignaturePlacement}
                          onDelete={deleteSignaturePlacement}
                          onDuplicate={duplicateSignaturePlacement}
                          onSelect={setSelectedSignaturePlacementId}
                          pageId={selectedPlannedPage.id}
                          placements={signaturePlacements.filter(
                            (placement) => placement.pageId === selectedPlannedPage.id
                          )}
                          selectedPlacementId={selectedSignaturePlacementId}
                        />
                      </div>
                    ) : (
                      <div className={pdfError ? "pdf-load-state is-error" : "pdf-load-state"}>
                        {pdfError ? (
                          <AlertCircle size={30} aria-hidden="true" />
                        ) : (
                          <Loader2 className="spin" size={30} aria-hidden="true" />
                        )}
                        <strong>
                          {pdfError
                            ? t("app.document.couldNotOpen")
                            : t("app.document.opening")}
                        </strong>
                        <span>
                          {pdfError
                            ? t(pdfOpenErrorTranslationKey(pdfError))
                            : formatPdfProgress(pdf.progress, formatNumber, t) ??
                              t("app.document.readingData")}
                        </span>
                      </div>
                    )}
                  </div>
                </>
              ) : (
                <div className="drop-zone">
                  <div className="drop-icon" aria-hidden="true">
                    <UploadCloud size={34} />
                  </div>
                  <h2>{t("app.drop.title")}</h2>
                  <p>{t("app.drop.description")}</p>
                  <button
                    className="primary"
                    disabled={scanOperationBusy}
                    onClick={chooseDocuments}
                    type="button"
                  >
                    {t("app.drop.browse")}
                  </button>
                </div>
              )}
            </section>
          </div>
        </section>

        <section
          aria-labelledby={`workflow-tab-${activeWorkflow.id}`}
          className="inspector-panel"
          id="workflow-details"
          role="tabpanel"
          tabIndex={0}
        >
          <div className="panel-heading">
            <span>{t("app.nextAction")}</span>
            <span className="small-pill">{displayWorkflowStage}</span>
          </div>

          <section className="active-workflow">
            <div className="active-icon">
              <ActiveWorkflowIcon size={24} aria-hidden="true" />
            </div>
            <h2>{displayWorkflow.title}</h2>
            <p>{displayWorkflow.description}</p>
            {scanWorkflowActive && !activeDocument ? (
              <button
                className="primary wide-button"
                disabled={scanOperationBusy}
                onClick={chooseDocuments}
                type="button"
              >
                {t("app.scan.addImages")}
              </button>
            ) : signatureWorkflowActive ? (
              <button
                className="primary wide-button"
                disabled={
                  !activeDocument ||
                  activeDocument.kind !== "pdf" ||
                  !selectedSignatureAsset ||
                  !selectedPlannedPage
                }
                onClick={() => placeSignatureAsset()}
                type="button"
              >
                {t("signature.action.placePageNumber", { page: selectedPage })}
              </button>
            ) : scanWorkflowActive ? (
              <button
                className="primary wide-button"
                disabled={!canCreateScanPdf}
                onClick={createScanPdf}
                title={describeScanExportAvailability({
                  desktopMode: mode === "desktop",
                  hasNativePaths: scanSourcePaths.length === scanImages.length,
                  imageCount: scanImages.length,
                  ocrReady: ocrAvailable,
                  protectionReady: outputProtectionIsValid(
                    scanOutputProtection,
                    qpdfAvailable
                  ),
                  recogniseText,
                  scanBusy: scanWorkflowBusy,
                  t
                })}
                type="button"
              >
                {scanBusy ? <Loader2 className="spin" size={17} aria-hidden="true" /> : null}
                {scanBusy
                  ? t("scan.export.creating")
                  : recogniseText && !ocrAvailable
                    ? t("scan.export.ocrNotReady")
                    : recogniseText
                      ? t("scan.export.searchablePdf")
                      : t("scan.export.pdf")}
              </button>
            ) : null}
          </section>

          {organiseWorkflowActive && pdf.document && selectedPlannedPage ? (
            <section className="page-actions-panel">
              <div className="page-actions-heading">
                <h3>{t("organise.actions.title")}</h3>
                <span>
                  {effectiveSelectedPageIds.length === 1
                    ? t("organise.actions.position", {
                        current: formatNumber(selectedPage),
                        total: formatNumber(pagePlan.pages.length)
                      })
                    : t("organise.actions.selection", {
                        count: formatNumber(effectiveSelectedPageIds.length),
                        current: formatNumber(selectedPage)
                      })}
                </span>
              </div>
              <div className="page-action-grid">
                <button
                  disabled={!canMoveSelectedPagesEarlier || editSafetyPending}
                  onClick={() => moveSelectedPage(-1)}
                  type="button"
                >
                  <ArrowLeft size={16} aria-hidden="true" />
                  {t("organise.actions.moveEarlier")}
                </button>
                <button
                  disabled={!canMoveSelectedPagesLater || editSafetyPending}
                  onClick={() => moveSelectedPage(1)}
                  type="button"
                >
                  <ArrowRight size={16} aria-hidden="true" />
                  {t("organise.actions.moveLater")}
                </button>
                <button disabled={editSafetyPending} onClick={duplicateSelectedPage} type="button">
                  <Copy size={16} aria-hidden="true" />
                  {t(
                    effectiveSelectedPageIds.length === 1
                      ? "organise.actions.duplicate"
                      : "organise.actions.duplicateMany"
                  )}
                </button>
                <button disabled={editSafetyPending} onClick={insertBlankPage} type="button">
                  <Plus size={16} aria-hidden="true" />
                  {t("organise.actions.blank")}
                </button>
                <button
                  disabled={editSafetyPending || mode !== "desktop"}
                  onClick={() => setImportPagesOpen(true)}
                  type="button"
                >
                  <FilePlus2 size={16} aria-hidden="true" />
                  {t("organise.actions.import")}
                </button>
                <button
                  disabled={editSafetyPending || mode !== "desktop" || !pageTransferReady}
                  onClick={() => setPageTransferOpen(true)}
                  type="button"
                >
                  <MoveRight size={16} aria-hidden="true" />
                  {t("organise.actions.transfer")}
                </button>
                <button disabled={editSafetyPending} onClick={() => handleEditorTool("rotate")} type="button">
                  <RotateCw size={16} aria-hidden="true" />
                  {t(
                    effectiveSelectedPageIds.length === 1
                      ? "organise.actions.rotate"
                      : "organise.actions.rotateMany"
                  )}
                </button>
                <button
                  className="danger-action"
                  disabled={
                    effectiveSelectedPageIds.length >= pagePlan.pages.length || editSafetyPending
                  }
                  onClick={() => handleEditorTool("delete")}
                  type="button"
                >
                  <Trash2 size={16} aria-hidden="true" />
                  {t(
                    effectiveSelectedPageIds.length === 1
                      ? "organise.actions.delete"
                      : "organise.actions.deleteMany"
                  )}
                </button>
              </div>
              <label className="blank-paper-field">
                {t("organise.actions.blankFormat")}
                <select
                  disabled={scanOperationBusy}
                  onChange={(event) => setSelectedPaperId(event.target.value)}
                  value={selectedPaper.id}
                >
                  {scanPresets.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {localiseScanPresetName(preset.id, preset.name, t)}
                    </option>
                  ))}
                </select>
              </label>
              <p>
                {describePlannedPage(
                  selectedPlannedPage.kind === "blank"
                    ? {
                        kind: "blank",
                        paper: localiseScanPresetName(
                          scanPresets.find(
                            (preset) => preset.name === selectedPlannedPage.paperName
                          )?.id ?? "",
                          selectedPlannedPage.paperName,
                          t
                        ),
                        rotation: selectedPlannedPage.rotation
                      }
                    : selectedPlannedPage.sourceId === "primary"
                      ? {
                          kind: "primary",
                          page: selectedPlannedPage.sourcePage,
                          rotation: selectedPlannedPage.rotation
                        }
                      : {
                          kind: "imported",
                          name: selectedImportedSource?.name ?? t("app.pages.imported"),
                          page: selectedPlannedPage.sourcePage,
                          rotation: selectedPlannedPage.rotation
                        },
                  t
                )}
              </p>
            </section>
          ) : null}

          {signatureWorkflowActive ? (
            <SignatureStudio
              assets={signatureAssets}
              canRedoPlacement={signaturePlacementHistory.canRedo}
              canUndoPlacement={signaturePlacementHistory.canUndo}
              certificateSigningAvailable={runtimeSupportsCertificateSigning}
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              documentLockOpenPassword={documentLockOpenPassword}
              documentLockOpenPasswordConfirmation={documentLockOpenPasswordConfirmation}
              documentLockOwnerPassword={documentLockOwnerPassword}
              documentLockOwnerPasswordConfirmation={documentLockOwnerPasswordConfirmation}
              documentLockPasswordsValid={documentLockPasswordsValid}
              documentLocked={documentLocked}
              hasPlacements={signaturePlaced}
              onAssetAdd={addSignatureAsset}
              onAssetRemove={removeSignatureAsset}
              onAssetSelect={setSelectedSignatureAssetId}
              onDocumentLockedChange={setDocumentLocked}
              onDocumentLockOpenPasswordChange={setDocumentLockOpenPassword}
              onDocumentLockOpenPasswordConfirmationChange={
                setDocumentLockOpenPasswordConfirmation
              }
              onDocumentLockOwnerPasswordChange={setDocumentLockOwnerPassword}
              onDocumentLockOwnerPasswordConfirmationChange={
                setDocumentLockOwnerPasswordConfirmation
              }
              onPlaceSelected={() => placeSignatureAsset()}
              onPlacementDelete={deleteSignaturePlacement}
              onPlacementDuplicate={duplicateSignaturePlacement}
              onPlacementLockChange={lockSignaturePlacement}
              onPlacementResize={resizeSignaturePlacement}
              onPlacementRotate={rotateSignaturePlacement}
              onRedoPlacement={signaturePlacementHistory.redo}
              onUndoPlacement={signaturePlacementHistory.undo}
              pyhankoAvailable={pyhankoAvailable}
              qpdfAvailable={qpdfAvailable}
              selectedAssetId={selectedSignatureAssetId}
              selectedPlacement={selectedSignaturePlacement}
              workspaceHasPendingChanges={workspaceHasPendingChanges}
            />
          ) : null}

          {contentWorkflowActive ? (
            <ContentEditStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {mergeWorkflowActive ? (
            <MergeStudio
              desktopMode={mode === "desktop"}
              initialRecoverySources={mergeRecoverySources}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              onRecoverySourcesChange={updateMergeRecoverySources}
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {splitWorkflowActive ? (
            <SplitStudio
              desktopMode={mode === "desktop"}
              initialRecoveryPlan={splitRecoveryPlan}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              onRecoveryPlanChange={updateSplitRecoveryPlan}
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {ocrWorkflowActive ? (
            <SearchableOcrStudio
              desktopMode={runtimeSupportsOcr}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              ocrLanguages={ocrLanguages}
              ocrReadinessBusy={ocrReadinessBusy}
              ocrReadinessDetail={
                ocrReadinessError ?? describeOcrReadiness(ocrReadiness, t)
              }
              ocrReady={ocrAvailable}
              onLanguageChange={setSelectedOcrLanguage}
              qpdfAvailable={qpdfAvailable}
              selectedLanguage={selectedOcrLanguage}
            />
          ) : null}

          {healthWorkflowActive ? (
            <HealthStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
            />
          ) : null}

          {archiveWorkflowActive ? (
            <ArchiveStudio
              archiveReadiness={archiveReadiness}
              desktopMode={runtimeSupportsArchivalPdf}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              ocrEngineAvailable={Boolean(
                ocrReadiness?.ocrMyPdf.available && ocrReadiness.tesseract.available
              )}
              ocrLanguages={ocrLanguages}
              onRefreshReadiness={refreshArchiveReadiness}
              qpdfAvailable={qpdfAvailable}
              readinessBusy={archiveReadinessBusy}
            />
          ) : null}

          {privacyWorkflowActive ? (
            <PrivacyStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {compressionWorkflowActive ? (
            <CompressionStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {batchWorkflowActive ? (
            <BatchRecipeStudio
              archiveReadiness={archiveReadiness}
              desktopMode={runtimeSupportsExternalProcesses}
              initialSourceOrigin={scanBatchHandoff?.origin}
              initialSourcePassword={
                scanBatchHandoff ? undefined : (pdf.openingPassword ?? undefined)
              }
              initialSourcePath={
                scanBatchHandoff?.path ??
                (activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined)
              }
              ocrEngineAvailable={Boolean(
                ocrReadiness?.ocrMyPdf.available && ocrReadiness.tesseract.available
              )}
              ocrLanguages={ocrLanguages}
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {comparisonWorkflowActive ? (
            <ComparisonStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
            />
          ) : null}

          {bookmarkWorkflowActive ? (
            <BookmarkStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {annotationWorkflowActive ? (
            <AnnotationStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {redactionWorkflowActive ? (
            <RedactionStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {formWorkflowActive ? (
            <FormStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {finishWorkflowActive ? (
            <PageFinishStudio
              desktopMode={mode === "desktop"}
              initialSourcePassword={pdf.openingPassword ?? undefined}
              initialSourcePath={
                activeDocument?.kind === "pdf" ? activeDocument.sourcePath : undefined
              }
              qpdfAvailable={qpdfAvailable}
            />
          ) : null}

          {printWorkflowActive && activeDocument?.kind === "pdf" && pdf.document ? (
            <PrintStudio
              assets={signatureAssets}
              currentPage={selectedPage}
              documentName={activeDocument.name}
              pages={printableWorkspacePages}
              placements={signaturePlacements}
            />
          ) : null}

          {protectionWorkflowActive ? (
            <ProtectionStudio desktopMode={mode === "desktop"} qpdfAvailable={qpdfAvailable} />
          ) : null}

          {scanWorkflowActive ? (
            <section className="scan-settings">
              <h3>{t("scan.settings.title")}</h3>
              <div className="scanner-control">
                <div className="scanner-control-heading">
                  <div className="scanner-control-title">
                    <ScanLine size={18} aria-hidden="true" />
                    <div>
                      <strong>{t("scanner.connected.title")}</strong>
                      <small>
                        {scannerDiscovery?.backendName ?? t("scanner.backend.local")}
                      </small>
                    </div>
                  </div>
                  <button
                    aria-label={t("scanner.refreshAria")}
                    className="icon-button"
                    disabled={
                      !connectedScanningAvailable ||
                      scannerDiscoveryBusy ||
                      scannerCaptureBusy ||
                      scanBusy
                    }
                    onClick={() => void refreshScanners()}
                    title={t("scanner.refreshAria")}
                    type="button"
                  >
                    <RefreshCw
                      className={scannerDiscoveryBusy ? "spin" : undefined}
                      size={16}
                      aria-hidden="true"
                    />
                  </button>
                </div>

                {!nativeMode ? (
                  <div className="engine-state is-info">
                    <Info size={16} aria-hidden="true" />
                    <span>{t("scanner.discovery.desktopOnly")}</span>
                  </div>
                ) : !connectedScanningAvailable ? (
                  <div className="engine-state is-info">
                    <Info size={16} aria-hidden="true" />
                    <span>{t("scanner.discovery.mobileUnavailable")}</span>
                  </div>
                ) : scannerDiscoveryBusy && !scannerDiscovery ? (
                  <div className="engine-state is-info" aria-live="polite">
                    <Loader2 className="spin" size={16} aria-hidden="true" />
                    <span>{t("scanner.discovery.searching")}</span>
                  </div>
                ) : !scannerDiscovery ? (
                  <div className="engine-state is-missing">
                    <AlertCircle size={16} aria-hidden="true" />
                    <span>{t("scanner.discovery.noResult")}</span>
                  </div>
                ) : !scannerDiscovery.available || scannerDiscovery.devices.length === 0 ? (
                  <div
                    className={`engine-state ${scannerDiscovery.available ? "is-info" : "is-missing"}`}
                  >
                    {scannerDiscovery.available ? (
                      <Info size={16} aria-hidden="true" />
                    ) : (
                      <AlertCircle size={16} aria-hidden="true" />
                    )}
                    <span>{describeScannerDiscovery(scannerDiscovery, t, formatNumber)}</span>
                  </div>
                ) : selectedScanner ? (
                  <>
                    <label>
                      {t("scanner.label")}
                      <select
                        disabled={scanOperationBusy || scannerDiscoveryBusy}
                        onChange={(event) => setSelectedScannerId(event.target.value)}
                        value={selectedScannerId}
                      >
                        {scannerDiscovery.devices.map((device) => (
                          <option key={device.id} value={device.id}>
                            {device.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <div
                      className="scanner-capabilities"
                      aria-label={t("scanner.capabilities.aria")}
                    >
                      {selectedScanner.flatbed ? (
                        <span>{t("scanner.capability.flatbed")}</span>
                      ) : null}
                      {selectedScanner.feeder ? (
                        <span>{t("scanner.capability.feeder")}</span>
                      ) : null}
                      {selectedScanner.duplex ? (
                        <span>{t("scanner.capability.duplex")}</span>
                      ) : null}
                      <span>
                        {formatNumber(Math.min(...scannerDpiOptions))}-
                        {formatNumber(Math.max(...scannerDpiOptions))} DPI
                      </span>
                    </div>
                    <div className="scanner-field-grid">
                      <label>
                        {t("scanner.source.label")}
                        <select
                          disabled={scanOperationBusy}
                          onChange={(event) =>
                            setScannerSource(event.target.value as ScannerSource)
                          }
                          value={scannerSource}
                        >
                          {selectedScanner.flatbed ? (
                            <option value="flatbed">{t("scanner.source.flatbed")}</option>
                          ) : null}
                          {selectedScanner.feeder ? (
                            <option value="feeder">{t("scanner.source.documentFeeder")}</option>
                          ) : null}
                        </select>
                      </label>
                      <label>
                        {t("scanner.pageLimit")}
                        <select
                          disabled={scanOperationBusy || scannerSource === "flatbed"}
                          onChange={(event) => setScannerPageLimit(Number(event.target.value))}
                          value={scannerSource === "flatbed" ? 1 : scannerPageLimit}
                        >
                          {[1, 10, 25, 50, 100, 200].map((pageCount) => (
                            <option key={pageCount} value={pageCount}>
                              {formatNumber(pageCount)}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>
                    {selectedScanner.duplex && selectedScanner.feeder ? (
                      <label>
                        <input
                          checked={scannerDuplex}
                          disabled={scanOperationBusy || scannerSource !== "feeder"}
                          onChange={(event) => setScannerDuplex(event.target.checked)}
                          type="checkbox"
                        />
                        {t("scanner.duplex")}
                      </label>
                    ) : null}
                    <button
                      className="primary wide-button scanner-capture-button"
                      disabled={!scannerCanCapture}
                      onClick={() => void captureFromScanner()}
                      type="button"
                    >
                      {scannerCaptureBusy ? (
                        <Loader2 className="spin" size={17} aria-hidden="true" />
                      ) : (
                        <ScanLine size={17} aria-hidden="true" />
                      )}
                      {scannerCaptureImportBusy
                        ? t("scanner.capture.opening")
                        : scannerCaptureBusy
                          ? t("scanner.capture.capturing")
                          : t("scanner.capture.pages")}
                    </button>
                    {scannerCaptureJob.job ? (
                      <PdfJobProgress
                        cancelling={scannerCaptureCancelBusy}
                        connectionError={scannerCaptureJob.connectionError}
                        job={scannerCaptureJob.job}
                        onCancel={() => void cancelScannerCapture()}
                        onRetry={() => void captureFromScanner()}
                        retryDisabled={!scannerCanCapture}
                      />
                    ) : null}
                    {scannerCaptureImportError ? (
                      <div className="scanner-import-error" role="alert">
                        <AlertCircle size={16} aria-hidden="true" />
                        <span>{scannerCaptureImportError}</span>
                        <button
                          disabled={scannerCaptureImportBusy}
                          onClick={retryScannerCaptureImport}
                          type="button"
                        >
                          <RefreshCw size={14} aria-hidden="true" />
                          {t("scanner.capture.retryOpen")}
                        </button>
                        <button
                          disabled={scannerCaptureImportBusy}
                          onClick={discardScannerCaptureImport}
                          type="button"
                        >
                          <Trash2 size={14} aria-hidden="true" />
                          {t("scanner.capture.discard")}
                        </button>
                      </div>
                    ) : null}
                  </>
                ) : null}
              </div>
              <label>
                {t("scan.paper.label")}
                <select
                  disabled={scanOperationBusy}
                  onChange={(event) => setSelectedPaperId(event.target.value)}
                  value={selectedPaper.id}
                >
                  {scanPresets.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {localiseScanPresetName(preset.id, preset.name, t)}
                    </option>
                  ))}
                </select>
              </label>
              <p>
                {localiseScanPresetDescription(
                  selectedPaper.id,
                  selectedPaper.description,
                  t
                )}
              </p>
              <label>
                {t("scan.colour.label")}
                <select
                  disabled={scanOperationBusy}
                  onChange={(event) => setScanColourMode(event.target.value as ScanColourMode)}
                  value={scanColourMode}
                >
                  {scannerColourModeOptions.map((colourMode) => (
                    <option key={colourMode} value={colourMode}>
                      {scanColourModeLabel(colourMode, t)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {t("scan.resolution.label")}
                <select
                  disabled={scanOperationBusy}
                  onChange={(event) => setScanDpi(Number(event.target.value))}
                  value={scanDpi}
                >
                  {[...scannerDpiOptions]
                    .sort((left, right) => left - right)
                    .map((dpi) => (
                      <option key={dpi} value={dpi}>
                        {formatNumber(dpi)} DPI
                        {dpi === 300 ? ` - ${t("scan.resolution.recommended")}` : ""}
                      </option>
                    ))}
                </select>
              </label>
              <label>
                {t("scan.margin.label")}
                <select
                  disabled={scanOperationBusy}
                  onChange={(event) => setScanMarginPt(Number(event.target.value))}
                  value={scanMarginPt}
                >
                  <option value="0">{t("common.none")}</option>
                  <option value="12">{t("scan.margin.narrow")}</option>
                  <option value="18">{t("scan.margin.normal")}</option>
                  <option value="36">{t("scan.margin.wide")}</option>
                </select>
              </label>
              <label className="scan-quality-field">
                <span>
                  {t("scan.imageQuality")} <strong>{formatNumber(scanJpegQuality)}%</strong>
                </span>
                <input
                  disabled={scanOperationBusy}
                  max="96"
                  min="60"
                  onChange={(event) => setScanJpegQuality(Number(event.target.value))}
                  step="2"
                  type="range"
                  value={scanJpegQuality}
                />
              </label>
              <div className="scan-cleanup-options">
                <div className="scan-setting-heading">
                  <strong>{t("scan.cleanup.title")}</strong>
                  <small>{t("scan.cleanup.onlyNew")}</small>
                </div>
                <label>
                  <input
                    checked={scanAutoCrop}
                    disabled={scanOperationBusy}
                    onChange={(event) => setScanAutoCrop(event.target.checked)}
                    type="checkbox"
                  />
                  {t("scan.cleanup.crop")}
                </label>
                <label>
                  <input
                    checked={scanCorrectPerspective}
                    disabled={scanOperationBusy}
                    onChange={(event) => setScanCorrectPerspective(event.target.checked)}
                    type="checkbox"
                  />
                  {t("scan.cleanup.perspective")}
                </label>
                <label>
                  <input
                    checked={scanRemoveShadows}
                    disabled={scanOperationBusy}
                    onChange={(event) => setScanRemoveShadows(event.target.checked)}
                    type="checkbox"
                  />
                  {t("scan.cleanup.shadows")}
                </label>
              </div>
              {selectedScanImage ? (
                <div className="scan-quality-preview">
                  <div className="scan-setting-heading">
                    <strong>{t("scan.preview.title")}</strong>
                    {scanPreviewBusy ? (
                      <span className="scan-preview-progress" aria-live="polite">
                        <Loader2 className="spin" size={14} aria-hidden="true" />
                        {t("scan.preview.preparing")}
                      </span>
                    ) : (
                      <small>{t("scan.preview.selected")}</small>
                    )}
                  </div>
                  <div className="scan-preview-grid">
                    <figure>
                      <div className="scan-preview-image">
                        <img
                          alt={t("scan.preview.originalAlt", { name: selectedScanImage.name })}
                          src={selectedScanImage.url}
                        />
                      </div>
                      <figcaption>{t("scan.preview.original")}</figcaption>
                    </figure>
                    <figure>
                      <div className="scan-preview-image">
                        {scanPreview ? (
                          <img
                            alt={t("scan.preview.cleanedAlt", { name: selectedScanImage.name })}
                            src={scanPreview.url}
                          />
                        ) : scanPreviewError ? (
                          <AlertCircle size={19} aria-hidden="true" />
                        ) : selectedScanImage.path ? (
                          <Loader2 className="spin" size={19} aria-hidden="true" />
                        ) : (
                          <Info size={19} aria-hidden="true" />
                        )}
                      </div>
                      <figcaption>{t("scan.preview.cleaned")}</figcaption>
                    </figure>
                  </div>
                  {scanPreview ? (
                    <div className="scan-preview-findings" aria-label={t("scan.preview.resultsAria")}>
                      {scanAutoCrop || scanCorrectPerspective ? (
                        <span className={scanPreview.pageBoundaryDetected ? "is-ready" : "is-review"}>
                          {scanPreview.pageBoundaryDetected
                            ? t("scan.preview.pageDetected")
                            : t("scan.preview.framingRetained")}
                        </span>
                      ) : null}
                      {scanPreview.cropped ? (
                        <span className="is-ready">{t("scan.preview.cropped")}</span>
                      ) : null}
                      {scanPreview.perspectiveCorrected ? (
                        <span className="is-ready">{t("scan.preview.perspective")}</span>
                      ) : null}
                      {scanPreview.shadowRemoved ? (
                        <span className="is-ready">{t("scan.preview.lighting")}</span>
                      ) : null}
                      {scanPreview.usedImageMagick ? (
                        <span>{t("scan.preview.imageMagick")}</span>
                      ) : null}
                    </div>
                  ) : scanPreviewError ? (
                    <p className="scan-preview-error">{scanPreviewError}</p>
                  ) : !selectedScanImage.path ? (
                    <p>{t("scan.preview.reopen")}</p>
                  ) : null}
                  {scanPreviewJob.job && scanPreviewJobMatchesConfiguration ? (
                    <PdfJobProgress
                      cancelling={scanPreviewCancelBusy}
                      connectionError={scanPreviewJob.connectionError}
                      job={scanPreviewJob.job}
                      onCancel={() => void cancelScanPreview()}
                      onRetry={retryScanPreview}
                      retryDisabled={
                        !scanPreviewConfiguration ||
                        scanOperationBusy ||
                        scanPreviewStarting
                      }
                    />
                  ) : null}
                </div>
              ) : null}
              <div
                className={`engine-state ${ocrAvailable ? "is-ready" : ocrReadinessBusy ? "is-info" : "is-missing"}`}
                aria-live="polite"
              >
                {ocrReadinessBusy ? (
                  <Loader2 className="spin" size={16} aria-hidden="true" />
                ) : ocrAvailable ? (
                  <CheckCircle2 size={16} aria-hidden="true" />
                ) : (
                  <AlertCircle size={16} aria-hidden="true" />
                )}
                <span>
                  {ocrReadinessBusy
                    ? t("ocr.engine.checkingDetail")
                    : (ocrReadinessError ??
                      describeOcrReadiness(ocrReadiness, t))}
                </span>
              </div>
              <label>
                <input
                  checked={recogniseText}
                  disabled={scanOperationBusy || (!ocrAvailable && !recogniseText)}
                  onChange={(event) => setRecogniseText(event.target.checked)}
                  type="checkbox"
                />
                {t("scan.ocr.recognise")}
              </label>
              <label>
                {t("scan.ocr.language")}
                <select
                  disabled={
                    !runtimeSupportsOcr ||
                    ocrLanguages.length === 0 ||
                    ocrReadinessBusy ||
                    scanOperationBusy
                  }
                  onChange={(event) => setSelectedOcrLanguage(event.target.value)}
                  value={selectedOcrLanguage}
                >
                  {ocrLanguages.length === 0 ? (
                    <option value={selectedOcrLanguage}>{t("scan.ocr.languagesNone")}</option>
                  ) : (
                    ocrLanguages.map((language) => (
                      <option key={language.code} value={language.code}>
                        {localiseOcrLanguage(language.code, language.name, t)} ({language.code})
                      </option>
                    ))
                  )}
                </select>
              </label>
              <label>
                <input
                  checked={straightenScan}
                  disabled={!ocrAvailable || !recogniseText || scanOperationBusy}
                  onChange={(event) => setStraightenScan(event.target.checked)}
                  type="checkbox"
                />
                {t("scan.ocr.deskew")}
              </label>
              <button
                className="wide-button ocr-review-button"
                disabled={
                  !ocrReviewAvailable ||
                  !selectedScanImage?.path ||
                  !scanPreview ||
                  scanPreviewBusy ||
                  ocrReadinessBusy ||
                  scanOperationBusy
                }
                onClick={() => void reviewSelectedScanOcr()}
                title={t("scan.ocr.reviewTitle")}
                type="button"
              >
                {ocrReviewJob.isActive ? (
                  <Loader2 className="spin" size={17} aria-hidden="true" />
                ) : (
                  <FileSearch size={17} aria-hidden="true" />
                )}
                {ocrReviewJob.isActive
                  ? t("scan.ocr.reviewRunning")
                  : t("scan.ocr.review")}
              </button>
              {ocrWordHints.length > 0 ? (
                <p className="ocr-hint-summary">
                  {t(
                    ocrWordHints.length === 1
                      ? "scan.hints.queued.one"
                      : "scan.hints.queued.other",
                    { count: formatNumber(ocrWordHints.length) }
                  )}
                  <button onClick={() => setOcrWordHints([])} type="button">
                    {t("common.clear")}
                  </button>
                </p>
              ) : null}
              <OutputProtectionFields
                disabled={scanOperationBusy}
                onChange={(value) => {
                  setScanOutputProtection(value);
                  setOperationStatus(null);
                }}
                qpdfAvailable={qpdfAvailable}
                value={scanOutputProtection}
              />
              <p>{t("scan.note")}</p>
            </section>
          ) : null}

          <section className="checklist">
            <h3>{t("app.flow.title")}</h3>
            {checklist.map((item, index) => {
              const complete = index === 0 ? Boolean(activeDocument) : false;

              return (
                <div className="check-row" key={item}>
                  {complete ? (
                    <CheckCircle2 size={18} aria-hidden="true" />
                  ) : (
                    <span className="check-dot" />
                  )}
                  <span>{item}</span>
                </div>
              );
            })}
          </section>

          <section className="quick-settings">
            <h3>{t("app.exportSafety.title")}</h3>
            <label>
              <input checked disabled readOnly type="checkbox" />
              {t("app.exportSafety.alwaysNew")}
            </label>
            <label>
              <input checked disabled readOnly type="checkbox" />
              {t("app.exportSafety.warnSignatures")}
            </label>
            <label>
              <input checked disabled readOnly type="checkbox" />
              {t("app.exportSafety.previewCompression")}
            </label>
          </section>

          <div className="notice">
            <AlertCircle size={17} aria-hidden="true" />
            <span>
              {protectionWorkflowActive
                ? t("app.notice.protection")
                : isPdfDocument && pdf.document
                  ? t("app.notice.pdf")
                  : isScanDocument
                    ? t("app.notice.scan")
                    : t("app.notice.local")}
            </span>
          </div>

          {activeDocument ? (
            <button
              className="ghost wide-button"
              disabled={scanWorkflowBusy}
              onClick={resetWorkspace}
              title={
                scannerCaptureBusy
                  ? t("app.close.title.scanner")
                  : scanBusy
                    ? t("app.close.title.scan")
                    : ocrReviewBusy || ocrReviewJob.isActive
                      ? t("app.close.title.ocrReview")
                      : scanPreviewBusy
                        ? t("app.close.title.scanPreview")
                      : undefined
              }
              type="button"
            >
              {scannerCaptureBusy
                ? t("app.close.scanner")
                : scanBusy
                  ? t("app.close.scan")
                  : ocrReviewBusy || ocrReviewJob.isActive
                    ? t("app.close.ocrReview")
                    : scanPreviewBusy
                      ? t("app.close.scanPreview")
                      : t("app.close.document")}
            </button>
          ) : null}
        </section>
      </section>
      <ImportPagesDialog
        desktopMode={mode === "desktop"}
        onClose={() => setImportPagesOpen(false)}
        onImport={importPdfPages}
        open={importPagesOpen}
      />
      <PageTransferDialog
        desktopMode={mode === "desktop"}
        onClose={() => setPageTransferOpen(false)}
        onMoveComplete={completePageTransferMove}
        open={pageTransferOpen}
        qpdfAvailable={qpdfAvailable}
        selectedPages={pageTransferSelectedPages}
        signatureAssets={signatureAssets}
        signaturePlacements={signaturePlacements}
        sourceDocumentName={activeDocument?.kind === "pdf" ? activeDocument.name : ""}
        sourcePageCount={pagePlan.pages.length}
        sources={pageTransferSources}
      />
      <OcrReviewDialog
        busy={ocrReviewBusy || ocrReviewJob.isActive}
        cancelling={ocrReviewCancelBusy}
        connectionError={ocrReviewJob.connectionError}
        error={ocrReviewError}
        existingHintCount={ocrWordHints.length}
        imageUrl={scanPreview?.url ?? null}
        job={ocrReviewJob.job}
        onApplyHints={applyOcrWordHints}
        onCancel={() => void cancelOcrReview()}
        onClose={closeOcrReview}
        onRetry={() => void reviewSelectedScanOcr()}
        notice={ocrReviewNotice}
        pageName={selectedScanImage?.name ?? t("scan.selectedPage")}
        result={ocrReviewResult}
        retryDisabled={
          !ocrReviewAvailable ||
          !selectedScanImage?.path ||
          !scanPreview ||
          scanPreviewBusy ||
          ocrReadinessBusy ||
          scanOperationBusy
        }
        visible={ocrReviewOpen}
      />
      <OperationAuditDialog
        onClose={() => setOperationAuditOpen(false)}
        visible={operationAuditOpen}
      />
      <UpdateDialog
        desktopMode={nativeMode}
        onClose={() => setUpdateDialogOpen(false)}
        visible={updateDialogOpen}
      />
      {pdf.passwordRequest && activeDocument?.kind === "pdf" ? (
        <PdfPasswordDialog documentName={activeDocument.name} request={pdf.passwordRequest} />
      ) : null}
    </main>
  );
}

function BlankPdfPage({
  ariaLabel,
  heightPt,
  label,
  rotation,
  widthPt,
  zoom
}: {
  ariaLabel: string;
  heightPt: number;
  label: string;
  rotation: PageRotation;
  widthPt: number;
  zoom: number;
}) {
  const landscape = rotation === 90 || rotation === 270;
  const width = (landscape ? heightPt : widthPt) * zoom;
  const height = (landscape ? widthPt : heightPt) * zoom;

  return (
    <div
      aria-label={ariaLabel}
      className="blank-pdf-page"
      style={{ height: `${height}px`, width: `${width}px` }}
    >
      <span>{label}</span>
    </div>
  );
}

function millimetresToPoints(value: number) {
  return (value * 72) / 25.4;
}

function isSupportedInput(file: File) {
  return isPdfFile(file) || isImageFile(file);
}

function isPdfFile(file: File) {
  return file.type === "application/pdf" || file.name.toLowerCase().endsWith(".pdf");
}

function isImageFile(file: File) {
  const fileName = file.name.toLowerCase();

  return (
    file.type.startsWith("image/") ||
    supportedImageExtensions.some((extension) => fileName.endsWith(extension))
  );
}

function isImagePath(path: string) {
  const normalised = path.toLocaleLowerCase("en-GB");
  return supportedImageExtensions.some((extension) => normalised.endsWith(extension));
}

async function readScannerCaptureFiles(paths: string[]): Promise<File[]> {
  const files: File[] = [];
  for (const path of paths) {
    const data = await invoke<ArrayBuffer>("read_local_document", { path });
    files.push(
      new File([data], fileNameFromPath(path), {
        type: mimeTypeFromPath(path)
      })
    );
  }
  return files;
}

function mimeTypeFromPath(path: string) {
  const extension = path.split(".").pop()?.toLowerCase();
  const types: Record<string, string> = {
    avif: "image/avif",
    bmp: "image/bmp",
    gif: "image/gif",
    heic: "image/heic",
    heif: "image/heif",
    jpeg: "image/jpeg",
    jpg: "image/jpeg",
    pdf: "application/pdf",
    png: "image/png",
    pbm: "image/x-portable-bitmap",
    pgm: "image/x-portable-graymap",
    pnm: "image/x-portable-anymap",
    ppm: "image/x-portable-pixmap",
    tif: "image/tiff",
    tiff: "image/tiff",
    webp: "image/webp"
  };

  return extension ? (types[extension] ?? "application/octet-stream") : "application/octet-stream";
}

function scanColourModeLabel(colourMode: ScanColourMode, t: Translate) {
  switch (colourMode) {
    case "greyscale":
      return t("scan.colour.greyscale");
    case "monochrome":
      return t("scan.colour.monochrome");
    default:
      return t("scan.colour.colour");
  }
}

function fileNameFromPath(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function suggestedExportPath(sourcePath: string, signed: boolean) {
  return sourcePath.replace(/\.pdf$/i, signed ? "-signed.pdf" : "-organised.pdf");
}

function suggestedScanPath(sourcePath: string) {
  const extensionIndex = sourcePath.lastIndexOf(".");
  const separatorIndex = Math.max(sourcePath.lastIndexOf("/"), sourcePath.lastIndexOf("\\"));
  const base = extensionIndex > separatorIndex ? sourcePath.slice(0, extensionIndex) : sourcePath;
  return `${base}-scan.pdf`;
}

function describeExportAvailability({
  activeDocument,
  certificateRewriteAcknowledged,
  certificateRiskAcknowledgementRequired,
  desktopMode,
  documentLockPasswordsValid,
  documentLocked,
  editSafetyPending,
  exportBusy,
  qpdfAvailable,
  signaturePlaced,
  t
}: {
  activeDocument: SelectedDocument | null;
  certificateRewriteAcknowledged: boolean;
  certificateRiskAcknowledgementRequired: boolean;
  desktopMode: boolean;
  documentLockPasswordsValid: boolean;
  documentLocked: boolean;
  editSafetyPending: boolean;
  exportBusy: boolean;
  qpdfAvailable: boolean;
  signaturePlaced: boolean;
  t: Translate;
}) {
  if (exportBusy) return t("organise.export.availability.inProgress");
  if (editSafetyPending) return t("organise.export.availability.waitForSafety");
  if (certificateRiskAcknowledgementRequired && !certificateRewriteAcknowledged) {
    return t("organise.export.availability.acknowledge");
  }
  if (!desktopMode) return t("organise.export.availability.desktopOnly");
  if (activeDocument?.kind !== "pdf") return t("organise.export.availability.needsPdf");
  if (!activeDocument.sourcePath) return t("organise.export.availability.needsSource");
  if (documentLocked && !qpdfAvailable) {
    return t("organise.export.availability.lockNeedsQpdf");
  }
  if (documentLocked && !signaturePlaced) {
    return t("organise.export.availability.lockNeedsSignature");
  }
  if (documentLocked && !documentLockPasswordsValid) {
    return t("organise.export.availability.lockNeedsPasswords");
  }
  if (signaturePlaced) return t("organise.export.availability.signature");
  return t("organise.export.availability.organised");
}

function pageSelectionModeFromModifiers({
  ctrlKey,
  metaKey,
  shiftKey
}: {
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}): PageSelectionMode {
  if (shiftKey) {
    return ctrlKey || metaKey ? "extend-range" : "range";
  }
  return ctrlKey || metaKey ? "toggle" : "single";
}

function sameStringArray(left: readonly string[], right: readonly string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function validPdfPassword(value: string) {
  return (
    value.length >= 8 &&
    new TextEncoder().encode(value).length <= 127 &&
    !/[\r\n\0]/.test(value)
  );
}

function describeScanExportAvailability({
  desktopMode,
  hasNativePaths,
  imageCount,
  ocrReady,
  protectionReady,
  recogniseText,
  scanBusy,
  t
}: {
  desktopMode: boolean;
  hasNativePaths: boolean;
  imageCount: number;
  ocrReady: boolean;
  protectionReady: boolean;
  recogniseText: boolean;
  scanBusy: boolean;
  t: Translate;
}) {
  if (scanBusy) return t("scan.availability.busy");
  if (!desktopMode) return t("scan.availability.desktopOnly");
  if (imageCount === 0) return t("scan.availability.imagesRequired");
  if (!hasNativePaths) return t("scan.availability.reopen");
  if (recogniseText && !ocrReady) return t("scan.availability.ocrRequired");
  if (!protectionReady) {
    return t("scan.availability.passwords");
  }
  return t("scan.availability.ready");
}

function formatFileSize(
  bytes: number,
  localeFormatter?: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const formatValue = (value: number, fractionDigits = 0) =>
    localeFormatter
      ? localeFormatter(value, {
          maximumFractionDigits: fractionDigits,
          minimumFractionDigits: fractionDigits
        })
      : value.toFixed(fractionDigits);
  if (bytes < 1024) {
    return `${formatValue(bytes)} B`;
  }

  const units = ["KB", "MB", "GB"];
  let size = bytes / 1024;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }

  return `${formatValue(size, size >= 10 ? 1 : 2)} ${units[unitIndex]}`;
}

function formatPdfProgress(
  progress: { loaded: number; total?: number } | null,
  formatNumber: (value: number) => string,
  t: Translate
) {
  if (!progress?.total || progress.total <= 0) {
    return null;
  }

  const percentage = Math.min(100, Math.round((progress.loaded / progress.total) * 100));
  return t("app.document.readingProgress", { percentage: formatNumber(percentage) });
}

function describeSearchStatus(
  query: string,
  result: {
    error: PdfSearchErrorCode | null;
    matches: Array<{ count: number; pageNumber: number }>;
    pagesSearched: number;
    searching: boolean;
    totalMatches: number;
    totalPages: number;
  },
  currentResultIndex: number,
  formatNumber: (value: number) => string,
  t: Translate
) {
  if (query.trim().length < 2) {
    return t("search.typeTwo");
  }
  if (result.error) {
    return t("search.failed");
  }
  if (result.searching) {
    return t("search.searching", {
      current: formatNumber(result.pagesSearched),
      total: formatNumber(result.totalPages)
    });
  }
  if (result.totalMatches === 0) {
    return t("search.noMatches");
  }

  const pageCount = result.matches.length;
  const key =
    result.totalMatches === 1
      ? pageCount === 1
        ? "search.results.oneOne"
        : "search.results.oneOther"
      : pageCount === 1
        ? "search.results.otherOne"
        : "search.results.otherOther";
  return t(key, {
    current: formatNumber(Math.min(currentResultIndex + 1, pageCount)),
    matches: formatNumber(result.totalMatches),
    pages: formatNumber(pageCount)
  });
}
