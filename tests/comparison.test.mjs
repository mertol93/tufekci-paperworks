import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  comparePageText,
  comparisonGeometryChanged,
  diffRgbaPixels,
  normaliseComparisonText
} from "../src/comparison.ts";

test("keeps PDF.js comparison bounded, progressive and cancellable", () => {
  const studio = readFileSync(
    new URL("../src/ComparisonStudio.tsx", import.meta.url),
    "utf8"
  );

  assert.match(studio, /const MAX_COMPARISON_PAGES = 2_000/u);
  assert.match(studio, /const MAX_TEXT_CHARACTERS = 500_000/u);
  assert.match(studio, /const MAX_TEXT_ITEMS = 100_000/u);
  assert.match(studio, /const MAX_VISUAL_PIXELS = 2_000_000/u);
  assert.match(studio, /setAnalysisProgress\(progress\)/u);
  assert.match(studio, /analysisRunRef\.current \+= 1/u);
  assert.match(studio, /tasks\.left\.destroy\(\)/u);
  assert.match(studio, /reader\.cancel\(\)/u);
  assert.match(studio, /task\.cancel\(\)/u);
  assert.match(studio, /leftPage\?\.cleanup\(\)/u);
  assert.match(studio, /rightPage\?\.cleanup\(\)/u);
});

test("normalises Unicode and whitespace before comparison", () => {
  assert.equal(normaliseComparisonText("  Cafe\u0301\n\tinvoice  "), "Café invoice");
  assert.equal(comparePageText("Same text", "Same   text").exact, true);
});

test("reports bounded added and removed word counts with useful snippets", () => {
  const result = comparePageText(
    "The approved total is twenty pounds.",
    "The revised total is twenty-five pounds today."
  );

  assert.equal(result.exact, false);
  assert.equal(result.removedWords, 2);
  assert.equal(result.addedWords, 3);
  assert.match(result.leftSnippet, /approved/);
  assert.match(result.rightSnippet, /revised/);
  assert.ok(result.similarity > 50 && result.similarity < 100);
});

test("distinguishes reordered text even when its word inventory matches", () => {
  const result = comparePageText("alpha beta gamma", "gamma beta alpha");
  assert.equal(result.exact, false);
  assert.equal(result.similarity, 100);
  assert.equal(result.addedWords, 0);
  assert.equal(result.removedWords, 0);
});

test("counts words without punctuation and reports the token ceiling", () => {
  const punctuation = comparePageText("Approved.", "Approved!");
  assert.equal(punctuation.leftWordCount, 1);
  assert.equal(punctuation.rightWordCount, 1);

  const densePage = Array.from({ length: 50_001 }, () => "word").join(" ");
  const bounded = comparePageText(densePage, densePage);
  assert.equal(bounded.leftWordCount, 50_000);
  assert.equal(bounded.rightWordCount, 50_000);
  assert.equal(bounded.truncated, true);
});

test("compares page geometry with a points tolerance and normalised rotation", () => {
  assert.equal(
    comparisonGeometryChanged(
      { width: 595, height: 842, rotation: 0 },
      { width: 595.2, height: 842.2, rotation: 360 }
    ),
    false
  );
  assert.equal(
    comparisonGeometryChanged(
      { width: 595, height: 842, rotation: 0 },
      { width: 612, height: 792, rotation: 0 }
    ),
    true
  );
});

test("builds a thresholded visual difference map", () => {
  const left = new Uint8ClampedArray([
    255, 255, 255, 255,
    20, 20, 20, 255
  ]);
  const right = new Uint8ClampedArray([
    250, 250, 250, 255,
    255, 255, 255, 255
  ]);
  const result = diffRgbaPixels(left, right, 12);

  assert.equal(result.changedPixels, 1);
  assert.equal(result.totalPixels, 2);
  assert.equal(result.changedPercent, 50);
  assert.deepEqual([...result.pixels.slice(4, 8)], [194, 58, 53, 255]);
});

test("rejects mismatched visual buffers", () => {
  assert.throws(
    () => diffRgbaPixels(new Uint8ClampedArray(4), new Uint8ClampedArray(8), 20),
    /matching RGBA dimensions/
  );
});
