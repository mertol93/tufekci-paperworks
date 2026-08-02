import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync
} from "node:fs";
import { basename, join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";

function fail(message) {
  throw new Error(message);
}

function findApplicationBundles(root) {
  const bundles = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (!entry.isDirectory()) {
        continue;
      }
      const path = join(directory, entry.name);
      if (entry.name.endsWith(".app") && existsSync(join(path, "Info.plist"))) {
        bundles.push(path);
        continue;
      }
      visit(path);
    }
  };
  visit(root);
  return bundles.sort((left, right) => left.localeCompare(right, "en-GB"));
}

function plistAsJson(path) {
  const result = spawnSync("plutil", ["-convert", "json", "-o", "-", path], {
    encoding: "utf8"
  });
  if (result.error || result.status !== 0) {
    fail("The simulator Info.plist could not be read with plutil.");
  }
  return JSON.parse(result.stdout);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

const buildRoot = resolve(process.argv[2] ?? "src-tauri/gen/apple");
const evidenceDirectory = resolve(
  process.argv[3] ?? "artifacts/apple-mobile-simulator"
);

try {
  if (process.platform !== "darwin") {
    fail("Apple mobile bundle verification is available only on macOS.");
  }
  const bundles = findApplicationBundles(buildRoot);
  const applications = bundles
    .map((path) => ({ path, plist: plistAsJson(join(path, "Info.plist")) }))
    .filter(
      ({ plist }) =>
        plist.CFBundleIdentifier === "org.tufekci.paperworks" &&
        (plist.CFBundleSupportedPlatforms ?? []).includes("iPhoneSimulator")
    );
  if (applications.length !== 1) {
    fail(
      `Expected one Tüfekci Paperworks simulator application, found ${applications.length} among ${bundles.length} bundles.`
    );
  }
  const [{ path: applicationPath, plist }] = applications;
  const deviceFamilies = (plist.UIDeviceFamily ?? []).map(Number);
  if (!deviceFamilies.includes(1) || !deviceFamilies.includes(2)) {
    fail("The simulator application does not support both iPhone and iPad families.");
  }
  if (Number.parseFloat(plist.MinimumOSVersion ?? "0") < 16) {
    fail("The simulator application has an unexpected iOS deployment target.");
  }
  if (
    plist.LSSupportsOpeningDocumentsInPlace !== true ||
    plist.UIApplicationSupportsIndirectInputEvents !== true
  ) {
    fail("The reviewed iOS document or pointer settings are missing from the bundle.");
  }
  const executablePath = join(applicationPath, plist.CFBundleExecutable ?? "");
  if (!existsSync(executablePath) || statSync(executablePath).size === 0) {
    fail("The simulator application executable is missing or empty.");
  }

  mkdirSync(evidenceDirectory, { recursive: true });
  const archivePath = join(evidenceDirectory, "tufekci-paperworks-ios-simulator.zip");
  const archive = spawnSync(
    "ditto",
    ["-c", "-k", "--sequesterRsrc", "--keepParent", applicationPath, archivePath],
    { encoding: "utf8" }
  );
  if (archive.error || archive.status !== 0) {
    fail("The verified simulator application could not be archived.");
  }
  const report = {
    schemaVersion: 1,
    productName: plist.CFBundleDisplayName ?? plist.CFBundleName ?? basename(applicationPath, ".app"),
    bundleIdentifier: plist.CFBundleIdentifier,
    version: plist.CFBundleShortVersionString,
    buildVersion: plist.CFBundleVersion,
    minimumSystemVersion: plist.MinimumOSVersion,
    deviceFamilies,
    platform: "iPhoneSimulator",
    applicationPath: relative(buildRoot, applicationPath).replaceAll("\\", "/"),
    executableBytes: statSync(executablePath).size,
    executableSha256: sha256(executablePath),
    archiveBytes: statSync(archivePath).size,
    archiveSha256: sha256(archivePath)
  };
  writeFileSync(
    join(evidenceDirectory, "apple-mobile-simulator-report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
    "utf8"
  );
  process.stdout.write(
    `Verified ${report.productName} for iPhone and iPad simulator (${report.archiveBytes} bytes).\n`
  );
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
