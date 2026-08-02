import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  normaliseOcrExpectedText,
  requiredOcrFixtures,
  summariseOcrPixels,
  validateOcrCorpusManifest
} from "../scripts/check-ocr-corpus.mjs";
import {
  createDeterministicRandom,
  expectedTextForFixture,
  ocrFixtureDefinitions
} from "../scripts/generate-ocr-corpus.mjs";

function completeManifest() {
  return {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    generator: "paperworks-synthetic-ocr-v1",
    fixtures: requiredOcrFixtures.map((fixture) => ({
      expectedTextFilename: `${fixture.stem}.txt`,
      filename: `${fixture.stem}.png`,
      height: fixture.height,
      language: fixture.language,
      minimumRecall: fixture.minimumRecall,
      name: `${fixture.stem} fixture`,
      physicalRotationDegrees: fixture.physicalRotationDegrees,
      pngBytes: 1_024,
      pngSha256: "A".repeat(64),
      profile: fixture.profile,
      textBytes: 256,
      textSha256: "B".repeat(64),
      width: fixture.width
    }))
  };
}

test("accepts the complete bounded synthetic OCR contract", () => {
  const manifest = completeManifest();
  assert.equal(validateOcrCorpusManifest(manifest), manifest);
});

test("rejects missing, unsafe, duplicate and inconsistent OCR fixtures", () => {
  const missing = completeManifest();
  missing.fixtures.pop();
  assert.throws(
    () => validateOcrCorpusManifest(missing),
    /complete required fixture set/u
  );

  const unsafe = completeManifest();
  unsafe.fixtures[0].filename = "../english.png";
  assert.throws(() => validateOcrCorpusManifest(unsafe), /safe plain PNG/u);

  const duplicate = completeManifest();
  duplicate.fixtures[1].filename = duplicate.fixtures[0].filename;
  duplicate.fixtures[1].expectedTextFilename =
    duplicate.fixtures[0].expectedTextFilename;
  assert.throws(() => validateOcrCorpusManifest(duplicate), /unique matching pairs/u);

  const weakened = completeManifest();
  weakened.fixtures[0].minimumRecall = 0.5;
  assert.throws(() => validateOcrCorpusManifest(weakened), /contract is invalid/u);
});

test("uses a stable non-zero seeded noise sequence", () => {
  const random = createDeterministicRandom(0x54554645);
  const observed = Array.from({ length: 5 }, () => random());
  assert.deepEqual(
    observed.map((value) => value.toFixed(9)),
    ["0.434304799", "0.236195216", "0.451148440", "0.035847195", "0.644748603"]
  );
  assert.throws(() => createDeterministicRandom(1.5), /seed must be an integer/u);
});

test("keeps expected fixture text public, multilingual and UK English", () => {
  assert.equal(ocrFixtureDefinitions.length, 4);
  const english = expectedTextForFixture(ocrFixtureDefinitions[0]);
  const turkish = expectedTextForFixture(ocrFixtureDefinitions[1]);
  assert.match(english, /\borganise\b[\s\S]+\bcolour\b[\s\S]+\brecognise\b[\s\S]+\blicence\b/u);
  assert.match(turkish, /ç/u);
  assert.match(turkish, /ğ/u);
  assert.match(turkish, /ı/u);
  assert.match(turkish, /İ/u);
  assert.match(turkish, /ö/u);
  assert.match(turkish, /ş/u);
  assert.match(turkish, /ü/u);
  assert.equal(normaliseOcrExpectedText("  colour\nlicence  "), "colour licence");
});

test("summarises bounded OCR fixture pixels", () => {
  const pixels = new Uint8ClampedArray([
    255, 255, 255, 255,
    10, 20, 30, 255,
    180, 180, 180, 255,
    230, 230, 230, 255
  ]);
  const result = summariseOcrPixels(pixels, 2, 2);
  assert.equal(result.darkPixels, 1);
  assert.equal(result.lightPixels, 2);
  assert.equal(result.minimumLuminance, 18);
  assert.equal(result.maximumLuminance, 255);
  assert.match(result.rgbaSha256, /^[0-9A-F]{64}$/u);
  assert.throws(
    () => summariseOcrPixels(pixels, 3, 2),
    /invalid bounded dimensions/u
  );
});

test("keeps synthetic OCR generation in cross-platform CI and release evidence", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [packageJson, ciWorkflow, releaseWorkflow] = await Promise.all([
    readFile(`${workspace}package.json`, "utf8").then(JSON.parse),
    readFile(`${workspace}.github/workflows/ci.yml`, "utf8"),
    readFile(`${workspace}.github/workflows/release.yml`, "utf8")
  ]);

  assert.match(packageJson.scripts["qa:ocr-corpus"], /generate-ocr-corpus/u);
  assert.match(packageJson.scripts["qa:ocr-corpus"], /check-ocr-corpus/u);
  assert.match(
    ciWorkflow,
    /Verify generated OCR corpus[\s\S]+qa:ocr-corpus[\s\S]+ocr-corpus-\$\{\{ runner\.os \}\}/u
  );
  assert.match(
    releaseWorkflow,
    /Verify generated OCR corpus[\s\S]+qa:ocr-corpus[\s\S]+release-ocr-corpus-\$\{\{ runner\.os \}\}/u
  );
});
