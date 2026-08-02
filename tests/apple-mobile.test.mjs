import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { validateAppleMobileConfiguration } from "../scripts/check-apple-mobile.mjs";

const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const organiser = readFileSync(new URL("../src/pageSelection.ts", import.meta.url), "utf8");
const merge = readFileSync(new URL("../src/MergeStudio.tsx", import.meta.url), "utf8");
const signatureLayer = readFileSync(
  new URL("../src/VisualSignatureLayer.tsx", import.meta.url),
  "utf8"
);
const directProbeSources = [
  "archive.rs",
  "certificate.rs",
  "ocr.rs",
  "pdf_tools.rs",
  "protection.rs"
].map((name) =>
  readFileSync(new URL(`../src-tauri/src/${name}`, import.meta.url), "utf8")
);

test("validates the checked-in iPhone and iPad build contract", () => {
  assert.deepEqual(validateAppleMobileConfiguration(), {
    minimumSystemVersion: "16.0",
    simulatorRunner: "macos-15",
    supportsIPhone: true,
    supportsIPad: true
  });
});

test("keeps touch alternatives for page, merge and signature manipulation", () => {
  assert.match(app, /moveSelectedPage\(-1\)/u);
  assert.match(app, /moveSelectedPage\(1\)/u);
  assert.match(organiser, /movePagesByStep/u);
  assert.match(merge, /ArrowUp/u);
  assert.match(merge, /ArrowDown/u);
  assert.match(signatureLayer, /onPointerDown/u);
  assert.match(signatureLayer, /setPointerCapture/u);
});

test("gates desktop engines while retaining the native mobile PDF core", () => {
  assert.match(app, /runtimeSupportsOcr/u);
  assert.match(app, /runtimeSupportsCertificateSigning/u);
  assert.match(app, /runtimeSupportsArchivalPdf/u);
  assert.match(app, /connectedScanningAvailable/u);
  assert.match(app, /runtime\.mobile/u);
  assert.match(app, /setMode\("desktop"\)/u);
  for (const source of directProbeSources) {
    assert.match(source, /current_capabilities/u);
  }
});
