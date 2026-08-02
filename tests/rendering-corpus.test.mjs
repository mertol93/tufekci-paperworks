import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  normaliseRenderingText,
  requiredRenderingFixtureFiles,
  summariseRenderedPixels,
  validateRenderingCorpusManifest
} from "../scripts/check-rendering-corpus.mjs";

function validFixture(filename) {
  const rejected = filename === "malformed-truncated.pdf";
  return {
    name: filename,
    filename,
    expectedOutcome: rejected ? "reject" : "render",
    pageCount: rejected ? null : 1,
    password: null,
    requirePasswordChallenge: false,
    samplePages: rejected ? [] : [1],
    minimumInkPixels: rejected ? 0 : 1,
    expectedText: [],
    expectedCodePoints: [],
    requireRtl: false,
    requireNoText: false,
    minimumAnnotations: 0,
    expectedPageSizes: rejected ? [] : [[595, 842]]
  };
}

function completeManifest() {
  return {
    schemaVersion: 1,
    fixtures: requiredRenderingFixtureFiles.map(validFixture)
  };
}

test("accepts the complete bounded rendering-corpus contract", () => {
  const manifest = completeManifest();
  assert.equal(validateRenderingCorpusManifest(manifest), manifest);
});

test("rejects missing, unsafe, duplicate, and inconsistent fixtures", () => {
  const incomplete = completeManifest();
  incomplete.fixtures.pop();
  assert.throws(
    () => validateRenderingCorpusManifest(incomplete),
    /complete required fixture set/u
  );

  const unsafe = completeManifest();
  unsafe.fixtures[0].filename = "../private.pdf";
  assert.throws(() => validateRenderingCorpusManifest(unsafe), /safe plain PDF name/u);

  const duplicate = completeManifest();
  duplicate.fixtures[0].filename = duplicate.fixtures[1].filename.toUpperCase();
  assert.throws(() => validateRenderingCorpusManifest(duplicate), /safe plain PDF name|duplicate/u);

  const badPage = completeManifest();
  badPage.fixtures[0].samplePages = [2];
  assert.throws(() => validateRenderingCorpusManifest(badPage), /sampled pages/u);

  const badChallenge = completeManifest();
  badChallenge.fixtures[0].requirePasswordChallenge = true;
  assert.throws(() => validateRenderingCorpusManifest(badChallenge), /needs a test password/u);
});

test("normalises fixture text without losing multilingual characters", () => {
  assert.equal(
    normaliseRenderingText("  Paperworks\n\u6587\u66f8  \u0627\u062e\u062a\u0628\u0627\u0631 "),
    "Paperworks \u6587\u66f8 \u0627\u062e\u062a\u0628\u0627\u0631"
  );
});

test("summarises rendered RGBA pixels with deterministic evidence", () => {
  const pixels = new Uint8ClampedArray([
    255, 255, 255, 255,
    10, 20, 30, 255,
    220, 30, 40, 255,
    255, 255, 255, 255
  ]);
  const result = summariseRenderedPixels(pixels, 2, 2);
  assert.equal(result.inkPixels, 2);
  assert.equal(result.colourfulPixels, 2);
  assert.equal(result.minimumLuminance, 18);
  assert.equal(result.maximumLuminance, 255);
  assert.match(result.rgbaSha256, /^[0-9A-F]{64}$/u);
  assert.throws(
    () => summariseRenderedPixels(pixels, 3, 2),
    /invalid bounded dimensions/u
  );
});

test("keeps the rendering corpus in cross-platform CI and tagged-release evidence", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [packageJson, ciWorkflow, releaseWorkflow] = await Promise.all([
    readFile(`${workspace}package.json`, "utf8").then(JSON.parse),
    readFile(`${workspace}.github/workflows/ci.yml`, "utf8"),
    readFile(`${workspace}.github/workflows/release.yml`, "utf8")
  ]);

  assert.equal(packageJson.devDependencies["@napi-rs/canvas"], "^1.0.2");
  assert.match(packageJson.scripts["qa:rendering-corpus"], /check-rendering-corpus/u);
  assert.match(
    ciWorkflow,
    /Verify generated PDF\.js rendering corpus[\s\S]+qa:rendering-corpus[\s\S]+rendering-corpus-\$\{\{ runner\.os \}\}/u
  );
  assert.match(
    releaseWorkflow,
    /Verify generated PDF\.js rendering corpus[\s\S]+qa:rendering-corpus[\s\S]+release-rendering-\$\{\{ runner\.os \}\}/u
  );
  assert.match(
    releaseWorkflow,
    /pattern: release-rendering-\*[\s\S]+release-rendering-evidence\/\*/u
  );
});
