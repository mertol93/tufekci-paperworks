import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { createReadStream } from "node:fs";
import {
  lstat,
  mkdtemp,
  mkdir,
  open,
  readFile,
  readdir,
  rm,
  stat,
  writeFile
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { validatePlatformSigningOverlay } from "./generate-platform-signing-config.mjs";

const productName = "Tüfekci Paperworks";
const packageName = "tufekci-paperworks";
const maximumEntries = 1_024;
const minimumPackageBytes = 64 * 1024;
const maximumPackageBytes = 2 * 1024 * 1024 * 1024;
const maximumCommandBytes = 4 * 1024 * 1024;
const packageFormats = Object.freeze({
  linux: Object.freeze(["appimage", "deb", "rpm"]),
  macos: Object.freeze(["dmg"]),
  windows: Object.freeze(["msi", "nsis"])
});
const architectureNames = Object.freeze({
  0x014c: "x86",
  0x8664: "x64",
  0xaa64: "arm64"
});
const packageContainers = Object.freeze({
  appimage: "appimage-elf-squashfs",
  deb: "debian-ar-package",
  dmg: "apple-udif-disk-image",
  msi: "msi-compound-file",
  nsis: "nsis-portable-executable",
  rpm: "rpm-package"
});
const elfArchitectureNames = Object.freeze({
  3: "x86",
  62: "x64",
  183: "arm64"
});

export function classifyBundleFilename(fileName) {
  const lower = fileName.normalize("NFC").toLocaleLowerCase("en-GB");
  if (lower.endsWith(".msi")) return "msi";
  if (lower.endsWith("-setup.exe")) return "nsis";
  if (lower.endsWith(".appimage")) return "appimage";
  if (lower.endsWith(".deb")) return "deb";
  if (lower.endsWith(".rpm")) return "rpm";
  if (lower.endsWith(".dmg")) return "dmg";
  return null;
}

export function parsePortableExecutableHeader(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 256 || bytes.subarray(0, 2).toString("ascii") !== "MZ") {
    throw new Error("The Windows installer does not contain a valid DOS executable header.");
  }
  const peOffset = bytes.readUInt32LE(0x3c);
  if (
    peOffset < 0x40 ||
    peOffset + 26 > bytes.length ||
    !bytes.subarray(peOffset, peOffset + 4).equals(Buffer.from([0x50, 0x45, 0, 0]))
  ) {
    throw new Error("The Windows installer does not contain a bounded PE header.");
  }
  const machine = bytes.readUInt16LE(peOffset + 4);
  const optionalMagic = bytes.readUInt16LE(peOffset + 24);
  const architecture = architectureNames[machine];
  if (!architecture || ![0x10b, 0x20b].includes(optionalMagic)) {
    throw new Error("The Windows installer uses an unsupported PE architecture or format.");
  }
  return { architecture, optionalMagic };
}

export function parseElfHeader(bytes) {
  if (
    !Buffer.isBuffer(bytes) ||
    bytes.length < 64 ||
    !bytes.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))
  ) {
    throw new Error("The Linux payload does not contain a valid ELF header.");
  }
  const elfClass = bytes[4];
  const byteOrder = bytes[5];
  if (![1, 2].includes(elfClass) || ![1, 2].includes(byteOrder)) {
    throw new Error("The Linux payload uses an unsupported ELF class or byte order.");
  }
  const machine = byteOrder === 1 ? bytes.readUInt16LE(18) : bytes.readUInt16BE(18);
  const architecture = elfArchitectureNames[machine];
  if (!architecture) {
    throw new Error("The Linux payload uses an unsupported ELF architecture.");
  }
  return { architecture, bits: elfClass === 2 ? 64 : 32 };
}

export function parseMachOArchitectures(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 32) {
    throw new Error("The macOS payload does not contain a bounded Mach-O header.");
  }
  const magic = bytes.readUInt32BE(0);
  if ([0xcafebabe, 0xcafebabf].includes(magic)) {
    const count = bytes.readUInt32BE(4);
    const stride = magic === 0xcafebabf ? 32 : 20;
    if (count < 1 || count > 16 || 8 + count * stride > bytes.length) {
      throw new Error("The universal macOS payload has an invalid architecture table.");
    }
    const values = [];
    for (let index = 0; index < count; index += 1) {
      values.push(machArchitecture(bytes.readUInt32BE(8 + index * stride)));
    }
    return [...new Set(values)].sort();
  }

  const littleEndian = magic === 0xcffaedfe || magic === 0xcefaedfe;
  const bigEndian = magic === 0xfeedfacf || magic === 0xfeedface;
  if (!littleEndian && !bigEndian) {
    throw new Error("The macOS payload does not contain a recognised Mach-O header.");
  }
  const cpuType = littleEndian ? bytes.readUInt32LE(4) : bytes.readUInt32BE(4);
  return [machArchitecture(cpuType)];
}

export function versionEquivalent(actual, expected) {
  if (typeof actual !== "string" || typeof expected !== "string") return false;
  const withoutEpoch = actual.trim().replace(/^\d+:/u, "");
  const withoutPackageRelease = withoutEpoch.replace(/-\d+(?:\.[A-Za-z0-9]+)*$/u, "");
  const normalise = (value) =>
    value
      .normalize("NFC")
      .toLocaleLowerCase("en-GB")
      .replace(/[^a-z0-9]+/gu, ".")
      .replace(/^\.+|\.+$/gu, "");
  return normalise(withoutEpoch) === normalise(expected) || normalise(withoutPackageRelease) === normalise(expected);
}

export function validateBundleInventory(candidates, platform, architecture, releaseVersion) {
  const expectedFormats = packageFormats[platform];
  if (!expectedFormats || !["x64", "arm64", "universal"].includes(architecture)) {
    throw new Error("The package platform or architecture is unsupported.");
  }
  if (!Array.isArray(candidates) || candidates.length !== expectedFormats.length) {
    throw new Error(`The ${platform} bundle set must contain exactly ${expectedFormats.length} release packages.`);
  }
  const seenFormats = new Set();
  const seenNames = new Set();
  for (const candidate of candidates) {
    if (!candidate || !expectedFormats.includes(candidate.format) || seenFormats.has(candidate.format)) {
      throw new Error(`The ${platform} bundle set contains a missing, duplicate or unexpected package format.`);
    }
    const key = candidate.fileName.normalize("NFC").toLocaleLowerCase("en-GB");
    if (
      seenNames.has(key) ||
      candidate.fileName !== path.basename(candidate.fileName) ||
      candidate.fileName.length > 240 ||
      /[\0\r\n]/u.test(candidate.fileName) ||
      !candidate.fileName.normalize("NFC").includes(releaseVersion)
    ) {
      throw new Error("A release package filename is unsafe, duplicated, stale or does not include the release version.");
    }
    if (!filenameMatchesArchitecture(candidate.fileName, candidate.format, architecture)) {
      throw new Error(`The ${candidate.format} package filename does not match the required ${architecture} architecture.`);
    }
    seenFormats.add(candidate.format);
    seenNames.add(key);
  }
  return [...candidates].sort(
    (left, right) => expectedFormats.indexOf(left.format) - expectedFormats.indexOf(right.format)
  );
}

export function validatePackageEvidenceReport(value) {
  requireExactFields(
    value,
    [
      "architecture",
      "expectedSignerIdentity",
      "identifier",
      "packageCount",
      "packages",
      "payloadConsistency",
      "platform",
      "product",
      "releaseVersion",
      "schemaVersion",
      "signaturePolicy"
    ],
    "package evidence report"
  );
  if (
    value.schemaVersion !== 2 ||
    value.product !== productName ||
    value.identifier !== "org.tufekci.paperworks" ||
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(value.releaseVersion) ||
    !packageFormats[value.platform] ||
    !["x64", "arm64", "universal"].includes(value.architecture) ||
    !["unsigned-allowed", "signed-required"].includes(value.signaturePolicy) ||
    !["not-applicable", "verified"].includes(value.payloadConsistency)
  ) {
    throw new Error("The package evidence report identity is invalid.");
  }
  const expectedFormats = packageFormats[value.platform];
  if (
    value.packageCount !== expectedFormats.length ||
    !Array.isArray(value.packages) ||
    value.packages.length !== expectedFormats.length
  ) {
    throw new Error("The package evidence report has an incomplete package set.");
  }
  const seen = new Set();
  for (const entry of value.packages) {
    requireExactFields(
      entry,
      [
        "architecture",
        "bytes",
        "container",
        "fileName",
        "format",
        "metadataVersion",
        "notarisationStatus",
        "payloadSha256",
        "productName",
        "sha256",
        "signatureStatus",
        "signerIdentity",
        "timestampStatus"
      ],
      "package evidence entry"
    );
    if (
      !expectedFormats.includes(entry.format) ||
      seen.has(entry.format) ||
      entry.fileName !== path.basename(entry.fileName) ||
      entry.fileName.length < 5 ||
      entry.fileName.length > 240 ||
      /[\0\r\n/\\]/u.test(entry.fileName) ||
      !entry.fileName.normalize("NFC").includes(value.releaseVersion) ||
      entry.architecture !== value.architecture ||
      !Number.isSafeInteger(entry.bytes) ||
      entry.bytes < minimumPackageBytes ||
      entry.bytes > maximumPackageBytes ||
      entry.productName !== productName ||
      entry.container !== packageContainers[entry.format] ||
      typeof entry.metadataVersion !== "string" ||
      entry.metadataVersion.length < 1 ||
      entry.metadataVersion.length > 80 ||
      !/^[0-9A-F]{64}$/u.test(entry.sha256) ||
      !(entry.payloadSha256 === null || /^[0-9A-F]{64}$/u.test(entry.payloadSha256)) ||
      !["ad-hoc", "not-applicable", "unsigned", "valid"].includes(entry.signatureStatus) ||
      !(entry.signerIdentity === null || typeof entry.signerIdentity === "string") ||
      !["missing", "not-applicable", "valid"].includes(entry.timestampStatus) ||
      !["not-applicable", "unverified", "valid"].includes(entry.notarisationStatus)
    ) {
      throw new Error(`The package evidence entry is invalid for ${entry?.format ?? "unknown"}.`);
    }
    if (!filenameMatchesArchitecture(entry.fileName, entry.format, value.architecture)) {
      throw new Error(`The ${entry.format} evidence filename does not match its architecture.`);
    }
    if (
      (entry.format === "msi" && !/^\d+\.\d+\.\d+\.\d+$/u.test(entry.metadataVersion)) ||
      (entry.format !== "msi" && !versionEquivalent(entry.metadataVersion, value.releaseVersion))
    ) {
      throw new Error(`The ${entry.format} package metadata version does not match the release.`);
    }
    seen.add(entry.format);
  }
  validatePackageSigningEvidence(value);
  if (value.platform === "linux") {
    const payloads = new Set(value.packages.map((entry) => entry.payloadSha256));
    if (
      value.payloadConsistency !== "verified" ||
      payloads.size !== 1 ||
      payloads.has(null) ||
      value.packages.some((entry) => entry.signatureStatus !== "not-applicable")
    ) {
      throw new Error("The Linux package payloads are not byte-for-byte consistent.");
    }
  } else if (value.payloadConsistency !== "not-applicable") {
    throw new Error("Package payload consistency is only asserted for the Linux bundle set.");
  } else if (
    value.platform === "windows" &&
    value.packages.some(
      (entry) => entry.payloadSha256 !== null || !["unsigned", "valid"].includes(entry.signatureStatus)
    )
  ) {
    throw new Error("The Windows package evidence contains invalid payload or signature fields.");
  } else if (
    value.platform === "macos" &&
    value.packages.some(
      (entry) => entry.payloadSha256 === null || entry.signatureStatus === "not-applicable"
    )
  ) {
    throw new Error("The macOS package evidence does not verify its application payload and signature state.");
  }
  return value;
}

export function validatePackageSigningEvidence(value) {
  const { expectedSignerIdentity, packages, platform, signaturePolicy } = value;
  if (signaturePolicy === "unsigned-allowed" && expectedSignerIdentity !== null) {
    throw new Error("Unsigned package evidence must not claim an expected signer identity.");
  }
  if (platform === "linux") {
    if (
      expectedSignerIdentity !== null ||
      packages.some(
        (entry) =>
          entry.signatureStatus !== "not-applicable" ||
          entry.signerIdentity !== null ||
          entry.timestampStatus !== "not-applicable" ||
          entry.notarisationStatus !== "not-applicable"
      )
    ) {
      throw new Error("Linux package evidence contains inapplicable publisher-signing fields.");
    }
    return;
  }
  if (platform === "windows") {
    if (
      !(expectedSignerIdentity === null || /^[0-9A-F]{40}$/u.test(expectedSignerIdentity)) ||
      packages.some(
        (entry) =>
          entry.notarisationStatus !== "not-applicable" ||
          (entry.signatureStatus === "unsigned" &&
            (entry.signerIdentity !== null || entry.timestampStatus !== "missing")) ||
          (entry.signatureStatus === "valid" &&
            (!/^[0-9A-F]{40}$/u.test(entry.signerIdentity ?? "") ||
              !["missing", "valid"].includes(entry.timestampStatus)))
      )
    ) {
      throw new Error("The Windows package publisher-signing evidence is invalid.");
    }
    if (
      signaturePolicy === "signed-required" &&
      (!expectedSignerIdentity ||
        packages.some(
          (entry) =>
            entry.signatureStatus !== "valid" ||
            entry.signerIdentity !== expectedSignerIdentity ||
            entry.timestampStatus !== "valid"
        ))
    ) {
      throw new Error("A Windows package does not satisfy the signed release policy.");
    }
    return;
  }
  if (
    !(expectedSignerIdentity === null || /^[A-Z0-9]{10}$/u.test(expectedSignerIdentity)) ||
    packages.some(
      (entry) =>
        (["ad-hoc", "unsigned"].includes(entry.signatureStatus) &&
          (entry.signerIdentity !== null ||
            entry.timestampStatus !== "missing" ||
            entry.notarisationStatus !== "unverified")) ||
        (entry.signatureStatus === "valid" &&
          (!/^[A-Z0-9]{10}$/u.test(entry.signerIdentity ?? "") ||
            !["missing", "valid"].includes(entry.timestampStatus) ||
            !["unverified", "valid"].includes(entry.notarisationStatus)))
    )
  ) {
    throw new Error("The macOS package publisher-signing evidence is invalid.");
  }
  if (
    signaturePolicy === "signed-required" &&
    (!expectedSignerIdentity ||
      packages.some(
        (entry) =>
          entry.signatureStatus !== "valid" ||
          entry.signerIdentity !== expectedSignerIdentity ||
          entry.timestampStatus !== "valid" ||
          entry.notarisationStatus !== "valid"
      ))
  ) {
    throw new Error("The macOS package does not satisfy the signed and notarised release policy.");
  }
}

export async function collectBundleCandidates(bundleRoot) {
  const root = path.resolve(bundleRoot);
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("The release bundle root must be an ordinary directory.");
  }
  const candidates = [];
  const appBundles = [];
  let entriesSeen = 0;

  async function visit(directory, depth) {
    if (depth > 8) throw new Error("The release bundle tree is too deeply nested.");
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name, "en-GB"));
    for (const entry of entries) {
      entriesSeen += 1;
      if (entriesSeen > maximumEntries) throw new Error("The release bundle tree exceeds its entry limit.");
      const absolutePath = path.join(directory, entry.name);
      const metadata = await lstat(absolutePath);
      if (metadata.isSymbolicLink()) {
        if (classifyBundleFilename(entry.name) || entry.name.toLocaleLowerCase("en-GB").endsWith(".app")) {
          throw new Error("A release package candidate must not be a symbolic link.");
        }
        continue;
      }
      if (metadata.isDirectory()) {
        if (entry.name.toLocaleLowerCase("en-GB").endsWith(".app")) {
          appBundles.push({ absolutePath, fileName: entry.name });
        } else {
          await visit(absolutePath, depth + 1);
        }
        continue;
      }
      const format = classifyBundleFilename(entry.name);
      if (!format) continue;
      if (!metadata.isFile() || metadata.size < minimumPackageBytes || metadata.size > maximumPackageBytes) {
        throw new Error(`The ${format} package is not an ordinary file within the release size bounds.`);
      }
      candidates.push({ absolutePath, bytes: metadata.size, fileName: entry.name, format });
    }
  }

  await visit(root, 0);
  return { appBundles, candidates };
}

async function loadSigningExpectation(configPath, platform, signaturePolicy) {
  if (!configPath) {
    if (signaturePolicy === "signed-required") {
      throw new Error("Signed release verification requires the generated platform-signing overlay.");
    }
    return null;
  }
  const metadata = await lstat(configPath);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size < 2 ||
    metadata.size > 64 * 1024
  ) {
    throw new Error("The platform-signing overlay is unsafe or outside its size limit.");
  }
  let overlay;
  try {
    const bytes = await readFile(configPath);
    overlay = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new Error("The platform-signing overlay is not strict UTF-8 JSON.");
  }
  const expectation = validatePlatformSigningOverlay(overlay, platform);
  if (signaturePolicy === "unsigned-allowed" && expectation !== null) {
    throw new Error("An expected publisher identity requires the signed release policy.");
  }
  return expectation;
}

export async function checkReleaseBundles(workspace, options) {
  const packageJson = JSON.parse(await readFile(path.join(workspace, "package.json"), "utf8"));
  const tauriConfig = JSON.parse(await readFile(path.join(workspace, "src-tauri", "tauri.conf.json"), "utf8"));
  if (
    packageJson.name !== packageName ||
    packageJson.version !== tauriConfig.version ||
    tauriConfig.productName !== productName ||
    tauriConfig.identifier !== "org.tufekci.paperworks"
  ) {
    throw new Error("The application and Tauri release identities do not match.");
  }
  const platform = options.platform;
  const architecture = options.architecture;
  const signaturePolicy = options.signaturePolicy ?? "unsigned-allowed";
  const expectedSignerIdentity = await loadSigningExpectation(
    options.signingConfigPath,
    platform,
    signaturePolicy
  );
  const inventory = await collectBundleCandidates(options.bundleRoot);
  const candidates = validateBundleInventory(
    inventory.candidates,
    platform,
    architecture,
    packageJson.version
  );
  let packages;
  if (platform === "windows") {
    if (process.platform !== "win32") throw new Error("Windows packages must be inspected on Windows.");
    packages = await Promise.all(
      candidates.map((candidate) =>
        inspectWindowsPackage(
          candidate,
          architecture,
          packageJson.version,
          tauriConfig,
          signaturePolicy,
          expectedSignerIdentity
        )
      )
    );
  } else if (platform === "linux") {
    if (process.platform !== "linux") throw new Error("Linux packages must be inspected on Linux.");
    packages = [];
    for (const candidate of candidates) {
      packages.push(await inspectLinuxPackage(candidate, architecture, packageJson.version));
    }
  } else if (platform === "macos") {
    if (process.platform !== "darwin") throw new Error("macOS packages must be inspected on macOS.");
    if (inventory.appBundles.length !== 1) {
      throw new Error("The macOS bundle set must contain exactly one application bundle.");
    }
    packages = [
      await inspectMacPackage(
        candidates[0],
        architecture,
        packageJson.version,
        tauriConfig.identifier,
        signaturePolicy,
        expectedSignerIdentity
      )
    ];
  } else {
    throw new Error("The package platform is unsupported.");
  }
  await populatePackageHashes(packages, candidates);

  const report = {
    schemaVersion: 2,
    product: productName,
    releaseVersion: packageJson.version,
    identifier: tauriConfig.identifier,
    platform,
    architecture,
    signaturePolicy,
    expectedSignerIdentity,
    packageCount: packages.length,
    payloadConsistency: platform === "linux" ? "verified" : "not-applicable",
    packages
  };
  validatePackageEvidenceReport(report);
  await mkdir(options.evidenceDirectory, { recursive: true });
  const reportPath = path.join(
    options.evidenceDirectory,
    `package-report-${platform}-${architecture}.json`
  );
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  validatePackageEvidenceReport(JSON.parse(await readFile(reportPath, "utf8")));
  return report;
}

async function inspectWindowsPackage(
  candidate,
  architecture,
  releaseVersion,
  tauriConfig,
  signaturePolicy,
  expectedSignerIdentity
) {
  const first = await readWindow(candidate.absolutePath, 0, 64 * 1024);
  let metadata;
  let container;
  let metadataVersion;
  if (candidate.format === "msi") {
    if (!first.subarray(0, 8).equals(Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]))) {
      throw new Error("The MSI package does not contain a Compound File Binary header.");
    }
    metadata = windowsPackageMetadata(candidate.absolutePath, "msi");
    const expectedWixVersion = tauriConfig.bundle?.windows?.wix?.version;
    if (
      metadata.productName !== productName ||
      metadata.productVersion !== expectedWixVersion ||
      (architecture === "x64" && !/^x64(?:;|$)/iu.test(metadata.template))
    ) {
      throw new Error("The MSI product identity, version or architecture is invalid.");
    }
    metadataVersion = metadata.productVersion;
    container = "msi-compound-file";
  } else {
    const pe = parsePortableExecutableHeader(first);
    if (pe.architecture !== "x86" || (!first.includes(Buffer.from("Nullsoft")) && !first.includes(Buffer.from("NSIS")))) {
      throw new Error("The NSIS package does not contain the expected bounded bootstrap metadata.");
    }
    metadata = windowsPackageMetadata(candidate.absolutePath, "nsis");
    if (metadata.productName !== productName || metadata.productVersion !== releaseVersion) {
      throw new Error("The NSIS product identity or version is invalid.");
    }
    metadataVersion = metadata.productVersion;
    container = "nsis-portable-executable";
  }
  const signing = normaliseWindowsSigningEvidence(
    metadata,
    signaturePolicy,
    expectedSignerIdentity
  );
  return commonPackageEvidence(candidate, {
    architecture,
    container,
    metadataVersion,
    payloadSha256: null,
    ...signing
  });
}

function windowsPackageMetadata(filePath, format) {
  const script = format === "msi" ? windowsMsiScript : windowsExeScript;
  const result = runCommand(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
    `${format.toUpperCase()} metadata inspection`,
    60_000,
    { PAPERWORKS_PACKAGE_PATH: filePath }
  );
  try {
    return JSON.parse(result.stdout.trim());
  } catch {
    throw new Error(`The ${format.toUpperCase()} metadata inspector returned invalid JSON.`);
  }
}

async function inspectLinuxPackage(candidate, architecture, releaseVersion) {
  const first = await readWindow(candidate.absolutePath, 0, 64 * 1024);
  const workspace = await mkdtemp(path.join(tmpdir(), "paperworks-package-"));
  let metadataVersion = releaseVersion;
  let container;
  try {
    if (candidate.format === "appimage") {
      const header = parseElfHeader(first);
      if (header.architecture !== architecture || header.bits !== 64) {
        throw new Error("The AppImage launcher architecture does not match the release target.");
      }
      const mode = (await stat(candidate.absolutePath)).mode;
      if ((mode & 0o111) === 0) throw new Error("The AppImage package is not executable.");
      runCommand(
        candidate.absolutePath,
        ["--appimage-extract"],
        "AppImage extraction",
        2 * 60_000,
        {},
        workspace
      );
      container = "appimage-elf-squashfs";
    } else if (candidate.format === "deb") {
      const packageIdentity = runCommand(
        "dpkg-deb",
        ["--field", candidate.absolutePath, "Package"],
        "Debian package identity",
        30_000
      ).stdout.trim();
      metadataVersion = runCommand(
        "dpkg-deb",
        ["--field", candidate.absolutePath, "Version"],
        "Debian package version",
        30_000
      ).stdout.trim();
      const packageArchitecture = runCommand(
        "dpkg-deb",
        ["--field", candidate.absolutePath, "Architecture"],
        "Debian package architecture",
        30_000
      ).stdout.trim();
      if (
        packageIdentity !== packageName ||
        packageArchitecture !== linuxMetadataArchitecture(architecture, "deb") ||
        !versionEquivalent(metadataVersion, releaseVersion)
      ) {
        throw new Error("The Debian package identity, version or architecture is invalid.");
      }
      runCommand(
        "dpkg-deb",
        ["--extract", candidate.absolutePath, workspace],
        "Debian package extraction",
        2 * 60_000
      );
      container = "debian-ar-package";
      if (first.subarray(0, 8).toString("ascii") !== "!<arch>\n") {
        throw new Error("The Debian package does not contain an ar archive header.");
      }
    } else {
      if (!first.subarray(0, 4).equals(Buffer.from([0xed, 0xab, 0xee, 0xdb]))) {
        throw new Error("The RPM package does not contain an RPM lead header.");
      }
      const fields = runCommand(
        "rpm",
        ["-qp", "--queryformat", "%{NAME}\\n%{VERSION}-%{RELEASE}\\n%{ARCH}\\n", candidate.absolutePath],
        "RPM package metadata",
        60_000
      ).stdout.trim().split(/\r?\n/u);
      metadataVersion = fields[1] ?? "";
      if (
        fields.length !== 3 ||
        fields[0] !== packageName ||
        fields[2] !== linuxMetadataArchitecture(architecture, "rpm") ||
        !versionEquivalent(metadataVersion, releaseVersion)
      ) {
        throw new Error("The RPM package identity, version or architecture is invalid.");
      }
      runCommand(
        "bsdtar",
        ["-xf", candidate.absolutePath, "-C", workspace],
        "RPM package extraction",
        2 * 60_000
      );
      container = "rpm-package";
    }
    const payloadRoot = candidate.format === "appimage" ? path.join(workspace, "squashfs-root") : workspace;
    const payload = await inspectLinuxPayload(payloadRoot, architecture);
    return commonPackageEvidence(candidate, {
      architecture,
      container,
      metadataVersion,
      payloadSha256: payload.sha256,
      signatureStatus: "not-applicable"
    });
  } finally {
    await rm(workspace, { force: true, recursive: true });
  }
}

async function inspectLinuxPayload(root, architecture) {
  const binaryPath = path.join(root, "usr", "bin", packageName);
  const binaryMetadata = await lstat(binaryPath);
  if (
    !binaryMetadata.isFile() ||
    binaryMetadata.isSymbolicLink() ||
    binaryMetadata.size < 1024 * 1024 ||
    binaryMetadata.size > 512 * 1024 * 1024 ||
    (binaryMetadata.mode & 0o111) === 0
  ) {
    throw new Error("The Linux package payload executable is missing, unsafe or outside its size bounds.");
  }
  const header = parseElfHeader(await readWindow(binaryPath, 0, 4096));
  if (header.architecture !== architecture || header.bits !== 64) {
    throw new Error("The Linux package payload architecture does not match the release target.");
  }
  const desktopDirectory = path.join(root, "usr", "share", "applications");
  const desktopFiles = (await readdir(desktopDirectory, { withFileTypes: true })).filter(
    (entry) => entry.isFile() && entry.name.toLocaleLowerCase("en-GB").endsWith(".desktop")
  );
  if (desktopFiles.length !== 1) throw new Error("The Linux package must contain one desktop entry.");
  const desktopPath = path.join(desktopDirectory, desktopFiles[0].name);
  const desktopMetadata = await lstat(desktopPath);
  if (!desktopMetadata.isFile() || desktopMetadata.isSymbolicLink() || desktopMetadata.size > 64 * 1024) {
    throw new Error("The Linux desktop entry is unsafe or exceeds its size limit.");
  }
  const desktop = await readFile(desktopPath, "utf8");
  const name = desktop.match(/^Name=(.+)$/mu)?.[1]?.trim();
  const executable = desktop.match(/^Exec=(.+)$/mu)?.[1]?.trim();
  if (name !== productName || !executable || !new RegExp(`(?:^|/)${packageName}(?:\\s|$)`, "u").test(executable)) {
    throw new Error("The Linux desktop entry does not match the product identity and executable.");
  }
  return { sha256: await sha256File(binaryPath) };
}

async function inspectMacPackage(
  candidate,
  architecture,
  releaseVersion,
  identifier,
  signaturePolicy,
  expectedSignerIdentity
) {
  if (architecture !== "universal") throw new Error("The macOS release package must use the universal target.");
  const tail = await readWindow(candidate.absolutePath, candidate.bytes - 512, 512);
  if (tail.subarray(0, 4).toString("ascii") !== "koly") {
    throw new Error("The macOS package does not contain a UDIF trailer.");
  }
  const mount = await mkdtemp(path.join(tmpdir(), "paperworks-dmg-"));
  let attached = false;
  try {
    runCommand(
      "hdiutil",
      ["attach", "-readonly", "-nobrowse", "-mountpoint", mount, candidate.absolutePath],
      "DMG attachment",
      2 * 60_000
    );
    attached = true;
    const apps = (await readdir(mount, { withFileTypes: true })).filter(
      (entry) => entry.isDirectory() && entry.name.toLocaleLowerCase("en-GB").endsWith(".app")
    );
    if (apps.length !== 1) throw new Error("The DMG must contain exactly one application bundle.");
    const appPath = path.join(mount, apps[0].name);
    const plistPath = path.join(appPath, "Contents", "Info.plist");
    const plistOutput = runCommand(
      "plutil",
      ["-convert", "json", "-o", "-", plistPath],
      "macOS application metadata",
      30_000
    ).stdout;
    let plist;
    try {
      plist = JSON.parse(plistOutput);
    } catch {
      throw new Error("The macOS application metadata is not valid JSON.");
    }
    const observedName = plist.CFBundleDisplayName ?? plist.CFBundleName;
    const metadataVersion = String(plist.CFBundleShortVersionString ?? "");
    if (
      observedName !== productName ||
      plist.CFBundleIdentifier !== identifier ||
      !versionEquivalent(metadataVersion, releaseVersion) ||
      typeof plist.CFBundleExecutable !== "string" ||
      !/^[^/\\\0\r\n]{1,128}$/u.test(plist.CFBundleExecutable)
    ) {
      throw new Error("The macOS application identity, version or executable name is invalid.");
    }
    const binaryPath = path.join(appPath, "Contents", "MacOS", plist.CFBundleExecutable);
    const binaryMetadata = await lstat(binaryPath);
    if (!binaryMetadata.isFile() || binaryMetadata.isSymbolicLink() || binaryMetadata.size < 1024 * 1024) {
      throw new Error("The macOS application executable is missing or unsafe.");
    }
    const architectures = parseMachOArchitectures(await readWindow(binaryPath, 0, 16 * 1024));
    if (architectures.join(",") !== "arm64,x64") {
      throw new Error("The macOS application executable is not universal for Intel and Apple Silicon.");
    }
    const signing = inspectMacSigningEvidence(appPath, signaturePolicy, expectedSignerIdentity);
    return commonPackageEvidence(candidate, {
      architecture,
      container: "apple-udif-disk-image",
      metadataVersion,
      payloadSha256: await sha256File(binaryPath),
      ...signing
    });
  } finally {
    if (attached) {
      runCommand("hdiutil", ["detach", mount], "DMG detachment", 60_000);
    }
    await rm(mount, { force: true, recursive: true });
  }
}

function runMacAssessment(command, args, label) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: maximumCommandBytes,
    shell: false,
    timeout: 60_000
  });
  if (result.error || result.signal || !Number.isInteger(result.status)) {
    throw new Error(`${label} could not complete.`);
  }
  return {
    status: result.status,
    output: boundedDiagnostic(`${result.stdout ?? ""}\n${result.stderr ?? ""}`, 16 * 1024)
  };
}

function inspectMacSigningEvidence(appPath, signaturePolicy, expectedSignerIdentity) {
  const verification = runMacAssessment(
    "codesign",
    ["--verify", "--deep", "--strict", "--verbose=2", appPath],
    "The macOS code-signature verifier"
  );
  if (verification.status !== 0) {
    return normaliseMacSigningEvidence(
      { verification, metadata: null, gatekeeper: null, stapler: null },
      signaturePolicy,
      expectedSignerIdentity
    );
  }
  const metadata = runMacAssessment(
    "codesign",
    ["-d", "--verbose=4", appPath],
    "The macOS code-signature metadata check"
  );
  const gatekeeper = runMacAssessment(
    "spctl",
    ["--assess", "--type", "execute", "--verbose=4", appPath],
    "The macOS Gatekeeper assessment"
  );
  const stapler = runMacAssessment(
    "xcrun",
    ["stapler", "validate", appPath],
    "The macOS notarisation-ticket check"
  );
  return normaliseMacSigningEvidence(
    { verification, metadata, gatekeeper, stapler },
    signaturePolicy,
    expectedSignerIdentity
  );
}

export function normaliseMacSigningEvidence(
  observation,
  signaturePolicy,
  expectedSignerIdentity
) {
  const verification = observation?.verification;
  if (!verification || !Number.isInteger(verification.status)) {
    throw new Error("The macOS code-signature evidence is invalid.");
  }
  if (verification.status !== 0) {
    if (/not signed|code object is not signed/iu.test(verification.output ?? "")) {
      if (signaturePolicy === "signed-required") {
        throw new Error("The macOS application is unsigned.");
      }
      return {
        signatureStatus: "unsigned",
        signerIdentity: null,
        timestampStatus: "missing",
        notarisationStatus: "unverified"
      };
    }
    throw new Error("The macOS application contains an invalid code signature.");
  }
  const metadata = observation.metadata;
  if (!metadata || metadata.status !== 0 || typeof metadata.output !== "string") {
    throw new Error("The macOS code-signature metadata is invalid.");
  }
  if (/Signature=adhoc/iu.test(metadata.output) || !/^Authority=/mu.test(metadata.output)) {
    if (signaturePolicy === "signed-required") {
      throw new Error("The macOS application contains only an ad-hoc signature.");
    }
    return {
      signatureStatus: "ad-hoc",
      signerIdentity: null,
      timestampStatus: "missing",
      notarisationStatus: "unverified"
    };
  }
  const signerIdentity = metadata.output.match(/^TeamIdentifier=([A-Z0-9]{10})$/mu)?.[1] ?? "";
  const timestamp = metadata.output.match(/^Timestamp=(.+)$/mu)?.[1]?.trim() ?? "";
  if (
    !/^Authority=Developer ID Application:/mu.test(metadata.output) ||
    !/^[A-Z0-9]{10}$/u.test(signerIdentity)
  ) {
    throw new Error("The macOS application has no bounded Developer ID signer identity.");
  }
  const timestampStatus = timestamp && timestamp.toLocaleLowerCase("en-GB") !== "none"
    ? "valid"
    : "missing";
  const gatekeeperValid =
    observation.gatekeeper?.status === 0 &&
    /source=Notarized Developer ID/iu.test(observation.gatekeeper?.output ?? "");
  const staplerValid = observation.stapler?.status === 0;
  const notarisationStatus = gatekeeperValid && staplerValid ? "valid" : "unverified";
  if (
    signaturePolicy === "signed-required" &&
    (signerIdentity !== expectedSignerIdentity ||
      timestampStatus !== "valid" ||
      notarisationStatus !== "valid")
  ) {
    throw new Error("The macOS application does not match the expected signed and notarised identity.");
  }
  return {
    signatureStatus: "valid",
    signerIdentity,
    timestampStatus,
    notarisationStatus
  };
}

function commonPackageEvidence(candidate, values) {
  return {
    format: candidate.format,
    fileName: candidate.fileName.normalize("NFC"),
    bytes: candidate.bytes,
    sha256: null,
    productName,
    architecture: values.architecture,
    container: values.container,
    metadataVersion: values.metadataVersion,
    payloadSha256: values.payloadSha256,
    signatureStatus: values.signatureStatus,
    signerIdentity: values.signerIdentity ?? null,
    timestampStatus: values.timestampStatus ?? "not-applicable",
    notarisationStatus: values.notarisationStatus ?? "not-applicable"
  };
}

async function populatePackageHashes(packages, candidates) {
  for (const entry of packages) {
    const candidate = candidates.find((value) => value.format === entry.format);
    entry.sha256 = await sha256File(candidate.absolutePath);
  }
  return packages;
}

export function normaliseWindowsSigningEvidence(metadata, signaturePolicy, expectedSignerIdentity) {
  if (!metadata || typeof metadata !== "object" || Array.isArray(metadata)) {
    throw new Error("The Windows Authenticode metadata is invalid.");
  }
  if (metadata.signatureStatus === "NotSigned") {
    if (signaturePolicy === "signed-required") {
      throw new Error("A Windows release package is unsigned.");
    }
    return {
      signatureStatus: "unsigned",
      signerIdentity: null,
      timestampStatus: "missing",
      notarisationStatus: "not-applicable"
    };
  }
  if (metadata.signatureStatus !== "Valid") {
    throw new Error("A Windows release package contains an invalid Authenticode signature.");
  }
  const signerIdentity = typeof metadata.signerThumbprint === "string"
    ? metadata.signerThumbprint.toUpperCase()
    : "";
  const timestampIdentity = typeof metadata.timeStamperThumbprint === "string"
    ? metadata.timeStamperThumbprint.toUpperCase()
    : "";
  if (!/^[0-9A-F]{40}$/u.test(signerIdentity)) {
    throw new Error("A Windows release package has no bounded Authenticode signer identity.");
  }
  const timestampStatus = /^[0-9A-F]{40}$/u.test(timestampIdentity) ? "valid" : "missing";
  if (
    signaturePolicy === "signed-required" &&
    (signerIdentity !== expectedSignerIdentity || timestampStatus !== "valid")
  ) {
    throw new Error("A Windows release package does not match the expected timestamped signer.");
  }
  return {
    signatureStatus: "valid",
    signerIdentity,
    timestampStatus,
    notarisationStatus: "not-applicable"
  };
}

function filenameMatchesArchitecture(fileName, format, architecture) {
  const lower = fileName.toLocaleLowerCase("en-GB");
  if (architecture === "universal") return format === "dmg" && /(?:_|-)universal(?:\.|_)/u.test(lower);
  if (architecture === "x64") {
    if (["msi", "nsis"].includes(format)) return /(?:_|-)x64(?:_|-|\.)/u.test(lower);
    if (["appimage", "deb"].includes(format)) return /(?:_|-)amd64(?:\.|_)/u.test(lower);
    if (format === "rpm") return /\.x86_64\.rpm$/u.test(lower);
  }
  if (architecture === "arm64") {
    if (["appimage", "deb"].includes(format)) return /(?:_|-)arm64(?:\.|_)/u.test(lower);
    if (format === "rpm") return /\.aarch64\.rpm$/u.test(lower);
  }
  return false;
}

function linuxMetadataArchitecture(architecture, format) {
  if (architecture === "x64") return format === "rpm" ? "x86_64" : "amd64";
  if (architecture === "arm64") return format === "rpm" ? "aarch64" : "arm64";
  throw new Error("The Linux package architecture is unsupported.");
}

function machArchitecture(cpuType) {
  if (cpuType === 0x01000007) return "x64";
  if (cpuType === 0x0100000c) return "arm64";
  throw new Error("The macOS payload contains an unsupported architecture.");
}

async function readWindow(filePath, offset, length) {
  const handle = await open(filePath, "r");
  try {
    const buffer = Buffer.alloc(length);
    const { bytesRead } = await handle.read(buffer, 0, length, offset);
    return buffer.subarray(0, bytesRead);
  } finally {
    await handle.close();
  }
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  await new Promise((resolve, reject) => {
    const input = createReadStream(filePath);
    input.on("data", (chunk) => hash.update(chunk));
    input.on("error", reject);
    input.on("end", resolve);
  });
  return hash.digest("hex").toUpperCase();
}

function runCommand(command, args, label, timeout, extraEnvironment = {}, cwd = undefined) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, ...extraEnvironment },
    maxBuffer: maximumCommandBytes,
    shell: false,
    timeout,
    windowsHide: true
  });
  if (result.error) throw new Error(`${label} could not start: ${boundedDiagnostic(result.error.message)}`);
  if (result.status !== 0) {
    throw new Error(
      `${label} failed: ${boundedDiagnostic((result.stderr || result.stdout || "No diagnostic was returned.").trim())}`
    );
  }
  return { stderr: result.stderr ?? "", stdout: result.stdout ?? "" };
}

function boundedDiagnostic(value, maximumLength = 2_048) {
  const text = String(value).replace(/[\0\r]/gu, "").trim();
  return text.length <= maximumLength ? text : `${text.slice(0, maximumLength)} ...`;
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

function parseArguments(argv, workspace) {
  if (argv.length < 2) {
    throw new Error(
      "Usage: node scripts/check-release-bundles.mjs <bundle-root> <evidence-directory> --platform <windows|macos|linux> --architecture <x64|arm64|universal> [--signature-policy <unsigned-allowed|signed-required>] [--signing-config <path>]"
    );
  }
  const options = {
    bundleRoot: path.resolve(workspace, argv[0]),
    evidenceDirectory: path.resolve(workspace, argv[1]),
    signaturePolicy: "unsigned-allowed"
  };
  for (let index = 2; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!value) throw new Error(`Release package option ${flag} is missing its value.`);
    if (flag === "--platform") options.platform = value;
    else if (flag === "--architecture") options.architecture = value;
    else if (flag === "--signature-policy") options.signaturePolicy = value;
    else if (flag === "--signing-config") options.signingConfigPath = path.resolve(workspace, value);
    else throw new Error(`Unknown release package option: ${flag}.`);
  }
  return options;
}

const windowsMsiScript = String.raw`
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$path = $env:PAPERWORKS_PACKAGE_PATH
$installer = New-Object -ComObject WindowsInstaller.Installer
$database = $installer.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $installer, @($path, 0))
function Read-Property([string]$name) {
  $tick = [char]96
  $query = 'SELECT ' + $tick + 'Value' + $tick + ' FROM ' + $tick + 'Property' + $tick +
    ' WHERE ' + $tick + 'Property' + $tick + " = '$name'"
  $view = $database.GetType().InvokeMember('OpenView', 'InvokeMethod', $null, $database, @($query))
  $view.GetType().InvokeMember('Execute', 'InvokeMethod', $null, $view, $null) | Out-Null
  $record = $view.GetType().InvokeMember('Fetch', 'InvokeMethod', $null, $view, $null)
  if ($record) { return $record.GetType().InvokeMember('StringData', 'GetProperty', $null, $record, 1) }
  return $null
}
$summary = $database.GetType().InvokeMember('SummaryInformation', 'GetProperty', $null, $database, $null)
$signature = Get-AuthenticodeSignature -LiteralPath $path
[PSCustomObject]@{
  productName = (Read-Property 'ProductName')
  productVersion = (Read-Property 'ProductVersion')
  template = $summary.GetType().InvokeMember('Property', 'GetProperty', $null, $summary, 7)
  signatureStatus = $signature.Status.ToString()
  signerThumbprint = if ($signature.SignerCertificate) { $signature.SignerCertificate.Thumbprint } else { $null }
  timeStamperThumbprint = if ($signature.TimeStamperCertificate) { $signature.TimeStamperCertificate.Thumbprint } else { $null }
} | ConvertTo-Json -Compress
`;

const windowsExeScript = String.raw`
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$path = $env:PAPERWORKS_PACKAGE_PATH
$version = (Get-Item -LiteralPath $path).VersionInfo
$signature = Get-AuthenticodeSignature -LiteralPath $path
[PSCustomObject]@{
  productName = $version.ProductName
  productVersion = $version.ProductVersion
  signatureStatus = $signature.Status.ToString()
  signerThumbprint = if ($signature.SignerCertificate) { $signature.SignerCertificate.Thumbprint } else { $null }
  timeStamperThumbprint = if ($signature.TimeStamperCertificate) { $signature.TimeStamperCertificate.Thumbprint } else { $null }
} | ConvertTo-Json -Compress
`;

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const options = parseArguments(process.argv.slice(2), workspace);
  checkReleaseBundles(workspace, options)
    .then((report) => {
      process.stdout.write(
        `Verified ${report.packageCount} ${report.platform} release package${report.packageCount === 1 ? "" : "s"} for ${report.releaseVersion}.\n`
      );
    })
    .catch((error) => {
      process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
      process.exitCode = 1;
    });
}
