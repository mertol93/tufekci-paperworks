export type FinishPageInfo = {
  heightPt: number;
  pageNumber: number;
  rotation: number;
  widthPt: number;
};

export type CropPoints = {
  bottomPt: number;
  leftPt: number;
  rightPt: number;
  topPt: number;
};

export type ResizePoints = {
  heightPt: number;
  marginPt: number;
  widthPt: number;
};

export type FinishPreviewLayout = {
  cropHeightPt: number;
  cropWidthPt: number;
  outputHeightPt: number;
  outputWidthPt: number;
  sourceHeightPercent: number;
  sourceLeftPercent: number;
  sourceTopPercent: number;
  sourceWidthPercent: number;
};

export type PaperPreset = {
  heightMm: number;
  id: string;
  name: string;
  widthMm: number;
};

export const finishPaperPresets: PaperPreset[] = [
  { id: "a4", name: "A4", widthMm: 210, heightMm: 297 },
  { id: "letter", name: "US Letter", widthMm: 215.9, heightMm: 279.4 },
  { id: "legal", name: "US Legal", widthMm: 215.9, heightMm: 355.6 },
  { id: "a3", name: "A3", widthMm: 297, heightMm: 420 },
  { id: "a5", name: "A5", widthMm: 148, heightMm: 210 },
  { id: "custom", name: "Custom", widthMm: 210, heightMm: 297 }
];

export function millimetresToPoints(value: number) {
  return (value * 72) / 25.4;
}

export function pointsToMillimetres(value: number) {
  return (value * 25.4) / 72;
}

export function parseFinishPageRange(value: string, pageCount: number) {
  const input = value.trim();
  if (!Number.isInteger(pageCount) || pageCount < 1) {
    return { error: "The PDF does not contain any pages.", pages: [] as number[] };
  }
  if (!input || input.toLocaleLowerCase("en-GB") === "all") {
    return { error: null, pages: Array.from({ length: pageCount }, (_, index) => index + 1) };
  }
  const pages = new Set<number>();
  for (const rawPart of input.split(",")) {
    const part = rawPart.trim();
    if (!part) {
      return { error: "The page selection contains an empty item.", pages: [] as number[] };
    }
    const keyword = part.toLocaleLowerCase("en-GB");
    if (keyword === "odd" || keyword === "even") {
      const parity = keyword === "odd" ? 1 : 0;
      for (let page = 1; page <= pageCount; page += 1) {
        if (page % 2 === parity) pages.add(page);
      }
      continue;
    }
    const match = /^(\d+)(?:\s*-\s*(\d+))?$/.exec(part);
    if (!match) {
      return { error: `“${part}” is not a valid page or range.`, pages: [] as number[] };
    }
    const start = Number(match[1]);
    const end = Number(match[2] ?? match[1]);
    if (start < 1 || start > pageCount || end < 1 || end > pageCount) {
      return {
        error: `The selection must stay between pages 1 and ${pageCount}.`,
        pages: [] as number[]
      };
    }
    for (let page = Math.min(start, end); page <= Math.max(start, end); page += 1) {
      pages.add(page);
    }
  }
  return { error: null, pages: [...pages].sort((left, right) => left - right) };
}

export function computeFinishPreview(
  page: FinishPageInfo,
  crop: CropPoints | null,
  resize: ResizePoints | null
): FinishPreviewLayout | null {
  const left = crop?.leftPt ?? 0;
  const right = crop?.rightPt ?? 0;
  const top = crop?.topPt ?? 0;
  const bottom = crop?.bottomPt ?? 0;
  const cropWidth = page.widthPt - left - right;
  const cropHeight = page.heightPt - top - bottom;
  if (
    ![page.widthPt, page.heightPt, left, right, top, bottom].every(Number.isFinite) ||
    cropWidth < 36 ||
    cropHeight < 36
  ) {
    return null;
  }
  const outputWidth = resize?.widthPt ?? cropWidth;
  const outputHeight = resize?.heightPt ?? cropHeight;
  const margin = resize?.marginPt ?? 0;
  if (
    ![outputWidth, outputHeight, margin].every(Number.isFinite) ||
    outputWidth < 36 ||
    outputHeight < 36 ||
    margin < 0 ||
    margin * 2 + 36 > outputWidth ||
    margin * 2 + 36 > outputHeight
  ) {
    return null;
  }
  const scale = resize
    ? Math.min(
        (outputWidth - margin * 2) / cropWidth,
        (outputHeight - margin * 2) / cropHeight
      )
    : 1;
  const cropLeft = resize ? (outputWidth - cropWidth * scale) / 2 : 0;
  const cropTop = resize ? (outputHeight - cropHeight * scale) / 2 : 0;
  return {
    cropHeightPt: cropHeight,
    cropWidthPt: cropWidth,
    outputHeightPt: outputHeight,
    outputWidthPt: outputWidth,
    sourceHeightPercent: (page.heightPt * scale * 100) / outputHeight,
    sourceLeftPercent: ((cropLeft - left * scale) * 100) / outputWidth,
    sourceTopPercent: ((cropTop - top * scale) * 100) / outputHeight,
    sourceWidthPercent: (page.widthPt * scale * 100) / outputWidth
  };
}

export function expandFinishTemplate(
  template: string,
  pageNumber: number,
  pageCount: number,
  fileName: string
) {
  return template
    .split("{page}").join(String(pageNumber))
    .split("{pages}").join(String(pageCount))
    .split("{file}").join(fileName);
}

export function formatBatesNumber(
  prefix: string,
  suffix: string,
  startNumber: number,
  digits: number,
  selectedIndex: number
) {
  const number = Math.max(0, Math.trunc(startNumber)) + Math.max(0, selectedIndex);
  return `${prefix}${String(number).padStart(Math.max(1, Math.trunc(digits)), "0")}${suffix}`;
}

export function colourToPdfComponents(colour: string): [number, number, number] {
  const match = /^#([0-9a-f]{6})$/i.exec(colour);
  if (!match) return [0, 0, 0];
  return [0, 2, 4].map((offset) => Number.parseInt(match[1].slice(offset, offset + 2), 16) / 255) as [
    number,
    number,
    number
  ];
}
