import type { ProcessedSignature } from "./signature.ts";

export const MAX_VISUAL_SIGNATURE_ASSETS = 32;
export const MAX_VISUAL_SIGNATURE_PLACEMENTS = 128;
export const VISUAL_SIGNATURE_DRAG_TYPE = "application/x-tufekci-visual-signature";

export type VisualMarkKind = "initials" | "signature";
export type VisualMarkMethod = "draw" | "image" | "type";
export type VisualSignaturePosition = "centre" | "left" | "right";

export type VisualSignatureAsset = ProcessedSignature & {
  id: string;
  kind: VisualMarkKind;
  method: VisualMarkMethod;
  name: string;
};

export type VisualSignaturePlacement = {
  assetId: string;
  id: string;
  leftRatio: number;
  locked: boolean;
  pageId: string;
  rotationDegrees: number;
  topRatio: number;
  widthRatio: number;
};

export type VisualSignatureExportAsset = {
  id: string;
  pngDataUrl: string;
};

export type VisualSignatureExportPlacement = {
  assetId: string;
  id: string;
  leftRatio: number;
  pageNumber: number;
  rotationDegrees: number;
  topRatio: number;
  widthRatio: number;
};

const MIN_WIDTH_RATIO = 0.04;
const MAX_WIDTH_RATIO = 0.68;
const PAGE_INSET_RATIO = 0.015;
const DEFAULT_WIDTH_RATIO = 0.28;
const ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,63}$/u;

export function createVisualSignatureAsset(
  id: string,
  name: string,
  kind: VisualMarkKind,
  method: VisualMarkMethod,
  prepared: ProcessedSignature
): VisualSignatureAsset {
  requireSafeId("Visual mark", id);
  const trimmedName = name.trim();
  if (!trimmedName || new TextEncoder().encode(trimmedName).length > 256) {
    throw new Error("The visual mark name must contain between 1 and 256 UTF-8 bytes.");
  }
  if (
    !Number.isSafeInteger(prepared.width) ||
    !Number.isSafeInteger(prepared.height) ||
    prepared.width < 1 ||
    prepared.height < 1
  ) {
    throw new Error("The visual mark has invalid image dimensions.");
  }
  if (!prepared.dataUrl.startsWith("data:image/png;base64,")) {
    throw new Error("The visual mark must be a prepared transparent PNG image.");
  }
  return { ...prepared, id, kind, method, name: trimmedName };
}

export function createVisualSignaturePlacement(
  id: string,
  asset: VisualSignatureAsset,
  pageId: string,
  pageAspect: number,
  position: VisualSignaturePosition = "right",
  centre?: { x: number; y: number }
): VisualSignaturePlacement {
  requireSafeId("Visual mark placement", id);
  requireSafeId("Visual mark page", pageId);
  let placement: VisualSignaturePlacement = {
    assetId: asset.id,
    id,
    leftRatio: 0,
    locked: false,
    pageId,
    rotationDegrees: 0,
    topRatio: 0,
    widthRatio: DEFAULT_WIDTH_RATIO
  };
  const heightRatio = visualSignatureHeightRatio(asset, placement.widthRatio, pageAspect);
  if (centre) {
    placement = {
      ...placement,
      leftRatio: centre.x - placement.widthRatio / 2,
      topRatio: centre.y - heightRatio / 2
    };
  } else {
    placement = {
      ...placement,
      leftRatio:
        position === "left"
          ? 0.055
          : position === "centre"
            ? (1 - placement.widthRatio) / 2
            : 0.945 - placement.widthRatio,
      topRatio: 0.935 - heightRatio
    };
  }
  return normaliseVisualSignaturePlacement(placement, asset, pageAspect);
}

export function normaliseVisualSignaturePlacement(
  placement: VisualSignaturePlacement,
  asset: VisualSignatureAsset,
  pageAspect: number
): VisualSignaturePlacement {
  const aspect = validPageAspect(pageAspect);
  const rotationDegrees = normaliseRotation(placement.rotationDegrees);
  const radians = (-rotationDegrees * Math.PI) / 180;
  const imageAspect = asset.height / asset.width;
  const widthCoefficient = Math.abs(Math.cos(radians)) + imageAspect * Math.abs(Math.sin(radians));
  const heightCoefficient =
    (Math.abs(Math.sin(radians)) + imageAspect * Math.abs(Math.cos(radians))) * aspect;
  const available = 1 - PAGE_INSET_RATIO * 2;
  const maximumForRotation = available / Math.max(widthCoefficient, heightCoefficient, 1);
  const maximumWidth = Math.max(0.001, Math.min(MAX_WIDTH_RATIO, maximumForRotation));
  const widthRatio = clamp(
    finiteOr(placement.widthRatio, DEFAULT_WIDTH_RATIO),
    Math.min(MIN_WIDTH_RATIO, maximumWidth),
    maximumWidth
  );
  const heightRatio = visualSignatureHeightRatio(asset, widthRatio, aspect);
  const boundingWidth = widthRatio * widthCoefficient;
  const boundingHeight = widthRatio * heightCoefficient;
  const centreX = clamp(
    finiteOr(placement.leftRatio, 0) + widthRatio / 2,
    PAGE_INSET_RATIO + boundingWidth / 2,
    1 - PAGE_INSET_RATIO - boundingWidth / 2
  );
  const centreY = clamp(
    finiteOr(placement.topRatio, 0) + heightRatio / 2,
    PAGE_INSET_RATIO + boundingHeight / 2,
    1 - PAGE_INSET_RATIO - boundingHeight / 2
  );

  return {
    ...placement,
    leftRatio: centreX - widthRatio / 2,
    rotationDegrees,
    topRatio: centreY - heightRatio / 2,
    widthRatio
  };
}

export function moveVisualSignaturePlacement(
  placement: VisualSignaturePlacement,
  asset: VisualSignatureAsset,
  deltaXRatio: number,
  deltaYRatio: number,
  pageAspect: number
): VisualSignaturePlacement {
  return normaliseVisualSignaturePlacement(
    {
      ...placement,
      leftRatio: placement.leftRatio + finiteOr(deltaXRatio, 0),
      topRatio: placement.topRatio + finiteOr(deltaYRatio, 0)
    },
    asset,
    pageAspect
  );
}

export function resizeVisualSignaturePlacement(
  placement: VisualSignaturePlacement,
  asset: VisualSignatureAsset,
  widthRatio: number,
  pageAspect: number
): VisualSignaturePlacement {
  const previousHeight = visualSignatureHeightRatio(asset, placement.widthRatio, pageAspect);
  const centre = {
    x: placement.leftRatio + placement.widthRatio / 2,
    y: placement.topRatio + previousHeight / 2
  };
  const nextHeight = visualSignatureHeightRatio(asset, widthRatio, pageAspect);
  return normaliseVisualSignaturePlacement(
    {
      ...placement,
      leftRatio: centre.x - widthRatio / 2,
      topRatio: centre.y - nextHeight / 2,
      widthRatio
    },
    asset,
    pageAspect
  );
}

export function rotateVisualSignaturePlacement(
  placement: VisualSignaturePlacement,
  asset: VisualSignatureAsset,
  rotationDegrees: number,
  pageAspect: number
): VisualSignaturePlacement {
  return normaliseVisualSignaturePlacement(
    { ...placement, rotationDegrees },
    asset,
    pageAspect
  );
}

export function duplicateVisualSignaturePlacement(
  placement: VisualSignaturePlacement,
  id: string,
  asset: VisualSignatureAsset,
  pageAspect: number
): VisualSignaturePlacement {
  requireSafeId("Visual mark placement", id);
  return normaliseVisualSignaturePlacement(
    {
      ...placement,
      id,
      leftRatio: placement.leftRatio + 0.025,
      locked: false,
      topRatio: placement.topRatio + 0.025
    },
    asset,
    pageAspect
  );
}

export function visualSignatureHeightRatio(
  asset: Pick<VisualSignatureAsset, "height" | "width">,
  widthRatio: number,
  pageAspect: number
) {
  return widthRatio * (asset.height / asset.width) * validPageAspect(pageAspect);
}

export function cloneVisualSignaturePlacements(placements: VisualSignaturePlacement[]) {
  return placements.map((placement) => ({ ...placement }));
}

export function partitionVisualSignaturePlacements(
  placements: readonly VisualSignaturePlacement[],
  availablePageIds: ReadonlySet<string>
) {
  const attached: VisualSignaturePlacement[] = [];
  const detached: VisualSignaturePlacement[] = [];
  placements.forEach((placement) => {
    (availablePageIds.has(placement.pageId) ? attached : detached).push({ ...placement });
  });
  return { attached, detached };
}

export function mergeDetachedVisualSignaturePlacements(
  retained: readonly VisualSignaturePlacement[],
  detached: readonly VisualSignaturePlacement[]
) {
  const merged = new Map<string, VisualSignaturePlacement>();
  [...retained, ...detached].forEach((placement) => merged.set(placement.id, { ...placement }));
  return [...merged.values()].slice(-MAX_VISUAL_SIGNATURE_PLACEMENTS);
}

export function visualSignatureExportPayload(
  assets: VisualSignatureAsset[],
  placements: VisualSignaturePlacement[],
  pageIds: string[]
): {
  visualSignatureAssets: VisualSignatureExportAsset[];
  visualSignaturePlacements: VisualSignatureExportPlacement[];
} {
  if (assets.length > MAX_VISUAL_SIGNATURE_ASSETS) {
    throw new Error(`A document may use no more than ${MAX_VISUAL_SIGNATURE_ASSETS} visual marks.`);
  }
  if (placements.length > MAX_VISUAL_SIGNATURE_PLACEMENTS) {
    throw new Error(
      `A document may contain no more than ${MAX_VISUAL_SIGNATURE_PLACEMENTS} visual mark placements.`
    );
  }
  const assetMap = new Map(assets.map((asset) => [asset.id, asset]));
  const pageMap = new Map(pageIds.map((pageId, index) => [pageId, index + 1]));
  const placementIds = new Set<string>();
  const usedAssetIds = new Set<string>();
  const visualSignaturePlacements = placements.map((placement) => {
    requireSafeId("Visual mark placement", placement.id);
    if (placementIds.has(placement.id)) {
      throw new Error("Visual mark placement identifiers must be unique.");
    }
    placementIds.add(placement.id);
    const asset = assetMap.get(placement.assetId);
    const pageNumber = pageMap.get(placement.pageId);
    if (!asset) {
      throw new Error("A visual mark placement refers to a missing session asset.");
    }
    if (!pageNumber) {
      throw new Error("A visual mark placement refers to a page that is no longer present.");
    }
    if (
      ![
        placement.leftRatio,
        placement.topRatio,
        placement.widthRatio,
        placement.rotationDegrees
      ].every(Number.isFinite)
    ) {
      throw new Error("A visual mark placement contains invalid geometry.");
    }
    usedAssetIds.add(asset.id);
    return {
      assetId: asset.id,
      id: placement.id,
      leftRatio: placement.leftRatio,
      pageNumber,
      rotationDegrees: placement.rotationDegrees,
      topRatio: placement.topRatio,
      widthRatio: placement.widthRatio
    };
  });
  const visualSignatureAssets = assets
    .filter((asset) => usedAssetIds.has(asset.id))
    .map((asset) => {
      requireSafeId("Visual mark", asset.id);
      if (!asset.dataUrl.startsWith("data:image/png;base64,")) {
        throw new Error("Every exported visual mark must be a prepared transparent PNG image.");
      }
      return { id: asset.id, pngDataUrl: asset.dataUrl };
    });
  if (visualSignatureAssets.length !== usedAssetIds.size) {
    throw new Error("Visual mark asset identifiers must be unique.");
  }
  return { visualSignatureAssets, visualSignaturePlacements };
}

export function createVisualSignatureId(prefix: "asset" | "placement") {
  const randomId = globalThis.crypto?.randomUUID?.().replace(/-/gu, "");
  if (randomId) {
    return `${prefix}:${randomId}`;
  }
  return `${prefix}:${Date.now().toString(36)}${Math.random().toString(36).slice(2, 12)}`;
}

function requireSafeId(label: string, value: string) {
  if (!ID_PATTERN.test(value)) {
    throw new Error(`${label} identifiers must contain 1 to 64 safe characters.`);
  }
}

function normaliseRotation(value: number) {
  const finite = finiteOr(value, 0);
  const wrapped = ((finite + 180) % 360 + 360) % 360 - 180;
  return Object.is(wrapped, -0) ? 0 : wrapped;
}

function validPageAspect(value: number) {
  return Number.isFinite(value) && value > 0.05 && value < 20 ? value : 1;
}

function finiteOr(value: number, fallback: number) {
  return Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
