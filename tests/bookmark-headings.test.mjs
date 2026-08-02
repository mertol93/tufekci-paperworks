import assert from "node:assert/strict";
import test from "node:test";
import {
  detectHeadingSuggestions,
  normaliseHeading
} from "../src/bookmarkHeadings.ts";

test("normalises heading whitespace and compatibility characters", () => {
  assert.equal(normaliseHeading("  １.  Introduction\n"), "1. Introduction");
});

test("detects heading levels while excluding ordinary body lines", () => {
  const suggestions = detectHeadingSuggestions([
    { pageNumber: 1, text: "1 Introduction", fontSize: 20, y: 760 },
    { pageNumber: 1, text: "This is ordinary paragraph text", fontSize: 11, y: 720 },
    { pageNumber: 2, text: "1.1 Scope", fontSize: 16, y: 760 },
    { pageNumber: 2, text: "More ordinary paragraph text", fontSize: 11, y: 720 },
    { pageNumber: 3, text: "1.1.1 Exceptions", fontSize: 14, y: 760 },
    { pageNumber: 3, text: "Further ordinary paragraph text", fontSize: 11, y: 720 }
  ]);

  assert.deepEqual(
    suggestions.map(({ title, pageNumber, level }) => ({ title, pageNumber, level })),
    [
      { title: "1 Introduction", pageNumber: 1, level: 0 },
      { title: "1.1 Scope", pageNumber: 2, level: 1 },
      { title: "1.1.1 Exceptions", pageNumber: 3, level: 2 }
    ]
  );
});

test("removes repeated running headers and duplicate page lines", () => {
  const suggestions = detectHeadingSuggestions([
    { pageNumber: 1, text: "Quarterly Report", fontSize: 16, y: 800 },
    { pageNumber: 2, text: "Quarterly Report", fontSize: 16, y: 800 },
    { pageNumber: 3, text: "Quarterly Report", fontSize: 16, y: 800 },
    { pageNumber: 1, text: "Executive Summary", fontSize: 22, y: 720 },
    { pageNumber: 1, text: "Executive Summary", fontSize: 22, y: 719 },
    { pageNumber: 1, text: "Body copy for the first page", fontSize: 11, y: 680 },
    { pageNumber: 2, text: "Body copy for the second page", fontSize: 11, y: 680 },
    { pageNumber: 3, text: "Body copy for the third page", fontSize: 11, y: 680 }
  ]);

  assert.deepEqual(suggestions.map((item) => item.title), ["Executive Summary"]);
});

test("bounds suggestions per page and overall", () => {
  const headings = Array.from({ length: 20 }, (_, index) => ({
    pageNumber: 1,
    text: `Heading ${index + 1}`,
    fontSize: 20,
    y: 800 - index * 20
  }));
  const body = Array.from({ length: 30 }, (_, index) => ({
    pageNumber: 1,
    text: `ordinary paragraph copy line ${index + 1}`,
    fontSize: 11,
    y: 380 - index * 10
  }));
  const lines = [...headings, ...body];
  assert.equal(detectHeadingSuggestions(lines).length, 8);
  assert.equal(detectHeadingSuggestions(lines, 3).length, 3);
});
