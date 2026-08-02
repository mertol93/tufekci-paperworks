import { createHash } from "node:crypto";
import { lstat, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const maxFixtureBytes = 64 * 1024 * 1024;
const maxCorpusBytes = 128 * 1024 * 1024;
const maxRenderedPixels = 1_500 * 1_500;
const fixtureFields = [
  "expectedCodePoints",
  "expectedOutcome",
  "expectedPageSizes",
  "expectedText",
  "filename",
  "minimumAnnotations",
  "minimumInkPixels",
  "name",
  "pageCount",
  "password",
  "requireNoText",
  "requirePasswordChallenge",
  "requireRtl",
  "samplePages"
];

export const requiredRenderingFixtureFiles = Object.freeze([
  "accessibility-review.pdf",
  "annotations-and-form.pdf",
  "cjk-rtl-type3.pdf",
  "encrypted-aes256.pdf",
  "malformed-truncated.pdf",
  "range-loading.pdf",
  "scanned-image.pdf",
  "signed-structure.pdf",
  "unusual-page-sizes.pdf"
]);

export function validateRenderingCorpusManifest(value, requireComplete = true) {
  requireExactFields(value, ["fixtures", "schemaVersion"], "rendering corpus manifest");
  if (value.schemaVersion !== 1) {
    throw new Error("The rendering corpus schema version is unsupported.");
  }
  if (!Array.isArray(value.fixtures) || value.fixtures.length === 0 || value.fixtures.length > 32) {
    throw new Error("The rendering corpus must contain between 1 and 32 fixtures.");
  }

  const seen = new Set();
  for (const fixture of value.fixtures) {
    requireExactFields(fixture, fixtureFields, "rendering fixture");
    if (typeof fixture.name !== "string" || !fixture.name || fixture.name.length > 80) {
      throw new Error("Every rendering fixture needs a bounded name.");
    }
    if (
      typeof fixture.filename !== "string" ||
      !/^[a-z0-9][a-z0-9-]*\.pdf$/u.test(fixture.filename)
    ) {
      throw new Error("Every rendering fixture filename must be a safe plain PDF name.");
    }
    const filenameKey = fixture.filename.toLocaleLowerCase("en-GB");
    if (seen.has(filenameKey)) {
      throw new Error(`The rendering corpus contains a duplicate file: ${fixture.filename}.`);
    }
    seen.add(filenameKey);

    if (!["render", "reject"].includes(fixture.expectedOutcome)) {
      throw new Error(`The rendering outcome is invalid for ${fixture.filename}.`);
    }
    validateStringList(fixture.expectedText, "expected text", 64, 4_096);
    validateStringList(fixture.expectedCodePoints, "expected code points", 64, 256);
    if (
      fixture.expectedCodePoints.some(
        (candidate) => Array.from(candidate).length !== 1
      )
    ) {
      throw new Error(`Expected code points must contain one character: ${fixture.filename}.`);
    }
    for (const field of ["requirePasswordChallenge", "requireRtl", "requireNoText"]) {
      if (typeof fixture[field] !== "boolean") {
        throw new Error(`The ${field} flag must be Boolean for ${fixture.filename}.`);
      }
    }
    if (
      !Number.isInteger(fixture.minimumInkPixels) ||
      fixture.minimumInkPixels < 0 ||
      fixture.minimumInkPixels > maxRenderedPixels
    ) {
      throw new Error(`The minimum ink-pixel count is invalid for ${fixture.filename}.`);
    }
    if (
      !Number.isInteger(fixture.minimumAnnotations) ||
      fixture.minimumAnnotations < 0 ||
      fixture.minimumAnnotations > 1_000
    ) {
      throw new Error(`The annotation minimum is invalid for ${fixture.filename}.`);
    }
    if (
      fixture.password !== null &&
      (typeof fixture.password !== "string" ||
        fixture.password.length === 0 ||
        Buffer.byteLength(fixture.password, "utf8") > 127 ||
        /[\0\r\n]/u.test(fixture.password))
    ) {
      throw new Error(`The test password is invalid for ${fixture.filename}.`);
    }
    if (fixture.requirePasswordChallenge && !fixture.password) {
      throw new Error(`A password challenge needs a test password: ${fixture.filename}.`);
    }

    if (fixture.expectedOutcome === "reject") {
      if (
        fixture.pageCount !== null ||
        fixture.password !== null ||
        fixture.samplePages.length !== 0 ||
        fixture.expectedPageSizes.length !== 0
      ) {
        throw new Error(`Rejected fixtures cannot declare render results: ${fixture.filename}.`);
      }
      continue;
    }

    if (!Number.isInteger(fixture.pageCount) || fixture.pageCount < 1 || fixture.pageCount > 20_000) {
      throw new Error(`The page count is invalid for ${fixture.filename}.`);
    }
    if (
      !Array.isArray(fixture.samplePages) ||
      fixture.samplePages.length === 0 ||
      fixture.samplePages.length > 32 ||
      new Set(fixture.samplePages).size !== fixture.samplePages.length ||
      fixture.samplePages.some(
        (pageNumber) =>
          !Number.isInteger(pageNumber) ||
          pageNumber < 1 ||
          pageNumber > fixture.pageCount
      )
    ) {
      throw new Error(`The sampled pages are invalid for ${fixture.filename}.`);
    }
    if (
      !Array.isArray(fixture.expectedPageSizes) ||
      ![0, fixture.pageCount].includes(fixture.expectedPageSizes.length) ||
      fixture.expectedPageSizes.some(
        (size) =>
          !Array.isArray(size) ||
          size.length !== 2 ||
          size.some(
            (dimension) =>
              typeof dimension !== "number" ||
              !Number.isFinite(dimension) ||
              dimension < 1 ||
              dimension > 14_400
          )
      )
    ) {
      throw new Error(`The expected page sizes are invalid for ${fixture.filename}.`);
    }
  }

  if (requireComplete) {
    const actual = [...seen].sort();
    const expected = requiredRenderingFixtureFiles.map((candidate) =>
      candidate.toLocaleLowerCase("en-GB")
    ).sort();
    if (
      actual.length !== expected.length ||
      actual.some((candidate, index) => candidate !== expected[index])
    ) {
      throw new Error("The rendering corpus does not contain the complete required fixture set.");
    }
  }
  return value;
}

export function normaliseRenderingText(value) {
  return String(value).normalize("NFKC").replace(/\s+/gu, " ").trim();
}

export function summariseRenderedPixels(bytes, width, height) {
  if (
    !(bytes instanceof Uint8ClampedArray) ||
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width * height > maxRenderedPixels ||
    bytes.length !== width * height * 4
  ) {
    throw new Error("Rendered pixel data has invalid bounded dimensions.");
  }

  let colourfulPixels = 0;
  let inkPixels = 0;
  let maximumLuminance = 0;
  let minimumLuminance = 255;
  for (let offset = 0; offset < bytes.length; offset += 4) {
    const red = bytes[offset];
    const green = bytes[offset + 1];
    const blue = bytes[offset + 2];
    const alpha = bytes[offset + 3];
    const luminance = Math.round((red * 299 + green * 587 + blue * 114) / 1_000);
    minimumLuminance = Math.min(minimumLuminance, luminance);
    maximumLuminance = Math.max(maximumLuminance, luminance);
    if (alpha > 0 && (red < 245 || green < 245 || blue < 245)) {
      inkPixels += 1;
    }
    if (alpha > 0 && Math.max(red, green, blue) - Math.min(red, green, blue) >= 20) {
      colourfulPixels += 1;
    }
  }
  return {
    colourfulPixels,
    height,
    inkPixels,
    maximumLuminance,
    minimumLuminance,
    rgbaSha256: sha256(bytes),
    width
  };
}

export async function checkRenderingCorpus(workspace, corpusArgument) {
  const corpusDirectory = path.resolve(
    workspace,
    corpusArgument || "qa-fixtures"
  );
  const manifestPath = path.join(corpusDirectory, "rendering-corpus.json");
  await requireOrdinaryFile(manifestPath, 1024 * 1024);
  const manifestBytes = await readFile(manifestPath);
  const manifestText = new TextDecoder("utf-8", { fatal: true }).decode(manifestBytes);
  if (manifestText.charCodeAt(0) === 0xfeff || manifestText.includes("\r")) {
    throw new Error("The rendering corpus manifest must be UTF-8 with LF line endings.");
  }
  let manifest;
  try {
    manifest = validateRenderingCorpusManifest(JSON.parse(manifestText));
  } catch (error) {
    throw new Error(
      `The rendering corpus manifest is invalid: ${error instanceof Error ? error.message : error}`
    );
  }

  const renderer = await loadRenderer(workspace);
  const fixtureResults = [];
  let corpusBytes = manifestBytes.length;
  let renderedDocuments = 0;
  let renderedPages = 0;
  let rejectedDocuments = 0;

  for (const fixture of manifest.fixtures) {
    const fixturePath = path.join(corpusDirectory, fixture.filename);
    const metadata = await requireOrdinaryFile(fixturePath, maxFixtureBytes);
    corpusBytes += metadata.size;
    if (corpusBytes > maxCorpusBytes) {
      throw new Error("The rendering corpus exceeds the 128 MiB limit.");
    }
    const bytes = await readFile(fixturePath);
    if (fixture.expectedOutcome === "reject") {
      await expectInvalidPdf(renderer, bytes, fixture.filename);
      rejectedDocuments += 1;
      fixtureResults.push({
        bytes: bytes.length,
        filename: fixture.filename,
        outcome: "rejected",
        sha256: sha256(bytes)
      });
      continue;
    }

    if (fixture.requirePasswordChallenge) {
      await expectPasswordChallenge(renderer, bytes, fixture);
    }
    const result = await inspectRenderablePdf(renderer, bytes, fixture);
    renderedDocuments += 1;
    renderedPages += result.pageCount;
    fixtureResults.push({
      bytes: bytes.length,
      filename: fixture.filename,
      outcome: "rendered",
      sha256: sha256(bytes),
      ...result
    });
  }

  const report = {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    appVersion: JSON.parse(
      await readFile(path.join(workspace, "package.json"), "utf8")
    ).version,
    platform: process.platform,
    architecture: process.arch,
    pdfjsVersion: renderer.version,
    canvasVersion: JSON.parse(
      await readFile(
        path.join(workspace, "node_modules", "@napi-rs", "canvas", "package.json"),
        "utf8"
      )
    ).version,
    manifestSha256: sha256(manifestBytes),
    corpusBytes,
    renderedDocuments,
    renderedPages,
    rejectedDocuments,
    fixtures: fixtureResults
  };
  const reportFilename = `rendering-report-${process.platform}-${process.arch}.json`;
  await writeFile(
    path.join(corpusDirectory, reportFilename),
    `${JSON.stringify(report, null, 2)}\n`,
    "utf8"
  );
  return { ...report, reportFilename };
}

async function loadRenderer(workspace) {
  const {
    DOMMatrix,
    ImageData,
    Path2D,
    createCanvas
  } = await import("@napi-rs/canvas");
  Object.assign(globalThis, {
    DOMMatrix,
    ImageData,
    Path2D
  });
  const pdfjs = await import("pdfjs-dist/legacy/build/pdf.mjs");
  const assetRoot = path
    .resolve(workspace, "node_modules", "pdfjs-dist")
    .replaceAll("\\", "/");
  const directoryUrl = (name) => `${assetRoot}/${name}/`;
  return {
    ...pdfjs,
    createCanvas,
    documentOptions: {
      cMapPacked: true,
      cMapUrl: directoryUrl("cmaps"),
      disableWorker: true,
      enableXfa: false,
      iccUrl: directoryUrl("iccs"),
      isEvalSupported: false,
      isImageDecoderSupported: false,
      isOffscreenCanvasSupported: false,
      standardFontDataUrl: directoryUrl("standard_fonts"),
      stopAtErrors: true,
      useSystemFonts: false,
      useWorkerFetch: false,
      verbosity: pdfjs.VerbosityLevel.ERRORS,
      wasmUrl: directoryUrl("wasm")
    }
  };
}

async function expectInvalidPdf(renderer, bytes, filename) {
  const task = renderer.getDocument({
    ...renderer.documentOptions,
    data: new Uint8Array(bytes)
  });
  try {
    await task.promise;
    throw new Error(`Malformed fixture unexpectedly opened: ${filename}.`);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("Malformed fixture unexpectedly")) {
      throw error;
    }
    if (error?.name !== "InvalidPDFException") {
      throw new Error(`Malformed fixture failed with the wrong error class: ${filename}.`);
    }
  } finally {
    await task.destroy().catch(() => {});
  }
}

async function expectPasswordChallenge(renderer, bytes, fixture) {
  const noPasswordTask = renderer.getDocument({
    ...renderer.documentOptions,
    data: new Uint8Array(bytes)
  });
  try {
    await noPasswordTask.promise;
    throw new Error(`Encrypted fixture opened without a password: ${fixture.filename}.`);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("Encrypted fixture opened")) {
      throw error;
    }
    if (
      error?.name !== "PasswordException" ||
      error?.code !== renderer.PasswordResponses.NEED_PASSWORD
    ) {
      throw new Error(`Encrypted fixture did not request a password: ${fixture.filename}.`);
    }
  } finally {
    await noPasswordTask.destroy().catch(() => {});
  }

  const wrongPasswordTask = renderer.getDocument({
    ...renderer.documentOptions,
    data: new Uint8Array(bytes),
    password: `${fixture.password}-incorrect`
  });
  try {
    await wrongPasswordTask.promise;
    throw new Error(`Encrypted fixture accepted the wrong password: ${fixture.filename}.`);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("Encrypted fixture accepted")) {
      throw error;
    }
    if (
      error?.name !== "PasswordException" ||
      error?.code !== renderer.PasswordResponses.INCORRECT_PASSWORD
    ) {
      throw new Error(`Encrypted fixture did not reject a wrong password: ${fixture.filename}.`);
    }
  } finally {
    await wrongPasswordTask.destroy().catch(() => {});
  }
}

async function inspectRenderablePdf(renderer, bytes, fixture) {
  const task = renderer.getDocument({
    ...renderer.documentOptions,
    data: new Uint8Array(bytes),
    password: fixture.password || undefined
  });
  try {
    const document = await task.promise;
    if (document.numPages !== fixture.pageCount) {
      throw new Error(
        `${fixture.filename} has ${document.numPages} pages; ${fixture.pageCount} were expected.`
      );
    }
    const samplePages = new Set(fixture.samplePages);
    const textParts = [];
    const directions = new Set();
    const pixelSamples = [];
    let annotationCount = 0;
    let operatorCount = 0;

    for (let pageNumber = 1; pageNumber <= document.numPages; pageNumber += 1) {
      const page = await document.getPage(pageNumber);
      const unitViewport = page.getViewport({ scale: 1 });
      if (fixture.expectedPageSizes.length > 0) {
        const [expectedWidth, expectedHeight] = fixture.expectedPageSizes[pageNumber - 1];
        if (
          Math.abs(unitViewport.width - expectedWidth) > 0.01 ||
          Math.abs(unitViewport.height - expectedHeight) > 0.01
        ) {
          throw new Error(`The page geometry differs for ${fixture.filename}, page ${pageNumber}.`);
        }
      }
      const operatorList = await page.getOperatorList();
      if (operatorList.fnArray.length === 0) {
        throw new Error(`PDF.js found no drawing operators in ${fixture.filename}, page ${pageNumber}.`);
      }
      operatorCount += operatorList.fnArray.length;

      if (samplePages.has(pageNumber)) {
        const textContent = await page.getTextContent();
        for (const item of textContent.items) {
          if (typeof item.str === "string") {
            textParts.push(item.str);
          }
          if (typeof item.dir === "string") {
            directions.add(item.dir);
          }
        }
        annotationCount += (
          await page.getAnnotations({ intent: "display" })
        ).length;
        const pixels = await renderPage(renderer, page, unitViewport);
        if (pixels.inkPixels < fixture.minimumInkPixels) {
          throw new Error(
            `${fixture.filename}, page ${pageNumber} rendered only ${pixels.inkPixels} ink pixels.`
          );
        }
        if (pixels.inkPixels / (pixels.width * pixels.height) >= 0.95) {
          throw new Error(`${fixture.filename}, page ${pageNumber} rendered as an implausibly full page.`);
        }
        pixelSamples.push({ pageNumber, ...pixels });
      }
      page.cleanup();
    }

    const text = normaliseRenderingText(textParts.join(" "));
    if (fixture.requireNoText && text) {
      throw new Error(`${fixture.filename} unexpectedly exposed searchable text.`);
    }
    for (const expected of fixture.expectedText) {
      if (!text.includes(normaliseRenderingText(expected))) {
        throw new Error(`${fixture.filename} is missing expected searchable fixture text.`);
      }
    }
    for (const codePoint of fixture.expectedCodePoints) {
      if (!text.includes(codePoint)) {
        throw new Error(`${fixture.filename} is missing an expected Unicode code point.`);
      }
    }
    if (fixture.requireRtl && !directions.has("rtl")) {
      throw new Error(`${fixture.filename} did not expose a right-to-left text run.`);
    }
    if (annotationCount < fixture.minimumAnnotations) {
      throw new Error(`${fixture.filename} exposed too few display annotations.`);
    }

    return {
      annotationCount,
      operatorCount,
      pageCount: document.numPages,
      pixelSamples,
      rightToLeftText: directions.has("rtl"),
      searchableText: Boolean(text)
    };
  } finally {
    await task.destroy().catch(() => {});
  }
}

async function renderPage(renderer, page, unitViewport) {
  const scale = Math.min(1.25, 1_200 / Math.max(unitViewport.width, unitViewport.height));
  const viewport = page.getViewport({ scale });
  const width = Math.max(1, Math.ceil(viewport.width));
  const height = Math.max(1, Math.ceil(viewport.height));
  if (width * height > maxRenderedPixels) {
    throw new Error("A rendering-corpus page exceeds the canvas pixel limit.");
  }
  const canvas = renderer.createCanvas(width, height);
  const context = canvas.getContext("2d");
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, width, height);
  await page.render({
    annotationMode: renderer.AnnotationMode.ENABLE_FORMS,
    background: "rgb(255,255,255)",
    canvasContext: context,
    viewport
  }).promise;
  return summariseRenderedPixels(
    context.getImageData(0, 0, width, height).data,
    width,
    height
  );
}

async function requireOrdinaryFile(candidate, maximumBytes) {
  const metadata = await lstat(candidate);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size === 0 ||
    metadata.size > maximumBytes
  ) {
    throw new Error(`Rendering corpus files must be ordinary and bounded: ${path.basename(candidate)}.`);
  }
  return metadata;
}

function validateStringList(value, label, maximumItems, maximumBytes) {
  if (
    !Array.isArray(value) ||
    value.length > maximumItems ||
    value.some(
      (candidate) =>
        typeof candidate !== "string" ||
        !candidate ||
        /[\0\r\n]/u.test(candidate)
    ) ||
    Buffer.byteLength(value.join(""), "utf8") > maximumBytes
  ) {
    throw new Error(`The rendering fixture ${label} is invalid.`);
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

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex").toUpperCase();
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const report = await checkRenderingCorpus(workspace, process.argv[2]);
  process.stdout.write(
    `Rendering corpus passed for ${report.renderedDocuments} documents and ${report.renderedPages} pages, with ${report.rejectedDocuments} malformed input rejected (${report.reportFilename}).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
