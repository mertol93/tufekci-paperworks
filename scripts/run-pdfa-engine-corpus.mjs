import { spawnSync } from "node:child_process";
import { lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { checkOcrCorpus } from "./check-ocr-corpus.mjs";
import { firstVersionLine, parseTesseractLanguages } from "./run-ocr-engine-corpus.mjs";

const requiredProfiles = Object.freeze(["pdfa-1b", "pdfa-2b", "pdfa-3b"]);
const profileMarker = "PAPERWORKS_PDFA_PROFILE_V1";
const batchMarker = "PAPERWORKS_PDFA_BATCH_V1";
const maximumCommandOutputBytes = 16 * 1024 * 1024;

export function parsePdfaMarkers(output) {
  if (
    typeof output !== "string" ||
    Buffer.byteLength(output, "utf8") > maximumCommandOutputBytes
  ) {
    throw new Error("PDF/A corpus output is missing or exceeds its safety limit.");
  }

  const profiles = [];
  let batch = null;
  for (const line of output.split(/\r?\n/u)) {
    const profileOffset = line.indexOf(`${profileMarker}\t`);
    if (profileOffset >= 0) {
      const fields = line.slice(profileOffset).split("\t");
      if (
        fields.length !== 7 ||
        fields[0] !== profileMarker ||
        !requiredProfiles.includes(fields[1]) ||
        fields[2] !== "1" ||
        fields[3] !== "1" ||
        fields[4] !== "0" ||
        fields[5] !== "0" ||
        !isEngineVersion(fields[6]) ||
        profiles.some((entry) => entry.profile === fields[1])
      ) {
        throw new Error("The native PDF/A test emitted malformed or duplicate profile evidence.");
      }
      profiles.push({
        compliant: true,
        failedChecks: 0,
        failedRules: 0,
        pageCount: 1,
        profile: fields[1],
        searchableTextPages: 1,
        validatorVersion: fields[6]
      });
    }

    const batchOffset = line.indexOf(`${batchMarker}\t`);
    if (batchOffset >= 0) {
      const fields = line.slice(batchOffset).split("\t");
      if (
        batch ||
        fields.length !== 5 ||
        fields[0] !== batchMarker ||
        fields[1] !== "pdfa-2b" ||
        fields[2] !== "1" ||
        fields[3] !== "1" ||
        fields[4] !== "validated"
      ) {
        throw new Error("The native PDF/A test emitted malformed or duplicate batch evidence.");
      }
      batch = {
        independentlyValidated: true,
        outputCount: 1,
        profile: "pdfa-2b",
        searchableTextPages: 1
      };
    }
  }

  profiles.sort((left, right) =>
    requiredProfiles.indexOf(left.profile) - requiredProfiles.indexOf(right.profile)
  );
  if (
    profiles.length !== requiredProfiles.length ||
    profiles.some((entry, index) => entry.profile !== requiredProfiles[index])
  ) {
    throw new Error("PDF/A evidence is missing one or more required conformance profiles.");
  }
  if (!batch) {
    throw new Error("PDF/A evidence is missing the reusable batch recipe result.");
  }
  const validatorVersions = new Set(profiles.map((entry) => entry.validatorVersion));
  if (validatorVersions.size !== 1) {
    throw new Error("PDF/A profile evidence was not produced by one validator version.");
  }
  return { batch, profiles, validatorVersion: profiles[0].validatorVersion };
}

export function validatePdfaEngineReport(value) {
  requireExactFields(
    value,
    [
      "architecture",
      "batch",
      "corpusManifestSha256",
      "engines",
      "platform",
      "product",
      "profiles",
      "releaseVersion",
      "schemaVersion"
    ],
    "PDF/A engine report"
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
    throw new Error("The PDF/A engine report identity is invalid.");
  }

  if (!Array.isArray(value.profiles) || value.profiles.length !== requiredProfiles.length) {
    throw new Error("The PDF/A engine report profile set is incomplete.");
  }
  for (const [index, observed] of value.profiles.entries()) {
    requireExactFields(
      observed,
      [
        "compliant",
        "failedChecks",
        "failedRules",
        "pageCount",
        "profile",
        "searchableTextPages",
        "validatorVersion"
      ],
      "PDF/A profile evidence"
    );
    if (
      observed.profile !== requiredProfiles[index] ||
      observed.compliant !== true ||
      observed.pageCount !== 1 ||
      observed.searchableTextPages !== 1 ||
      observed.failedRules !== 0 ||
      observed.failedChecks !== 0 ||
      !isEngineVersion(observed.validatorVersion)
    ) {
      throw new Error(`The PDF/A profile evidence is invalid for ${observed.profile}.`);
    }
  }

  requireExactFields(
    value.batch,
    ["independentlyValidated", "outputCount", "profile", "searchableTextPages"],
    "PDF/A batch evidence"
  );
  if (
    value.batch.independentlyValidated !== true ||
    value.batch.outputCount !== 1 ||
    value.batch.profile !== "pdfa-2b" ||
    value.batch.searchableTextPages !== 1
  ) {
    throw new Error("The PDF/A batch evidence is invalid.");
  }

  requireExactFields(
    value.engines,
    ["ghostscript", "ocrMyPdf", "tesseract", "veraPdf"],
    "PDF/A engines"
  );
  for (const [key, command] of [
    ["ghostscript", value.platform === "win32" ? "gswin64c" : "gs"],
    ["ocrMyPdf", "ocrmypdf"],
    ["veraPdf", "verapdf"]
  ]) {
    requireExactFields(value.engines[key], ["command", "version"], `${key} engine`);
    if (
      value.engines[key].command !== command ||
      !isEngineVersion(value.engines[key].version)
    ) {
      throw new Error(`The ${key} engine evidence is invalid.`);
    }
  }
  requireExactFields(
    value.engines.tesseract,
    ["command", "requiredLanguageData", "version"],
    "Tesseract engine"
  );
  if (
    value.engines.tesseract.command !== "tesseract" ||
    !isEngineVersion(value.engines.tesseract.version) ||
    JSON.stringify(value.engines.tesseract.requiredLanguageData) !== JSON.stringify(["eng"])
  ) {
    throw new Error("The Tesseract engine evidence is invalid.");
  }
  const validatorVersions = new Set(
    value.profiles.map((entry) => entry.validatorVersion)
  );
  if (
    validatorVersions.size !== 1 ||
    !validatorVersions.has(value.engines.veraPdf.version)
  ) {
    throw new Error("The retained veraPDF version does not match the profile evidence.");
  }
  return value;
}

export function requireMinimumMajorVersion(value, minimum, label) {
  if (
    typeof value !== "string" ||
    !Number.isSafeInteger(minimum) ||
    minimum < 1 ||
    typeof label !== "string" ||
    label.length === 0
  ) {
    throw new Error("The engine version requirement is invalid.");
  }
  const match = value.match(/(?:^|\D)(\d+)(?:\.\d+)?/u);
  const major = match ? Number.parseInt(match[1], 10) : Number.NaN;
  if (!Number.isSafeInteger(major) || major < minimum) {
    throw new Error(`${label} ${minimum} or later is required for PDF/A evidence.`);
  }
  return value;
}

export async function runPdfaEngineCorpus(workspace, corpusArgument, outputArgument) {
  const corpusDirectory = path.resolve(
    workspace,
    corpusArgument || "qa-fixtures/ocr-corpus"
  );
  const outputDirectory = path.resolve(
    workspace,
    outputArgument || "qa-fixtures/pdfa-engine"
  );
  const corpusReport = await checkOcrCorpus(workspace, corpusDirectory);
  const ocrMyPdf = runCommand("ocrmypdf", ["--version"], "OCRmyPDF version check", 15_000);
  const ocrMyPdfVersion = requireMinimumMajorVersion(
    firstVersionLine(`${ocrMyPdf.stdout}\n${ocrMyPdf.stderr}`, "OCRmyPDF"),
    17,
    "OCRmyPDF"
  );
  const tesseract = runCommand("tesseract", ["--version"], "Tesseract version check", 15_000);
  const languages = runCommand(
    "tesseract",
    ["--list-langs"],
    "Tesseract language discovery",
    15_000
  );
  if (
    !parseTesseractLanguages(`${languages.stdout}\n${languages.stderr}`).includes("eng")
  ) {
    throw new Error("Install Tesseract English language data before running PDF/A evidence.");
  }
  const ghostscript = firstAvailableCommand(
    process.platform === "win32" ? ["gswin64c"] : ["gs"],
    ["--version"],
    "Ghostscript version check"
  );

  const cargoResult = runCommand(
    "cargo",
    [
      "test",
      "--manifest-path",
      path.join(workspace, "src-tauri", "Cargo.toml"),
      "live_pdfa",
      "--",
      "--ignored",
      "--nocapture",
      "--test-threads=1"
    ],
    "Native PDF/A engine corpus",
    45 * 60_000,
    { PAPERWORKS_OCR_CORPUS: corpusDirectory }
  );
  const markers = parsePdfaMarkers(`${cargoResult.stdout}\n${cargoResult.stderr}`);
  const packageJson = JSON.parse(
    await readFile(path.join(workspace, "package.json"), "utf8")
  );
  const report = validatePdfaEngineReport({
    architecture: process.arch,
    batch: markers.batch,
    corpusManifestSha256: corpusReport.corpusManifestSha256,
    engines: {
      ghostscript: {
        command: ghostscript.command,
        version: firstVersionLine(
          `${ghostscript.stdout}\n${ghostscript.stderr}`,
          "Ghostscript"
        )
      },
      ocrMyPdf: {
        command: "ocrmypdf",
        version: ocrMyPdfVersion
      },
      tesseract: {
        command: "tesseract",
        requiredLanguageData: ["eng"],
        version: firstVersionLine(`${tesseract.stdout}\n${tesseract.stderr}`, "Tesseract")
      },
      veraPdf: {
        command: "verapdf",
        version: markers.validatorVersion
      }
    },
    platform: process.platform,
    product: "Tüfekci Paperworks",
    profiles: markers.profiles,
    releaseVersion: packageJson.version,
    schemaVersion: 1
  });
  await ensureOrdinaryDirectory(outputDirectory);
  const reportFilename = `pdfa-engine-report-${process.platform}-${process.arch}.json`;
  await writeReport(
    path.join(outputDirectory, reportFilename),
    `${JSON.stringify(report, null, 2)}\n`
  );
  return { ...report, reportFilename };
}

function firstAvailableCommand(candidates, args, label) {
  let lastError;
  for (const candidate of candidates) {
    try {
      return { command: candidate, ...runCommand(candidate, args, label, 15_000) };
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError || new Error(`${label} could not find a supported command.`);
}

function isEngineVersion(value) {
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
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    throw new Error(`${label} failed with exit code ${result.status}.`);
  }
  return { stderr: result.stderr || "", stdout: result.stdout || "" };
}

async function ensureOrdinaryDirectory(directory) {
  try {
    const metadata = await lstat(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Refusing to use a non-ordinary PDF/A evidence directory.");
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") throw error;
    await mkdir(directory, { recursive: true });
  }
}

async function writeReport(candidate, text) {
  try {
    const metadata = await lstat(candidate);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error("Refusing to replace a non-ordinary PDF/A engine report.");
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") throw error;
  }
  await writeFile(candidate, text, "utf8");
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const report = await runPdfaEngineCorpus(workspace, process.argv[2], process.argv[3]);
  process.stdout.write(
    `PDF/A engine corpus passed ${report.profiles.length} profiles and the batch recipe (${report.reportFilename}).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
