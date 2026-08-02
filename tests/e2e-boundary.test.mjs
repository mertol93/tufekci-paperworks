import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { checkProductionE2eBoundary } from "../scripts/check-production-e2e-boundary.mjs";

test("keeps the WebDriver capability behind the dedicated Cargo and Tauri boundary", async () => {
  const [cargo, library, productionConfig, e2eConfig, vite, enabledBridge, disabledBridge, packageJson] =
    await Promise.all([
      readFile(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8"),
      readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
      readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
      readFile(new URL("../src-tauri/tauri.e2e.conf.json", import.meta.url), "utf8"),
      readFile(new URL("../vite.config.ts", import.meta.url), "utf8"),
      readFile(new URL("../src/e2eBridgeEnabled.ts", import.meta.url), "utf8"),
      readFile(new URL("../src/e2eBridgeDisabled.ts", import.meta.url), "utf8"),
      readFile(new URL("../package.json", import.meta.url), "utf8")
    ]);

  assert.match(cargo, /e2e = \["dep:tauri-plugin-wdio", "dep:tauri-plugin-wdio-webdriver"\]/u);
  assert.match(library, /#\[cfg\(feature = "e2e"\)\]/u);
  assert.match(library, /org\.tufekci\.paperworks\.e2e/u);
  assert.doesNotMatch(productionConfig, /wdio|withGlobalTauri|paperworks\.e2e/iu);
  assert.match(e2eConfig, /org\.tufekci\.paperworks\.e2e/u);
  assert.match(e2eConfig, /"withGlobalTauri": true/u);
  assert.match(e2eConfig, /"wdio:default"/u);
  assert.match(vite, /mode === "e2e"[\s\S]{0,100}e2eBridgeEnabled\.ts/u);
  assert.match(enabledBridge, /import "@wdio\/tauri-plugin"/u);
  assert.match(enabledBridge, /delete window\.__paperworksE2eOpenPaths/u);
  assert.match(enabledBridge, /delete window\.__paperworksE2eSavePath/u);
  assert.doesNotMatch(disabledBridge, /@wdio|window/u);
  assert.equal(JSON.parse(packageJson).overrides["@wdio/native-utils"], "2.5.0");
});

test("rejects end-to-end markers in production assets", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "paperworks-production-boundary-"));
  try {
    await mkdir(path.join(root, "assets"));
    await writeFile(path.join(root, "index.html"), "<!doctype html><script src='assets/app.js'></script>\n");
    await writeFile(path.join(root, "assets", "app.js"), "globalThis.paperworks = true;\n");
    const report = await checkProductionE2eBoundary(root);
    assert.equal(report.files, 2);

    await writeFile(
      path.join(root, "assets", "app.js"),
      `globalThis.${["wdio", "Tauri"].join("")} = true;\n`
    );
    await assert.rejects(checkProductionE2eBoundary(root), /end-to-end test marker/u);
  } finally {
    await rm(root, { force: true, recursive: true });
  }
});

test("pins the native desktop evidence viewport across hosted runners", async () => {
  const [nativeSpec, ciWorkflow, releaseWorkflow] = await Promise.all([
    readFile(new URL("../e2e/native-shell.e2e.mjs", import.meta.url), "utf8"),
    readFile(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8"),
    readFile(new URL("../.github/workflows/release.yml", import.meta.url), "utf8")
  ]);

  assert.match(nativeSpec, /browser\.setWindowSize\(1_280, 820\)/u);
  assert.match(nativeSpec, /height: window\.innerHeight/u);
  assert.match(nativeSpec, /width: window\.innerWidth/u);
  assert.doesNotMatch(nativeSpec, /browser\.getWindowSize/u);
  assert.match(nativeSpec, /native window width was/u);
  assert.match(ciWorkflow, /-screen 0 1280x900x24/u);
  assert.match(releaseWorkflow, /-screen 0 1280x900x24/u);
});

test("starts native PDF search only after the visible page finishes rendering", async () => {
  const nativeSpec = await readFile(
    new URL("../e2e/native-shell.e2e.mjs", import.meta.url),
    "utf8"
  );

  assert.match(nativeSpec, /await waitForPageRenderCompletion\(\);/u);
  assert.match(nativeSpec, /\.pdf-canvas-container\.is-page \.pdf-render-state/u);
  assert.match(nativeSpec, /reverse: true/u);
  assert.match(nativeSpec, /Final status:/u);
});
