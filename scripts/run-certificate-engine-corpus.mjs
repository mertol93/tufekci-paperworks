import { spawnSync } from "node:child_process";
import { constants as fsConstants } from "node:fs";
import {
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile
} from "node:fs/promises";
import { createHash } from "node:crypto";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const evidenceMarker = "PAPERWORKS_CERTIFICATE_V1";
const maximumCommandOutputBytes = 16 * 1024 * 1024;
const disposablePassphrase = "paperworks-certificate-test";
const disposablePdfPassword = "paperworks-test";

export function parseCertificateMarker(output) {
  if (
    typeof output !== "string" ||
    Buffer.byteLength(output, "utf8") > maximumCommandOutputBytes
  ) {
    throw new Error("Certificate corpus output is missing or exceeds its safety limit.");
  }
  const matches = output
    .split(/\r?\n/u)
    .map((line) => line.slice(line.indexOf(`${evidenceMarker}\t`)))
    .filter((line) => line.startsWith(`${evidenceMarker}\t`));
  if (matches.length !== 1) {
    throw new Error("Certificate evidence must contain exactly one native result marker.");
  }
  const fields = matches[0].split("\t");
  if (
    fields.length !== 8 ||
    fields[0] !== evidenceMarker ||
    fields[1] !== "1" ||
    fields[2] !== "2" ||
    fields[3] !== "1" ||
    fields[4] !== "1" ||
    fields[5] !== "1" ||
    !["0", "1"].includes(fields[6]) ||
    !isEngineVersion(fields[7])
  ) {
    throw new Error("The native certificate test emitted malformed result evidence.");
  }
  return {
    encryptedSourcePreserved: true,
    engineVersion: fields[7],
    incrementalSignatureCount: 2,
    integrityAndTrustSeparated: true,
    timestampTested: fields[6] === "1",
    trustedValidation: true,
    visibleSignaturePublished: true
  };
}

export function validateCertificateEngineReport(value, options = {}) {
  requireExactFields(
    value,
    [
      "architecture",
      "engines",
      "platform",
      "product",
      "releaseVersion",
      "scenarios",
      "schemaVersion",
      "sourceFixtureSha256"
    ],
    "certificate engine report"
  );
  if (
    value.schemaVersion !== 1 ||
    value.product !== "Tüfekci Paperworks" ||
    typeof value.releaseVersion !== "string" ||
    !/^0\.1\.0-alpha\.\d+$/u.test(value.releaseVersion) ||
    !["darwin", "linux", "win32"].includes(value.platform) ||
    typeof value.architecture !== "string" ||
    !/^[A-Za-z0-9_-]{1,32}$/u.test(value.architecture) ||
    typeof value.sourceFixtureSha256 !== "string" ||
    !/^[0-9A-F]{64}$/u.test(value.sourceFixtureSha256)
  ) {
    throw new Error("The certificate engine report identity is invalid.");
  }
  requireExactFields(value.engines, ["openSsl", "pyHanko"], "certificate engines");
  for (const [key, command] of [
    ["openSsl", "openssl"],
    ["pyHanko", "pyhanko"]
  ]) {
    requireExactFields(value.engines[key], ["command", "version"], `${key} engine`);
    if (
      value.engines[key].command !== command ||
      !isEngineVersion(value.engines[key].version)
    ) {
      throw new Error(`The ${key} certificate engine evidence is invalid.`);
    }
  }
  requireExactFields(
    value.scenarios,
    [
      "encryptedSourcePreserved",
      "incrementalSignatureCount",
      "integrityAndTrustSeparated",
      "timestampTested",
      "trustedValidation",
      "visibleSignaturePublished"
    ],
    "certificate scenarios"
  );
  if (
    value.scenarios.encryptedSourcePreserved !== true ||
    value.scenarios.incrementalSignatureCount !== 2 ||
    value.scenarios.integrityAndTrustSeparated !== true ||
    typeof value.scenarios.timestampTested !== "boolean" ||
    value.scenarios.trustedValidation !== true ||
    value.scenarios.visibleSignaturePublished !== true ||
    value.scenarios.engineVersion !== undefined
  ) {
    throw new Error("The certificate engine scenario evidence is invalid.");
  }
  if (options.requireTimestamp === true && value.scenarios.timestampTested !== true) {
    throw new Error("RFC 3161 timestamp evidence is required for this release gate.");
  }
  return value;
}

export async function runCertificateEngineCorpus(workspace, sourceArgument, outputArgument) {
  const timestampPolicy = process.env.PAPERWORKS_REQUIRE_CERTIFICATE_TIMESTAMP;
  if (timestampPolicy && !["0", "1"].includes(timestampPolicy)) {
    throw new Error("The certificate timestamp policy must be exactly 0 or 1.");
  }
  const requireTimestamp = timestampPolicy === "1";
  if (requireTimestamp && !process.env.PAPERWORKS_TEST_TSA_URL) {
    throw new Error("The release certificate gate requires PAPERWORKS_TEST_TSA_URL.");
  }
  const sourceDirectory = path.resolve(workspace, sourceArgument || "qa-fixtures");
  const outputDirectory = path.resolve(
    workspace,
    outputArgument || "qa-fixtures/certificate-engine"
  );
  const sourceFixture = path.join(sourceDirectory, "annotations-and-form.pdf");
  await requireOrdinaryBoundedFile(sourceFixture, "certificate source fixture", 64 * 1024 * 1024);

  const pyHankoVersion = firstVersionLine(
    runCommand("pyhanko", ["--version"], "pyHanko version check", 15_000),
    "pyHanko"
  );
  const openSslVersion = firstVersionLine(
    runCommand("openssl", ["version"], "OpenSSL version check", 15_000),
    "OpenSSL"
  );
  const packageJson = JSON.parse(
    await readFile(path.join(workspace, "package.json"), "utf8")
  );
  const temporaryDirectory = await mkdtemp(
    path.join(os.tmpdir(), "paperworks-certificate-corpus-")
  );
  try {
    const unsignedPdf = path.join(temporaryDirectory, "unsigned.pdf");
    const encryptedPdf = path.join(temporaryDirectory, "encrypted.pdf");
    const certificate = path.join(temporaryDirectory, "signer.p12");
    const certificatePem = path.join(temporaryDirectory, "trust-root.pem");
    const privateKey = path.join(temporaryDirectory, "signer-key.pem");
    await copyFile(sourceFixture, unsignedPdf, fsConstants.COPYFILE_EXCL);
    await writePrivateFile(
      path.join(temporaryDirectory, "passphrase.txt"),
      disposablePassphrase
    );
    await writePrivateFile(
      path.join(temporaryDirectory, "pdf-password.txt"),
      disposablePdfPassword
    );

    const opensslEnvironment = await openSslEnvironment();
    runCommand(
      "openssl",
      [
        "req",
        "-x509",
        "-newkey",
        "rsa:3072",
        "-keyout",
        privateKey,
        "-out",
        certificatePem,
        "-sha256",
        "-days",
        "2",
        "-nodes",
        "-subj",
        "/CN=Tufekci Paperworks Disposable Test Signer/O=Local QA",
        "-addext",
        "basicConstraints=critical,CA:FALSE",
        "-addext",
        "keyUsage=critical,digitalSignature,nonRepudiation",
        "-addext",
        "extendedKeyUsage=emailProtection"
      ],
      "Disposable certificate generation",
      60_000,
      opensslEnvironment
    );
    runCommand(
      "openssl",
      [
        "pkcs12",
        "-export",
        "-out",
        certificate,
        "-inkey",
        privateKey,
        "-in",
        certificatePem,
        "-passout",
        `pass:${disposablePassphrase}`,
        "-name",
        "PaperworksDisposableSigner"
      ],
      "Disposable PKCS#12 generation",
      30_000,
      opensslEnvironment
    );
    runCommand(
      "pyhanko",
      [
        "encrypt",
        "--password",
        disposablePdfPassword,
        unsignedPdf,
        encryptedPdf
      ],
      "Encrypted certificate fixture generation",
      60_000
    );

    const nativeResult = runCommand(
      "cargo",
      [
        "test",
        "--manifest-path",
        path.join(workspace, "src-tauri", "Cargo.toml"),
        "live_certificate_corpus",
        "--",
        "--ignored",
        "--nocapture",
        "--test-threads=1"
      ],
      "Native certificate engine corpus",
      15 * 60_000,
      { PAPERWORKS_CERTIFICATE_CORPUS: temporaryDirectory }
    );
    const marker = parseCertificateMarker(nativeResult);
    if (marker.engineVersion !== pyHankoVersion) {
      throw new Error("The native certificate evidence used an unexpected pyHanko version.");
    }
    const report = validateCertificateEngineReport({
      architecture: process.arch,
      engines: {
        openSsl: { command: "openssl", version: openSslVersion },
        pyHanko: { command: "pyhanko", version: pyHankoVersion }
      },
      platform: process.platform,
      product: "Tüfekci Paperworks",
      releaseVersion: packageJson.version,
      scenarios: {
        encryptedSourcePreserved: marker.encryptedSourcePreserved,
        incrementalSignatureCount: marker.incrementalSignatureCount,
        integrityAndTrustSeparated: marker.integrityAndTrustSeparated,
        timestampTested: marker.timestampTested,
        trustedValidation: marker.trustedValidation,
        visibleSignaturePublished: marker.visibleSignaturePublished
      },
      schemaVersion: 1,
      sourceFixtureSha256: await sha256File(sourceFixture)
    }, { requireTimestamp });
    await ensureOrdinaryDirectory(outputDirectory);
    const reportFilename = `certificate-engine-report-${process.platform}-${process.arch}.json`;
    await writeReport(
      path.join(outputDirectory, reportFilename),
      `${JSON.stringify(report, null, 2)}\n`
    );
    return { ...report, reportFilename };
  } finally {
    await rm(temporaryDirectory, { force: true, recursive: true });
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
  return `${result.stdout || ""}\n${result.stderr || ""}`;
}

function firstVersionLine(output, label) {
  const value = output
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .find(Boolean);
  if (!isEngineVersion(value)) {
    throw new Error(`${label} did not report a bounded single-line version.`);
  }
  return value;
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

async function openSslEnvironment() {
  let pythonPrefix = null;
  try {
    pythonPrefix = firstVersionLine(
      runCommand(
        "python",
        ["-c", "import sys; print(sys.prefix)"],
        "Python prefix discovery",
        15_000
      ),
      "Python prefix"
    );
  } catch {
    // A standard OpenSSL installation does not require Python-based discovery.
  }
  const candidates = [
    process.env.OPENSSL_CONF,
    process.env.CONDA_PREFIX
      ? path.join(process.env.CONDA_PREFIX, "Library", "ssl", "openssl.cnf")
      : null,
    pythonPrefix ? path.join(pythonPrefix, "Library", "ssl", "openssl.cnf") : null,
    pythonPrefix ? path.join(pythonPrefix, "ssl", "openssl.cnf") : null
  ].filter(Boolean);
  for (const candidate of candidates) {
    try {
      const metadata = await lstat(candidate);
      if (metadata.isFile() && !metadata.isSymbolicLink()) {
        return { OPENSSL_CONF: candidate };
      }
    } catch (error) {
      if (!error || error.code !== "ENOENT") throw error;
    }
  }
  return {};
}

async function requireOrdinaryBoundedFile(candidate, label, maximumBytes) {
  const metadata = await lstat(candidate).catch(() => null);
  if (
    !metadata ||
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size === 0 ||
    metadata.size > maximumBytes
  ) {
    throw new Error(`The ${label} is missing, unsafe, empty or outside its size limit.`);
  }
}

async function ensureOrdinaryDirectory(directory) {
  try {
    const metadata = await lstat(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("Refusing to use a non-ordinary certificate evidence directory.");
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") throw error;
    await mkdir(directory, { recursive: true });
  }
}

async function writePrivateFile(candidate, value) {
  await writeFile(candidate, value, { encoding: "utf8", flag: "wx", mode: 0o600 });
}

async function writeReport(candidate, text) {
  try {
    const metadata = await lstat(candidate);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error("Refusing to replace a non-ordinary certificate engine report.");
    }
  } catch (error) {
    if (!error || error.code !== "ENOENT") throw error;
  }
  await writeFile(candidate, text, "utf8");
}

async function sha256File(candidate) {
  return createHash("sha256")
    .update(await readFile(candidate))
    .digest("hex")
    .toUpperCase();
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const report = await runCertificateEngineCorpus(
    workspace,
    process.argv[2],
    process.argv[3]
  );
  process.stdout.write(
    `Certificate engine corpus passed visible, incremental, encrypted and trust-separation scenarios (${report.reportFilename}).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
