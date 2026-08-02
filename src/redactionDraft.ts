export type NormalisedPoint = {
  x: number;
  y: number;
};

export type NormalisedRect = NormalisedPoint & {
  height: number;
  width: number;
};

export type RedactionColour = "black" | "white";

export type RedactionDraft = {
  colour: RedactionColour;
  id: string;
  label?: string;
  pageNumber: number;
  rect: NormalisedRect;
  source: "manual" | "search";
};

export type RedactionRegionInput = NormalisedRect & {
  colour: RedactionColour;
};

export type RedactionHistory = {
  future: RedactionDraft[][];
  past: RedactionDraft[][];
  present: RedactionDraft[];
};

export type PdfSearchTextItem = {
  hasEOL?: boolean;
  height?: number;
  str: string;
  transform: number[];
  width: number;
};

export type SearchIndexSegment = {
  end: number;
  rect: NormalisedRect;
  start: number;
};

export type PageSearchIndex = {
  segments: SearchIndexSegment[];
  text: string;
};

export type SearchMode = "email" | "literal" | "pattern";

export type SearchMatch = {
  end: number;
  rects: NormalisedRect[];
  start: number;
  text: string;
};

export type SearchMatchResult = {
  error: string | null;
  matches: SearchMatch[];
  truncated: boolean;
};

const MIN_RECT_SIZE = 0.002;
const SEARCH_RECT_PADDING = 0.0025;
const MAX_HISTORY_STEPS = 100;

export function toRedactionRegionInput(redaction: RedactionDraft): RedactionRegionInput {
  return {
    colour: redaction.colour,
    height: redaction.rect.height,
    width: redaction.rect.width,
    x: redaction.rect.x,
    y: redaction.rect.y
  };
}

export function createRedactionHistory(redactions: RedactionDraft[] = []): RedactionHistory {
  return { future: [], past: [], present: redactions };
}

export function commitRedactionHistory(
  history: RedactionHistory,
  redactions: RedactionDraft[]
): RedactionHistory {
  if (sameDrafts(history.present, redactions)) {
    return history;
  }
  return {
    future: [],
    past: [...history.past.slice(-(MAX_HISTORY_STEPS - 1)), history.present],
    present: redactions
  };
}

export function undoRedactionHistory(history: RedactionHistory): RedactionHistory {
  const previous = history.past[history.past.length - 1];
  if (!previous) {
    return history;
  }
  return {
    future: [history.present, ...history.future],
    past: history.past.slice(0, -1),
    present: previous
  };
}

export function redoRedactionHistory(history: RedactionHistory): RedactionHistory {
  const next = history.future[0];
  if (!next) {
    return history;
  }
  return {
    future: history.future.slice(1),
    past: [...history.past, history.present],
    present: next
  };
}

export function normalisedPoint(
  clientX: number,
  clientY: number,
  bounds: Pick<DOMRect, "height" | "left" | "top" | "width">
): NormalisedPoint {
  return {
    x: clamp((clientX - bounds.left) / Math.max(1, bounds.width), 0, 1),
    y: clamp((clientY - bounds.top) / Math.max(1, bounds.height), 0, 1)
  };
}

export function rectBetween(start: NormalisedPoint, end: NormalisedPoint): NormalisedRect {
  const x = Math.min(start.x, end.x);
  const y = Math.min(start.y, end.y);
  return {
    x,
    y,
    width: Math.max(0, Math.max(start.x, end.x) - x),
    height: Math.max(0, Math.max(start.y, end.y) - y)
  };
}

export function isUsableRedactionRect(rect: NormalisedRect) {
  return rect.width >= MIN_RECT_SIZE && rect.height >= MIN_RECT_SIZE;
}

export function translateRedactionRect(
  rect: NormalisedRect,
  start: NormalisedPoint,
  current: NormalisedPoint
): NormalisedRect {
  return {
    ...rect,
    x: clamp(rect.x + current.x - start.x, 0, Math.max(0, 1 - rect.width)),
    y: clamp(rect.y + current.y - start.y, 0, Math.max(0, 1 - rect.height))
  };
}

export function buildPageSearchIndex(
  items: PdfSearchTextItem[],
  viewportTransform: number[],
  viewportWidth: number,
  viewportHeight: number
): PageSearchIndex {
  if (viewportTransform.length !== 6 || viewportWidth <= 0 || viewportHeight <= 0) {
    return { segments: [], text: "" };
  }
  let text = "";
  const segments: SearchIndexSegment[] = [];
  let previous: { hasEOL: boolean; rect: NormalisedRect } | null = null;

  for (const item of items) {
    if (!item.str || item.transform.length !== 6 || !Number.isFinite(item.width)) {
      continue;
    }
    const rect = textItemRect(item, viewportTransform, viewportWidth, viewportHeight);
    if (!rect) {
      continue;
    }
    if (previous) {
      const lineChange =
        previous.hasEOL ||
        Math.abs(previous.rect.y + previous.rect.height / 2 - (rect.y + rect.height / 2)) >
          Math.max(previous.rect.height, rect.height) * 0.8;
      const gap = rect.x - (previous.rect.x + previous.rect.width);
      if (lineChange) {
        text += "\n";
      } else if (gap > Math.min(previous.rect.height, rect.height) * 0.08) {
        text += " ";
      }
    }
    const start = text.length;
    text += item.str;
    segments.push({ end: text.length, rect, start });
    previous = { hasEOL: Boolean(item.hasEOL), rect };
  }

  return { segments, text };
}

export function findPageSearchMatches(
  index: PageSearchIndex,
  mode: SearchMode,
  query: string,
  matchCase: boolean,
  maximum = 1_000
): SearchMatchResult {
  const expression = searchExpression(mode, query, matchCase);
  if (typeof expression === "string") {
    return { error: expression, matches: [], truncated: false };
  }
  const matches: SearchMatch[] = [];
  let result: RegExpExecArray | null;
  while ((result = expression.exec(index.text)) !== null) {
    if (!result[0]) {
      expression.lastIndex += 1;
      continue;
    }
    const start = result.index;
    const end = start + result[0].length;
    const rects = rectanglesForRange(index.segments, start, end);
    if (rects.length > 0) {
      matches.push({ end, rects, start, text: result[0] });
    }
    if (matches.length >= maximum) {
      return { error: null, matches, truncated: expression.exec(index.text) !== null };
    }
  }
  return { error: null, matches, truncated: false };
}

function searchExpression(
  mode: SearchMode,
  query: string,
  matchCase: boolean
): RegExp | string {
  const flags = matchCase ? "gu" : "giu";
  if (mode === "email") {
    return /[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?(?:\.[A-Z0-9](?:[A-Z0-9-]{0,61}[A-Z0-9])?)+/giu;
  }

  const value = query.trim();
  if (!value) {
    return mode === "pattern"
      ? "Enter a wildcard pattern. Use * for a short run, ? for one character, or # for one digit."
      : "Enter text or a name to find.";
  }
  if (value.length > 512) {
    return "Search text can contain at most 512 characters.";
  }
  if (mode === "literal") {
    return new RegExp(escapeRegularExpression(value), flags);
  }
  if (!/[?#]|[^*\s]/u.test(value)) {
    return "A wildcard pattern cannot contain only asterisks.";
  }
  let source = "";
  for (const character of value) {
    if (character === "*") {
      source += "[^\\r\\n]{0,80}?";
    } else if (character === "?") {
      source += "[^\\r\\n]";
    } else if (character === "#") {
      source += "\\d";
    } else {
      source += escapeRegularExpression(character);
    }
  }
  return new RegExp(source, flags);
}

function textItemRect(
  item: PdfSearchTextItem,
  viewportTransform: number[],
  viewportWidth: number,
  viewportHeight: number
): NormalisedRect | null {
  const transform = multiplyTransforms(viewportTransform, item.transform);
  const baselineLength = Math.hypot(viewportTransform[0], viewportTransform[1]) * item.width;
  const baselineScale = Math.hypot(transform[0], transform[1]);
  const verticalX = transform[2];
  const verticalY = transform[3];
  const baselineX = baselineScale > 0 ? (transform[0] / baselineScale) * baselineLength : baselineLength;
  const baselineY = baselineScale > 0 ? (transform[1] / baselineScale) * baselineLength : 0;
  const points = [
    [transform[4], transform[5]],
    [transform[4] + baselineX, transform[5] + baselineY],
    [transform[4] + verticalX, transform[5] + verticalY],
    [transform[4] + baselineX + verticalX, transform[5] + baselineY + verticalY]
  ];
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  const left = clamp(Math.min(...xs) / viewportWidth, 0, 1);
  const right = clamp(Math.max(...xs) / viewportWidth, 0, 1);
  const top = clamp(Math.min(...ys) / viewportHeight, 0, 1);
  const bottom = clamp(Math.max(...ys) / viewportHeight, 0, 1);
  const width = right - left;
  const height = bottom - top;
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return null;
  }
  return { x: left, y: top, width, height };
}

function rectanglesForRange(
  segments: SearchIndexSegment[],
  start: number,
  end: number
): NormalisedRect[] {
  const pieces = segments
    .filter((segment) => segment.start < end && segment.end > start)
    .map((segment) => {
      const length = Math.max(1, segment.end - segment.start);
      const localStart = clamp((start - segment.start) / length, 0, 1);
      const localEnd = clamp((end - segment.start) / length, 0, 1);
      return {
        x: segment.rect.x + segment.rect.width * localStart,
        y: segment.rect.y,
        width: segment.rect.width * Math.max(0, localEnd - localStart),
        height: segment.rect.height
      };
    })
    .filter((rect) => rect.width > 0 && rect.height > 0)
    .sort((left, right) => left.y - right.y || left.x - right.x);

  const lines: NormalisedRect[] = [];
  for (const piece of pieces) {
    const previous = lines[lines.length - 1];
    const sameLine =
      previous &&
      Math.abs(previous.y + previous.height / 2 - (piece.y + piece.height / 2)) <=
        Math.max(previous.height, piece.height) * 0.65;
    if (sameLine && previous) {
      const right = Math.max(previous.x + previous.width, piece.x + piece.width);
      const bottom = Math.max(previous.y + previous.height, piece.y + piece.height);
      previous.x = Math.min(previous.x, piece.x);
      previous.y = Math.min(previous.y, piece.y);
      previous.width = right - previous.x;
      previous.height = bottom - previous.y;
    } else {
      lines.push({ ...piece });
    }
  }
  return lines.map((rect) => padRect(rect, SEARCH_RECT_PADDING));
}

function padRect(rect: NormalisedRect, padding: number): NormalisedRect {
  const x = clamp(rect.x - padding, 0, 1);
  const y = clamp(rect.y - padding, 0, 1);
  const right = clamp(rect.x + rect.width + padding, 0, 1);
  const bottom = clamp(rect.y + rect.height + padding, 0, 1);
  return { x, y, width: right - x, height: bottom - y };
}

function multiplyTransforms(left: number[], right: number[]) {
  return [
    left[0] * right[0] + left[2] * right[1],
    left[1] * right[0] + left[3] * right[1],
    left[0] * right[2] + left[2] * right[3],
    left[1] * right[2] + left[3] * right[3],
    left[0] * right[4] + left[2] * right[5] + left[4],
    left[1] * right[4] + left[3] * right[5] + left[5]
  ];
}

function escapeRegularExpression(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function sameDrafts(left: RedactionDraft[], right: RedactionDraft[]) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}
