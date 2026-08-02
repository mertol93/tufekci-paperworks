import { spawnSync } from "node:child_process";
import { lstat, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { checkOcrCorpus } from "./check-ocr-corpus.mjs";

const requiredCases = Object.freeze([
  Object.freeze({ name: "english", language: "eng", minimumRecall: 0.85 }),
  Object.freeze({ name: "turkish", language: "tur", minimumRecall: 0.75 }),
  Object.freeze({ name: "rotated", language: "eng", minimumRecall: 0.8 }),
  Object.freeze({ name: "noisy", language: "eng", minimumRecall: 0.65 })
]);
const requiredTesseractLanguages = Object.freeze(["eng", "osd", "tur"]);
const caseMarker = "PAPERWORKS_OCR_CASE_V1";
const maximumCommandOutputBytes = 16 * 1024 * 1024;

export function parseTesseractLanguages(output) {
  if (typeof output !== "string" || Buffer.byteLength(output, "utf8") > 1024 * 1024) {
    throw new Error("Tesseract language output is missing or exceeds its safety limit.");
  }
  return [...new Set(
    output
      .split(/\r?\n/u)
      .map((line) => line.trim())
      .filter(
        (line) =>
          line &&
          !line.startsWith("List of available languages") &&
          /^[A-Za-z0-9_-]{1,32}$/u.test(line)
      )
  )].sort();
}

export function firstVersionLine(output, label) {
  if (typeof output !== "string" || Buffer.byteLength(output, "utf8") > 1024 * 1024) {
    throw new Error(`${label} version output is missing or exceeds its safety limit.`);
  }
  const line = output
    .split(/\r?\n/u)
    .map((candidate) => candidate.trim())
    .find(Boolean);
  if (!line || line.length > 200 || /[\0\t]/u.test(line)) {
    throw new Error(`${label} did not return a bounded version line.`);
  }
  return line;
}

export function parseOcrCaseMarkers(output) {
  if (
    typeof output !== "string" ||
    Buffer.byteLength(output, "utf8") > maximumCommandOutputBytes
  ) {
    throw new Error("OCR corpus test output is missing or exceeds its safety limit.");
  }
  const cases = [];
  const seen = new Set();
  for (const line of output.split(/\r?\n/u)) {
    const markerOffset = line.indexOf(`${caseMarker}\t`);
    if (markerOffset < 0) {
      continue;
    }
    const fields = line.slice(markerOffset).split("\t");
    if (
      fields.length !== 7 ||
      fields[0] !== caseMarker ||
      !/^[a-z]{3,16}$/u.test(fields[1]) ||
      !/^[a-z]{3}(?:\+[a-z]{3})*$/u.test(fields[2]) ||
      !/^(?:0(?:\.\d{6})?|1(?:\.0{6})?)$/u.test(fields[3]) ||
      !/^(?:0(?:\.\d{6})?|1(?:\.0{6})?)$/u.test(fields[4]) ||
      fields[5] !== "1" ||
      fields[6] !== "progress-verified" ||
      seen.has(fields[1])
    ) {
      throw new Error("The native OCR corpus test emitted a malformed or duplicate case marker.");
    }
    seen.add(fields[1]);
    cases.push({
      language: fields[2],
      minimumRecall: Number(fields[4]),
      name: fields[1],
      observedRecall: Number(fields[3]),
      progressVerified: true,
      searchableTextPages: Number(fields[5])
    });
  }

  for (const required of requiredCases) {
    const observed = cases.find((candidate) => candidate.name === required.name);
    if (
      !observed ||
      observed.language !== required.language ||
      observed.minimumRecall !== required.minimumRecall ||
      observed.observedRecall < required.minimumRecall ||
      observed.observedRecall > 1
    ) {
      throw new Error(`OCR corpus evidence is missing or invalid for ${required.name}.`);
    }
  }
  if (cases.length !== requiredCases.length) {
    throw new Error("The native OCR corpus test emitted unexpected case evidence.");
  }
  return cases.sort((left, right) => left.name.localeCompare(right.name, "en-GB"));
}

export function validateOcrEngineReport(value) {
  requireExactFields(
    value,
    [
      "architecture",
      "cases",
      "corpusManifestSha256",
      "engines",
      "platform",
      "product",
      "releaseVersion",
      "schemaVersion"
    ],
    "OCR engine report"
  );
  if (
    value.schemaVersion !== 1 ||
    value.product !== "Tüfekci Paperworks" ||
    typeof value.releaseVersion !== "string" ||
    !/^0\.1\.0-alpha\.\d+$/u.test(value.releaseVersion) ||
    !["darwin", "linux", "win32"].includes(value.platform) ||
    typeof value.architecture !== "string" ||
    !/^[A-Za-z0-9_-]{1,32}$/u.test(value.architecture) ||
    typeof value.corpusManifestSha256 !== "string" ||
    !/^[0-9A-F]{64}$/u.test(value.corpusManifestSha256)
  ) {
    throw new Error("The OCR engine report identity is invalid.");
  }
  if (!Array.isArray(value.cases) || value.cases.length !== requiredCases.length) {
    throw new Error("The OCR engine report case set is incomplete.");
  }
  const seen = new Set();
  for (const observed of value.cases) {
    requireExactFields(
      observed,
      [
        "language",
        "minimumRecall",
        "name",
        "observedRecall",
        "progressVerified",
        "searchableTextPages"
      ],
      "OCR engine case"
    );
    const required = requiredCases.find((candidate) => candidate.name === observed.name);
    if (
      !required ||
      seen.has(observed.name) ||
      observed.language !== required.language ||
      observed.minimumRecall !== required.minimumRecall ||
      typeof observed.observedRecall !== "number" ||
      !Number.isFinite(observed.observedRecall) ||
      observed.observedRecall < required.minimumRecall ||
      observed.observedRecall > 1 ||
      observed.progressVerified !== true ||
      observed.searchableTextPages !== 1
    ) {
      throw new Error(`The OCR engine report case is invalid for ${observed.name}.`);
    }
    seen.add(observed.name);
  }

  requireExactFields(value.engines, ["ocrMyPdf", "tesseract"], "OCR engines");
  requireExactFields(
    value.engines.ocrMyPdf,
    ["command", "version"],
    "OCRmyPDF engine"
  );
  requireExactFields(
    value.engines.tesseract,
    ["command", "requiredLanguageData", "version"],
    "Tesseract engine"
  );
  if (
    value.engines.ocrMyPdf.command !== "ocrmypdf" ||
    value.engines.tesseract.command !== "tesseract" ||
    !isBoundedVersion(value.engines.ocrMyPdf.version) ||
    !isBoundedVersion(value.engines.tesseract.version) ||
    !Array.isArray(value.engines.tesseract.requiredLanguageData) ||
    value.engines.tesseract.requiredLanguageData.length !==
      requiredTesseractLanguages.length ||
    value.engines.tesseract.requiredLanguageData.some(
      (language, index) => language !== requiredTesseractLanguages[index]
    )
  ) {
    throw new Error("The OCR engine report versions or language data are invalid.");
  }
  return value;
}

export async function runOcrEngineCorpus(workspace, corpusArgument) {
  const corpusDirectory = path.resolve(
    workspace,
    corpusArgument || "qa-fixtures/ocr-corpus"
  );
  const corpusReport = await checkOcrCorpus(workspace, corpusDirectory);
  const ocrMyPdfVersionOutput = runCommand(
    "ocrmypdf",
    ["--version"],
    "OCRmyPDF version check",
    15_000
  );
  const tesseractVersionOutput = runCommand(
    "tesseract",
    ["--version"],
    "Tesseract version check",
    15_000
  );
  const languageOutput = runCommand(
    "tesseract",
    ["--list-langs"],
    "Tesseract language discovery",
    15_000
  );
  const installedLanguages = parseTesseractLanguages(
    `${languageOutput.stdout}\n${languageOutput.stderr}`
  );
  const missingLanguages = requiredTesseractLanguages.filter(
    (language) => !installedLanguages.includes(language)
  );
  if (missingLanguages.length > 0) {
    throw new Error(
      `Install the missing Tesseract language data before running OCR evidence: ${missingLanguages.join(", ")}.`
    );
  }

  const cargoResult = runCommand(
    "cargo",
    [
      "test",
      "--manifest-path",
      path.join(workspace, "src-tauri", "Cargo.toml"),
      "live_ocr_corpus",
      "--",
      "--ignored",
      "--nocapture",
      "--test-threads=1"
    ],
    "Native OCR corpus test",
    30 * 60_000,
    {
      PAPERWORKS_OCR_CORPUS: corpusDirectory
    }
  );
  let cases;
  try {
    cases = parseOcrCaseMarkers(`${cargoResult.stdout}\n${cargoResult.stderr}`);
  } catch (error) {
    if (cargoResult.stdout) {
      process.stdout.write(cargoResult.stdout);
    }
    if (cargoResult.stderr) {
      process.stderr.write(cargoResult.stderr);
    }
    throw error;
  }
  const packageJson = JSON.parse(
    await readFile(path.join(workspace, "package.json"), "utf8")
  );
  const report = validateOcrEngineReport({
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    architecture: process.arch,
    cases,
    corpusManifestSha256: corpusReport.corpusManifestSha256,
    engines: {
      ocrMyPdf: {
        command: "ocrmypdf",
        version: firstVersionLine(
          `${ocrMyPdfVersionOutput.stdout}\n${ocrMyPdfVersionOutput.stderr}`,
          "OCRmyPDF"
        )
      },
      tesseract: {
        command: "tesseract",
        requiredLanguageData: requiredTesseractLanguages,
        version: firstVersionLine(
          `${tesseractVersionOutput.stdout}\n${tesseractVersionOutput.stderr}`,
          "Tesseract"
        )
      }
    },
    platform: process.platform,
    releaseVersion: packageJson.version
  });
  const reportFilename = `ocr-engine-report-${process.platform}-${process.arch}.json`;
  await writeReport(
    path.join(corpusDirectory, reportFilename),
    `${JSON.stringify(report, null, 2)}\n`
  );
  return { ...report, reportFilename };
}

function isBoundedVersion(value) {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 200 &&
    !/[\0\r\n\t]/u.test(value)
  );
}

function requireExactFields(value, expectedFields, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`The ${label} must be an object.`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...expectedFields].sort();
  if (
    actual.length !== expected.length ||
    actual.some((field, index) => field !== expected[index])
  ) {
    throw new Error(`The ${label} contains missing or unknown fields.`);
  }
}

function runCommand(command, args, label, timeout, additionalEnvironment = {}) {
  const result = spawnSync(command, args, {
    cwd: undefined,
    encoding: "utf8",
    env: { ...process.env, ...additionalEnvironment },
    maxBuffer: maximumCommandOutputBytes,
    timeout,
    windowsHide: true
  });
  if (result.error) {
    if (result.error.code === "ENOENT") {
      throw new Error(`${label} could not start because '${command}' is not installed or is not on PATH.`);
    }
    if (result.error.code === "ETIMEDOUT") {
      throw new Error(`${label} exceeded its ${Math.ceil(timeout / 60_000)} minute limit.`);
    }
    throw new Error(`${label} could not start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    if (result.stdout) {
      process.stdout.write(result.stdout);
    }
    if (result.stderr) {
      process.stderr.write(result.stderr);
    }
    throw new Error(`${label} failed with exit code ${result.status}.`);
  }
  return {
    stderr: result.stderr || "",
    stdout: result.stdout || ""
  };
}

async function writeReport(candidate, text) {
  try {
    const metadata = await lstat(candidate);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error("Refusing to replace a non-ordinary OCR engine report.");
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") {
      throw error;
    }
  }
  await writeFile(candidate, text, "utf8");
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const report = await runOcrEngineCorpus(workspace, process.argv[2]);
  process.stdout.write(
    `OCR engine corpus passed ${report.cases.length} cases with searchable text and recall evidence (${report.reportFilename}).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
