import type { PlannedPage } from "./usePagePlan";

export const PAGE_TRANSFER_DRAG_TYPE = "application/x-tufekci-paperworks-page-transfer";
export const MAX_TRANSFER_OUTPUT_PAGES = 50_000;

export type PageTransferMode = "copy" | "move";

export type PageTransferSourceMapping = {
  destinationSourceId: string;
  sourceId: string;
};

export type PageTransferPlan = {
  pageIdMap: ReadonlyMap<string, string>;
  pages: PlannedPage[];
  sourceMappings: PageTransferSourceMapping[];
  transferredPageIds: string[];
};

export function createPageTransferPlan(
  destinationPageCount: number,
  selectedPages: readonly PlannedPage[],
  insertionIndex: number
): PageTransferPlan {
  requirePageCount(destinationPageCount);
  if (!Number.isInteger(insertionIndex) || insertionIndex < 0 || insertionIndex > destinationPageCount) {
    throw new Error("The destination insertion point is outside the document.");
  }
  if (selectedPages.length === 0) {
    throw new Error("Select at least one page to transfer.");
  }
  if (destinationPageCount + selectedPages.length > MAX_TRANSFER_OUTPUT_PAGES) {
    throw new Error(
      `The destination would exceed the ${MAX_TRANSFER_OUTPUT_PAGES}-page transfer limit.`
    );
  }

  const selectedIds = new Set<string>();
  const sourceIdMap = new Map<string, string>();
  const sourceMappings: PageTransferSourceMapping[] = [];
  const pageIdMap = new Map<string, string>();
  const transferredPageIds: string[] = [];

  const transferredPages = selectedPages.map((page, index): PlannedPage => {
    validateSelectedPage(page, selectedIds);
    const transferredId = `transfer:page:${index + 1}`;
    pageIdMap.set(page.id, transferredId);
    transferredPageIds.push(transferredId);

    if (page.kind === "blank") {
      return { ...page, id: transferredId };
    }

    let destinationSourceId = sourceIdMap.get(page.sourceId);
    if (!destinationSourceId) {
      destinationSourceId = `transfer-source-${sourceIdMap.size + 1}`;
      sourceIdMap.set(page.sourceId, destinationSourceId);
      sourceMappings.push({ destinationSourceId, sourceId: page.sourceId });
    }
    return { ...page, id: transferredId, sourceId: destinationSourceId };
  });

  const destinationPages = Array.from(
    { length: destinationPageCount },
    (_, index): PlannedPage => ({
      id: `destination:source:${index + 1}`,
      kind: "source",
      rotation: 0,
      sourceId: "primary",
      sourcePage: index + 1
    })
  );
  destinationPages.splice(insertionIndex, 0, ...transferredPages);

  return {
    pageIdMap,
    pages: destinationPages,
    sourceMappings,
    transferredPageIds
  };
}

export function canMovePagesBetweenDocuments(
  sourcePageCount: number,
  selectedPageCount: number
) {
  return (
    Number.isInteger(sourcePageCount) &&
    Number.isInteger(selectedPageCount) &&
    sourcePageCount > 1 &&
    selectedPageCount > 0 &&
    selectedPageCount < sourcePageCount
  );
}

function requirePageCount(pageCount: number) {
  if (
    !Number.isInteger(pageCount) ||
    pageCount < 1 ||
    pageCount > MAX_TRANSFER_OUTPUT_PAGES
  ) {
    throw new Error("The destination page count is outside the supported transfer limit.");
  }
}

function validateSelectedPage(page: PlannedPage, selectedIds: Set<string>) {
  if (!page.id || selectedIds.has(page.id)) {
    throw new Error("Transferred pages must have unique identifiers.");
  }
  selectedIds.add(page.id);

  if (page.kind === "source") {
    if (!page.sourceId.trim() || !Number.isInteger(page.sourcePage) || page.sourcePage < 1) {
      throw new Error("A transferred page refers to an invalid source PDF page.");
    }
    return;
  }

  if (
    !Number.isFinite(page.widthPt) ||
    !Number.isFinite(page.heightPt) ||
    page.widthPt <= 0 ||
    page.heightPt <= 0
  ) {
    throw new Error("A transferred blank page has invalid dimensions.");
  }
}
