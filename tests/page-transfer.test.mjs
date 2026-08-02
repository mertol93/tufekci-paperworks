import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  MAX_TRANSFER_OUTPUT_PAGES,
  canMovePagesBetweenDocuments,
  createPageTransferPlan
} from "../src/pageTransfer.ts";

const source = (id, sourceId, sourcePage, rotation = 0) => ({
  id,
  kind: "source",
  rotation,
  sourceId,
  sourcePage
});

test("inserts selected pages in order at an exact destination boundary", () => {
  const plan = createPageTransferPlan(
    3,
    [source("page-b", "primary", 2, 90), source("page-d", "primary", 4)],
    1
  );

  assert.deepEqual(
    plan.pages.map((page) => [page.id, page.kind, page.rotation]),
    [
      ["destination:source:1", "source", 0],
      ["transfer:page:1", "source", 90],
      ["transfer:page:2", "source", 0],
      ["destination:source:2", "source", 0],
      ["destination:source:3", "source", 0]
    ]
  );
  assert.deepEqual(plan.transferredPageIds, ["transfer:page:1", "transfer:page:2"]);
  assert.equal(plan.pageIdMap.get("page-b"), "transfer:page:1");
});

test("deduplicates source PDFs while retaining imported and blank page details", () => {
  const plan = createPageTransferPlan(
    1,
    [
      source("primary-1", "primary", 1),
      source("primary-2", "primary", 2, 180),
      {
        heightPt: 792,
        id: "blank-1",
        kind: "blank",
        paperName: "Letter",
        rotation: 270,
        widthPt: 612
      },
      source("imported-1", "import-a", 7)
    ],
    1
  );

  assert.deepEqual(plan.sourceMappings, [
    { destinationSourceId: "transfer-source-1", sourceId: "primary" },
    { destinationSourceId: "transfer-source-2", sourceId: "import-a" }
  ]);
  assert.deepEqual(
    plan.pages.slice(1).map((page) =>
      page.kind === "source"
        ? [page.sourceId, page.sourcePage, page.rotation]
        : [page.paperName, page.widthPt, page.heightPt, page.rotation]
    ),
    [
      ["transfer-source-1", 1, 0],
      ["transfer-source-1", 2, 180],
      ["Letter", 612, 792, 270],
      ["transfer-source-2", 7, 0]
    ]
  );
});

test("supports insertion before the first and after the last destination page", () => {
  const before = createPageTransferPlan(2, [source("p1", "primary", 1)], 0);
  const after = createPageTransferPlan(2, [source("p1", "primary", 1)], 2);

  assert.equal(before.pages[0].id, "transfer:page:1");
  assert.equal(after.pages.at(-1).id, "transfer:page:1");
});

test("rejects malformed, duplicate, empty, and oversized transfer plans", () => {
  assert.throws(() => createPageTransferPlan(2, [], 0), /Select at least one/u);
  assert.throws(
    () => createPageTransferPlan(2, [source("same", "primary", 1), source("same", "primary", 2)], 1),
    /unique identifiers/u
  );
  assert.throws(() => createPageTransferPlan(2, [source("p1", "", 1)], 1), /invalid source/u);
  assert.throws(() => createPageTransferPlan(2, [source("p1", "primary", 1)], 3), /insertion point/u);
  assert.throws(
    () => createPageTransferPlan(MAX_TRANSFER_OUTPUT_PAGES, [source("p1", "primary", 1)], 1),
    /exceed/u
  );
});

test("allows move only when at least one source page remains", () => {
  assert.equal(canMovePagesBetweenDocuments(4, 2), true);
  assert.equal(canMovePagesBetweenDocuments(4, 4), false);
  assert.equal(canMovePagesBetweenDocuments(1, 1), false);
  assert.equal(canMovePagesBetweenDocuments(4, 0), false);
});

test("the graphical shell wires reviewed cross-document drag and verified publication", async () => {
  const [app, dialog, jobs] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/PageTransferDialog.tsx", import.meta.url), "utf8").catch(() => ""),
    readFile(new URL("../src/usePdfJob.ts", import.meta.url), "utf8")
  ]);

  assert.match(app, /PageTransferDialog/u);
  assert.match(dialog, /PAGE_TRANSFER_DRAG_TYPE/u);
  assert.match(dialog, /page-import-inspection/u);
  assert.match(dialog, /usePdfJob<ExportResult>\(desktopMode, "page-transfer"\)/u);
  assert.match(dialog, /takeE2eOpenSelection/u);
  assert.match(dialog, /takeE2eSaveSelection/u);
  assert.match(dialog, /onMoveComplete/u);
  assert.match(dialog, /transferSourcePageCount/u);
  assert.match(jobs, /storageScope/u);
});
