import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const allowedChannels = new Set(["alpha", "beta", "stable"]);
const maximumPublicKeyLength = 4_096;
const maximumPrivateKeyLength = 65_536;

function requiredText(value, label, maximumLength) {
  if (typeof value !== "string" || !value.trim() || value.length > maximumLength) {
    throw new Error(`${label} is missing or outside its size limit.`);
  }
  if (/\0/u.test(value)) {
    throw new Error(`${label} contains an invalid control character.`);
  }
  return value.trim();
}

function decodeCanonicalBase64(value, label) {
  if (
    value.length % 4 !== 0 ||
    !/^[A-Za-z0-9+/]+={0,2}$/u.test(value)
  ) {
    throw new Error(`${label} is not canonical base64.`);
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.toString("base64") !== value) {
    throw new Error(`${label} is not canonical base64.`);
  }
  return decoded;
}

function validatePublicKey(value) {
  const publicKey = requiredText(value, "The updater public key", maximumPublicKeyLength);
  if (publicKey !== value) {
    throw new Error("The updater public key must be one line without surrounding space.");
  }
  const decoded = decodeCanonicalBase64(publicKey, "The updater public key");
  let document;
  try {
    document = new TextDecoder("utf-8", { fatal: true }).decode(decoded);
  } catch {
    throw new Error("The updater public key does not contain a UTF-8 Minisign document.");
  }
  if (/\0|\r/u.test(document)) {
    throw new Error("The updater public key contains an invalid control character.");
  }
  const lines = (document.endsWith("\n") ? document.slice(0, -1) : document).split("\n");
  let keyBytes;
  try {
    keyBytes = decodeCanonicalBase64(lines[1] ?? "", "The embedded Minisign public key");
  } catch {
    throw new Error("The updater public key is not a valid Minisign public-key document.");
  }
  if (
    lines.length !== 2 ||
    !lines[0].startsWith("untrusted comment: ") ||
    lines[0].length > 512 ||
    keyBytes.length !== 42 ||
    keyBytes[0] !== 0x45 ||
    ![0x44, 0x64].includes(keyBytes[1])
  ) {
    throw new Error("The updater public key is not a valid Minisign public-key document.");
  }
  return publicKey;
}

function validateEndpoint(value, channel) {
  const endpoint = requiredText(value, "The updater endpoint", 2_048);
  if (endpoint !== value) {
    throw new Error("The updater endpoint must not contain surrounding space.");
  }
  let parsed;
  try {
    parsed = new URL(endpoint);
  } catch {
    throw new Error("The updater endpoint is not a valid URL.");
  }
  if (
    parsed.protocol !== "https:" ||
    parsed.username ||
    parsed.password ||
    parsed.hash ||
    !parsed.hostname
  ) {
    throw new Error("The updater endpoint must be an ordinary HTTPS URL without credentials.");
  }
  const channelMarker = `updates-${channel}`;
  if (!parsed.pathname.toLowerCase().split("/").includes(channelMarker)) {
    throw new Error(`The updater endpoint must contain the ${channelMarker} channel segment.`);
  }
  return parsed.href;
}

export function validateUpdaterReleaseEnvironment(environment) {
  const channelValue = requiredText(
    environment.PAPERWORKS_UPDATE_CHANNEL,
    "The updater channel",
    16
  );
  if (channelValue !== environment.PAPERWORKS_UPDATE_CHANNEL) {
    throw new Error("The updater channel must not contain surrounding space.");
  }
  const channel = channelValue.toLowerCase();
  if (!allowedChannels.has(channel)) {
    throw new Error("The updater channel must be alpha, beta, or stable.");
  }
  const endpoint = validateEndpoint(environment.PAPERWORKS_UPDATE_ENDPOINT, channel);
  const publicKey = validatePublicKey(environment.PAPERWORKS_UPDATE_PUBLIC_KEY);
  requiredText(
    environment.TAURI_SIGNING_PRIVATE_KEY,
    "The private updater signing key",
    maximumPrivateKeyLength
  );
  if (
    environment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD !== undefined &&
    (typeof environment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD !== "string" ||
      environment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD.length > 4_096 ||
      /\0/u.test(environment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD))
  ) {
    throw new Error("The updater signing-key password is outside its size limit.");
  }
  return { channel, endpoint, publicKey };
}

export function updaterReleaseOverlay() {
  return {
    $schema: "https://schema.tauri.app/config/2",
    bundle: {
      createUpdaterArtifacts: true
    }
  };
}

export async function writeUpdaterReleaseConfig(destination, environment) {
  const configuration = validateUpdaterReleaseEnvironment(environment);
  const overlay = updaterReleaseOverlay();
  await mkdir(path.dirname(destination), { recursive: true });
  await writeFile(destination, `${JSON.stringify(overlay, null, 2)}\n`, "utf8");
  const written = JSON.parse(await readFile(destination, "utf8"));
  if (JSON.stringify(written) !== JSON.stringify(overlay)) {
    throw new Error("The generated updater overlay could not be verified.");
  }
  return configuration;
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const destination = path.resolve(
    workspace,
    process.argv[2] ?? "src-tauri/updater.release.conf.json"
  );
  if (destination !== workspace && !destination.startsWith(`${workspace}${path.sep}`)) {
    throw new Error("The updater overlay destination must stay inside the workspace.");
  }
  const configuration = await writeUpdaterReleaseConfig(destination, process.env);
  process.stdout.write(
    `Signed updater overlay generated for the ${configuration.channel} channel; private key material was not written.\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
