import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  aggregatePackageEvidence,
  validatePackageEvidenceSummary
} from "../scripts/check-package-evidence.mjs";
import {
  classifyBundleFilename,
  parseElfHeader,
  parseMachOArchitectures,
  parsePortableExecutableHeader,
  normaliseMacSigningEvidence,
  normaliseWindowsSigningEvidence,
  validateBundleInventory,
  validatePackageEvidenceReport,
  versionEquivalent
} from "../scripts/check-release-bundles.mjs";

const version = "0.1.0-alpha.1";
const digest = "A".repeat(64);

test("parses bounded PE, ELF and universal Mach-O architecture headers", () => {
  const pe = Buffer.alloc(512);
  pe.write("MZ", 0, "ascii");
  pe.writeUInt32LE(0x80, 0x3c);
  pe.set([0x50, 0x45, 0, 0], 0x80);
  pe.writeUInt16LE(0x8664, 0x84);
  pe.writeUInt16LE(0x20b, 0x98);
  assert.deepEqual(parsePortableExecutableHeader(pe), { architecture: "x64", optionalMagic: 0x20b });

  const elf = Buffer.alloc(64);
  elf.set([0x7f, 0x45, 0x4c, 0x46, 2, 1], 0);
  elf.writeUInt16LE(62, 18);
  assert.deepEqual(parseElfHeader(elf), { architecture: "x64", bits: 64 });

  const macho = Buffer.alloc(48);
  macho.writeUInt32BE(0xcafebabe, 0);
  macho.writeUInt32BE(2, 4);
  macho.writeUInt32BE(0x01000007, 8);
  macho.writeUInt32BE(0x0100000c, 28);
  assert.deepEqual(parseMachOArchitectures(macho), ["arm64", "x64"]);

  assert.throws(() => parsePortableExecutableHeader(Buffer.alloc(512)), /DOS executable/u);
  assert.throws(() => parseElfHeader(Buffer.alloc(64)), /valid ELF/u);
  assert.throws(() => parseMachOArchitectures(Buffer.alloc(48)), /recognised Mach-O/u);
});

test("classifies package formats and rejects stale or incomplete inventories", () => {
  assert.equal(classifyBundleFilename("Paperworks_0.1.0_amd64.AppImage"), "appimage");
  assert.equal(classifyBundleFilename("Paperworks_0.1.0_x64-setup.exe"), "nsis");
  assert.equal(classifyBundleFilename("notes.txt"), null);

  const linux = [
    { fileName: `Tufekci Paperworks_${version}_amd64.AppImage`, format: "appimage" },
    { fileName: `Tufekci Paperworks_${version}_amd64.deb`, format: "deb" },
    { fileName: `Tufekci Paperworks-${version}-1.x86_64.rpm`, format: "rpm" }
  ];
  assert.deepEqual(
    validateBundleInventory(linux, "linux", "x64", version).map((entry) => entry.format),
    ["appimage", "deb", "rpm"]
  );
  assert.throws(
    () => validateBundleInventory(linux.slice(0, 2), "linux", "x64", version),
    /exactly 3/u
  );
  assert.throws(
    () => validateBundleInventory(linux.map((entry) => ({ ...entry, fileName: entry.fileName.replace(version, "0.0.1") })), "linux", "x64", version),
    /stale/u
  );
});

test("matches package-manager forms of one release version without accepting another release", () => {
  assert.equal(versionEquivalent(version, version), true);
  assert.equal(versionEquivalent(`${version}-1`, version), true);
  assert.equal(versionEquivalent("0.1.0_alpha.1-1", version), true);
  assert.equal(versionEquivalent("1:0.1.0-alpha.1-1", version), true);
  assert.equal(versionEquivalent("0.1.0-alpha.2-1", version), false);
});

test("accepts strict path-free package evidence and enforces signed-release policy", () => {
  const report = windowsReport();
  assert.equal(validatePackageEvidenceReport(report), report);

  const leaked = windowsReport();
  leaked.packages[0].localPath = "C:\\private\\package.msi";
  assert.throws(() => validatePackageEvidenceReport(leaked), /unknown fields/u);

  const signedRequired = windowsReport();
  signedRequired.signaturePolicy = "signed-required";
  signedRequired.expectedSignerIdentity = "B".repeat(40);
  assert.throws(() => validatePackageEvidenceReport(signedRequired), /signed release policy/u);
  signedRequired.packages.forEach((entry) => {
    entry.signatureStatus = "valid";
    entry.signerIdentity = "B".repeat(40);
    entry.timestampStatus = "valid";
  });
  assert.equal(validatePackageEvidenceReport(signedRequired), signedRequired);
});

test("binds Windows Authenticode evidence to the configured timestamped signer", () => {
  const signer = "C".repeat(40);
  const evidence = normaliseWindowsSigningEvidence(
    {
      signatureStatus: "Valid",
      signerThumbprint: signer.toLowerCase(),
      timeStamperThumbprint: "D".repeat(40)
    },
    "signed-required",
    signer
  );
  assert.deepEqual(evidence, {
    signatureStatus: "valid",
    signerIdentity: signer,
    timestampStatus: "valid",
    notarisationStatus: "not-applicable"
  });
  assert.throws(
    () =>
      normaliseWindowsSigningEvidence(
        { signatureStatus: "Valid", signerThumbprint: signer, timeStamperThumbprint: null },
        "signed-required",
        signer
      ),
    /expected timestamped signer/u
  );
});

test("requires Developer ID, timestamp, Gatekeeper, and stapled-ticket evidence on macOS", () => {
  const observation = {
    verification: { status: 0, output: "valid on disk" },
    metadata: {
      status: 0,
      output: [
        "Authority=Developer ID Application: Tufekci Paperworks (A1B2C3D4E5)",
        "TeamIdentifier=A1B2C3D4E5",
        "Timestamp=27 Jul 2026 at 12:30:00"
      ].join("\n")
    },
    gatekeeper: { status: 0, output: "source=Notarized Developer ID" },
    stapler: { status: 0, output: "The validate action worked!" }
  };
  assert.deepEqual(
    normaliseMacSigningEvidence(observation, "signed-required", "A1B2C3D4E5"),
    {
      signatureStatus: "valid",
      signerIdentity: "A1B2C3D4E5",
      timestampStatus: "valid",
      notarisationStatus: "valid"
    }
  );
  assert.throws(
    () =>
      normaliseMacSigningEvidence(
        { ...observation, stapler: { status: 1, output: "ticket missing" } },
        "signed-required",
        "A1B2C3D4E5"
      ),
    /signed and notarised identity/u
  );
});

test("requires byte-identical executable payloads across AppImage, deb and rpm", () => {
  const report = linuxReport();
  assert.equal(validatePackageEvidenceReport(report), report);
  report.packages[2].payloadSha256 = "B".repeat(64);
  assert.throws(() => validatePackageEvidenceReport(report), /not byte-for-byte consistent/u);
});

test("keeps package identity ASCII while preserving the branded Linux desktop name", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [linuxConfig, desktopTemplate, sourceChecker] = await Promise.all([
    readFile(`${workspace}src-tauri/tauri.linux.conf.json`, "utf8").then(JSON.parse),
    readFile(`${workspace}src-tauri/linux/tufekci-paperworks.desktop`, "utf8"),
    readFile(`${workspace}scripts/check-source-tree.mjs`, "utf8")
  ]);
  assert.equal(linuxConfig.productName, "Tufekci Paperworks");
  assert.equal(linuxConfig.bundle.linux.deb.desktopTemplate, "linux/tufekci-paperworks.desktop");
  assert.equal(linuxConfig.bundle.linux.rpm.desktopTemplate, "linux/tufekci-paperworks.desktop");
  assert.match(desktopTemplate, /^Name=Tüfekci Paperworks$/mu);
  assert.match(desktopTemplate, /^Exec=\{\{exec\}\}$/mu);
  assert.match(sourceChecker, /"\.desktop"/u);
});

test("accepts only one complete cross-platform package evidence summary", () => {
  const summary = {
    schemaVersion: 2,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    signaturePolicy: "unsigned-allowed",
    packageReports: [
      reportSummary("linux", "x64", ["appimage", "deb", "rpm"]),
      reportSummary("macos", "universal", ["dmg"]),
      reportSummary("windows", "x64", ["msi", "nsis"])
    ],
    linuxInstallEvidence: {
      reportSha256: digest,
      caseCount: 3,
      distributions: ["debian", "fedora", "ubuntu"]
    }
  };
  assert.equal(validatePackageEvidenceSummary(summary), summary);
  summary.packageReports[0].packages[0].fileName = summary.packageReports[2].packages[0].fileName;
  assert.throws(() => validatePackageEvidenceSummary(summary), /duplicated/u);
});

test("retains the expected publisher identity in signed package summaries", () => {
  const windows = reportSummary("windows", "x64", ["msi", "nsis"]);
  windows.expectedSignerIdentity = "B".repeat(40);
  windows.packages.forEach((entry) => {
    entry.signatureStatus = "valid";
    entry.signerIdentity = windows.expectedSignerIdentity;
    entry.timestampStatus = "valid";
  });
  const macos = reportSummary("macos", "universal", ["dmg"]);
  macos.expectedSignerIdentity = "A1B2C3D4E5";
  macos.packages.forEach((entry) => {
    entry.signatureStatus = "valid";
    entry.signerIdentity = macos.expectedSignerIdentity;
    entry.timestampStatus = "valid";
    entry.notarisationStatus = "valid";
  });
  const summary = {
    schemaVersion: 2,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    signaturePolicy: "signed-required",
    packageReports: [reportSummary("linux", "x64", ["appimage", "deb", "rpm"]), macos, windows],
    linuxInstallEvidence: {
      reportSha256: digest,
      caseCount: 3,
      distributions: ["debian", "fedora", "ubuntu"]
    }
  };
  assert.equal(validatePackageEvidenceSummary(summary), summary);
  windows.packages[0].signerIdentity = "C".repeat(40);
  assert.throws(() => validatePackageEvidenceSummary(summary), /signed release policy/u);
});

test("aggregates only the exact release package evidence set", async (context) => {
  const root = await mkdtemp(path.join(tmpdir(), "paperworks-package-evidence-test-"));
  context.after(() => rm(root, { force: true, recursive: true }));
  const workspace = path.join(root, "workspace");
  const packageEvidence = path.join(root, "package-evidence");
  const linuxInstallEvidence = path.join(root, "linux-install-evidence");
  const output = path.join(root, "output");
  await Promise.all([
    mkdir(workspace, { recursive: true }),
    mkdir(packageEvidence, { recursive: true }),
    mkdir(linuxInstallEvidence, { recursive: true })
  ]);
  await writeFile(path.join(workspace, "package.json"), `${JSON.stringify({ version })}\n`, "utf8");
  await Promise.all([
    writeJson(path.join(packageEvidence, "package-report-linux-x64.json"), linuxReport()),
    writeJson(path.join(packageEvidence, "package-report-macos-universal.json"), macosReport()),
    writeJson(path.join(packageEvidence, "package-report-windows-x64.json"), windowsReport()),
    writeJson(path.join(linuxInstallEvidence, "linux-install-report-x64.json"), linuxInstallReport())
  ]);

  const summary = await aggregatePackageEvidence(
    workspace,
    packageEvidence,
    linuxInstallEvidence,
    output
  );
  assert.equal(summary.releaseVersion, version);
  assert.deepEqual(summary.packageReports.map((entry) => entry.platform), ["linux", "macos", "windows"]);
  assert.deepEqual(summary.linuxInstallEvidence.distributions, ["debian", "fedora", "ubuntu"]);
  assert.deepEqual(
    JSON.parse(await readFile(path.join(output, "package-evidence-summary.json"), "utf8")),
    summary
  );

  await writeFile(path.join(packageEvidence, "unexpected.json"), "{}\n", "utf8");
  await assert.rejects(
    aggregatePackageEvidence(workspace, packageEvidence, linuxInstallEvidence, output),
    /unexpected files/u
  );
});

test("gates tagged metadata on native package reports and their aggregate", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [packageJson, verifier, workflow] = await Promise.all([
    readFile(`${workspace}package.json`, "utf8").then(JSON.parse),
    readFile(`${workspace}scripts/check-release-bundles.mjs`, "utf8"),
    readFile(`${workspace}.github/workflows/release.yml`, "utf8")
  ]);
  assert.match(packageJson.scripts["release:verify-bundles"], /check-release-bundles/u);
  assert.match(packageJson.scripts["release:package-evidence"], /check-package-evidence/u);
  assert.match(workflow, /Verify release package structure and identity[\s\S]+release:verify-bundles/u);
  assert.match(workflow, /package-report-\*\.json/u);
  assert.match(workflow, /Verify the complete package evidence set[\s\S]+release:package-evidence/u);
  assert.match(workflow, /ubuntu-22\.04[\s\S]+packagePlatform: "linux"/u);
  assert.match(verifier, /\["stapler", "validate", appPath\]/u);
});

function windowsReport() {
  return {
    schemaVersion: 2,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    identifier: "org.tufekci.paperworks",
    platform: "windows",
    architecture: "x64",
    signaturePolicy: "unsigned-allowed",
    expectedSignerIdentity: null,
    packageCount: 2,
    payloadConsistency: "not-applicable",
    packages: [
      packageEntry("msi", `Tüfekci Paperworks_${version}_x64_en-US.msi`, "msi-compound-file", "0.1.0.1", null, "unsigned"),
      packageEntry("nsis", `Tüfekci Paperworks_${version}_x64-setup.exe`, "nsis-portable-executable", version, null, "unsigned")
    ]
  };
}

function linuxReport() {
  return {
    schemaVersion: 2,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    identifier: "org.tufekci.paperworks",
    platform: "linux",
    architecture: "x64",
    signaturePolicy: "unsigned-allowed",
    expectedSignerIdentity: null,
    packageCount: 3,
    payloadConsistency: "verified",
    packages: [
      packageEntry("appimage", `Tufekci Paperworks_${version}_amd64.AppImage`, "appimage-elf-squashfs", version, digest, "not-applicable"),
      packageEntry("deb", `Tufekci Paperworks_${version}_amd64.deb`, "debian-ar-package", version, digest, "not-applicable"),
      packageEntry("rpm", `Tufekci Paperworks-${version}-1.x86_64.rpm`, "rpm-package", `${version}-1`, digest, "not-applicable")
    ]
  };
}

function macosReport() {
  return {
    schemaVersion: 2,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    identifier: "org.tufekci.paperworks",
    platform: "macos",
    architecture: "universal",
    signaturePolicy: "unsigned-allowed",
    expectedSignerIdentity: null,
    packageCount: 1,
    payloadConsistency: "not-applicable",
    packages: [{
      ...packageEntry(
        "dmg",
        `Tüfekci Paperworks_${version}_universal.dmg`,
        "apple-udif-disk-image",
        version,
        digest,
        "ad-hoc"
      ),
      architecture: "universal"
    }]
  };
}

function linuxInstallReport() {
  return {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    platform: "linux",
    architecture: "x64",
    cases: [
      linuxInstallCase("ubuntu:22.04", "ubuntu", "22.04", "appimage", "extracted-on-baseline"),
      linuxInstallCase("debian:13-slim", "debian", "13", "deb", "installed-and-linked"),
      linuxInstallCase("fedora:43", "fedora", "43", "rpm", "installed-and-linked")
    ]
  };
}

function linuxInstallCase(containerImage, distribution, distributionVersion, format, verification) {
  return {
    architecture: "x64",
    containerImage,
    containerImageId: digest,
    distribution,
    distributionVersion,
    format,
    packageVersion: version,
    verification
  };
}

async function writeJson(filePath, value) {
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function packageEntry(format, fileName, container, metadataVersion, payloadSha256, signatureStatus) {
  const unsignedMac = signatureStatus === "ad-hoc";
  const inapplicable = signatureStatus === "not-applicable";
  return {
    format,
    fileName,
    bytes: 128 * 1024,
    sha256: digest,
    productName: "Tüfekci Paperworks",
    architecture: "x64",
    container,
    metadataVersion,
    payloadSha256,
    signatureStatus,
    signerIdentity: null,
    timestampStatus: inapplicable ? "not-applicable" : "missing",
    notarisationStatus: inapplicable
      ? "not-applicable"
      : unsignedMac
        ? "unverified"
        : "not-applicable"
  };
}

function reportSummary(platform, architecture, formats) {
  return {
    platform,
    architecture,
    expectedSignerIdentity: null,
    reportSha256: digest,
    packageCount: formats.length,
    packages: formats.map((format) => {
      const linux = platform === "linux";
      return {
        fileName: `${platform}-${format}-${version}.${format === "nsis" ? "exe" : format}`,
        format,
        sha256: digest,
        signatureStatus: linux ? "not-applicable" : "unsigned",
        signerIdentity: null,
        timestampStatus: linux ? "not-applicable" : "missing",
        notarisationStatus: platform === "macos" ? "unverified" : "not-applicable"
      };
    })
  };
}
