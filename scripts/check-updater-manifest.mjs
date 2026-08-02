import { createHash } from "node:crypto";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { releaseUpdateChannel } from "./check-release-version.mjs";

const maximumManifestBytes = 256 * 1024;
const requiredPlatforms = [
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-x86_64",
  "windows-x86_64"
];

function objectFields(value, allowed, required, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const fields = Object.keys(value);
  if (fields.some((field) => !allowed.includes(field))) {
    throw new Error(`${label} contains an unknown field.`);
  }
  if (required.some((field) => !fields.includes(field))) {
    throw new Error(`${label} is missing a required field.`);
  }
}

function boundedText(value, label, maximumLength, allowEmpty = false) {
  if (
    typeof value !== "string" ||
    (!allowEmpty && !value.trim()) ||
    value.length > maximumLength ||
    /\0|\r/u.test(value)
  ) {
    throw new Error(`${label} is missing or outside its text limits.`);
  }
  return value;
}

function decodeCanonicalBase64(value, label) {
  if (value.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/u.test(value)) {
    throw new Error(`${label} is not canonical base64.`);
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.toString("base64") !== value) {
    throw new Error(`${label} is not canonical base64.`);
  }
  return decoded;
}

function validateSignature(value, platform) {
  const signature = boundedText(value, `The ${platform} updater signature`, 4_096);
  if (signature !== signature.trim()) {
    throw new Error(`The ${platform} updater signature must be one line.`);
  }
  let document;
  try {
    document = new TextDecoder("utf-8", { fatal: true }).decode(
      decodeCanonicalBase64(signature, `The ${platform} updater signature`)
    );
  } catch {
    throw new Error(`The ${platform} updater signature is not a valid encoded Minisign document.`);
  }
  if (/\0|\r/u.test(document)) {
    throw new Error(`The ${platform} updater signature contains an invalid control character.`);
  }
  const lines = (document.endsWith("\n") ? document.slice(0, -1) : document).split("\n");
  let primarySignature;
  let globalSignature;
  try {
    primarySignature = decodeCanonicalBase64(
      lines[1] ?? "",
      `The ${platform} primary Minisign signature`
    );
    globalSignature = decodeCanonicalBase64(
      lines[3] ?? "",
      `The ${platform} global Minisign signature`
    );
  } catch {
    throw new Error(`The ${platform} updater signature is not a valid encoded Minisign document.`);
  }
  if (
    lines.length !== 4 ||
    !lines[0].startsWith("untrusted comment: ") ||
    !lines[2].startsWith("trusted comment: ") ||
    lines[0].length > 512 ||
    lines[2].length > 512 ||
    primarySignature.length !== 74 ||
    primarySignature[0] !== 0x45 ||
    ![0x44, 0x64].includes(primarySignature[1]) ||
    globalSignature.length !== 64
  ) {
    throw new Error(`The ${platform} updater signature is not a valid encoded Minisign document.`);
  }
  return signature;
}

function validateAssetUrl(value, repository, tag, platform, releaseAssetIds) {
  const urlText = boundedText(value, `The ${platform} updater URL`, 2_048);
  let url;
  try {
    url = new URL(urlText);
  } catch {
    throw new Error(`The ${platform} updater URL is invalid.`);
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error(`The ${platform} updater URL must be an ordinary HTTPS asset URL.`);
  }

  const directPrefix = `/${repository}/releases/download/${encodeURIComponent(tag)}/`;
  const apiPrefix = `/repos/${repository}/releases/assets/`;
  const direct = url.hostname === "github.com" && url.pathname.startsWith(directPrefix);
  const api =
    url.hostname === "api.github.com" &&
    url.pathname.startsWith(apiPrefix) &&
    /^\d+$/u.test(url.pathname.slice(apiPrefix.length));
  if (!direct && !api) {
    throw new Error(`The ${platform} updater URL does not belong to the immutable release.`);
  }
  if (api) {
    const assetId = Number(url.pathname.slice(apiPrefix.length));
    if (!releaseAssetIds?.has(assetId)) {
      throw new Error(
        `The ${platform} updater API asset does not belong to the immutable release inventory.`
      );
    }
  }
  return api ? "api" : "direct";
}

function validPlatformKey(value) {
  return /^(?:darwin-(?:aarch64|universal|x86_64)|linux-x86_64|windows-x86_64)(?:-[a-z0-9]+)?$/u.test(
    value
  );
}

export function validateUpdaterManifest(manifest, options, sourceBytes) {
  const { channel, releaseAssetIds, repository, tag, version } = options;
  if (channel !== releaseUpdateChannel(version)) {
    throw new Error("The updater channel does not match the release version.");
  }
  if (tag !== `v${version}`) {
    throw new Error("The updater release tag does not match the release version.");
  }
  if (
    typeof repository !== "string" ||
    repository.length > 200 ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(repository)
  ) {
    throw new Error("The updater repository identity is invalid.");
  }

  objectFields(
    manifest,
    ["notes", "platforms", "pub_date", "version"],
    ["platforms", "version"],
    "The updater manifest"
  );
  const manifestVersion = boundedText(manifest.version, "The updater version", 128);
  if (manifestVersion.replace(/^v/u, "") !== version) {
    throw new Error("The updater manifest version does not match the release.");
  }
  if (manifest.notes !== undefined) {
    boundedText(manifest.notes, "The updater notes", 32_768, true);
  }
  if (manifest.pub_date !== undefined) {
    boundedText(manifest.pub_date, "The updater publication date", 64);
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/u.test(manifest.pub_date)) {
      throw new Error("The updater publication date must be UTC RFC 3339 text.");
    }
  }
  objectFields(manifest.platforms, Object.keys(manifest.platforms ?? {}), [], "Updater platforms");
  const platformKeys = Object.keys(manifest.platforms).sort();
  if (
    platformKeys.length < requiredPlatforms.length ||
    platformKeys.length > 16 ||
    platformKeys.some((key) => !validPlatformKey(key)) ||
    requiredPlatforms.some((key) => !platformKeys.includes(key))
  ) {
    throw new Error("The updater manifest does not contain the exact supported platform families.");
  }
  let apiAssetUrlCount = 0;
  for (const platform of platformKeys) {
    const entry = manifest.platforms[platform];
    objectFields(entry, ["signature", "url"], ["signature", "url"], `${platform} updater`);
    validateSignature(entry.signature, platform);
    if (validateAssetUrl(entry.url, repository, tag, platform, releaseAssetIds) === "api") {
      apiAssetUrlCount += 1;
    }
  }

  const bytes = Buffer.isBuffer(sourceBytes)
    ? sourceBytes
    : Buffer.from(`${JSON.stringify(manifest)}\n`, "utf8");
  return {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    releaseTag: tag,
    channel,
    repository,
    platformKeys,
    apiAssetUrlCount,
    manifestSha256: createHash("sha256").update(bytes).digest("hex").toUpperCase()
  };
}

function parseArguments(argv) {
  const options = { manifestPath: argv[0] };
  const flags = new Map([
    ["--channel", "channel"],
    ["--release-assets", "releaseAssetsPath"],
    ["--report", "report"],
    ["--repository", "repository"],
    ["--tag", "tag"],
    ["--version", "version"]
  ]);
  for (let index = 1; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    const field = flags.get(flag);
    if (!value || !field) {
      throw new Error("The updater manifest command arguments are invalid.");
    }
    options[field] = value;
  }
  for (const field of [
    "channel",
    "manifestPath",
    "releaseAssetsPath",
    "repository",
    "tag",
    "version"
  ]) {
    if (!options[field]) {
      throw new Error(`The updater manifest command requires ${field}.`);
    }
  }
  return options;
}

async function readReleaseAssetIds(sourcePath) {
  const metadata = await stat(sourcePath);
  if (!metadata.isFile() || metadata.size < 2 || metadata.size > maximumManifestBytes) {
    throw new Error("The release asset inventory is not an ordinary file within the size limit.");
  }
  let inventory;
  try {
    inventory = JSON.parse(await readFile(sourcePath, "utf8"));
  } catch {
    throw new Error("The release asset inventory is not valid UTF-8 JSON.");
  }
  objectFields(inventory, ["assetIds"], ["assetIds"], "The release asset inventory");
  if (
    !Array.isArray(inventory.assetIds) ||
    inventory.assetIds.length === 0 ||
    inventory.assetIds.length > 500 ||
    new Set(inventory.assetIds).size !== inventory.assetIds.length ||
    inventory.assetIds.some(
      (assetId) => !Number.isSafeInteger(assetId) || assetId <= 0
    )
  ) {
    throw new Error("The release asset inventory contains invalid asset identifiers.");
  }
  return new Set(inventory.assetIds);
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const metadata = await stat(options.manifestPath);
  if (!metadata.isFile() || metadata.size < 2 || metadata.size > maximumManifestBytes) {
    throw new Error("The updater manifest is not an ordinary file within the size limit.");
  }
  const bytes = await readFile(options.manifestPath);
  let manifest;
  try {
    manifest = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("The updater manifest is not valid UTF-8 JSON.");
  }
  const releaseAssetIds = await readReleaseAssetIds(options.releaseAssetsPath);
  const report = validateUpdaterManifest(manifest, { ...options, releaseAssetIds }, bytes);
  if (options.report) {
    await mkdir(path.dirname(path.resolve(options.report)), { recursive: true });
    await writeFile(options.report, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  }
  process.stdout.write(
    `Updater manifest verified for ${report.releaseTag} on the ${report.channel} channel (${report.platformKeys.length} platform entries).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
