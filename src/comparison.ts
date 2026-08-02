export type ComparisonPageGeometry = {
  height: number;
  rotation: number;
  width: number;
};

export type TextComparison = {
  addedWords: number;
  exact: boolean;
  leftSnippet: string;
  leftWordCount: number;
  removedWords: number;
  rightSnippet: string;
  rightWordCount: number;
  similarity: number;
  truncated: boolean;
};

export type PixelDifference = {
  changedPercent: number;
  changedPixels: number;
  pixels: Uint8ClampedArray;
  totalPixels: number;
};

const MAX_COMPARISON_TOKENS = 50_000;
const SNIPPET_CONTEXT_BEFORE = 7;
const SNIPPET_CONTEXT_AFTER = 18;

export function normaliseComparisonText(value: string) {
  return value.normalize("NFKC").replace(/\s+/gu, " ").trim();
}

export function comparePageText(
  leftValue: string,
  rightValue: string,
  leftTruncated = false,
  rightTruncated = false
): TextComparison {
  const left = normaliseComparisonText(leftValue);
  const right = normaliseComparisonText(rightValue);
  const exact = left === right;
  const leftTokenResult = tokenise(left);
  const rightTokenResult = tokenise(right);
  const leftTokens = leftTokenResult.tokens;
  const rightTokens = rightTokenResult.tokens;
  const leftNormalisedTokens = leftTokens.map((token) => token.toLowerCase());
  const rightNormalisedTokens = rightTokens.map((token) => token.toLowerCase());
  const leftCounts = countTokens(leftNormalisedTokens);
  const rightCounts = countTokens(rightNormalisedTokens);
  let commonWords = 0;

  for (const [token, leftCount] of leftCounts) {
    commonWords += Math.min(leftCount, rightCounts.get(token) ?? 0);
  }

  const totalWords = leftTokens.length + rightTokens.length;
  const similarity = totalWords === 0 ? 100 : roundOneDecimal((commonWords * 200) / totalWords);
  const differenceIndex = firstDifferenceIndex(leftNormalisedTokens, rightNormalisedTokens);

  return {
    addedWords: Math.max(0, rightTokens.length - commonWords),
    exact,
    leftSnippet: differenceSnippet(leftTokens, differenceIndex),
    leftWordCount: leftTokens.length,
    removedWords: Math.max(0, leftTokens.length - commonWords),
    rightSnippet: differenceSnippet(rightTokens, differenceIndex),
    rightWordCount: rightTokens.length,
    similarity,
    truncated:
      leftTruncated ||
      rightTruncated ||
      leftTokenResult.truncated ||
      rightTokenResult.truncated
  };
}

export function comparisonGeometryChanged(
  left: ComparisonPageGeometry,
  right: ComparisonPageGeometry,
  tolerance = 0.5
) {
  return (
    Math.abs(left.width - right.width) > tolerance ||
    Math.abs(left.height - right.height) > tolerance ||
    normaliseRotation(left.rotation) !== normaliseRotation(right.rotation)
  );
}

export function diffRgbaPixels(
  left: Uint8ClampedArray,
  right: Uint8ClampedArray,
  threshold: number
): PixelDifference {
  if (left.length !== right.length || left.length % 4 !== 0) {
    throw new Error("Pixel buffers must have matching RGBA dimensions.");
  }
  const boundedThreshold = Math.max(0, Math.min(255, threshold));
  const pixels = new Uint8ClampedArray(left.length);
  let changedPixels = 0;

  for (let offset = 0; offset < left.length; offset += 4) {
    const redDifference = Math.abs(left[offset] - right[offset]);
    const greenDifference = Math.abs(left[offset + 1] - right[offset + 1]);
    const blueDifference = Math.abs(left[offset + 2] - right[offset + 2]);
    const changed = Math.max(redDifference, greenDifference, blueDifference) > boundedThreshold;

    if (changed) {
      changedPixels += 1;
      const leftLuminance = luminance(left, offset);
      const rightLuminance = luminance(right, offset);
      if (rightLuminance + boundedThreshold < leftLuminance) {
        pixels[offset] = 35;
        pixels[offset + 1] = 93;
        pixels[offset + 2] = 216;
      } else if (leftLuminance + boundedThreshold < rightLuminance) {
        pixels[offset] = 194;
        pixels[offset + 1] = 58;
        pixels[offset + 2] = 53;
      } else {
        pixels[offset] = 151;
        pixels[offset + 1] = 64;
        pixels[offset + 2] = 167;
      }
      pixels[offset + 3] = 255;
    } else {
      const grey = Math.round(238 + Math.min(luminance(left, offset), 170) * 0.08);
      pixels[offset] = grey;
      pixels[offset + 1] = grey;
      pixels[offset + 2] = grey;
      pixels[offset + 3] = 255;
    }
  }

  const totalPixels = left.length / 4;
  return {
    changedPercent: totalPixels === 0 ? 0 : roundOneDecimal((changedPixels * 100) / totalPixels),
    changedPixels,
    pixels,
    totalPixels
  };
}

function tokenise(value: string) {
  const matches = value.match(/[\p{L}\p{N}]+(?:['\u2019-][\p{L}\p{N}]+)*/gu) ?? [];
  return {
    tokens: matches.slice(0, MAX_COMPARISON_TOKENS),
    truncated: matches.length > MAX_COMPARISON_TOKENS
  };
}

function countTokens(tokens: string[]) {
  const counts = new Map<string, number>();
  for (const token of tokens) {
    counts.set(token, (counts.get(token) ?? 0) + 1);
  }
  return counts;
}

function firstDifferenceIndex(left: string[], right: string[]) {
  const sharedLength = Math.min(left.length, right.length);
  for (let index = 0; index < sharedLength; index += 1) {
    if (left[index] !== right[index]) {
      return index;
    }
  }
  return left.length === right.length ? -1 : sharedLength;
}

function differenceSnippet(tokens: string[], differenceIndex: number) {
  if (differenceIndex < 0) {
    return "";
  }
  if (tokens.length === 0) {
    return "No selectable text on this page";
  }
  const start = Math.max(0, differenceIndex - SNIPPET_CONTEXT_BEFORE);
  const end = Math.min(tokens.length, differenceIndex + SNIPPET_CONTEXT_AFTER);
  return `${start > 0 ? "... " : ""}${tokens.slice(start, end).join(" ")}${end < tokens.length ? " ..." : ""}`;
}

function normaliseRotation(rotation: number) {
  return ((Math.round(rotation) % 360) + 360) % 360;
}

function luminance(pixels: Uint8ClampedArray, offset: number) {
  return pixels[offset] * 0.2126 + pixels[offset + 1] * 0.7152 + pixels[offset + 2] * 0.0722;
}

function roundOneDecimal(value: number) {
  return Math.round(value * 10) / 10;
}
