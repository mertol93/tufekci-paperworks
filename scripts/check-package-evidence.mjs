import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { validatePackageEvidenceReport } from "./check-release-bundles.mjs";
import { validateLinuxInstallReport } from "./run-linux-package-install-gate.mjs";

const expectedPackageReports = Object.freeze([
  Object.freeze({ architecture: "x64", fileName: "package-report-linux-x64.json", platform: "linux" }),
  Object.freeze({ architecture: "universal", fileName: "package-report-macos-universal.json", platform: "macos" }),
  Object.freeze({ architecture: "x64", fileName: "package-report-windows-x64.json", platform: "windows" })
]);
const installReportName = "linux-install-report-x64.json";
const maximumReportBytes = 1024 * 1024;

export function validatePackageEvidenceSummary(value) {
  requireExactFields(
    value,
    [
      "linuxInstallEvidence",
      "packageReports",
      "product",
      "releaseVersion",
      "schemaVersion",
      "signaturePolicy"
    ],
    "package evidence summary"
  );
  if (
    value.schemaVersion !== 2 ||
    value.product !== "Tüfekci Paperworks" ||
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(value.releaseVersion) ||
    !["unsigned-allowed", "signed-required"].includes(value.signaturePolicy) ||
    !Array.isArray(value.packageReports) ||
    value.packageReports.length !== expectedPackageReports.length
  ) {
    throw new Error("The package evidence summary identity is invalid.");
  }
  const seenPlatforms = new Set();
  const seenPackageNames = new Set();
  for (const report of value.packageReports) {
    requireExactFields(
      report,
      [
        "architecture",
        "expectedSignerIdentity",
        "packageCount",
        "packages",
        "platform",
        "reportSha256"
      ],
      "package report summary"
    );
    const expected = expectedPackageReports.find((candidate) => candidate.platform === report.platform);
    if (
      !expected ||
      seenPlatforms.has(report.platform) ||
      report.architecture !== expected.architecture ||
      !Number.isSafeInteger(report.packageCount) ||
      report.packageCount < 1 ||
      report.packageCount > 3 ||
      !/^[0-9A-F]{64}$/u.test(report.reportSha256) ||
      !Array.isArray(report.packages) ||
      report.packages.length !== report.packageCount
    ) {
      throw new Error(`The package report summary is invalid for ${report?.platform ?? "unknown"}.`);
    }
    if (
      (value.signaturePolicy === "unsigned-allowed" && report.expectedSignerIdentity !== null) ||
      (report.platform === "linux" && report.expectedSignerIdentity !== null) ||
      (value.signaturePolicy === "signed-required" &&
        report.platform === "windows" &&
        !/^[0-9A-F]{40}$/u.test(report.expectedSignerIdentity ?? "")) ||
      (value.signaturePolicy === "signed-required" &&
        report.platform === "macos" &&
        !/^[A-Z0-9]{10}$/u.test(report.expectedSignerIdentity ?? ""))
    ) {
      throw new Error(`The expected publisher identity is invalid for ${report.platform}.`);
    }
    for (const entry of report.packages) {
      requireExactFields(
        entry,
        [
          "fileName",
          "format",
          "notarisationStatus",
          "sha256",
          "signatureStatus",
          "signerIdentity",
          "timestampStatus"
        ],
        "summarised release package"
      );
      const nameKey = entry.fileName.normalize("NFC").toLocaleLowerCase("en-GB");
      if (
        seenPackageNames.has(nameKey) ||
        entry.fileName !== path.basename(entry.fileName) ||
        entry.fileName.length > 240 ||
        /[\0\r\n/\\]/u.test(entry.fileName) ||
        !/^[a-z]+$/u.test(entry.format) ||
        !/^[0-9A-F]{64}$/u.test(entry.sha256) ||
        !["ad-hoc", "not-applicable", "unsigned", "valid"].includes(entry.signatureStatus) ||
        !(entry.signerIdentity === null || typeof entry.signerIdentity === "string") ||
        !["missing", "not-applicable", "valid"].includes(entry.timestampStatus) ||
        !["not-applicable", "unverified", "valid"].includes(entry.notarisationStatus)
      ) {
        throw new Error("A summarised release package is invalid or duplicated.");
      }
      if (
        report.platform === "linux" &&
        (entry.signatureStatus !== "not-applicable" ||
          entry.signerIdentity !== null ||
          entry.timestampStatus !== "not-applicable" ||
          entry.notarisationStatus !== "not-applicable")
      ) {
        throw new Error("A Linux package summary contains inapplicable signing evidence.");
      }
      if (
        value.signaturePolicy === "signed-required" &&
        report.platform === "windows" &&
        (entry.signatureStatus !== "valid" ||
          entry.signerIdentity !== report.expectedSignerIdentity ||
          entry.timestampStatus !== "valid" ||
          entry.notarisationStatus !== "not-applicable")
      ) {
        throw new Error("A Windows package summary does not satisfy the signed release policy.");
      }
      if (
        value.signaturePolicy === "signed-required" &&
        report.platform === "macos" &&
        (entry.signatureStatus !== "valid" ||
          entry.signerIdentity !== report.expectedSignerIdentity ||
          entry.timestampStatus !== "valid" ||
          entry.notarisationStatus !== "valid")
      ) {
        throw new Error("A macOS package summary does not satisfy the signed and notarised release policy.");
      }
      seenPackageNames.add(nameKey);
    }
    seenPlatforms.add(report.platform);
  }
  requireExactFields(
    value.linuxInstallEvidence,
    ["caseCount", "distributions", "reportSha256"],
    "Linux install evidence summary"
  );
  if (
    value.linuxInstallEvidence.caseCount !== 3 ||
    !/^[0-9A-F]{64}$/u.test(value.linuxInstallEvidence.reportSha256) ||
    !Array.isArray(value.linuxInstallEvidence.distributions) ||
    value.linuxInstallEvidence.distributions.join(",") !== "debian,fedora,ubuntu"
  ) {
    throw new Error("The Linux install evidence summary is incomplete.");
  }
  return value;
}

export async function aggregatePackageEvidence(
  workspace,
  packageEvidenceDirectory,
  linuxInstallDirectory,
  outputDirectory
) {
  const packageJson = JSON.parse(await readFile(path.join(workspace, "package.json"), "utf8"));
  await requireExactDirectoryFiles(
    packageEvidenceDirectory,
    expectedPackageReports.map((entry) => entry.fileName)
  );
  await requireExactDirectoryFiles(linuxInstallDirectory, [installReportName]);

  const packageReports = [];
  let signaturePolicy = null;
  for (const expected of expectedPackageReports) {
    const filePath = path.join(packageEvidenceDirectory, expected.fileName);
    const { bytes, value } = await readBoundedJson(filePath);
    validatePackageEvidenceReport(value);
    if (
      value.platform !== expected.platform ||
      value.architecture !== expected.architecture ||
      value.releaseVersion !== packageJson.version
    ) {
      throw new Error(`Package evidence identity does not match the release for ${expected.platform}.`);
    }
    signaturePolicy ??= value.signaturePolicy;
    if (value.signaturePolicy !== signaturePolicy) {
      throw new Error("Platform package reports use inconsistent signature policies.");
    }
    packageReports.push({
      platform: value.platform,
      architecture: value.architecture,
      expectedSignerIdentity: value.expectedSignerIdentity,
      reportSha256: sha256(bytes),
      packageCount: value.packageCount,
      packages: value.packages.map((entry) => ({
        fileName: entry.fileName,
        format: entry.format,
        sha256: entry.sha256,
        signatureStatus: entry.signatureStatus,
        signerIdentity: entry.signerIdentity,
        timestampStatus: entry.timestampStatus,
        notarisationStatus: entry.notarisationStatus
      }))
    });
  }

  const installPath = path.join(linuxInstallDirectory, installReportName);
  const install = await readBoundedJson(installPath);
  validateLinuxInstallReport(install.value);
  if (install.value.releaseVersion !== packageJson.version) {
    throw new Error("Linux package-install evidence does not match the release version.");
  }
  const summary = {
    schemaVersion: 2,
    product: "Tüfekci Paperworks",
    releaseVersion: packageJson.version,
    signaturePolicy,
    packageReports,
    linuxInstallEvidence: {
      reportSha256: sha256(install.bytes),
      caseCount: install.value.cases.length,
      distributions: install.value.cases.map((entry) => entry.distribution).sort()
    }
  };
  validatePackageEvidenceSummary(summary);
  await mkdir(outputDirectory, { recursive: true });
  const outputPath = path.join(outputDirectory, "package-evidence-summary.json");
  await writeFile(outputPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  validatePackageEvidenceSummary(JSON.parse(await readFile(outputPath, "utf8")));
  return summary;
}

async function requireExactDirectoryFiles(directory, expectedNames) {
  const metadata = await lstat(directory);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("A package evidence input must be an ordinary directory.");
  }
  const entries = await readdir(directory, { withFileTypes: true });
  const observed = [];
  for (const entry of entries) {
    const filePath = path.join(directory, entry.name);
    const fileMetadata = await lstat(filePath);
    if (!entry.isFile() || !fileMetadata.isFile() || fileMetadata.isSymbolicLink()) {
      throw new Error("Package evidence inputs must contain only ordinary files.");
    }
    observed.push(entry.name);
  }
  observed.sort((left, right) => left.localeCompare(right, "en-GB"));
  const expected = [...expectedNames].sort((left, right) => left.localeCompare(right, "en-GB"));
  if (observed.length !== expected.length || observed.some((name, index) => name !== expected[index])) {
    throw new Error("The package evidence input set is incomplete or contains unexpected files.");
  }
}

async function readBoundedJson(filePath) {
  const metadata = await lstat(filePath);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 2 || metadata.size > maximumReportBytes) {
    throw new Error("A package evidence report is unsafe or outside its size bounds.");
  }
  const bytes = await readFile(filePath);
  try {
    return { bytes, value: JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)) };
  } catch {
    throw new Error("A package evidence report is not strict UTF-8 JSON.");
  }
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex").toUpperCase();
}

function requireExactFields(value, fields, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...fields].sort();
  if (actual.length !== expected.length || actual.some((field, index) => field !== expected[index])) {
    throw new Error(`${label} contains missing or unknown fields.`);
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const packageEvidenceDirectory = path.resolve(workspace, process.argv[2] ?? "release-package-evidence");
  const linuxInstallDirectory = path.resolve(workspace, process.argv[3] ?? "release-linux-install-evidence");
  const outputDirectory = path.resolve(workspace, process.argv[4] ?? "release-package-summary");
  aggregatePackageEvidence(workspace, packageEvidenceDirectory, linuxInstallDirectory, outputDirectory)
    .then((summary) => {
      process.stdout.write(
        `Verified ${summary.packageReports.length} platform package reports and ${summary.linuxInstallEvidence.caseCount} Linux distribution checks.\n`
      );
    })
    .catch((error) => {
      process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
      process.exitCode = 1;
    });
}
