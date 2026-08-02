import { AnnotationMode } from "pdfjs-dist";
import type { PDFDocumentProxy } from "./pdf";
import {
  PRINT_DPI,
  calculatePrintBudget,
  type PrintBudgetErrorCode,
  type PrintQuality
} from "./print";
import type { PageRotation } from "./usePagePlan";
import type { VisualSignatureAsset, VisualSignaturePlacement } from "./visualSignatures";

export type PrintableWorkspacePage =
  | {
      document: PDFDocumentProxy | null;
      id: string;
      kind: "source";
      rotation: PageRotation;
      sourcePage: number;
    }
  | {
      heightPt: number;
      id: string;
      kind: "blank";
      rotation: PageRotation;
      widthPt: number;
    };

export type PreparedPrintPage = {
  heightPt: number;
  pageNumber: number;
  pixelHeight: number;
  pixelWidth: number;
  url: string;
  widthPt: number;
};

export type PreparedPrintDocument = {
  dispose: () => void;
  pages: PreparedPrintPage[];
  totalPixels: number;
};

export type PrintPreparationProgress = {
  completed: number;
  phase: "checking" | "rendering";
  total: number;
};

export type PrintPreparationErrorCode =
  | PrintBudgetErrorCode
  | "asset-unavailable"
  | "cancelled"
  | "render-failed"
  | "source-unavailable";

export class PrintPreparationError extends Error {
  constructor(readonly code: PrintPreparationErrorCode) {
    super(code);
    this.name = "PrintPreparationError";
  }
}

type PreparePrintDocumentOptions = {
  assets: readonly VisualSignatureAsset[];
  includeVisualSignatures: boolean;
  onProgress?: (progress: PrintPreparationProgress) => void;
  pages: readonly PrintableWorkspacePage[];
  placements: readonly VisualSignaturePlacement[];
  quality: PrintQuality;
  selectedPageNumbers: readonly number[];
  signal?: AbortSignal;
};

type CheckedPrintPage = {
  heightPt: number;
  pageNumber: number;
  source: PrintableWorkspacePage;
  widthPt: number;
};

export async function preparePrintDocument({
  assets,
  includeVisualSignatures,
  onProgress,
  pages,
  placements,
  quality,
  selectedPageNumbers,
  signal
}: PreparePrintDocumentOptions): Promise<PreparedPrintDocument> {
  const checkedPages: CheckedPrintPage[] = [];
  const total = selectedPageNumbers.length;
  const urls: string[] = [];

  try {
    for (let index = 0; index < total; index += 1) {
      throwIfCancelled(signal);
      const pageNumber = selectedPageNumbers[index];
      const source = pages[pageNumber - 1];
      if (!source) {
        throw new PrintPreparationError("source-unavailable");
      }
      const size = await printablePageSize(source);
      checkedPages.push({ ...size, pageNumber, source });
      onProgress?.({ completed: index + 1, phase: "checking", total });
    }

    const budget = calculatePrintBudget(checkedPages, quality);
    if (budget.error) {
      throw new PrintPreparationError(budget.error);
    }

    const assetMap = new Map(assets.map((asset) => [asset.id, asset]));
    const imageCache = new Map<string, Promise<HTMLImageElement>>();
    const preparedPages: PreparedPrintPage[] = [];
    for (let index = 0; index < checkedPages.length; index += 1) {
      throwIfCancelled(signal);
      const checked = checkedPages[index];
      const prepared = await renderPrintPage({
        assetMap,
        checked,
        imageCache,
        includeVisualSignatures,
        placements,
        quality,
        signal
      });
      urls.push(prepared.url);
      preparedPages.push(prepared);
      onProgress?.({ completed: index + 1, phase: "rendering", total });
    }

    return {
      dispose: once(() => urls.forEach((url) => URL.revokeObjectURL(url))),
      pages: preparedPages,
      totalPixels: budget.totalPixels
    };
  } catch (reason) {
    urls.forEach((url) => URL.revokeObjectURL(url));
    if (reason instanceof PrintPreparationError) {
      throw reason;
    }
    if (signal?.aborted || isRenderingCancellation(reason)) {
      throw new PrintPreparationError("cancelled");
    }
    throw new PrintPreparationError("render-failed");
  }
}

async function printablePageSize(source: PrintableWorkspacePage) {
  if (source.kind === "blank") {
    return source.rotation === 90 || source.rotation === 270
      ? { heightPt: source.widthPt, widthPt: source.heightPt }
      : { heightPt: source.heightPt, widthPt: source.widthPt };
  }
  if (!source.document || source.sourcePage < 1 || source.sourcePage > source.document.numPages) {
    throw new PrintPreparationError("source-unavailable");
  }
  const page = await source.document.getPage(source.sourcePage);
  const viewport = page.getViewport({
    rotation: normaliseQuarterTurn(page.rotate + source.rotation),
    scale: 1
  });
  return { heightPt: viewport.height, widthPt: viewport.width };
}

async function renderPrintPage({
  assetMap,
  checked,
  imageCache,
  includeVisualSignatures,
  placements,
  quality,
  signal
}: {
  assetMap: ReadonlyMap<string, VisualSignatureAsset>;
  checked: CheckedPrintPage;
  imageCache: Map<string, Promise<HTMLImageElement>>;
  includeVisualSignatures: boolean;
  placements: readonly VisualSignaturePlacement[];
  quality: PrintQuality;
  signal?: AbortSignal;
}): Promise<PreparedPrintPage> {
  const scale = PRINT_DPI[quality] / 72;
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.ceil(checked.widthPt * scale));
  canvas.height = Math.max(1, Math.ceil(checked.heightPt * scale));
  const context = canvas.getContext("2d", { alpha: false });
  if (!context) {
    throw new PrintPreparationError("render-failed");
  }
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, canvas.width, canvas.height);

  try {
    if (checked.source.kind === "source") {
      const sourceDocument = checked.source.document;
      if (!sourceDocument) {
        throw new PrintPreparationError("source-unavailable");
      }
      const page = await sourceDocument.getPage(checked.source.sourcePage);
      throwIfCancelled(signal);
      const viewport = page.getViewport({
        rotation: normaliseQuarterTurn(page.rotate + checked.source.rotation),
        scale
      });
      const renderTask = page.render({
        annotationMode: AnnotationMode.ENABLE_STORAGE,
        background: "#ffffff",
        canvas,
        intent: "print",
        optionalContentConfigPromise: sourceDocument.getOptionalContentConfig({ intent: "print" }),
        viewport
      });
      const cancelRender = () => renderTask.cancel();
      signal?.addEventListener("abort", cancelRender, { once: true });
      try {
        await renderTask.promise;
      } finally {
        signal?.removeEventListener("abort", cancelRender);
      }
    }

    if (includeVisualSignatures) {
      const pagePlacements = placements.filter((placement) => placement.pageId === checked.source.id);
      for (const placement of pagePlacements) {
        throwIfCancelled(signal);
        const asset = assetMap.get(placement.assetId);
        if (!asset) {
          throw new PrintPreparationError("asset-unavailable");
        }
        const image = await cachedImage(asset, imageCache);
        drawVisualSignature(context, canvas, placement, asset, image);
      }
    }

    throwIfCancelled(signal);
    const blob = await canvasToPng(canvas);
    throwIfCancelled(signal);
    const url = URL.createObjectURL(blob);
    return {
      heightPt: checked.heightPt,
      pageNumber: checked.pageNumber,
      pixelHeight: canvas.height,
      pixelWidth: canvas.width,
      url,
      widthPt: checked.widthPt
    };
  } catch (reason) {
    if (reason instanceof PrintPreparationError) {
      throw reason;
    }
    if (signal?.aborted || isRenderingCancellation(reason)) {
      throw new PrintPreparationError("cancelled");
    }
    throw new PrintPreparationError("render-failed");
  } finally {
    canvas.width = 1;
    canvas.height = 1;
  }
}

function drawVisualSignature(
  context: CanvasRenderingContext2D,
  canvas: HTMLCanvasElement,
  placement: VisualSignaturePlacement,
  asset: VisualSignatureAsset,
  image: HTMLImageElement
) {
  const width = placement.widthRatio * canvas.width;
  const height = width * (asset.height / asset.width);
  const left = placement.leftRatio * canvas.width;
  const top = placement.topRatio * canvas.height;
  context.save();
  context.translate(left + width / 2, top + height / 2);
  context.rotate((placement.rotationDegrees * Math.PI) / 180);
  context.drawImage(image, -width / 2, -height / 2, width, height);
  context.restore();
}

function cachedImage(
  asset: VisualSignatureAsset,
  cache: Map<string, Promise<HTMLImageElement>>
) {
  let pending = cache.get(asset.id);
  if (!pending) {
    pending = loadImage(asset.dataUrl);
    cache.set(asset.id, pending);
  }
  return pending;
}

async function loadImage(url: string) {
  const image = new Image();
  image.decoding = "sync";
  image.src = url;
  if (typeof image.decode === "function") {
    await image.decode();
  } else {
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new PrintPreparationError("asset-unavailable"));
    });
  }
  if (!image.naturalWidth || !image.naturalHeight) {
    throw new PrintPreparationError("asset-unavailable");
  }
  return image;
}

function canvasToPng(canvas: HTMLCanvasElement) {
  return new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) {
        resolve(blob);
      } else {
        reject(new PrintPreparationError("render-failed"));
      }
    }, "image/png");
  });
}

function throwIfCancelled(signal?: AbortSignal) {
  if (signal?.aborted) {
    throw new PrintPreparationError("cancelled");
  }
}

function isRenderingCancellation(reason: unknown) {
  return reason instanceof Error && reason.name === "RenderingCancelledException";
}

function normaliseQuarterTurn(rotation: number) {
  return ((rotation % 360) + 360) % 360;
}

function once(action: () => void) {
  let called = false;
  return () => {
    if (!called) {
      called = true;
      action();
    }
  };
}
