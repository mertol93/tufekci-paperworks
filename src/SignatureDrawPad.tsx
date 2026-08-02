import {
  type PointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState
} from "react";
import { RotateCcw, Trash2 } from "lucide-react";
import {
  processedSignatureFromCanvas,
  type ProcessedSignature
} from "./signature";
import { useI18n } from "./I18nProvider";

type DrawPoint = { x: number; y: number };
type DrawStroke = { colour: string; points: DrawPoint[] };

type SignatureDrawPadProps = {
  colour: "black" | "blue";
  onPreparedChange: (signature: ProcessedSignature | null) => void;
  onPreparationError: (reason: unknown | null) => void;
  sourceName: string;
};

export function SignatureDrawPad({
  colour,
  onPreparedChange,
  onPreparationError,
  sourceName
}: SignatureDrawPadProps) {
  const { t } = useI18n();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const activeStrokeRef = useRef<DrawStroke | null>(null);
  const [strokes, setStrokes] = useState<DrawStroke[]>([]);

  const redraw = useCallback(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) return;
    context.clearRect(0, 0, canvas.width, canvas.height);
    for (const stroke of strokes) drawStroke(context, stroke);
    if (strokes.length === 0) {
      onPreparedChange(null);
      onPreparationError(null);
      return;
    }
    try {
      onPreparedChange(processedSignatureFromCanvas(canvas, sourceName, 10));
      onPreparationError(null);
    } catch (reason) {
      onPreparedChange(null);
      onPreparationError(reason);
    }
  }, [onPreparationError, onPreparedChange, sourceName, strokes]);

  useEffect(() => {
    redraw();
  }, [redraw]);

  const pointForEvent = (event: PointerEvent<HTMLCanvasElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / Math.max(1, rect.width)) * event.currentTarget.width,
      y: ((event.clientY - rect.top) / Math.max(1, rect.height)) * event.currentTarget.height
    };
  };

  const beginStroke = (event: PointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const point = pointForEvent(event);
    activeStrokeRef.current = {
      colour: colour === "blue" ? "#1844a6" : "#181b20",
      points: [point]
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const continueStroke = (event: PointerEvent<HTMLCanvasElement>) => {
    const active = activeStrokeRef.current;
    if (!active || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    event.preventDefault();
    const point = pointForEvent(event);
    const previous = active.points[active.points.length - 1];
    active.points.push(point);
    const context = event.currentTarget.getContext("2d");
    if (context) drawSegment(context, previous, point, active.colour);
  };

  const finishStroke = (event: PointerEvent<HTMLCanvasElement>) => {
    const active = activeStrokeRef.current;
    if (!active) return;
    event.preventDefault();
    activeStrokeRef.current = null;
    if (active.points.length === 1) {
      active.points.push({ x: active.points[0].x + 0.1, y: active.points[0].y + 0.1 });
    }
    setStrokes((current) => [...current, active]);
  };

  return (
    <div className="signature-draw-pad">
      <canvas
        aria-label={t("signature.draw.aria")}
        height={220}
        onPointerCancel={finishStroke}
        onPointerDown={beginStroke}
        onPointerMove={continueStroke}
        onPointerUp={finishStroke}
        ref={canvasRef}
        width={720}
      />
      <div className="signature-draw-actions">
        <button
          disabled={strokes.length === 0}
          onClick={() => setStrokes((current) => current.slice(0, -1))}
          type="button"
        >
          <RotateCcw size={15} aria-hidden="true" />
          {t("signature.draw.undo")}
        </button>
        <button
          disabled={strokes.length === 0}
          onClick={() => setStrokes([])}
          type="button"
        >
          <Trash2 size={15} aria-hidden="true" />
          {t("signature.draw.clear")}
        </button>
      </div>
    </div>
  );
}

function drawStroke(context: CanvasRenderingContext2D, stroke: DrawStroke) {
  for (let index = 1; index < stroke.points.length; index += 1) {
    drawSegment(context, stroke.points[index - 1], stroke.points[index], stroke.colour);
  }
}

function drawSegment(
  context: CanvasRenderingContext2D,
  start: DrawPoint,
  end: DrawPoint,
  colour: string
) {
  context.lineCap = "round";
  context.lineJoin = "round";
  context.lineWidth = 7;
  context.strokeStyle = colour;
  context.beginPath();
  context.moveTo(start.x, start.y);
  context.lineTo(end.x, end.y);
  context.stroke();
}
