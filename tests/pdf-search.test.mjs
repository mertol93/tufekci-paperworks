import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  classifyPdfSearchError,
  countPdfSearchOccurrences,
  normalisePdfSearchText
} from "../src/pdfSearch.ts";
import { extractPdfPageText, getPdfPageTextContent } from "../src/pdfText.ts";

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

test("shares one PDF.js text request between rendering and document search", async () => {
  let pageLoads = 0;
  let textLoads = 0;
  let releaseText = () => {};
  const textContent = {
    items: [{ str: "Business" }, { str: "card" }],
    lang: "en-GB",
    styles: {}
  };
  const pendingText = new Promise((resolve) => {
    releaseText = () => resolve(textContent);
  });
  const page = {
    getTextContent() {
      textLoads += 1;
      return pendingText;
    }
  };
  const document = {
    async getPage() {
      pageLoads += 1;
      return page;
    }
  };

  const renderingRequest = getPdfPageTextContent(document, 1, page);
  const searchRequest = getPdfPageTextContent(document, 1);
  assert.strictEqual(searchRequest, renderingRequest);
  releaseText();

  assert.equal(extractPdfPageText(await searchRequest), "Business card");
  assert.equal(textLoads, 1);
  assert.equal(pageLoads, 0);
});

test("evicts rejected PDF.js text requests so a page can be retried", async () => {
  let attempts = 0;
  const document = {
    async getPage() {
      return {
        async getTextContent() {
          attempts += 1;
          if (attempts === 1) {
            throw new Error("transient worker failure");
          }
          return { items: [], lang: null, styles: {} };
        }
      };
    }
  };

  await assert.rejects(getPdfPageTextContent(document, 1), /transient worker failure/u);
  assert.deepEqual(await getPdfPageTextContent(document, 1), {
    items: [],
    lang: null,
    styles: {}
  });
  assert.equal(attempts, 2);
});

test("wires locale-aware shared search and translated canvas outcomes", async () => {
  const [searchHook, sharedText, canvas, app] = await Promise.all([
    readFile(new URL("../src/usePdfSearch.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/pdfText.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/PdfPageCanvas.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8")
  ]);

  assert.match(searchHook, /normalisePdfSearchText\(query, locale\)/u);
  assert.match(searchHook, /classifyPdfSearchError\(reason\)/u);
  assert.match(searchHook, /getPdfPageTextContent\(document, pageNumber\)/u);
  assert.match(sharedText, /documentCache\.delete\(pageNumber\)/u);
  assert.doesNotMatch(searchHook, /reason\.message|Document search failed/u);
  assert.match(app, /usePdfSearch\(plannedSearchPages, searchQuery, locale\)/u);
  assert.match(app, /aria-atomic="true"[^>]+aria-live="polite"/u);

  assert.match(canvas, /useI18n\(\)/u);
  assert.match(canvas, /getPdfPageTextContent\(document, pageNumber, page\)/u);
  assert.doesNotMatch(canvas, /page\.streamTextContent\(\)/u);
  assert.match(canvas, /t\("pdfCanvas\.pageAria"/u);
  assert.match(canvas, /role=\{variant === "page" \? "alert" : undefined\}/u);
  assert.doesNotMatch(
    canvas,
    /Rendered PDF page|PDF annotations and form appearances|This page could not be rendered|Display only in Tufekci/u
  );
});
