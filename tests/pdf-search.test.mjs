import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  classifyPdfSearchError,
  countPdfSearchOccurrences,
  normalisePdfSearchText
} from "../src/pdfSearch.ts";

test("normalises compatibility text with the selected interface locale", () => {
  assert.equal(normalisePdfSearchText("  Ｂusiness CARD  ", "en-GB"), "business card");
  assert.equal(normalisePdfSearchText(" IĞDIR İSTANBUL ", "tr-TR"), "ığdır istanbul");
  assert.equal(normalisePdfSearchText("IĞDIR", "en-GB"), "iğdir");
  assert.equal(normalisePdfSearchText("STRAẞE", "de-DE"), "straße");
});

test("counts non-overlapping normalised search matches", () => {
  assert.equal(countPdfSearchOccurrences("business card business card", "business card"), 2);
  assert.equal(countPdfSearchOccurrences("aaaa", "aa"), 2);
  assert.equal(countPdfSearchOccurrences("document", "missing"), 0);
  assert.equal(countPdfSearchOccurrences("document", ""), 0);
});

test("reduces every PDF text extraction failure to one content-free code", () => {
  assert.equal(
    classifyPdfSearchError(new Error("Private parser failure at C:\\Users\\person\\file.pdf")),
    "text-unavailable"
  );
  assert.equal(classifyPdfSearchError("Private document text"), "text-unavailable");
  assert.deepEqual(Object.keys({ code: classifyPdfSearchError(null) }), ["code"]);
});

test("wires locale-aware retryable search and translated canvas outcomes", async () => {
  const [searchHook, canvas, app] = await Promise.all([
    readFile(new URL("../src/usePdfSearch.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/PdfPageCanvas.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  ]);

  assert.match(searchHook, /normalisePdfSearchText\(query, locale\)/u);
  assert.match(searchHook, /classifyPdfSearchError\(reason\)/u);
  assert.match(searchHook, /documentCache\.delete\(pageNumber\)/u);
  assert.doesNotMatch(searchHook, /reason\.message|Document search failed/u);
  assert.match(app, /usePdfSearch\(plannedSearchPages, searchQuery, locale\)/u);
  assert.match(app, /aria-atomic="true"[^>]+aria-live="polite"/u);

  assert.match(canvas, /useI18n\(\)/u);
  assert.match(canvas, /t\("pdfCanvas\.pageAria"/u);
  assert.match(canvas, /role=\{variant === "page" \? "alert" : undefined\}/u);
  assert.doesNotMatch(
    canvas,
    /Rendered PDF page|PDF annotations and form appearances|This page could not be rendered|Display only in Tufekci/u
  );
});
