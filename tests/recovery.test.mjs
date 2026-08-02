import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  toRecoveryMergeSources,
  toRecoverySplitPlan
} from "../src/recovery.ts";

test("merge recovery keeps plan fields and strips every extra value", () => {
  const sources = toRecoveryMergeSources([
    {
      id: "first",
      pageRange: "1-3, 7",
      password: "source secret",
      path: "C:\\Documents\\First.pdf",
      privateNote: "must not persist"
    }
  ]);

  assert.deepEqual(sources, [
    {
      id: "first",
      pageRange: "1-3, 7",
      sourcePath: "C:\\Documents\\First.pdf"
    }
  ]);
  assert.doesNotMatch(JSON.stringify(sources), /secret|privateNote|password/u);
});

test("split recovery contains only its source and page-group draft", () => {
  assert.deepEqual(
    toRecoverySplitPlan("C:\\Documents\\Source.pdf", "1-3; 7, 9"),
    {
      pageGroups: "1-3; 7, 9",
      sourcePath: "C:\\Documents\\Source.pdf"
    }
  );
  assert.equal(toRecoverySplitPlan(null, "1-3"), null);
});

test("recovery drafts never persist visual-signature artwork or placement state", async () => {
  const [app, recoveryTypes] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/recovery.ts", import.meta.url), "utf8")
  ]);
  const start = app.indexOf("const recoveryDraft = useMemo");
  const end = app.indexOf("const saveRecoveryDraft", start);
  assert.ok(start >= 0 && end > start, "the recovery allow-list must remain statically inspectable");
  const recoveryDraft = app.slice(start, end);

  assert.doesNotMatch(
    recoveryDraft,
    /signatureAssets|signaturePlacements|pngDataUrl|dataUrl|typedValue/u
  );
  assert.doesNotMatch(recoveryTypes, /VisualSignature|pngDataUrl|dataUrl/u);
  assert.match(recoveryDraft, /pages: pagePlan\.pages\.map/u);
});
