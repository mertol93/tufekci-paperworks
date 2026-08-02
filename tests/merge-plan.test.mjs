import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { displayMergePath, reorderMergePlan } from "../src/mergePlan.ts";

const plan = () => [{ id: "first" }, { id: "second" }, { id: "third" }];

test("reorders a merge source before the selected drop target", () => {
  assert.deepEqual(
    reorderMergePlan(plan(), "first", "third").map((item) => item.id),
    ["second", "first", "third"]
  );
  assert.deepEqual(
    reorderMergePlan(plan(), "third", "first").map((item) => item.id),
    ["third", "first", "second"]
  );
});

test("keeps merge plans unchanged for invalid or identical drag targets", () => {
  const current = plan();
  assert.equal(reorderMergePlan(current, "first", "first"), current);
  assert.equal(reorderMergePlan(current, "missing", "second"), current);
  assert.equal(reorderMergePlan(current, "first", "missing"), current);
});

test("hides Windows transport prefixes without changing ordinary display paths", () => {
  assert.equal(displayMergePath("\\\\?\\C:\\Docs\\source.pdf"), "C:\\Docs\\source.pdf");
  assert.equal(
    displayMergePath("\\\\?\\UNC\\server\\share\\source.pdf"),
    "\\\\server\\share\\source.pdf"
  );
  assert.equal(displayMergePath("/var/tmp/source.pdf"), "/var/tmp/source.pdf");
});

test("connects drag ordering and bookmark preservation to the graphical merge request", async () => {
  const studio = await readFile(new URL("../src/MergeStudio.tsx", import.meta.url), "utf8");
  assert.match(studio, /draggable=\{!busy\}/u);
  assert.match(studio, /onDrop=\{\(event\) => dropSource\(source\.id, event\)\}/u);
  assert.match(studio, /t\("merge\.navigation\.title"\)/u);
  assert.match(studio, /preserveBookmarks,/u);
  assert.match(studio, /takeE2eOpenSelection\(\)/u);
  assert.match(studio, /takeE2eSaveSelection\(\)/u);
});
