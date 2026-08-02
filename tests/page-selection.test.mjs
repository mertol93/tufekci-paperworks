import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  canMovePagesByStep,
  movePagesByStep,
  reorderPagesAtDrop,
  resolvePageSelection
} from "../src/pageSelection.ts";

const ids = ["a", "b", "c", "d", "e"];
const pages = ids.map((id) => ({ id }));
const pageIds = (value) => value.map((page) => page.id);

test("page selection supports single and ordered toggle selection without becoming empty", () => {
  assert.deepEqual(resolvePageSelection(ids, ["c"], "c", "c", "a", "single"), {
    activeId: "a",
    anchorId: "a",
    selectedIds: ["a"]
  });
  assert.deepEqual(resolvePageSelection(ids, ["c", "a"], "c", "c", "e", "toggle"), {
    activeId: "e",
    anchorId: "e",
    selectedIds: ["a", "c", "e"]
  });
  assert.deepEqual(resolvePageSelection(ids, ["c"], "c", "c", "c", "toggle"), {
    activeId: "c",
    anchorId: "c",
    selectedIds: ["c"]
  });
});

test("page selection creates and extends anchored document-order ranges", () => {
  assert.deepEqual(resolvePageSelection(ids, ["b"], "b", "b", "d", "range"), {
    activeId: "d",
    anchorId: "b",
    selectedIds: ["b", "c", "d"]
  });
  assert.deepEqual(resolvePageSelection(ids, ["a"], "a", "b", "d", "extend-range"), {
    activeId: "d",
    anchorId: "b",
    selectedIds: ["a", "b", "c", "d"]
  });
});

test("group drag preserves selected page order above and below the target", () => {
  assert.deepEqual(pageIds(reorderPagesAtDrop(pages, ["a", "b"], "a", "d")), [
    "c",
    "d",
    "a",
    "b",
    "e"
  ]);
  assert.deepEqual(pageIds(reorderPagesAtDrop(pages, ["d", "e"], "e", "b")), [
    "a",
    "d",
    "e",
    "b",
    "c"
  ]);
  assert.equal(reorderPagesAtDrop(pages, ["b", "c"], "b", "c"), pages);
});

test("dragging an unselected page does not unexpectedly move the existing selection", () => {
  assert.deepEqual(pageIds(reorderPagesAtDrop(pages, ["a", "b"], "d", "b")), [
    "a",
    "d",
    "b",
    "c",
    "e"
  ]);
});

test("step movement keeps non-contiguous selections stable and reports boundaries", () => {
  assert.deepEqual(pageIds(movePagesByStep(pages, ["b", "d"], -1)), [
    "b",
    "a",
    "d",
    "c",
    "e"
  ]);
  assert.deepEqual(pageIds(movePagesByStep(pages, ["b", "d"], 1)), [
    "a",
    "c",
    "b",
    "e",
    "d"
  ]);
  assert.equal(canMovePagesByStep(pages, ["a", "b"], -1), false);
  assert.equal(canMovePagesByStep(pages, ["a", "b"], 1), true);
  assert.equal(canMovePagesByStep(pages, ["d", "e"], 1), false);
});

test("the graphical organiser connects selection controls and group history operations", async () => {
  const [app, pagePlan] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/usePagePlan.ts", import.meta.url), "utf8")
  ]);

  assert.match(app, /aria-pressed=\{selectedPageIdSet\.has\(plannedPage\.id\)\}/u);
  assert.match(app, /pageSelectionModeFromModifiers\(event\)/u);
  assert.match(app, /application\/x-tufekci-paperworks-pages/u);
  assert.match(app, /pagePlan\.moveManyAtDrop\(/u);
  assert.match(app, /pagePlan\.rotateMany\(effectiveSelectedPageIds\)/u);
  assert.match(app, /pagePlan\.removeMany\(effectiveSelectedPageIds\)/u);
  assert.match(app, /pagePlan\.duplicateMany\(effectiveSelectedPageIds\)/u);
  assert.match(pagePlan, /commit\(\(pages\) => reorderPagesAtDrop/u);
  assert.match(pagePlan, /commit\(\(pages\) => movePagesByStep/u);
});
