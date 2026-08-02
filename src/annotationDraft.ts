export type AnnotationKind =
  | "text"
  | "highlight"
  | "stamp"
  | "freehand"
  | "rectangle"
  | "ellipse"
  | "line"
  | "image";

export type NormalisedPoint = {
  x: number;
  y: number;
};

export type NormalisedRect = NormalisedPoint & {
  height: number;
  width: number;
};

export type AnnotationDraft = {
  colour: string;
  fillColour: string | null;
  fontSize: number;
  id: string;
  imageDataUrl: string | null;
  kind: AnnotationKind;
  lineWidth: number;
  opacity: number;
  pageNumber: number;
  points: NormalisedPoint[];
  rect: NormalisedRect | null;
  sourceAnnotationId: string | null;
  stamp: string | null;
  start: NormalisedPoint | null;
  end: NormalisedPoint | null;
  text: string | null;
  viewerAnnotationId: string | null;
};

export type AnnotationHistory = {
  future: AnnotationDraft[][];
  past: AnnotationDraft[][];
  present: AnnotationDraft[];
};

export type AnnotationPayload = Omit<
  AnnotationDraft,
  "colour" | "fillColour" | "imageDataUrl" | "viewerAnnotationId"
> & {
  colour: [number, number, number];
  fillColour: [number, number, number] | null;
  imageDataUrl: string | null;
};

export type InspectedAnnotationPayload = AnnotationPayload & {
  sourceAnnotationId: string;
  viewerAnnotationId: string;
};

export type AnnotationChangeSet = {
  newAnnotations: AnnotationDraft[];
  removedExistingAnnotationIds: string[];
  updatedAnnotations: AnnotationDraft[];
};

const HISTORY_LIMIT = 100;

export function createAnnotationHistory(
  annotations: AnnotationDraft[] = []
): AnnotationHistory {
  return { future: [], past: [], present: annotations };
}

export function commitAnnotationHistory(
  history: AnnotationHistory,
  next: AnnotationDraft[]
): AnnotationHistory {
  if (next === history.present) {
    return history;
  }
  return {
    future: [],
    past: [...history.past.slice(-(HISTORY_LIMIT - 1)), history.present],
    present: next
  };
}

export function undoAnnotationHistory(history: AnnotationHistory): AnnotationHistory {
  const previous = history.past[history.past.length - 1];
  if (!previous) {
    return history;
  }
  return {
    future: [history.present, ...history.future].slice(0, HISTORY_LIMIT),
    past: history.past.slice(0, -1),
    present: previous
  };
}

export function redoAnnotationHistory(history: AnnotationHistory): AnnotationHistory {
  const next = history.future[0];
  if (!next) {
    return history;
  }
  return {
    future: history.future.slice(1),
    past: [...history.past.slice(-(HISTORY_LIMIT - 1)), history.present],
    present: next
  };
}

export function normalisedPoint(x: number, y: number): NormalisedPoint {
  return { x: clamp(x, 0, 1), y: clamp(y, 0, 1) };
}

export function rectBetween(
  start: NormalisedPoint,
  end: NormalisedPoint,
  minimum = 0.002
): NormalisedRect {
  const left = Math.min(start.x, end.x);
  const top = Math.min(start.y, end.y);
  return {
    height: Math.max(minimum, Math.abs(end.y - start.y)),
    width: Math.max(minimum, Math.abs(end.x - start.x)),
    x: Math.min(left, 1 - minimum),
    y: Math.min(top, 1 - minimum)
  };
}

export function annotationBounds(annotation: AnnotationDraft): NormalisedRect {
  if (annotation.rect) {
    return annotation.rect;
  }
  const points =
    annotation.kind === "line"
      ? [annotation.start, annotation.end].filter(
          (point): point is NormalisedPoint => point !== null
        )
      : annotation.points;
  if (points.length === 0) {
    return { height: 0, width: 0, x: 0, y: 0 };
  }
  const left = Math.min(...points.map((point) => point.x));
  const right = Math.max(...points.map((point) => point.x));
  const top = Math.min(...points.map((point) => point.y));
  const bottom = Math.max(...points.map((point) => point.y));
  return { height: bottom - top, width: right - left, x: left, y: top };
}

export function translateAnnotation(
  annotation: AnnotationDraft,
  requestedX: number,
  requestedY: number
): AnnotationDraft {
  const bounds = annotationBounds(annotation);
  const dx = clamp(requestedX, -bounds.x, 1 - bounds.x - bounds.width);
  const dy = clamp(requestedY, -bounds.y, 1 - bounds.y - bounds.height);
  const translatePoint = (point: NormalisedPoint | null) =>
    point ? normalisedPoint(point.x + dx, point.y + dy) : null;
  return {
    ...annotation,
    end: translatePoint(annotation.end),
    points: annotation.points.map((point) => translatePoint(point) as NormalisedPoint),
    rect: annotation.rect
      ? { ...annotation.rect, x: annotation.rect.x + dx, y: annotation.rect.y + dy }
      : null,
    start: translatePoint(annotation.start)
  };
}

export function annotationPayload(annotation: AnnotationDraft): AnnotationPayload {
  const { viewerAnnotationId: _viewerAnnotationId, ...payload } = annotation;
  return {
    ...payload,
    colour: hexToRgb(annotation.colour),
    fillColour: annotation.fillColour ? hexToRgb(annotation.fillColour) : null
  };
}

export function annotationDraftFromInspection(
  annotation: InspectedAnnotationPayload
): AnnotationDraft {
  return {
    ...annotation,
    colour: rgbToHex(annotation.colour),
    fillColour: annotation.fillColour ? rgbToHex(annotation.fillColour) : null
  };
}

export function annotationChangeSet(
  initial: AnnotationDraft[],
  current: AnnotationDraft[]
): AnnotationChangeSet {
  const initialBySource = new Map(
    initial
      .filter(
        (annotation): annotation is AnnotationDraft & { sourceAnnotationId: string } =>
          annotation.sourceAnnotationId !== null
      )
      .map((annotation) => [annotation.sourceAnnotationId, annotation])
  );
  const currentSourceIds = new Set<string>();
  const newAnnotations: AnnotationDraft[] = [];
  const updatedAnnotations: AnnotationDraft[] = [];

  for (const annotation of current) {
    if (!annotation.sourceAnnotationId) {
      newAnnotations.push(annotation);
      continue;
    }
    currentSourceIds.add(annotation.sourceAnnotationId);
    const original = initialBySource.get(annotation.sourceAnnotationId);
    if (!original || editableAnnotationState(original) !== editableAnnotationState(annotation)) {
      updatedAnnotations.push(annotation);
    }
  }

  return {
    newAnnotations,
    removedExistingAnnotationIds: [...initialBySource.keys()].filter(
      (sourceId) => !currentSourceIds.has(sourceId)
    ),
    updatedAnnotations
  };
}

export function hexToRgb(value: string): [number, number, number] {
  const normalised = /^#[0-9a-f]{6}$/i.test(value) ? value.slice(1) : "000000";
  return [0, 2, 4].map(
    (offset) => Number.parseInt(normalised.slice(offset, offset + 2), 16) / 255
  ) as [number, number, number];
}

export function rgbToHex(colour: [number, number, number]) {
  return `#${colour
    .map((component) =>
      Math.round(clamp(Number.isFinite(component) ? component : 0, 0, 1) * 255)
        .toString(16)
        .padStart(2, "0")
    )
    .join("")}`;
}

export function annotationLabel(kind: AnnotationKind) {
  switch (kind) {
    case "freehand":
      return "Freehand";
    case "rectangle":
      return "Rectangle";
    case "ellipse":
      return "Ellipse";
    case "highlight":
      return "Highlight";
    case "image":
      return "Image";
    case "line":
      return "Line";
    case "stamp":
      return "Stamp";
    case "text":
      return "Text box";
  }
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}

function editableAnnotationState(annotation: AnnotationDraft) {
  const {
    id: _id,
    sourceAnnotationId: _sourceAnnotationId,
    viewerAnnotationId: _viewerAnnotationId,
    ...state
  } = annotation;
  return JSON.stringify(state);
}
