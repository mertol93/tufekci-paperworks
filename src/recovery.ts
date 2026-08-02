import type { PlannedPage } from "./usePagePlan";

export type RecoveryScanSettings = {
  autoCrop?: boolean;
  colourMode: "colour" | "greyscale" | "monochrome";
  correctPerspective?: boolean;
  dpi: number;
  jpegQuality: number;
  marginPt: number;
  ocrLanguage: string;
  paperId: string;
  recogniseText: boolean;
  removeShadows?: boolean;
  straighten: boolean;
};

export type RecoveryMergeSource = {
  id: string;
  pageRange: string;
  sourcePath: string;
};

export type RecoverySplitPlan = {
  pageGroups: string;
  sourcePath: string;
};

export type RecoveryDocument =
  | {
      importedSources: Array<{
        certificateAcknowledged: boolean;
        certificateSignature: boolean;
        id: string;
        name: string;
        sourcePath: string;
      }>;
      kind: "pdf";
      name: string;
      pages: PlannedPage[];
      sourcePath: string;
    }
  | {
      kind: "scan";
      name: string;
      settings: RecoveryScanSettings;
      sourcePaths: string[];
    }
  | {
      kind: "merge";
      name: string;
      sources: RecoveryMergeSource[];
    }
  | {
      kind: "split";
      name: string;
      pageGroups: string;
      sourcePath: string;
    };

export type RecoverySnapshot = {
  activeWorkflowId: string;
  document: RecoveryDocument;
  savedAtUnixMs: number;
  selectedPage: number;
  version: 1;
  zoom: number;
};

export type RecoverySaveResult = {
  savedAtUnixMs: number;
};

export function recoveryDocumentName(snapshot: RecoverySnapshot) {
  return snapshot.document.name;
}

export function toRecoveryMergeSources(
  sources: ReadonlyArray<{ id: string; pageRange: string; path: string }>
): RecoveryMergeSource[] {
  return sources.map((source) => ({
    id: source.id,
    pageRange: source.pageRange,
    sourcePath: source.path
  }));
}

export function toRecoverySplitPlan(
  sourcePath: string | null,
  pageGroups: string
): RecoverySplitPlan | null {
  return sourcePath ? { pageGroups, sourcePath } : null;
}

export function formatRecoveryTime(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("en-GB", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(unixMilliseconds));
}
