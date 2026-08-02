import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  createVisualSignatureAsset,
  createVisualSignaturePlacement,
  duplicateVisualSignaturePlacement,
  mergeDetachedVisualSignaturePlacements,
  moveVisualSignaturePlacement,
  partitionVisualSignaturePlacements,
  resizeVisualSignaturePlacement,
  rotateVisualSignaturePlacement,
  visualSignatureExportPayload,
  visualSignatureHeightRatio
} from "../src/visualSignatures.ts";

const prepared = {
  dataUrl: "data:image/png;base64,c2lnbmF0dXJl",
  height: 100,
  sourceName: "signature.png",
  width: 300
};

const asset = createVisualSignatureAsset(
  "asset:primary",
  "Main signature",
  "signature",
  "image",
  prepared
);

test("creates named signature and initial assets with strict identifiers", () => {
  assert.equal(asset.name, "Main signature");
  assert.equal(asset.kind, "signature");
  assert.throws(
    () => createVisualSignatureAsset("../asset", "Unsafe", "initials", "type", prepared),
    /safe characters/u
  );
  assert.throws(
    () => createVisualSignatureAsset("asset:empty", "   ", "signature", "draw", prepared),
    /between 1 and 256/u
  );
});

test("creates, moves, resizes, and rotates bounded visual placements", () => {
  const pageAspect = 210 / 297;
  let placement = createVisualSignaturePlacement(
    "placement:one",
    asset,
    "page:one",
    pageAspect,
    "right"
  );
  assert.ok(placement.leftRatio > 0.6);
  assert.ok(placement.topRatio > 0.7);

  placement = moveVisualSignaturePlacement(placement, asset, 10, -10, pageAspect);
  assert.ok(placement.leftRatio + placement.widthRatio <= 1);
  assert.ok(placement.topRatio >= 0);

  placement = resizeVisualSignaturePlacement(placement, asset, 4, pageAspect);
  assert.ok(placement.widthRatio <= 0.68);

  placement = rotateVisualSignaturePlacement(placement, asset, 450, pageAspect);
  assert.equal(placement.rotationDegrees, 90);
  const height = visualSignatureHeightRatio(asset, placement.widthRatio, pageAspect);
  assert.ok(placement.topRatio + height <= 1);
});

test("duplicates placements with a fresh unlocked identity", () => {
  const original = {
    ...createVisualSignaturePlacement(
      "placement:one",
      asset,
      "page:one",
      0.7,
      "centre"
    ),
    locked: true
  };
  const duplicate = duplicateVisualSignaturePlacement(
    original,
    "placement:two",
    asset,
    0.7
  );
  assert.equal(duplicate.id, "placement:two");
  assert.equal(duplicate.locked, false);
  assert.notEqual(duplicate.leftRatio, original.leftRatio);
});

test("detaches page-bound placements and retains them for stable-ID page undo", () => {
  const first = createVisualSignaturePlacement(
    "placement:first",
    asset,
    "page:first",
    0.7,
    "centre"
  );
  const second = createVisualSignaturePlacement(
    "placement:second",
    asset,
    "page:second",
    0.7,
    "centre"
  );
  const partitioned = partitionVisualSignaturePlacements(
    [first, second],
    new Set(["page:second"])
  );

  assert.deepEqual(partitioned.attached.map((placement) => placement.id), ["placement:second"]);
  assert.deepEqual(partitioned.detached.map((placement) => placement.id), ["placement:first"]);
  assert.deepEqual(
    mergeDetachedVisualSignaturePlacements([first], [{ ...first, leftRatio: 0.2 }]).map(
      (placement) => [placement.id, placement.leftRatio]
    ),
    [["placement:first", 0.2]]
  );
});

test("builds a deduplicated export payload and maps stable page identities", () => {
  const secondAsset = createVisualSignatureAsset(
    "asset:unused",
    "Unused initials",
    "initials",
    "type",
    prepared
  );
  const first = createVisualSignaturePlacement(
    "placement:first",
    asset,
    "page:second",
    0.7,
    "left"
  );
  const second = duplicateVisualSignaturePlacement(
    first,
    "placement:second",
    asset,
    0.7
  );
  const payload = visualSignatureExportPayload(
    [asset, secondAsset],
    [first, second],
    ["page:first", "page:second"]
  );
  assert.deepEqual(payload.visualSignatureAssets, [
    { id: "asset:primary", pngDataUrl: prepared.dataUrl }
  ]);
  assert.deepEqual(
    payload.visualSignaturePlacements.map((placement) => placement.pageNumber),
    [2, 2]
  );
  assert.equal("locked" in payload.visualSignaturePlacements[0], false);
});

test("rejects missing assets, pages, and duplicate placement identifiers before export", () => {
  const placement = createVisualSignaturePlacement(
    "placement:duplicate",
    asset,
    "page:one",
    0.7
  );
  assert.throws(
    () => visualSignatureExportPayload([], [placement], ["page:one"]),
    /missing session asset/u
  );
  assert.throws(
    () => visualSignatureExportPayload([asset], [placement], []),
    /no longer present/u
  );
  assert.throws(
    () => visualSignatureExportPayload([asset], [placement, placement], ["page:one"]),
    /identifiers must be unique/u
  );
});

test("wires visual marks through private queued export and exact post-save verification", async () => {
  const [app, nativeExport, nativeJobs, nativeVault] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/export.rs", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/pdf_jobs.rs", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/signature_vault.rs", import.meta.url), "utf8")
  ]);

  assert.match(app, /const visualSignatures = visualSignatureExportPayload\(/u);
  assert.match(app, /signature: null,\s*\.\.\.visualSignatures/u);
  assert.match(app, /detachedSignaturePlacementsRef/u);
  assert.match(app, /partitionVisualSignaturePlacements/u);
  assert.match(nativeExport, /visual_signature_matrix\(page_box, page_rotation/u);
  assert.match(nativeExport, /if actual_count != \*expected_count/u);
  assert.match(
    nativeJobs,
    /queued_organise_snapshot_excludes_paths_passwords_and_signature_bytes/u
  );
  assert.match(nativeVault, /const LEGACY_PAYLOAD_VERSION: u8 = 1;/u);
  assert.match(nativeVault, /const PAYLOAD_VERSION: u8 = 2;/u);
  assert.match(nativeVault, /VisualMarkKind::Initials/u);
  assert.match(nativeVault, /VisualMarkMethod::Type/u);
});
