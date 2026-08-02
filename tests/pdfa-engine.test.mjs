import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  parsePdfaMarkers,
  requireMinimumMajorVersion,
  validatePdfaEngineReport
} from "../scripts/run-pdfa-engine-corpus.mjs";
import {
  createAutomatedInstallation,
  veraPdfRelease,
  verifyVeraPdfArchive
} from "../scripts/install-verapdf.mjs";

const markerOutput = `
test archive::tests::live_pdfa_profiles_convert_ocr_and_validate ... PAPERWORKS_PDFA_PROFILE_V1\tpdfa-1b\t1\t1\t0\t0\t1.30.2
PAPERWORKS_PDFA_PROFILE_V1\tpdfa-2b\t1\t1\t0\t0\t1.30.2
PAPERWORKS_PDFA_PROFILE_V1\tpdfa-3b\t1\t1\t0\t0\t1.30.2
test batch::tests::live_pdfa_batch_recipe_verifies_compliant_publication ... PAPERWORKS_PDFA_BATCH_V1\tpdfa-2b\t1\t1\tvalidated
`;

function validReport() {
  const evidence = parsePdfaMarkers(markerOutput);
  return {
    architecture: "x64",
    batch: evidence.batch,
    corpusManifestSha256: "A".repeat(64),
    engines: {
      ghostscript: { command: "gswin64c", version: "10.07.1" },
      ocrMyPdf: { command: "ocrmypdf", version: "17.8.1" },
      tesseract: {
        command: "tesseract",
        requiredLanguageData: ["eng"],
        version: "tesseract 5.4.0"
      },
      veraPdf: { command: "verapdf", version: evidence.validatorVersion }
    },
    platform: "win32",
    product: "Tüfekci Paperworks",
    profiles: evidence.profiles,
    releaseVersion: "0.1.0-alpha.1",
    schemaVersion: 1
  };
}

test("parses exact PDF/A profile and batch evidence", () => {
  const evidence = parsePdfaMarkers(markerOutput);
  assert.deepEqual(
    evidence.profiles.map((entry) => entry.profile),
    ["pdfa-1b", "pdfa-2b", "pdfa-3b"]
  );
  assert.equal(evidence.profiles.every((entry) => entry.compliant), true);
  assert.equal(evidence.profiles.every((entry) => entry.searchableTextPages === 1), true);
  assert.equal(evidence.batch.independentlyValidated, true);
  assert.equal(evidence.validatorVersion, "1.30.2");
});

test("rejects duplicate, incomplete, or contradictory PDF/A markers", () => {
  assert.throws(
    () => parsePdfaMarkers(`${markerOutput}${markerOutput}`),
    /duplicate profile evidence/iu
  );
  assert.throws(
    () => parsePdfaMarkers(markerOutput.replace(/^.*pdfa-3b.*\n/mu, "")),
    /missing one or more required/iu
  );
  assert.throws(
    () => parsePdfaMarkers(markerOutput.replace("pdfa-2b\t1\t1\t0\t0", "pdfa-2b\t1\t1\t1\t1")),
    /malformed/iu
  );
});

test("validates a path-free, closed-schema PDF/A release report", () => {
  assert.equal(validatePdfaEngineReport(validReport()).schemaVersion, 1);
  assert.throws(
    () => validatePdfaEngineReport({ ...validReport(), sourcePath: "C:\\Private\\source.pdf" }),
    /unknown fields/iu
  );
  const invalid = validReport();
  invalid.profiles[1] = { ...invalid.profiles[1], compliant: false };
  assert.throws(() => validatePdfaEngineReport(invalid), /invalid for pdfa-2b/iu);
});

test("requires OCRmyPDF 17 or later for archival evidence", () => {
  assert.equal(requireMinimumMajorVersion("17.8.1", 17, "OCRmyPDF"), "17.8.1");
  assert.equal(
    requireMinimumMajorVersion("ocrmypdf 18.0", 17, "OCRmyPDF"),
    "ocrmypdf 18.0"
  );
  assert.throws(
    () => requireMinimumMajorVersion("15.2.0", 17, "OCRmyPDF"),
    /17 or later/iu
  );
  assert.throws(
    () => requireMinimumMajorVersion("unknown", 17, "OCRmyPDF"),
    /17 or later/iu
  );
});

test("pins the official veraPDF installer and generates a CLI-only unattended plan", () => {
  assert.deepEqual(veraPdfRelease, {
    archiveBytes: 32_923_960,
    archiveSha256: "6CC6341CB1AF644044054B81F00A6590A7918ABB18F762243DE115258BCAD838",
    archiveUrl: "https://software.verapdf.org/rel/1.30/verapdf-greenfield-1.30.2-installer.zip",
    version: "1.30.2"
  });
  const xml = createAutomatedInstallation("C:\\Build & Evidence\\veraPDF");
  assert.match(xml, /C:\\Build &amp; Evidence\\veraPDF/u);
  assert.match(xml, /name="veraPDF CLI" selected="true"/u);
  assert.match(xml, /name="veraPDF GUI" selected="false"/u);
  assert.throws(() => verifyVeraPdfArchive(Buffer.alloc(4)), /size does not match/iu);
});

test("keeps PDF/A conversion on the shared job lifecycle and optional readiness boundary", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const studio = readFileSync(new URL("../src/ArchiveStudio.tsx", import.meta.url), "utf8");
  const nativeArchive = readFileSync(
    new URL("../src-tauri/src/archive.rs", import.meta.url),
    "utf8"
  );

  assert.match(studio, /usePdfJob<PdfArchiveResult>\(desktopMode, "archive"\)/u);
  assert.match(studio, /searchableTextPages/u);
  assert.match(studio, /localiseArchiveWarnings\(result\.warnings, t\)/u);
  assert.doesNotMatch(studio, /result\.warnings\.map/u);
  assert.doesNotMatch(studio, /invoke<.*>\("run_pdf_archive/u);
  assert.match(app, /invoke<PdfArchiveReadiness>\("pdf_archive_readiness"\)\.catch\(\(\) => null\)/u);
  assert.match(nativeArchive, /"--format",\s*"json"/u);
  assert.match(nativeArchive, /publish_prepared_file\(&candidate, &output\)/u);
  assert.match(nativeArchive, /MAX_VALIDATOR_OUTPUT_BYTES/u);
});

test("separates formal PDF/UA validation from bounded PDF/X structural preflight", () => {
  const studio = readFileSync(new URL("../src/ArchiveStudio.tsx", import.meta.url), "utf8");
  const enGb = readFileSync(new URL("../src/locales/en-GB.ts", import.meta.url), "utf8");
  const nativeArchive = readFileSync(
    new URL("../src-tauri/src/archive.rs", import.meta.url),
    "utf8"
  );
  const nativePdfx = readFileSync(new URL("../src-tauri/src/pdfx.rs", import.meta.url), "utf8");

  for (const profile of ["pdfua-1", "pdfua-2", "pdfx-1a-2001", "pdfx-3-2002", "pdfx-4"]) {
    assert.match(studio, new RegExp(profile, "u"));
  }
  assert.match(nativeArchive, /Self::PdfUa1 => Some\("ua1"\)/u);
  assert.match(nativeArchive, /Self::PdfUa2 => Some\("ua2"\)/u);
  assert.match(nativeArchive, /PdfConformanceAssessment::StructuralPreflight/u);
  assert.match(nativePdfx, /inspect_pdf_print_resources/u);
  assert.match(nativePdfx, /GTS_PDFX/u);
  assert.match(nativePdfx, /MediaBox/u);
  assert.match(studio, /localiseArchiveScope\(report\.assessment, t\)/u);
  assert.match(enGb, /not ISO certification, colourimetric proofing or print-service approval/u);
  assert.doesNotMatch(nativePdfx, /PdfConformanceOutcome::Conforms/u);
});

test("requires PDF/A engine evidence on every tagged desktop release", () => {
  const packageJson = JSON.parse(
    readFileSync(new URL("../package.json", import.meta.url), "utf8")
  );
  const releaseWorkflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8"
  );
  const corpusRunner = readFileSync(
    new URL("../scripts/run-pdfa-engine-corpus.mjs", import.meta.url),
    "utf8"
  );

  assert.match(packageJson.scripts["qa:pdfa-engine"], /run-pdfa-engine-corpus/u);
  assert.match(
    releaseWorkflow,
    /OCR, PDF\/A and certificate engine evidence \(\$\{\{ matrix\.platform \}\}\)[\s\S]+ubuntu-24\.04[\s\S]+macos-latest[\s\S]+windows-latest/u
  );
  assert.match(
    releaseWorkflow,
    /Install pinned veraPDF validator[\s\S]+install-verapdf\.mjs[\s\S]+Run PDF\/A conversion and validation corpus[\s\S]+qa:pdfa-engine/u
  );
  assert.equal(releaseWorkflow.match(/ocrmypdf==17\.8\.1/gu)?.length, 2);
  assert.match(
    releaseWorkflow,
    /release-pdfa-engine-\$\{\{ runner\.os \}\}-\$\{\{ runner\.arch \}\}[\s\S]+pdfa-engine-report-\*\.json/u
  );
  assert.match(
    releaseWorkflow,
    /pattern: release-pdfa-engine-\*[\s\S]+path: release-pdfa-engine-evidence[\s\S]+name: release-pdfa-engine-evidence/u
  );
  assert.match(
    releaseWorkflow,
    /Attach source, rendering, OCR, PDF\/A, certificate, native, package, updater, and metadata evidence[\s\S]+release-pdfa-engine-evidence\/\*[\s\S]+release-certificate-engine-evidence\/\*[\s\S]+release-native-e2e-evidence\/\*/u
  );
  assert.match(releaseWorkflow, /3A4C28D0AAC47AA7CCCD35A5932C55110376E9DBD966898DDE388B7FABA444A4/u);
  assert.match(corpusRunner, /process\.platform === "win32" \? \["gswin64c"\] : \["gs"\]/u);
  assert.doesNotMatch(corpusRunner, /gswin32c/u);
});
