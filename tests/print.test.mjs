import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  MAX_PRINT_JOB_PIXELS,
  MAX_PRINT_PAGES,
  calculatePrintBudget,
  parsePrintPageRange,
  resolvePrintPages,
  rotatedPageSize
} from "../src/print.ts";

test("print ranges accept pages and ranges, remove duplicates and preserve document order", () => {
  assert.deepEqual(parsePrintPageRange("5, 2-4, 3, 8", 10), {
    error: null,
    pages: [2, 3, 4, 5, 8]
  });
});

test("print ranges fail closed for malformed, reversed and out-of-document values", () => {
  assert.equal(parsePrintPageRange("1,,2", 4).error, "invalid");
  assert.equal(parsePrintPageRange("3-2", 4).error, "reversed");
  assert.equal(parsePrintPageRange("0,1", 4).error, "outside-document");
  assert.equal(parsePrintPageRange("1-5", 4).error, "outside-document");
});

test("print selection resolves all and current-page modes without parsing free text", () => {
  assert.deepEqual(resolvePrintPages("all", 3, 2, ""), {
    error: null,
    pages: [1, 2, 3]
  });
  assert.deepEqual(resolvePrintPages("current", 3, 2, "not a range"), {
    error: null,
    pages: [2]
  });
});

test("print selection enforces its bounded page count", () => {
  assert.equal(resolvePrintPages("all", MAX_PRINT_PAGES + 1, 1, "").error, "too-many-pages");
  assert.equal(
    parsePrintPageRange(`1-${MAX_PRINT_PAGES + 1}`, MAX_PRINT_PAGES + 1).error,
    "too-many-pages"
  );
});

test("print budgets use physical PDF points and reject an oversized aggregate job", () => {
  const a4 = { heightPt: 841.89, widthPt: 595.28 };
  const standard = calculatePrintBudget([a4], "standard");
  assert.equal(standard.error, null);
  assert.ok(standard.totalPixels > 2_000_000);
  assert.ok(standard.totalPixels < 3_000_000);

  const high = calculatePrintBudget([a4], "high");
  const pagesNeeded = Math.floor(MAX_PRINT_JOB_PIXELS / high.totalPixels) + 1;
  const oversized = calculatePrintBudget(Array.from({ length: pagesNeeded }, () => a4), "high");
  assert.equal(oversized.error, "job-too-large");
});

test("print geometry swaps physical dimensions for quarter turns", () => {
  assert.deepEqual(rotatedPageSize({ heightPt: 800, widthPt: 600 }, 90), {
    heightPt: 600,
    widthPt: 800
  });
  assert.deepEqual(rotatedPageSize({ heightPt: 800, widthPt: 600 }, 180), {
    heightPt: 800,
    widthPt: 600
  });
});

test("print preparation uses PDF print intent, current form storage and volatile preview URLs", async () => {
  const [renderer, studio, productionBridge] = await Promise.all([
    readFile(new URL("../src/printRenderer.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/PrintStudio.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/e2eBridgeDisabled.ts", import.meta.url), "utf8")
  ]);

  assert.match(renderer, /intent: "print"/u);
  assert.match(renderer, /AnnotationMode\.ENABLE_STORAGE/u);
  assert.match(renderer, /URL\.createObjectURL\(blob\)/u);
  assert.match(renderer, /URL\.revokeObjectURL\(url\)/u);
  assert.match(renderer, /placement\.pageId === checked\.source\.id/u);
  assert.match(studio, /requestSystemPrint\(\)/u);
  assert.match(studio, /createPortal\(/u);
  assert.match(productionBridge, /globalThis\.print\(\)/u);
});
