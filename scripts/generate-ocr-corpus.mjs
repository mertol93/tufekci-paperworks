import { createHash } from "node:crypto";
import { lstat, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createCanvas, GlobalFonts } from "@napi-rs/canvas";

const fontFamily = "Paperworks OCR Fixture";
const portraitWidth = 2_480;
const portraitHeight = 3_508;
const fixtureFontRelativePath =
  "node_modules/pdfjs-dist/standard_fonts/LiberationSans-Regular.ttf";
let fixtureFontRegistered = false;

export const ocrFixtureDefinitions = Object.freeze([
  Object.freeze({
    name: "English UK clean page",
    stem: "english",
    language: "eng",
    minimumRecall: 0.85,
    physicalRotationDegrees: 0,
    profile: "clean",
    title: "PAPERWORKS OCR RELEASE TEST",
    lines: Object.freeze([
      "Local document processing keeps private files on this computer.",
      "This synthetic page checks searchable text and dependable export.",
      "Please organise colour records, recognise each page and verify the licence.",
      "The quick brown fox jumps over the lazy dog beside the quiet harbour.",
      "Reference ALPHA 2048 BRAVO 7319 CHARLIE 5602."
    ])
  }),
  Object.freeze({
    name: "Turkish clean page",
    stem: "turkish",
    language: "tur",
    minimumRecall: 0.75,
    physicalRotationDegrees: 0,
    profile: "clean",
    title: "TÜFEKCİ PAPERWORKS TÜRKÇE OCR SINAVI",
    lines: Object.freeze([
      "Çevrimdışı belge işleme güvenli, hızlı ve özeldir.",
      "İstanbul, İzmir, Ankara, Şanlıurfa ve Gümüşhane kayıtları.",
      "Doğru sayfa sırası, görünür metin ve güvenilir dışa aktarma.",
      "Çalışma öğleden önce başladı; bütün ölçüler dikkatle incelendi.",
      "Referans ALFA 2048 BETA 7319 GAMMA 5602."
    ])
  }),
  Object.freeze({
    name: "Physically rotated English page",
    stem: "rotated",
    language: "eng",
    minimumRecall: 0.8,
    physicalRotationDegrees: 90,
    profile: "rotated",
    title: "ROTATED PAGE ORIENTATION TEST",
    lines: Object.freeze([
      "Paperworks should detect this page and turn it upright.",
      "Searchable text must preserve the complete reading order.",
      "Rotation, deskewing and export are checked without private documents.",
      "Five quartz boxes judge the sturdy paper workflow.",
      "Reference DELTA 8642 ECHO 9753 FOXTROT 2468."
    ])
  }),
  Object.freeze({
    name: "Noisy low-contrast English page",
    stem: "noisy",
    language: "eng",
    minimumRecall: 0.65,
    physicalRotationDegrees: 0,
    profile: "noisy",
    title: "NOISY SCAN RECOVERY TEST",
    lines: Object.freeze([
      "Faint text remains readable beneath uneven lighting and speckles.",
      "The page simulates a worn office scan with a soft shadow.",
      "Paperworks should recover useful words and create searchable output.",
      "Careful local processing protects confidential document content.",
      "Reference GOLF 1357 HOTEL 8024 INDIA 6193."
    ])
  })
]);

export function createDeterministicRandom(seed) {
  if (!Number.isInteger(seed)) {
    throw new Error("The deterministic random seed must be an integer.");
  }
  let state = seed >>> 0;
  if (state === 0) {
    state = 0x6d2b79f5;
  }
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}

export function expectedTextForFixture(fixture) {
  return [fixture.title, ...fixture.lines, "Synthetic release evidence - no personal information."]
    .join("\n")
    .concat("\n");
}

export async function generateOcrCorpus(workspace, outputArgument) {
  registerFixtureFont(workspace);
  const outputDirectory = path.resolve(
    workspace,
    outputArgument || "qa-fixtures/ocr-corpus"
  );
  await requireWritableDirectory(outputDirectory);

  const fixtures = [];
  for (const fixture of ocrFixtureDefinitions) {
    const expectedText = expectedTextForFixture(fixture);
    const portrait = renderPortraitFixture(fixture);
    const image =
      fixture.physicalRotationDegrees === 90
        ? rotateClockwise(portrait)
        : portrait;
    const pngBytes = image.toBuffer("image/png");
    const textBytes = Buffer.from(expectedText, "utf8");
    const filename = `${fixture.stem}.png`;
    const expectedTextFilename = `${fixture.stem}.txt`;
    await writeOwnedFile(path.join(outputDirectory, filename), pngBytes);
    await writeOwnedFile(
      path.join(outputDirectory, expectedTextFilename),
      textBytes
    );
    fixtures.push({
      expectedTextFilename,
      filename,
      height: image.height,
      language: fixture.language,
      minimumRecall: fixture.minimumRecall,
      name: fixture.name,
      physicalRotationDegrees: fixture.physicalRotationDegrees,
      pngBytes: pngBytes.length,
      pngSha256: sha256(pngBytes),
      profile: fixture.profile,
      textBytes: textBytes.length,
      textSha256: sha256(textBytes),
      width: image.width
    });
  }

  const manifest = {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    generator: "paperworks-synthetic-ocr-v1",
    fixtures
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  await writeOwnedFile(
    path.join(outputDirectory, "ocr-corpus.json"),
    manifestBytes
  );
  return { manifest, outputDirectory };
}

function registerFixtureFont(workspace) {
  if (fixtureFontRegistered) {
    return;
  }
  const fontPath = path.resolve(workspace, fixtureFontRelativePath);
  const registration = GlobalFonts.registerFromPath(fontPath, fontFamily);
  if (!registration) {
    throw new Error(
      "The bundled Liberation Sans font could not be registered for OCR fixtures."
    );
  }
  fixtureFontRegistered = true;
}

function renderPortraitFixture(fixture) {
  const canvas = createCanvas(portraitWidth, portraitHeight);
  const context = canvas.getContext("2d");
  drawBackground(context, fixture.profile);

  context.save();
  if (fixture.profile === "noisy") {
    context.translate(portraitWidth / 2, portraitHeight / 2);
    context.rotate(-0.008);
    context.translate(-portraitWidth / 2, -portraitHeight / 2);
  }
  drawDocumentFrame(context, fixture.profile);
  drawFixtureText(context, fixture);
  context.restore();

  if (fixture.profile === "noisy") {
    drawNoise(context);
  }
  return canvas;
}

function drawBackground(context, profile) {
  if (profile !== "noisy") {
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, portraitWidth, portraitHeight);
    return;
  }

  const lighting = context.createLinearGradient(0, 0, portraitWidth, portraitHeight);
  lighting.addColorStop(0, "#f4f3ed");
  lighting.addColorStop(0.48, "#e7e5dd");
  lighting.addColorStop(1, "#d6d4cb");
  context.fillStyle = lighting;
  context.fillRect(0, 0, portraitWidth, portraitHeight);

  const shadow = context.createRadialGradient(
    portraitWidth * 0.92,
    portraitHeight * 0.5,
    80,
    portraitWidth * 0.92,
    portraitHeight * 0.5,
    portraitWidth * 0.7
  );
  shadow.addColorStop(0, "rgba(55, 55, 50, 0.18)");
  shadow.addColorStop(1, "rgba(55, 55, 50, 0)");
  context.fillStyle = shadow;
  context.fillRect(0, 0, portraitWidth, portraitHeight);
}

function drawDocumentFrame(context, profile) {
  const ink = profile === "noisy" ? "#666660" : "#20252b";
  context.strokeStyle = ink;
  context.lineWidth = 5;
  context.strokeRect(170, 170, portraitWidth - 340, portraitHeight - 340);
  context.lineWidth = 3;
  context.beginPath();
  context.moveTo(250, 570);
  context.lineTo(portraitWidth - 250, 570);
  context.stroke();
}

function drawFixtureText(context, fixture) {
  const ink = fixture.profile === "noisy" ? "#555550" : "#111820";
  context.fillStyle = ink;
  context.textBaseline = "alphabetic";
  context.font = `76px "${fontFamily}"`;
  requireLineFits(context, fixture.title, 1_980);
  context.fillText(fixture.title, 250, 450);

  context.font = `56px "${fontFamily}"`;
  let y = 790;
  for (const line of fixture.lines) {
    requireLineFits(context, line, 1_980);
    context.fillText(line, 250, y);
    y += 230;
  }

  context.font = `44px "${fontFamily}"`;
  const footer = "Synthetic release evidence - no personal information.";
  requireLineFits(context, footer, 1_980);
  context.fillText(footer, 250, 3_220);
}

function requireLineFits(context, line, maximumWidth) {
  if (context.measureText(line).width > maximumWidth) {
    throw new Error(`An OCR fixture line is too wide for the page: ${line}`);
  }
}

function drawNoise(context) {
  const random = createDeterministicRandom(0x54554645);
  context.save();
  for (let index = 0; index < 7_500; index += 1) {
    const x = Math.floor(random() * portraitWidth);
    const y = Math.floor(random() * portraitHeight);
    const radius = 0.45 + random() * 1.8;
    const shade = Math.floor(45 + random() * 90);
    const alpha = 0.035 + random() * 0.12;
    context.fillStyle = `rgba(${shade}, ${shade}, ${shade}, ${alpha.toFixed(4)})`;
    context.beginPath();
    context.arc(x, y, radius, 0, Math.PI * 2);
    context.fill();
  }

  context.strokeStyle = "rgba(70, 70, 65, 0.09)";
  context.lineWidth = 18;
  context.beginPath();
  context.moveTo(1_265, 150);
  context.bezierCurveTo(1_220, 1_100, 1_350, 2_300, 1_290, 3_360);
  context.stroke();
  context.restore();
}

function rotateClockwise(source) {
  const output = createCanvas(source.height, source.width);
  const context = output.getContext("2d");
  context.translate(output.width, 0);
  context.rotate(Math.PI / 2);
  context.drawImage(source, 0, 0);
  return output;
}

async function requireWritableDirectory(directory) {
  await mkdir(directory, { recursive: true });
  const metadata = await lstat(directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("The OCR corpus output must be an ordinary directory.");
  }
}

async function writeOwnedFile(candidate, bytes) {
  try {
    const metadata = await lstat(candidate);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error(
        `Refusing to replace a non-ordinary OCR fixture: ${path.basename(candidate)}.`
      );
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") {
      throw error;
    }
  }
  await writeFile(candidate, bytes);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex").toUpperCase();
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const { manifest, outputDirectory } = await generateOcrCorpus(
    workspace,
    process.argv[2]
  );
  const totalBytes = manifest.fixtures.reduce(
    (sum, fixture) => sum + fixture.pngBytes + fixture.textBytes,
    0
  );
  process.stdout.write(
    `Generated ${manifest.fixtures.length} synthetic OCR fixtures (${totalBytes} bytes) in ${path.relative(workspace, outputDirectory)}.\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
