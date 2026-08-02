import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  BUILT_IN_BATCH_RECIPES,
  batchRecipeInputOriginLabel,
  batchRecipeSettingsError,
  buildBatchOutputFileNames,
  createCustomBatchRecipe,
  createVerifiedScanBatchSeed,
  parseStoredBatchRecipes,
  serialiseStoredBatchRecipes
} from "../src/batchRecipes.ts";

test("ships valid, distinct built-in batch recipes", () => {
  assert.equal(BUILT_IN_BATCH_RECIPES.length, 5);
  assert.equal(new Set(BUILT_IN_BATCH_RECIPES.map((recipe) => recipe.id)).size, 5);
  for (const recipe of BUILT_IN_BATCH_RECIPES) {
    assert.equal(batchRecipeSettingsError(recipe), null);
    assert.equal(recipe.custom, false);
  }
  const searchable = BUILT_IN_BATCH_RECIPES.find(
    (recipe) => recipe.id === "built-in-searchable-archive"
  );
  assert.equal(searchable?.recogniseText, true);
  assert.equal(searchable?.straighten, true);
  assert.equal(searchable?.ocrLanguage, "eng");
  const archival = BUILT_IN_BATCH_RECIPES.find(
    (recipe) => recipe.id === "built-in-pdfa-archive"
  );
  assert.equal(archival?.archiveProfile, "pdfa-2b");
});

test("requires searchable OCR before deskew and validates its language", () => {
  const base = BUILT_IN_BATCH_RECIPES[0];
  assert.match(
    batchRecipeSettingsError({ ...base, straighten: true }),
    /searchable OCR before deskewing/u
  );
  assert.match(
    batchRecipeSettingsError({ ...base, recogniseText: true, ocrLanguage: "eng;tur" }),
    /valid installed OCR language/u
  );
  assert.equal(
    batchRecipeSettingsError({
      ...base,
      cleanPrivacy: false,
      compress: false,
      recogniseText: true
    }),
    null
  );
  assert.equal(
    batchRecipeSettingsError({
      ...base,
      archiveProfile: "pdfa-2b",
      cleanPrivacy: false,
      compress: false,
      recogniseText: false
    }),
    null
  );
});

test("creates a bounded password-free hand-off for verified scan PDFs", () => {
  const connected = createVerifiedScanBatchSeed(
    "C:\\Scans\\verified-connected-scan.pdf",
    true
  );
  assert.deepEqual(connected, {
    origin: "connected-scanner",
    path: "C:\\Scans\\verified-connected-scan.pdf"
  });
  assert.deepEqual(Object.keys(connected).sort(), ["origin", "path"]);
  assert.equal(batchRecipeInputOriginLabel(connected.origin), "Connected scanner intake");

  const images = createVerifiedScanBatchSeed("/tmp/reviewed-images.pdf", false);
  assert.equal(images.origin, "image-scan");
  assert.equal(batchRecipeInputOriginLabel(images.origin), "Reviewed image scan");
  assert.throws(() => createVerifiedScanBatchSeed("line-one\nline-two.pdf", true), /invalid/u);
  assert.throws(() => createVerifiedScanBatchSeed("/tmp/not-a-pdf.png", false), /invalid/u);
  assert.throws(() => createVerifiedScanBatchSeed(" ", false), /invalid/u);
});

test("connects successful scanner exports to the reviewed Batch Recipe intake", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const studio = readFileSync(
    new URL("../src/BatchRecipeStudio.tsx", import.meta.url),
    "utf8"
  );

  assert.match(app, /loadScanBatch\(files, result\.paths, true\)/u);
  assert.match(app, /createVerifiedScanBatchSeed\(result\.outputPath, scanFromConnectedScanner\)/u);
  assert.match(app, /action: "batch-recipe"/u);
  assert.match(app, /setActiveWorkflowId\("batch"\)/u);
  assert.match(app, /initialSourceOrigin=\{scanBatchHandoff\?\.origin\}/u);
  assert.match(studio, /localiseBatchInputOrigin\(source\.origin, t\)/u);
  assert.match(studio, /"batch\.notice\.handoff\.connectedScanner"/u);
  assert.match(studio, /"batch\.notice\.handoff\.imageScan"/u);
});

test("builds portable, bounded, case-insensitively unique output names", () => {
  const veryLongName = `${"Résumé-".repeat(50)}.pdf`;
  const names = buildBatchOutputFileNames(
    ["C:\\Docs\\Report.pdf", "/tmp/report.pdf", "/tmp/report?.pdf", veryLongName],
    "Private sharing copies"
  );

  assert.deepEqual(names.slice(0, 3), [
    "Report-private-sharing-copies.pdf",
    "report-private-sharing-copies-2.pdf",
    "report-private-sharing-copies-3.pdf"
  ]);
  assert.equal(new Set(names.map((name) => name.toLocaleLowerCase("en-GB"))).size, names.length);
  assert.ok(names.every((name) => new TextEncoder().encode(name).length <= 240));
  assert.ok(names.every((name) => !/[<>:"/\\|?*]/.test(name)));
});

test("round-trips only recipe settings and strips sensitive extra properties", () => {
  const recipe = createCustomBatchRecipe("custom-1234", "Archive copy", {
    archiveProfile: "pdfa-2b",
    cleanPrivacy: true,
    compress: true,
    jpegQuality: 72,
    ocrLanguage: "eng+tur",
    privacyOptions: {
      removeActiveContent: true,
      removeAnnotationsAndForms: false,
      removeAttachments: true,
      removeMetadata: true,
      removeThumbnails: true
    },
    recogniseText: true,
    straighten: true
  });
  const serialised = serialiseStoredBatchRecipes([
    {
      ...recipe,
      inputPath: "C:\\Sensitive\\contract.pdf",
      origin: "connected-scanner",
      password: "never-store-this",
      findings: ["private"]
    }
  ]);

  assert.doesNotMatch(serialised, /Sensitive|contract|connected-scanner|never-store|findings/);
  const restored = parseStoredBatchRecipes(serialised);
  assert.equal(restored.length, 1);
  assert.equal(restored[0].name, "Archive copy");
  assert.equal(restored[0].jpegQuality, 72);
  assert.equal(restored[0].archiveProfile, "pdfa-2b");
  assert.equal(restored[0].ocrLanguage, "eng+tur");
  assert.equal(restored[0].recogniseText, true);
  assert.equal(restored[0].straighten, true);
  assert.equal(restored[0].outputSuffix, "archive-copy");
});

test("migrates version-one recipes with OCR safely disabled", () => {
  const serialised = serialiseStoredBatchRecipes([
    createCustomBatchRecipe("custom-legacy", "Legacy recipe", BUILT_IN_BATCH_RECIPES[0])
  ]);
  const candidate = JSON.parse(serialised).recipes[0];
  delete candidate.ocrLanguage;
  delete candidate.recogniseText;
  delete candidate.straighten;
  delete candidate.archiveProfile;

  const restored = parseStoredBatchRecipes(JSON.stringify({ recipes: [candidate], version: 1 }));
  assert.equal(restored.length, 1);
  assert.equal(restored[0].ocrLanguage, "eng");
  assert.equal(restored[0].recogniseText, false);
  assert.equal(restored[0].straighten, false);
  assert.equal(restored[0].archiveProfile, null);
});

test("migrates version-two recipes with PDF/A safely disabled", () => {
  const serialised = serialiseStoredBatchRecipes([
    createCustomBatchRecipe("custom-version-two", "Version two", BUILT_IN_BATCH_RECIPES[0])
  ]);
  const candidate = JSON.parse(serialised).recipes[0];
  delete candidate.archiveProfile;

  const restored = parseStoredBatchRecipes(JSON.stringify({ recipes: [candidate], version: 2 }));
  assert.equal(restored.length, 1);
  assert.equal(restored[0].archiveProfile, null);
});

test("drops malformed stored entries without hiding valid recipes", () => {
  const valid = JSON.parse(
    serialiseStoredBatchRecipes([
      createCustomBatchRecipe("custom-valid", "Valid recipe", {
        archiveProfile: null,
        cleanPrivacy: false,
        compress: true,
        jpegQuality: 80,
        ocrLanguage: "eng",
        privacyOptions: {
          removeActiveContent: true,
          removeAnnotationsAndForms: false,
          removeAttachments: true,
          removeMetadata: true,
          removeThumbnails: true
        },
        recogniseText: false,
        straighten: false
      })
    ])
  ).recipes[0];
  const restored = parseStoredBatchRecipes(
    JSON.stringify({
      version: 3,
      recipes: [
        null,
        { ...valid, id: "unsafe id", inputPath: "/private/file.pdf" },
        valid,
        { ...valid, id: "custom-bad-quality", jpegQuality: 10 }
      ]
    })
  );

  assert.deepEqual(restored.map((recipe) => recipe.id), ["custom-valid"]);
  assert.deepEqual(parseStoredBatchRecipes("not json"), []);
  assert.deepEqual(parseStoredBatchRecipes(JSON.stringify({ version: 4, recipes: [valid] })), []);
});
