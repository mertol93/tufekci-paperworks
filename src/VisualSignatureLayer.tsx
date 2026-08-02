import {
  type CSSProperties,
  type DragEvent,
  type KeyboardEvent,
  type PointerEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { Copy, LockKeyhole, RotateCw, Trash2 } from "lucide-react";
import {
  moveVisualSignaturePlacement,
  resizeVisualSignaturePlacement,
  rotateVisualSignaturePlacement,
  VISUAL_SIGNATURE_DRAG_TYPE,
  visualSignatureHeightRatio,
  type VisualSignatureAsset,
  type VisualSignaturePlacement
} from "./visualSignatures";
import { useI18n } from "./I18nProvider";

type VisualSignatureLayerProps = {
  assets: VisualSignatureAsset[];
  editable: boolean;
  onAdd: (
    assetId: string,
    centre: { x: number; y: number },
    pageAspect: number
  ) => void;
  onChange: (placement: VisualSignaturePlacement) => void;
  onDelete: (placementId: string) => void;
  onDuplicate: (placementId: string, pageAspect: number) => void;
  onSelect: (placementId: string | null) => void;
  pageId: string;
  placements: VisualSignaturePlacement[];
  selectedPlacementId: string | null;
};

type PointerInteraction =
  | {
      asset: VisualSignatureAsset;
      kind: "move";
      origin: VisualSignaturePlacement;
      pageAspect: number;
      pageHeight: number;
      pageWidth: number;
      pointerId: number;
      startX: number;
      startY: number;
    }
  | {
      asset: VisualSignatureAsset;
      kind: "resize";
      origin: VisualSignaturePlacement;
      pageAspect: number;
      pageHeight: number;
      pageWidth: number;
      pointerId: number;
      startX: number;
      startY: number;
    }
  | {
      asset: VisualSignatureAsset;
      centreX: number;
      centreY: number;
      kind: "rotate";
      origin: VisualSignaturePlacement;
      pageAspect: number;
      pointerId: number;
      startAngle: number;
    };

export function VisualSignatureLayer({
  assets,
  editable,
  onAdd,
  onChange,
  onDelete,
  onDuplicate,
  onSelect,
  pageId,
  placements,
  selectedPlacementId
}: VisualSignatureLayerProps) {
  const { t } = useI18n();
  const layerRef = useRef<HTMLDivElement>(null);
  const interactionRef = useRef<PointerInteraction | null>(null);
  const draftRef = useRef<VisualSignaturePlacement | null>(null);
  const [draft, setDraft] = useState<VisualSignaturePlacement | null>(null);
  const assetMap = useMemo(() => new Map(assets.map((asset) => [asset.id, asset])), [assets]);

  useEffect(() => {
    setDraft(null);
    draftRef.current = null;
    interactionRef.current = null;
  }, [pageId]);

  const displayedPlacements = placements.map((placement) =>
    draft?.id === placement.id ? draft : placement
  );

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    if (!editable) return;
    const assetId = event.dataTransfer.getData(VISUAL_SIGNATURE_DRAG_TYPE);
    if (!assetMap.has(assetId)) return;
    event.preventDefault();
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return;
    onAdd(
      assetId,
      {
        x: (event.clientX - rect.left) / rect.width,
        y: (event.clientY - rect.top) / rect.height
      },
      rect.width / rect.height
    );
  };

  const beginPointer = (
    event: PointerEvent<HTMLElement>,
    placement: VisualSignaturePlacement,
    asset: VisualSignatureAsset,
    kind: PointerInteraction["kind"]
  ) => {
    if (!editable || placement.locked || event.button !== 0 || !layerRef.current) return;
    event.preventDefault();
    event.stopPropagation();
    onSelect(placement.id);
    const pageRect = layerRef.current.getBoundingClientRect();
    const pageAspect = pageRect.width / Math.max(1, pageRect.height);
    if (kind === "rotate") {
      const heightRatio = visualSignatureHeightRatio(asset, placement.widthRatio, pageAspect);
      const centreX = pageRect.left + (placement.leftRatio + placement.widthRatio / 2) * pageRect.width;
      const centreY = pageRect.top + (placement.topRatio + heightRatio / 2) * pageRect.height;
      interactionRef.current = {
        asset,
        centreX,
        centreY,
        kind,
        origin: placement,
        pageAspect,
        pointerId: event.pointerId,
        startAngle: Math.atan2(event.clientY - centreY, event.clientX - centreX)
      };
    } else {
      interactionRef.current = {
        asset,
        kind,
        origin: placement,
        pageAspect,
        pageHeight: pageRect.height,
        pageWidth: pageRect.width,
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY
      };
    }
    draftRef.current = placement;
    setDraft(placement);
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const updatePointer = (event: PointerEvent<HTMLElement>) => {
    const interaction = interactionRef.current;
    if (!interaction || interaction.pointerId !== event.pointerId) return;
    event.preventDefault();
    let next: VisualSignaturePlacement;
    if (interaction.kind === "move") {
      next = moveVisualSignaturePlacement(
        interaction.origin,
        interaction.asset,
        (event.clientX - interaction.startX) / interaction.pageWidth,
        (event.clientY - interaction.startY) / interaction.pageHeight,
        interaction.pageAspect
      );
    } else if (interaction.kind === "resize") {
      next = resizeVisualSignaturePlacement(
        interaction.origin,
        interaction.asset,
        interaction.origin.widthRatio +
          (event.clientX - interaction.startX) / interaction.pageWidth,
        interaction.pageAspect
      );
    } else {
      const angle = Math.atan2(
        event.clientY - interaction.centreY,
        event.clientX - interaction.centreX
      );
      next = rotateVisualSignaturePlacement(
        interaction.origin,
        interaction.asset,
        interaction.origin.rotationDegrees + ((angle - interaction.startAngle) * 180) / Math.PI,
        interaction.pageAspect
      );
    }
    draftRef.current = next;
    setDraft(next);
  };

  const finishPointer = (event: PointerEvent<HTMLElement>) => {
    const interaction = interactionRef.current;
    if (!interaction || interaction.pointerId !== event.pointerId) return;
    event.preventDefault();
    const next = draftRef.current;
    interactionRef.current = null;
    draftRef.current = null;
    setDraft(null);
    if (next && !samePlacement(next, interaction.origin)) {
      onChange(next);
    }
  };

  const handleKey = (
    event: KeyboardEvent<HTMLDivElement>,
    placement: VisualSignaturePlacement,
    asset: VisualSignatureAsset
  ) => {
    if (!editable) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase("en-GB") === "d") {
      event.preventDefault();
      onDuplicate(placement.id, currentPageAspect(layerRef.current));
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && !placement.locked) {
      event.preventDefault();
      onDelete(placement.id);
      return;
    }
    const distance = event.shiftKey ? 0.02 : 0.005;
    const movement =
      event.key === "ArrowLeft"
        ? { x: -distance, y: 0 }
        : event.key === "ArrowRight"
          ? { x: distance, y: 0 }
          : event.key === "ArrowUp"
            ? { x: 0, y: -distance }
            : event.key === "ArrowDown"
              ? { x: 0, y: distance }
              : null;
    if (movement && !placement.locked) {
      event.preventDefault();
      onChange(
        moveVisualSignaturePlacement(
          placement,
          asset,
          movement.x,
          movement.y,
          currentPageAspect(layerRef.current)
        )
      );
    }
  };

  return (
    <div
      aria-label={t("signature.layer.aria")}
      className={`visual-signature-layer ${editable ? "is-editable" : ""}`}
      onClick={(event) => {
        if (event.target === event.currentTarget) onSelect(null);
      }}
      onDragOver={(event) => {
        if (editable && event.dataTransfer.types.includes(VISUAL_SIGNATURE_DRAG_TYPE)) {
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
        }
      }}
      onDrop={handleDrop}
      ref={layerRef}
    >
      {displayedPlacements.map((placement) => {
        const asset = assetMap.get(placement.assetId);
        if (!asset) return null;
        const selected = selectedPlacementId === placement.id;
        const style = {
          left: `${placement.leftRatio * 100}%`,
          top: `${placement.topRatio * 100}%`,
          transform: `rotate(${placement.rotationDegrees}deg)`,
          width: `${placement.widthRatio * 100}%`
        } satisfies CSSProperties;
        return (
          <div
            aria-label={t("signature.layer.placementAria", {
              kind: t(visualMarkKindKeys[asset.kind]),
              name: asset.name
            })}
            aria-selected={selected}
            className={`visual-signature-placement ${selected ? "is-selected" : ""} ${
              placement.locked ? "is-locked" : ""
            }`}
            data-placement-id={placement.id}
            key={placement.id}
            onClick={(event) => {
              event.stopPropagation();
              onSelect(placement.id);
            }}
            onKeyDown={(event) => handleKey(event, placement, asset)}
            onPointerCancel={finishPointer}
            onPointerDown={(event) => beginPointer(event, placement, asset, "move")}
            onPointerMove={updatePointer}
            onPointerUp={finishPointer}
            role="option"
            style={style}
            tabIndex={editable ? 0 : -1}
          >
            <img alt="" draggable={false} src={asset.dataUrl} />
            {placement.locked ? (
              <span className="visual-signature-lock" title={t("signature.layer.locked")}>
                <LockKeyhole size={12} aria-hidden="true" />
              </span>
            ) : null}
            {selected && editable ? (
              <>
                <div className="visual-signature-toolbar">
                  <button
                    aria-label={t("signature.layer.duplicateAria")}
                    onClick={(event) => {
                      event.stopPropagation();
                      onDuplicate(placement.id, currentPageAspect(layerRef.current));
                    }}
                    onPointerDown={(event) => event.stopPropagation()}
                    title={t("signature.placement.duplicate")}
                    type="button"
                  >
                    <Copy size={12} aria-hidden="true" />
                  </button>
                  <button
                    aria-label={t("signature.layer.deleteAria")}
                    disabled={placement.locked}
                    onClick={(event) => {
                      event.stopPropagation();
                      onDelete(placement.id);
                    }}
                    onPointerDown={(event) => event.stopPropagation()}
                    title={t("signature.placement.delete")}
                    type="button"
                  >
                    <Trash2 size={12} aria-hidden="true" />
                  </button>
                </div>
                {!placement.locked ? (
                  <>
                    <button
                      aria-label={t("signature.layer.rotateAria")}
                      className="visual-signature-rotate"
                      onPointerDown={(event) => beginPointer(event, placement, asset, "rotate")}
                      title={t("signature.layer.rotateTitle")}
                      type="button"
                    >
                      <RotateCw size={12} aria-hidden="true" />
                    </button>
                    <button
                      aria-label={t("signature.layer.resizeAria")}
                      className="visual-signature-resize"
                      onPointerDown={(event) => beginPointer(event, placement, asset, "resize")}
                      title={t("signature.layer.resizeTitle")}
                      type="button"
                    />
                  </>
                ) : null}
              </>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

function currentPageAspect(element: HTMLElement | null) {
  const rect = element?.getBoundingClientRect();
  return rect && rect.width > 0 && rect.height > 0 ? rect.width / rect.height : 1;
}

function samePlacement(left: VisualSignaturePlacement, right: VisualSignaturePlacement) {
  return (
    left.leftRatio === right.leftRatio &&
    left.topRatio === right.topRatio &&
    left.widthRatio === right.widthRatio &&
    left.rotationDegrees === right.rotationDegrees
  );
}

const visualMarkKindKeys = {
  initials: "signature.kind.initials",
  signature: "signature.kind.signature"
} as const;
