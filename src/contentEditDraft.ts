export type ContentRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type InspectedContentText = {
  sourceId: string;
  pageNumber: number;
  text: string;
  rect: ContentRect;
  fontLabel: string;
  fontSize: number;
};

export type InspectedContentImage = {
  sourceId: string;
  pageNumber: number;
  rect: ContentRect;
  pixelWidth: number;
  pixelHeight: number;
};

export type ContentTextDraft = InspectedContentText & {
  originalText: string;
};

export type ContentImageDraft = InspectedContentImage & {
  deleted: boolean;
  originalRect: ContentRect;
  replacementImageDataUrl: string | null;
};

export type ContentEditDraft = {
  images: ContentImageDraft[];
  text: ContentTextDraft[];
};

export type ContentEditHistory = {
  future: ContentEditDraft[];
  past: ContentEditDraft[];
  present: ContentEditDraft;
};

export type ContentEditPayload = {
  imageEdits: Array<{
    sourceId: string;
    delete: boolean;
    replacementImageDataUrl: string | null;
    rect: ContentRect;
  }>;
  textEdits: Array<{
    sourceId: string;
    replacementText: string;
  }>;
};

const HISTORY_LIMIT = 100;
const MIN_RECT_SIZE = 0.002;

export function contentDraftFromInspection(
  text: InspectedContentText[],
  images: InspectedContentImage[]
): ContentEditDraft {
  return {
    images: images.map((image) => ({
      ...image,
      deleted: false,
      originalRect: { ...image.rect },
      rect: { ...image.rect },
      replacementImageDataUrl: null
    })),
    text: text.map((run) => ({
      ...run,
      originalText: run.text,
      rect: { ...run.rect }
    }))
  };
}

export function createContentEditHistory(
  draft: ContentEditDraft = { images: [], text: [] }
): ContentEditHistory {
  return { future: [], past: [], present: draft };
}

export function commitContentEditHistory(
  history: ContentEditHistory,
  next: ContentEditDraft
): ContentEditHistory {
  if (next === history.present) {
    return history;
  }
  return {
    future: [],
    past: [...history.past.slice(-(HISTORY_LIMIT - 1)), history.present],
    present: next
  };
}

export function undoContentEditHistory(history: ContentEditHistory): ContentEditHistory {
  const previous = history.past.at(-1);
  if (!previous) {
    return history;
  }
  return {
    future: [history.present, ...history.future].slice(0, HISTORY_LIMIT),
    past: history.past.slice(0, -1),
    present: previous
  };
}

export function redoContentEditHistory(history: ContentEditHistory): ContentEditHistory {
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

export function updateContentText(
  draft: ContentEditDraft,
  sourceId: string,
  text: string
): ContentEditDraft {
  return {
    ...draft,
    text: draft.text.map((run) => (run.sourceId === sourceId ? { ...run, text } : run))
  };
}

export function updateContentImage(
  draft: ContentEditDraft,
  sourceId: string,
  updates: Partial<Pick<ContentImageDraft, "deleted" | "rect" | "replacementImageDataUrl">>
): ContentEditDraft {
  return {
    ...draft,
    images: draft.images.map((image) =>
      image.sourceId === sourceId
        ? {
            ...image,
            ...updates,
            rect: updates.rect ? boundedContentRect(updates.rect) : image.rect
          }
        : image
    )
  };
}

export function translateContentImage(
  image: ContentImageDraft,
  dx: number,
  dy: number
): ContentRect {
  return boundedContentRect({
    ...image.rect,
    x: image.rect.x + dx,
    y: image.rect.y + dy
  });
}

export function boundedContentRect(rect: ContentRect): ContentRect {
  const width = clamp(finite(rect.width, MIN_RECT_SIZE), MIN_RECT_SIZE, 1);
  const height = clamp(finite(rect.height, MIN_RECT_SIZE), MIN_RECT_SIZE, 1);
  return {
    width,
    height,
    x: clamp(finite(rect.x, 0), 0, 1 - width),
    y: clamp(finite(rect.y, 0), 0, 1 - height)
  };
}

export function contentEditPayload(draft: ContentEditDraft): ContentEditPayload {
  return {
    textEdits: draft.text
      .filter((run) => run.text !== run.originalText)
      .map((run) => ({
        replacementText: run.text,
        sourceId: run.sourceId
      })),
    imageEdits: draft.images
      .filter(
        (image) =>
          image.deleted ||
          image.replacementImageDataUrl !== null ||
          !sameRect(image.rect, image.originalRect)
      )
      .map((image) => ({
        delete: image.deleted,
        rect: { ...image.rect },
        replacementImageDataUrl: image.deleted ? null : image.replacementImageDataUrl,
        sourceId: image.sourceId
      }))
  };
}

export function contentEditCount(draft: ContentEditDraft): number {
  const payload = contentEditPayload(draft);
  return payload.textEdits.length + payload.imageEdits.length;
}

function sameRect(left: ContentRect, right: ContentRect): boolean {
  return (
    left.x === right.x &&
    left.y === right.y &&
    left.width === right.width &&
    left.height === right.height
  );
}

function finite(value: number, fallback: number) {
  return Number.isFinite(value) ? value : fallback;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
