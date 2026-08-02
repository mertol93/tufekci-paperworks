import { copyFileSync, chmodSync, mkdirSync, mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const tauriPlatform = (process.env.TAURI_ENV_PLATFORM ?? "").toLocaleLowerCase("en-GB");
const tauriTarget = (
  process.env.TAURI_ENV_TARGET_TRIPLE ??
  process.env.TARGET ??
  ""
).toLocaleLowerCase("en-GB");

if (
  process.platform !== "darwin" ||
  tauriPlatform === "ios" ||
  tauriTarget.includes("apple-ios")
) {
  process.exit(0);
}

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const projectDirectory = resolve(scriptDirectory, "..");
const sourcePath = join(
  projectDirectory,
  "src-tauri",
  "native",
  "macos-scanner",
  "main.m"
);
const outputDirectory = join(projectDirectory, "src-tauri", "binaries");
const baseName = "tufekci-paperworks-scanner";
const destinations = [
  join(outputDirectory, `${baseName}-aarch64-apple-darwin`),
  join(outputDirectory, `${baseName}-x86_64-apple-darwin`),
  join(outputDirectory, `${baseName}-universal-apple-darwin`),
  join(outputDirectory, baseName)
];

const inputModified = Math.max(
  statSync(sourcePath).mtimeMs,
  statSync(fileURLToPath(import.meta.url)).mtimeMs
);
if (
  destinations.every((destination) => {
    try {
      return statSync(destination).mtimeMs >= inputModified;
    } catch {
      return false;
    }
  })
) {
  process.exit(0);
}

mkdirSync(outputDirectory, { recursive: true });
const temporaryDirectory = mkdtempSync(join(tmpdir(), "paperworks-macos-scanner-"));

function runXcrun(arguments_, label) {
  const result = spawnSync("xcrun", arguments_, { stdio: "inherit" });
  if (result.error) {
    throw new Error(`${label} could not start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`${label} failed with exit code ${result.status ?? "unknown"}.`);
  }
}

function compileArchitecture(architecture, outputPath) {
  runXcrun(
    [
      "--sdk",
      "macosx",
      "clang",
      "-fobjc-arc",
      "-fmodules",
      "-Wall",
      "-Wextra",
      "-Werror",
      "-Os",
      "-mmacosx-version-min=12.0",
      "-arch",
      architecture,
      sourcePath,
      "-framework",
      "Foundation",
      "-framework",
      "ImageCaptureCore",
      "-o",
      outputPath
    ],
    `Image Capture bridge compilation for ${architecture}`
  );
}

try {
  const armBinary = join(temporaryDirectory, `${baseName}-arm64`);
  const intelBinary = join(temporaryDirectory, `${baseName}-x86_64`);
  const universalBinary = join(temporaryDirectory, `${baseName}-universal`);

  compileArchitecture("arm64", armBinary);
  compileArchitecture("x86_64", intelBinary);
  runXcrun(
    ["lipo", "-create", armBinary, intelBinary, "-output", universalBinary],
    "Universal Image Capture bridge creation"
  );

  copyFileSync(armBinary, destinations[0]);
  copyFileSync(intelBinary, destinations[1]);
  copyFileSync(universalBinary, destinations[2]);
  copyFileSync(universalBinary, destinations[3]);
  for (const destination of destinations) {
    chmodSync(destination, 0o755);
  }
  process.stdout.write("Built the universal macOS Image Capture scanner bridge.\n");
} finally {
  rmSync(temporaryDirectory, { force: true, recursive: true });
}
