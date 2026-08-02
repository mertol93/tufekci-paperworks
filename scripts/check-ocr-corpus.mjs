import { createHash } from "node:crypto";
import { lstat, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createCanvas, loadImage } from "@napi-rs/canvas";

const maximumPngBytes = 32 * 1024 * 1024;
const maximumTextBytes = 1024 * 1024;
const maximumCorpusBytes = 128 * 1024 * 1024;
const fixtureFields = [
  "expectedTextFilename",
  "filename",
  "height",
  "language",
  "minimumRecall",
  "name",
  "physicalRotationDegrees",
  "pngBytes",
  "pngSha256",
  "profile",
  "textBytes",
  "textSha256",
  "width"
];

export const requiredOcrFixtures = Object.freeze([
  Object.freeze({
    stem: "english",
    language: "eng",
    minimumRecall: 0.85,
    physicalRotationDegrees: 0,
    profile: "clean",
    width: 2_480,
    height: 3_508
  }),
  Object.freeze({
    stem: "turkish",
    language: "tur",
    minimumRecall: 0.75,
    physicalRotationDegrees: 0,
    profile: "clean",
    width: 2_480,
    height: 3_508
  }),
  Object.freeze({
    stem: "rotated",
    language: "eng",
    minimumRecall: 0.8,
    physicalRotationDegrees: 90,
    profile: "rotated",
    width: 3_508,
    height: 2_480
  }),
  Object.freeze({
    stem: "noisy",
    language: "eng",
    minimumRecall: 0.65,
    physicalRotationDegrees: 0,
    profile: "noisy",
    width: 2_480,
    height: 3_508
  })
]);

export function validateOcrCorpusManifest(value) {
  requireExactFields(
    value,
    ["fixtures", "generator", "product", "schemaVersion"],
    "OCR corpus manifest"
  );
  if (
    value.schemaVersion !== 1 ||
    value.product !== "Tüfekci Paperworks" ||
    value.generator !== "paperworks-synthetic-ocr-v1"
  ) {
    throw new Error("The OCR corpus identity or schema version is unsupported.");
  }
  if (
    !Array.isArray(value.fixtures) ||
    value.fixtures.length !== requiredOcrFixtures.length
  ) {
    throw new Error("The OCR corpus does not contain the complete required fixture set.");
  }

  const seen = new Set();
  for (const fixture of value.fixtures) {
    requireExactFields(fixture, fixtureFields, "OCR fixture");
    if (
      typeof fixture.filename !== "string" ||
      !/^[a-z][a-z0-9-]*\.png$/u.test(fixture.filename) ||
      typeof fixture.expectedTextFilename !== "string" ||
      !/^[a-z][a-z0-9-]*\.txt$/u.test(fixture.expectedTextFilename)
    ) {
      throw new Error("OCR fixture filenames must be safe plain PNG and text names.");
    }
    const stem = fixture.filename.slice(0, -4);
    if (
      fixture.expectedTextFilename !== `${stem}.txt` ||
      seen.has(stem.toLocaleLowerCase("en-GB"))
    ) {
      throw new Error("OCR fixture filenames must be unique matching pairs.");
    }
    seen.add(stem.toLocaleLowerCase("en-GB"));

    const required = requiredOcrFixtures.find((entry) => entry.stem === stem);
    if (
      !required ||
      fixture.language !== required.language ||
      fixture.minimumRecall !== required.minimumRecall ||
      fixture.physicalRotationDegrees !== required.physicalRotationDegrees ||
      fixture.profile !== required.profile ||
      fixture.width !== required.width ||
      fixture.height !== required.height
    ) {
      throw new Error(`The OCR fixture contract is invalid for ${fixture.filename}.`);
    }
    if (
      typeof fixture.name !== "string" ||
      fixture.name.length < 3 ||
      fixture.name.length > 100 ||
      /[\0\r\n]/u.test(fixture.name)
    ) {
      throw new Error(`The OCR fixture name is invalid for ${fixture.filename}.`);
    }
    validateByteCount(fixture.pngBytes, 1, maximumPngBytes, fixture.filename);
    validateByteCount(
      fixture.textBytes,
      1,
      maximumTextBytes,
      fixture.expectedTextFilename
    );
    for (const [field, label] of [
      ["pngSha256", fixture.filename],
      ["textSha256", fixture.expectedTextFilename]
    ]) {
      if (
        typeof fixture[field] !== "string" ||
        !/^[0-9A-F]{64}$/u.test(fixture[field])
      ) {
        throw new Error(`The SHA-256 digest is invalid for ${label}.`);
      }
    }
  }

  const expectedStems = requiredOcrFixtures.map((fixture) => fixture.stem).sort();
  const actualStems = [...seen].sort();
  if (actualStems.some((stem, index) => stem !== expectedStems[index])) {
    throw new Error("The OCR corpus does not contain the complete required fixture set.");
  }
  return value;
}

export function normaliseOcrExpectedText(value) {
  return String(value).normalize("NFKC").replace(/\s+/gu, " ").trim();
}

export function summariseOcrPixels(bytes, width, height) {
  if (
    !(bytes instanceof Uint8ClampedArray) ||
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width * height > 1_000_000 ||
    bytes.length !== width * height * 4
  ) {
    throw new Error("OCR fixture pixel data has invalid bounded dimensions.");
  }

  let darkPixels = 0;
  let lightPixels = 0;
  let minimumLuminance = 255;
  let maximumLuminance = 0;
  for (let offset = 0; offset < bytes.length; offset += 4) {
    const red = bytes[offset];
    const green = bytes[offset + 1];
    const blue = bytes[offset + 2];
    const alpha = bytes[offset + 3];
    const luminance = Math.round((red * 299 + green * 587 + blue * 114) / 1_000);
    minimumLuminance = Math.min(minimumLuminance, luminance);
    maximumLuminance = Math.max(maximumLuminance, luminance);
    if (alpha > 0 && luminance < 180) {
      darkPixels += 1;
    }
    if (alpha > 0 && luminance > 225) {
      lightPixels += 1;
    }
  }
  return {
    darkPixels,
    height,
    lightPixels,
    maximumLuminance,
    minimumLuminance,
    rgbaSha256: sha256(bytes),
    width
  };
}

export async function checkOcrCorpus(workspace, corpusArgument) {
  const corpusDirectory = path.resolve(
    workspace,
    corpusArgument || "qa-fixtures/ocr-corpus"
  );
  const manifestPath = path.join(corpusDirectory, "ocr-corpus.json");
  const manifestMetadata = await requireOrdinaryFile(
    manifestPath,
    maximumTextBytes
  );
  const manifestBytes = await readFile(manifestPath);
  const manifestText = decodeLfUtf8(manifestBytes, "The OCR corpus manifest");
  let manifest;
  try {
    manifest = validateOcrCorpusManifest(JSON.parse(manifestText));
  } catch (error) {
    throw new Error(
      `The OCR corpus manifest is invalid: ${error instanceof Error ? error.message : error}`
    );
  }

  let corpusBytes = manifestMetadata.size;
  const fixtureResults = [];
  for (const fixture of manifest.fixtures) {
    const pngPath = path.join(corpusDirectory, fixture.filename);
    const textPath = path.join(corpusDirectory, fixture.expectedTextFilename);
    const [pngMetadata, textMetadata] = await Promise.all([
      requireOrdinaryFile(pngPath, maximumPngBytes),
      requireOrdinaryFile(textPath, maximumTextBytes)
    ]);
    corpusBytes += pngMetadata.size + textMetadata.size;
    if (corpusBytes > maximumCorpusBytes) {
      throw new Error("The OCR corpus exceeds the 128 MiB safety limit.");
    }
    if (
      pngMetadata.size !== fixture.pngBytes ||
      textMetadata.size !== fixture.textBytes
    ) {
      throw new Error(`The recorded byte count does not match ${fixture.filename}.`);
    }

    const [pngBytes, expectedTextBytes] = await Promise.all([
      readFile(pngPath),
      readFile(textPath)
    ]);
    if (sha256(pngBytes) !== fixture.pngSha256) {
      throw new Error(`The PNG digest does not match ${fixture.filename}.`);
    }
    if (sha256(expectedTextBytes) !== fixture.textSha256) {
      throw new Error(
        `The expected-text digest does not match ${fixture.expectedTextFilename}.`
      );
    }
    if (
      pngBytes.length < 8 ||
      !pngBytes.subarray(0, 8).equals(
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
      )
    ) {
      throw new Error(`${fixture.filename} is not a PNG image.`);
    }

    const expectedText = decodeLfUtf8(
      expectedTextBytes,
      fixture.expectedTextFilename
    );
    validateExpectedText(fixture.filename.slice(0, -4), expectedText);
    const pixels = await inspectPng(pngBytes, fixture);
    fixtureResults.push({
      expectedTextSha256: fixture.textSha256,
      filename: fixture.filename,
      imageSha256: fixture.pngSha256,
      language: fixture.language,
      minimumRecall: fixture.minimumRecall,
      physicalRotationDegrees: fixture.physicalRotationDegrees,
      pixels,
      profile: fixture.profile
    });
  }

  const report = {
    schemaVersion: 1,
    product: manifest.product,
    architecture: process.arch,
    corpusBytes,
    corpusManifestSha256: sha256(manifestBytes),
    fixtures: fixtureResults,
    platform: process.platform
  };
  const reportFilename = `ocr-corpus-report-${process.platform}-${process.arch}.json`;
  await writeReport(
    path.join(corpusDirectory, reportFilename),
    `${JSON.stringify(report, null, 2)}\n`
  );
  return { ...report, reportFilename };
}

async function inspectPng(bytes, fixture) {
  const image = await loadImage(bytes);
  if (image.width !== fixture.width || image.height !== fixture.height) {
    throw new Error(`The decoded dimensions do not match ${fixture.filename}.`);
  }
  const scale = Math.min(1, 900 / Math.max(image.width, image.height));
  const width = Math.max(1, Math.round(image.width * scale));
  const height = Math.max(1, Math.round(image.height * scale));
  const canvas = createCanvas(width, height);
  const context = canvas.getContext("2d");
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, width, height);
  context.drawImage(image, 0, 0, width, height);
  const summary = summariseOcrPixels(
    context.getImageData(0, 0, width, height).data,
    width,
    height
  );
  if (
    summary.darkPixels < 1_000 ||
    summary.lightPixels < 10_000 ||
    summary.maximumLuminance - summary.minimumLuminance < 70
  ) {
    throw new Error(`${fixture.filename} does not contain a legible page image.`);
  }
  return summary;
}

function validateExpectedText(stem, text) {
  if (
    !text.endsWith("\n") ||
    text.length < 120 ||
    normaliseOcrExpectedText(text).split(" ").length < 20
  ) {
    throw new Error(`${stem}.txt does not contain enough bounded expected text.`);
  }
  if (stem === "english") {
    const normalised = normaliseOcrExpectedText(text).toLocaleLowerCase("en-GB");
    for (const word of ["organise", "colour", "recognise", "licence"]) {
      if (!normalised.includes(word)) {
        throw new Error(`english.txt is missing the UK English word '${word}'.`);
      }
    }
  }
  if (stem === "turkish") {
    for (const character of ["ç", "ğ", "ı", "İ", "ö", "ş", "ü"]) {
      if (!text.includes(character)) {
        throw new Error(`turkish.txt is missing the required character '${character}'.`);
      }
    }
  }
}

function decodeLfUtf8(bytes, label) {
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(`${label} must be valid UTF-8.`);
  }
  if (text.charCodeAt(0) === 0xfeff || text.includes("\r") || text.includes("\0")) {
    throw new Error(`${label} must be UTF-8 with LF line endings and no control data.`);
  }
  return text;
}

async function requireOrdinaryFile(candidate, maximumBytes) {
  const metadata = await lstat(candidate);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size === 0 ||
    metadata.size > maximumBytes
  ) {
    throw new Error(
      `OCR corpus files must be ordinary and bounded: ${path.basename(candidate)}.`
    );
  }
  return metadata;
}

function validateByteCount(value, minimum, maximum, label) {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`The recorded byte count is invalid for ${label}.`);
  }
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

async function writeReport(candidate, text) {
  try {
    const metadata = await lstat(candidate);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error("Refusing to replace a non-ordinary OCR corpus report.");
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") {
      throw error;
    }
  }
  await writeFile(candidate, text, "utf8");
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex").toUpperCase();
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const report = await checkOcrCorpus(workspace, process.argv[2]);
  process.stdout.write(
    `OCR corpus passed for ${report.fixtures.length} synthetic fixtures (${report.reportFilename}).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
