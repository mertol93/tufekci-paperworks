export type HeadingLine = {
  fontSize: number;
  pageNumber: number;
  text: string;
  y: number;
};

export type HeadingSuggestion = {
  confidence: number;
  fontSize: number;
  level: number;
  pageNumber: number;
  title: string;
};

const MAX_HEADING_CHARACTERS = 160;
const MAX_HEADING_WORDS = 24;
const MAX_HEADINGS_PER_PAGE = 8;

export function detectHeadingSuggestions(
  rawLines: HeadingLine[],
  maximumSuggestions = 1_000
): HeadingSuggestion[] {
  const lines = rawLines
    .filter(
      (line) =>
        Number.isFinite(line.fontSize) &&
        line.fontSize > 0 &&
        Number.isInteger(line.pageNumber) &&
        line.pageNumber > 0 &&
        Number.isFinite(line.y)
    )
    .map((line) => ({ ...line, text: normaliseHeading(line.text) }))
    .filter((line) => isPlausibleLine(line.text));
  if (lines.length === 0 || maximumSuggestions <= 0) {
    return [];
  }

  const bodySize = median(lines.map((line) => line.fontSize));
  const pageOccurrences = new Map<string, Set<number>>();
  for (const line of lines) {
    const key = line.text.toLowerCase();
    const pages = pageOccurrences.get(key) ?? new Set<number>();
    pages.add(line.pageNumber);
    pageOccurrences.set(key, pages);
  }

  const candidates = lines.filter((line) => {
    if ((pageOccurrences.get(line.text.toLowerCase())?.size ?? 0) >= 3) {
      return false;
    }
    const numbered = headingNumberDepth(line.text) !== null;
    const capitalised = looksLikeHeadingCase(line.text);
    return (
      line.fontSize >= bodySize * 1.18 ||
      (numbered && line.fontSize >= bodySize * 0.96) ||
      (capitalised && line.fontSize >= bodySize * 1.08)
    );
  });
  const sizeTiers = [...new Set(candidates.map((line) => roundHalf(line.fontSize)))]
    .sort((left, right) => right - left)
    .slice(0, 4);
  const seen = new Set<string>();
  const perPage = new Map<number, number>();
  const suggestions: HeadingSuggestion[] = [];

  candidates
    .sort((left, right) => left.pageNumber - right.pageNumber || right.y - left.y)
    .forEach((line) => {
      if (suggestions.length >= maximumSuggestions) {
        return;
      }
      const key = `${line.pageNumber}:${line.text.toLowerCase()}`;
      if (seen.has(key) || (perPage.get(line.pageNumber) ?? 0) >= MAX_HEADINGS_PER_PAGE) {
        return;
      }
      seen.add(key);
      perPage.set(line.pageNumber, (perPage.get(line.pageNumber) ?? 0) + 1);
      const sizeLevel = Math.min(3, Math.max(0, sizeTiers.indexOf(roundHalf(line.fontSize))));
      const numberedLevel = headingNumberDepth(line.text);
      const level = numberedLevel === null ? sizeLevel : Math.min(3, numberedLevel);
      const sizeRatio = line.fontSize / Math.max(bodySize, 0.1);
      suggestions.push({
        confidence: Math.round(Math.min(99, 62 + Math.max(0, sizeRatio - 1) * 32)),
        fontSize: Math.round(line.fontSize * 10) / 10,
        level,
        pageNumber: line.pageNumber,
        title: line.text
      });
    });

  return suggestions;
}

export function normaliseHeading(value: string) {
  return value.normalize("NFKC").replace(/\s+/gu, " ").trim();
}

function isPlausibleLine(value: string) {
  if (
    value.length < 3 ||
    value.length > MAX_HEADING_CHARACTERS ||
    value.split(/\s+/u).length > MAX_HEADING_WORDS ||
    /^(?:[ivxlcdm]+|\d+)[.)-]?$/iu.test(value) ||
    /^(?:https?:\/\/|www\.)/iu.test(value)
  ) {
    return false;
  }
  return /[\p{L}\p{N}]/u.test(value);
}

function looksLikeHeadingCase(value: string) {
  const letters = [...value].filter((character) => /\p{L}/u.test(character));
  if (letters.length < 3) {
    return false;
  }
  const upperCaseRatio = letters.filter(
    (character) => character === character.toLocaleUpperCase() && character !== character.toLocaleLowerCase()
  ).length / letters.length;
  const first = letters[0];
  return upperCaseRatio >= 0.72 || first === first.toLocaleUpperCase();
}

function headingNumberDepth(value: string) {
  const match = /^(\d+(?:\.\d+){0,4})[.)]?\s+/u.exec(value);
  return match ? match[1].split(".").length - 1 : null;
}

function median(values: number[]) {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}

function roundHalf(value: number) {
  return Math.round(value * 2) / 2;
}
