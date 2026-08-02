import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const schema = "https://schema.tauri.app/config/2";
const supportedPlatforms = new Set(["linux", "macos", "windows"]);
const maximumCertificateBytes = 1024 * 1024;
const maximumSecretText = 65_536;

function requiredText(value, label, maximumLength, minimumLength = 1) {
  if (
    typeof value !== "string" ||
    value.length < minimumLength ||
    value.length > maximumLength ||
    !value.trim() ||
    /\0/u.test(value)
  ) {
    throw new Error(`${label} is missing or outside its size limit.`);
  }
  return value;
}

function exactSingleLine(value, label, maximumLength, minimumLength = 1) {
  const text = requiredText(value, label, maximumLength, minimumLength);
  if (text !== text.trim() || /[\r\n]/u.test(text)) {
    throw new Error(`${label} must be one line without surrounding space.`);
  }
  return text;
}

function canonicalBase64Bytes(value, label) {
  const text = exactSingleLine(value, label, Math.ceil((maximumCertificateBytes * 4) / 3) + 4);
  if (text.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/u.test(text)) {
    throw new Error(`${label} must be canonical single-line base64.`);
  }
  const bytes = Buffer.from(text, "base64");
  if (
    bytes.length < 1024 ||
    bytes.length > maximumCertificateBytes ||
    bytes.toString("base64") !== text
  ) {
    throw new Error(`${label} is outside the certificate size limit or is not canonical base64.`);
  }
  return bytes;
}

function validateTimestampUrl(value) {
  const text = exactSingleLine(value, "The Windows timestamp URL", 2_048);
  let url;
  try {
    url = new URL(text);
  } catch {
    throw new Error("The Windows timestamp URL is invalid.");
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error("The Windows timestamp URL must be an ordinary HTTPS URL.");
  }
  return url.href;
}

function validatePkcs8PrivateKey(value) {
  const original = requiredText(
    value,
    "The Apple API private key",
    maximumSecretText,
    128
  );
  const text = original.endsWith("\n") ? original.slice(0, -1) : original;
  const beginBoundary = ["-----BEGIN", "PRIVATE", "KEY-----"].join(" ");
  const endBoundary = ["-----END", "PRIVATE", "KEY-----"].join(" ");
  const pemLines = text.split("\n");
  const encodedLines = pemLines.slice(1, -1);
  const encoded = encodedLines.join("");
  if (
    /\r/u.test(original) ||
    text !== text.trim() ||
    pemLines[0] !== beginBoundary ||
    pemLines.at(-1) !== endBoundary ||
    encodedLines.length === 0 ||
    encodedLines.some((line) => !line || line.length > 256) ||
    encoded.length % 4 !== 0 ||
    !/^[A-Za-z0-9+/]+={0,2}$/u.test(encoded)
  ) {
    throw new Error("The Apple API private key must be an LF-formatted canonical PKCS#8 PEM value.");
  }
  const bytes = Buffer.from(encoded, "base64");
  if (bytes.length < 64 || bytes.length > 16 * 1024 || bytes.toString("base64") !== encoded) {
    throw new Error("The Apple API private key must be an LF-formatted canonical PKCS#8 PEM value.");
  }
}

function windowsConfiguration(environment) {
  const thumbprint = exactSingleLine(
    environment.PAPERWORKS_WINDOWS_CERTIFICATE_THUMBPRINT,
    "The Windows certificate thumbprint",
    40
  ).toUpperCase();
  if (!/^[0-9A-F]{40}$/u.test(thumbprint)) {
    throw new Error("The Windows certificate thumbprint must be an exact SHA-1 thumbprint.");
  }
  const timestampUrl = validateTimestampUrl(environment.PAPERWORKS_WINDOWS_TIMESTAMP_URL);
  canonicalBase64Bytes(environment.WINDOWS_CERTIFICATE, "The Windows PFX certificate");
  exactSingleLine(
    environment.WINDOWS_CERTIFICATE_PASSWORD,
    "The Windows PFX password",
    4_096
  );
  return {
    expectedSignerIdentity: thumbprint,
    overlay: {
      $schema: schema,
      bundle: {
        windows: {
          certificateThumbprint: thumbprint,
          digestAlgorithm: "sha256",
          timestampUrl
        }
      }
    }
  };
}

function macosConfiguration(environment) {
  const teamId = exactSingleLine(
    environment.PAPERWORKS_APPLE_TEAM_ID,
    "The Apple team identifier",
    10
  ).toUpperCase();
  if (!/^[A-Z0-9]{10}$/u.test(teamId)) {
    throw new Error("The Apple team identifier must contain ten uppercase letters or digits.");
  }
  const signingIdentity = exactSingleLine(
    environment.APPLE_SIGNING_IDENTITY,
    "The Apple signing identity",
    256
  );
  const identityPrefix = "Developer ID Application: ";
  const identitySuffix = ` (${teamId})`;
  const hasIdentityBoundaries =
    signingIdentity.startsWith(identityPrefix) && signingIdentity.endsWith(identitySuffix);
  const identityName = hasIdentityBoundaries
    ? signingIdentity.slice(identityPrefix.length, -identitySuffix.length)
    : "";
  if (
    !hasIdentityBoundaries ||
    !identityName ||
    identityName !== identityName.trim() ||
    /[\u0000-\u001f\u007f]/u.test(identityName)
  ) {
    throw new Error("The Apple signing identity must be a Developer ID Application identity for the configured team.");
  }
  canonicalBase64Bytes(environment.APPLE_CERTIFICATE, "The Apple P12 certificate");
  exactSingleLine(
    environment.APPLE_CERTIFICATE_PASSWORD,
    "The Apple P12 password",
    4_096
  );
  exactSingleLine(environment.KEYCHAIN_PASSWORD, "The temporary keychain password", 4_096, 16);
  const issuer = exactSingleLine(environment.APPLE_API_ISSUER, "The Apple API issuer", 64);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/iu.test(issuer)) {
    throw new Error("The Apple API issuer must be a UUID.");
  }
  const keyId = exactSingleLine(environment.APPLE_API_KEY, "The Apple API key identifier", 10);
  if (!/^[A-Z0-9]{10}$/u.test(keyId)) {
    throw new Error("The Apple API key identifier must contain ten uppercase letters or digits.");
  }
  validatePkcs8PrivateKey(environment.APPLE_API_PRIVATE_KEY);
  return {
    expectedSignerIdentity: teamId,
    overlay: {
      $schema: schema,
      bundle: {
        macOS: {
          hardenedRuntime: true,
          signingIdentity
        }
      }
    }
  };
}

export function validatePlatformSigningEnvironment(platform, environment) {
  if (!supportedPlatforms.has(platform)) {
    throw new Error("The release package platform must be linux, macos, or windows.");
  }
  if (platform === "windows") return windowsConfiguration(environment);
  if (platform === "macos") return macosConfiguration(environment);
  return {
    expectedSignerIdentity: null,
    overlay: { $schema: schema, bundle: {} }
  };
}

function exactFields(value, fields, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...fields].sort();
  if (actual.length !== expected.length || actual.some((field, index) => field !== expected[index])) {
    throw new Error(`${label} contains missing or unknown fields.`);
  }
}

export function validatePlatformSigningOverlay(value, platform) {
  if (!supportedPlatforms.has(platform)) {
    throw new Error("The signing overlay platform is invalid.");
  }
  exactFields(value, ["$schema", "bundle"], "The signing overlay");
  if (value.$schema !== schema) {
    throw new Error("The signing overlay schema is invalid.");
  }
  if (platform === "linux") {
    exactFields(value.bundle, [], "The Linux signing bundle");
    return null;
  }
  if (platform === "windows") {
    exactFields(value.bundle, ["windows"], "The Windows signing bundle");
    exactFields(
      value.bundle.windows,
      ["certificateThumbprint", "digestAlgorithm", "timestampUrl"],
      "The Windows signing settings"
    );
    const { certificateThumbprint, digestAlgorithm, timestampUrl } = value.bundle.windows;
    if (
      !/^[0-9A-F]{40}$/u.test(certificateThumbprint) ||
      digestAlgorithm !== "sha256" ||
      validateTimestampUrl(timestampUrl) !== timestampUrl
    ) {
      throw new Error("The Windows signing settings are invalid.");
    }
    return certificateThumbprint;
  }
  exactFields(value.bundle, ["macOS"], "The macOS signing bundle");
  exactFields(
    value.bundle.macOS,
    ["hardenedRuntime", "signingIdentity"],
    "The macOS signing settings"
  );
  const { hardenedRuntime, signingIdentity } = value.bundle.macOS;
  const teamId = typeof signingIdentity === "string"
    ? signingIdentity.match(/ \(([A-Z0-9]{10})\)$/u)?.[1]
    : null;
  const identityPrefix = "Developer ID Application: ";
  const identitySuffix = teamId ? ` (${teamId})` : "";
  const identityName =
    teamId && signingIdentity.startsWith(identityPrefix)
      ? signingIdentity.slice(identityPrefix.length, -identitySuffix.length)
      : "";
  if (
    hardenedRuntime !== true ||
    !teamId ||
    !identityName ||
    identityName !== identityName.trim() ||
    /[\u0000-\u001f\u007f]/u.test(identityName)
  ) {
    throw new Error("The macOS signing settings are invalid.");
  }
  return teamId;
}

export async function writePlatformSigningConfig(destination, platform, environment) {
  const configuration = validatePlatformSigningEnvironment(platform, environment);
  validatePlatformSigningOverlay(configuration.overlay, platform);
  await mkdir(path.dirname(destination), { recursive: true });
  await writeFile(destination, `${JSON.stringify(configuration.overlay, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx"
  });
  const written = JSON.parse(await readFile(destination, "utf8"));
  const expectedSignerIdentity = validatePlatformSigningOverlay(written, platform);
  if (expectedSignerIdentity !== configuration.expectedSignerIdentity) {
    throw new Error("The generated platform-signing overlay could not be verified.");
  }
  return configuration;
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const destination = path.resolve(
    workspace,
    process.argv[2] ?? "src-tauri/platform-signing.release.conf.json"
  );
  if (destination !== workspace && !destination.startsWith(`${workspace}${path.sep}`)) {
    throw new Error("The platform-signing overlay destination must stay inside the workspace.");
  }
  const platform = exactSingleLine(
    process.env.PAPERWORKS_RELEASE_PLATFORM,
    "The release package platform",
    16
  ).toLowerCase();
  await writePlatformSigningConfig(destination, platform, process.env);
  process.stdout.write(
    `Publisher-signing overlay generated for ${platform}; private signing material was not written.\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
