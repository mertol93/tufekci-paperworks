export const PRINT_DPI = {
  high: 300,
  standard: 150
} as const;

export const MAX_PRINT_PAGES = 100;
export const MAX_PRINT_PAGE_PIXELS = 50_000_000;
export const MAX_PRINT_JOB_PIXELS = 120_000_000;
export const MAX_PRINT_RANGE_CHARACTERS = 512;

export type PrintQuality = keyof typeof PRINT_DPI;
export type PrintRangeMode = "all" | "current" | "custom";

export type PrintRangeErrorCode =
  | "empty"
  | "invalid"
  | "outside-document"
  | "reversed"
  | "too-long"
  | "too-many-pages";

export type PrintRangeResult =
  | { error: null; pages: number[] }
  | { error: PrintRangeErrorCode; pages: [] };

export type PrintPageSize = {
  heightPt: number;
  widthPt: number;
};

export type PrintBudgetErrorCode = "job-too-large" | "page-too-large" | "too-many-pages";

export type PrintBudget =
  | {
      error: null;
      pagePixels: number[];
      totalPixels: number;
    }
  | {
      error: PrintBudgetErrorCode;
      pagePixels: number[];
      totalPixels: number;
    };

export function resolvePrintPages(
  mode: PrintRangeMode,
  pageCount: number,
  currentPage: number,
  customRange: string
): PrintRangeResult {
  if (!Number.isSafeInteger(pageCount) || pageCount < 1) {
    return { error: "outside-document", pages: [] };
  }

  if (mode === "current") {
    if (!Number.isSafeInteger(currentPage) || currentPage < 1 || currentPage > pageCount) {
      return { error: "outside-document", pages: [] };
    }
    return { error: null, pages: [currentPage] };
  }

  if (mode === "all") {
    if (pageCount > MAX_PRINT_PAGES) {
      return { error: "too-many-pages", pages: [] };
    }
    return {
      error: null,
      pages: Array.from({ length: pageCount }, (_, index) => index + 1)
    };
  }

  return parsePrintPageRange(customRange, pageCount);
}

export function parsePrintPageRange(value: string, pageCount: number): PrintRangeResult {
  const range = value.trim();
  if (!range) {
    return { error: "empty", pages: [] };
  }
  if (range.length > MAX_PRINT_RANGE_CHARACTERS) {
    return { error: "too-long", pages: [] };
  }
  if (!Number.isSafeInteger(pageCount) || pageCount < 1) {
    return { error: "outside-document", pages: [] };
  }

  const selected = new Set<number>();
  for (const rawToken of range.split(",")) {
    const token = rawToken.trim();
    const match = /^(\d+)(?:\s*-\s*(\d+))?$/u.exec(token);
    if (!match) {
      return { error: "invalid", pages: [] };
    }

    const start = Number(match[1]);
    const end = match[2] === undefined ? start : Number(match[2]);
    if (
      !Number.isSafeInteger(start) ||
      !Number.isSafeInteger(end) ||
      start < 1 ||
      end < 1 ||
      start > pageCount ||
      end > pageCount
    ) {
      return { error: "outside-document", pages: [] };
    }
    if (start > end) {
      return { error: "reversed", pages: [] };
    }

    for (let page = start; page <= end; page += 1) {
      selected.add(page);
      if (selected.size > MAX_PRINT_PAGES) {
        return { error: "too-many-pages", pages: [] };
      }
    }
  }

  return { error: null, pages: [...selected].sort((left, right) => left - right) };
}

export function calculatePrintBudget(
  pageSizes: readonly PrintPageSize[],
  quality: PrintQuality
): PrintBudget {
  if (pageSizes.length > MAX_PRINT_PAGES) {
    return { error: "too-many-pages", pagePixels: [], totalPixels: 0 };
  }

  const dpi = PRINT_DPI[quality];
  const pagePixels: number[] = [];
  let totalPixels = 0;
  let error: PrintBudgetErrorCode | null = null;

  for (const size of pageSizes) {
    const width = Math.ceil((finitePositive(size.widthPt) * dpi) / 72);
    const height = Math.ceil((finitePositive(size.heightPt) * dpi) / 72);
    const pixels = width * height;
    pagePixels.push(pixels);
    totalPixels += pixels;
    if (pixels > MAX_PRINT_PAGE_PIXELS) {
      error = "page-too-large";
    }
  }

  if (!error && totalPixels > MAX_PRINT_JOB_PIXELS) {
    error = "job-too-large";
  }

  return { error, pagePixels, totalPixels } as PrintBudget;
}

export function rotatedPageSize(size: PrintPageSize, rotation: number): PrintPageSize {
  const normalised = ((Math.round(rotation / 90) * 90) % 360 + 360) % 360;
  return normalised === 90 || normalised === 270
    ? { heightPt: size.widthPt, widthPt: size.heightPt }
    : { heightPt: size.heightPt, widthPt: size.widthPt };
}

function finitePositive(value: number) {
  return Number.isFinite(value) && value > 0 ? value : Number.POSITIVE_INFINITY;
}
