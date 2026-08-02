import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  firstVersionLine,
  parseOcrCaseMarkers,
  parseTesseractLanguages,
  validateOcrEngineReport
} from "../scripts/run-ocr-engine-corpus.mjs";

const marker = "PAPERWORKS_OCR_CASE_V1";

function validMarkers() {
  return [
    `${marker}\tenglish\teng\t0.970000\t0.850000\t1\tprogress-verified`,
    `${marker}\tturkish\ttur\t0.880000\t0.750000\t1\tprogress-verified`,
    `${marker}\trotated\teng\t0.920000\t0.800000\t1\tprogress-verified`,
    `${marker}\tnoisy\teng\t0.760000\t0.650000\t1\tprogress-verified`
  ].join("\n");
}

function validReport() {
  return {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    releaseVersion: "0.1.0-alpha.1",
    architecture: "x64",
    cases: parseOcrCaseMarkers(validMarkers()),
    corpusManifestSha256: "A".repeat(64),
    engines: {
      ocrMyPdf: { command: "ocrmypdf", version: "17.8.1" },
      tesseract: {
        command: "tesseract",
        requiredLanguageData: ["eng", "osd", "tur"],
        version: "tesseract v5.4.0"
      }
    },
    platform: "win32"
  };
}

test("parses bounded OCR engine versions and required language data", () => {
  assert.equal(firstVersionLine("\nocrmypdf 17.8.1\n", "OCRmyPDF"), "ocrmypdf 17.8.1");
  assert.deepEqual(
    parseTesseractLanguages(
      "List of available languages in /usr/share/tessdata (4):\ntur\neng\nosd\neng\n"
    ),
    ["eng", "osd", "tur"]
  );
  assert.throws(() => firstVersionLine("\n", "OCRmyPDF"), /did not return/u);
});

test("accepts complete searchable OCR recall and progress evidence", () => {
  const cases = parseOcrCaseMarkers(
    validMarkers().replace(
      `${marker}\tenglish`,
      `test scan_export::tests::live_ocr_corpus ... ${marker}\tenglish`
    )
  );
  assert.deepEqual(
    cases.map((entry) => entry.name),
    ["english", "noisy", "rotated", "turkish"]
  );
  assert.ok(cases.every((entry) => entry.progressVerified));
  assert.ok(cases.every((entry) => entry.searchableTextPages === 1));
});

test("rejects weakened, duplicate, malformed and incomplete OCR evidence", () => {
  assert.throws(
    () => parseOcrCaseMarkers(validMarkers().replace("0.970000", "0.500000")),
    /missing or invalid for english/u
  );
  assert.throws(
    () =>
      parseOcrCaseMarkers(
        `${validMarkers()}\n${marker}\tenglish\teng\t0.970000\t0.850000\t1\tprogress-verified`
      ),
    /malformed or duplicate/u
  );
  assert.throws(
    () => parseOcrCaseMarkers(validMarkers().replace("progress-verified", "unchecked")),
    /malformed or duplicate/u
  );
  assert.throws(
    () => parseOcrCaseMarkers(validMarkers().split("\n").slice(0, 3).join("\n")),
    /missing or invalid for noisy/u
  );
});

test("accepts only path-free, text-free, versioned OCR engine reports", () => {
  const report = validReport();
  assert.equal(validateOcrEngineReport(report), report);

  const leakedPath = validReport();
  leakedPath.localPath = "C:\\private\\scan.pdf";
  assert.throws(() => validateOcrEngineReport(leakedPath), /unknown fields/u);

  const leakedText = validReport();
  leakedText.cases[0].recognisedText = "private words";
  assert.throws(() => validateOcrEngineReport(leakedText), /unknown fields/u);

  const weak = validReport();
  weak.cases[0].observedRecall = 0.2;
  assert.throws(() => validateOcrEngineReport(weak), /invalid for english/u);
});

test("keeps engine-backed OCR evidence mandatory on all tagged desktop releases", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [packageJson, releaseWorkflow, documentation, corpusRunner] = await Promise.all([
    readFile(`${workspace}package.json`, "utf8").then(JSON.parse),
    readFile(`${workspace}.github/workflows/release.yml`, "utf8"),
    readFile(`${workspace}docs/OCR_TESTING.md`, "utf8"),
    readFile(`${workspace}scripts/run-ocr-engine-corpus.mjs`, "utf8")
  ]);

  assert.match(packageJson.scripts["qa:ocr-engine"], /run-ocr-engine-corpus/u);
  assert.match(
    releaseWorkflow,
    /OCR, PDF\/A and certificate engine evidence \(\$\{\{ matrix\.platform \}\}\)[\s\S]+ubuntu-24\.04[\s\S]+macos-latest[\s\S]+windows-latest/u
  );
  assert.match(
    releaseWorkflow,
    /Run searchable OCR engine corpus[\s\S]+qa:ocr-engine[\s\S]+release-ocr-engine-\$\{\{ runner\.os \}\}/u
  );
  assert.match(
    releaseWorkflow,
    /pattern: release-ocr-engine-\*[\s\S]+release-ocr-engine-evidence\/\*/u
  );
  assert.match(documentation, /pinned native Windows evidence setup/u);
  assert.match(documentation, /batch-and-standalone publication test/u);
  assert.match(documentation, /direct\s+Recognise Text workflow/u);
  assert.match(corpusRunner, /"live_ocr_corpus"/u);
  assert.match(releaseWorkflow, /ced78752cc61322fb554c280d13360b35b8684e4/u);
  assert.match(releaseWorkflow, /489B9504E80D7184ED1AC9A1976647884EE71149DA231FF3C2C1DC15370F2F3D/u);
});
