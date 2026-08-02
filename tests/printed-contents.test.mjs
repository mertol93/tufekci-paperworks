import assert from "node:assert/strict";
import test from "node:test";
import {
  createPrintedContentsDraft,
  estimatePrintedContentsPageCount,
  printedContentsIsValid,
  printedContentsValidationMessage,
  selectPrintedContentsEntries,
  toPdfPrintedContents
} from "../src/printedContents.ts";

const bookmarks = [
  { level: 0, pageNumber: 1, title: "Introduction" },
  { level: 1, pageNumber: 2, title: "Scope" },
  { level: 2, pageNumber: 3, title: "Exceptions" },
  { level: 3, pageNumber: 4, title: "Examples" }
];

test("selects bounded bookmark levels and estimates A4 contents pages", () => {
  assert.deepEqual(
    selectPrintedContentsEntries(bookmarks, 1).map((bookmark) => bookmark.title),
    ["Introduction", "Scope"]
  );
  assert.equal(estimatePrintedContentsPageCount(0), 0);
  assert.equal(estimatePrintedContentsPageCount(38), 1);
  assert.equal(estimatePrintedContentsPageCount(39), 2);
});

test("keeps disabled printed contents valid and emits no request", () => {
  const draft = createPrintedContentsDraft();
  assert.equal(printedContentsIsValid(draft, []), true);
  assert.equal(toPdfPrintedContents(draft, []), null);
});

test("normalises a valid printed contents request", () => {
  const draft = {
    ...createPrintedContentsDraft(),
    enabled: true,
    maximumLevel: 1,
    title: "  İçindekiler  "
  };
  assert.equal(printedContentsIsValid(draft, bookmarks), true);
  assert.deepEqual(toPdfPrintedContents(draft, bookmarks), {
    addBookmark: true,
    maximumLevel: 1,
    title: "İçindekiler"
  });
});

test("rejects empty, oversized, controlled, and empty-level contents", () => {
  const enabled = { ...createPrintedContentsDraft(), enabled: true };
  assert.match(
    printedContentsValidationMessage({ ...enabled, title: "  " }, bookmarks),
    /Enter a title/u
  );
  assert.match(
    printedContentsValidationMessage({ ...enabled, title: "x".repeat(129) }, bookmarks),
    /at most 128 characters/u
  );
  assert.match(
    printedContentsValidationMessage({ ...enabled, title: "Contents\nprivate" }, bookmarks),
    /control characters/u
  );
  assert.match(
    printedContentsValidationMessage(enabled, [{ ...bookmarks[0], level: 4 }]),
    /Add a bookmark/u
  );
});

test("rejects invalid level bounds before publication", () => {
  const draft = { ...createPrintedContentsDraft(), enabled: true, maximumLevel: 7 };
  assert.equal(printedContentsIsValid(draft, bookmarks), false);
  assert.throws(() => toPdfPrintedContents(draft, bookmarks), /valid bookmark level/u);
});
