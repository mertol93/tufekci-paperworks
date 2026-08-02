import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { releaseUpdateChannel } from "./check-release-version.mjs";

function exactFields(value, fields, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...fields].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label} fields do not match the promotion contract.`);
  }
}

function validRepository(value) {
  return (
    typeof value === "string" &&
    value.length <= 200 &&
    /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(value)
  );
}

function validateReleaseUrl(value, repository, tag) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("The source release URL is invalid.");
  }
  const expectedPath = `/${repository}/releases/tag/${encodeURIComponent(tag)}`;
  if (
    url.protocol !== "https:" ||
    url.hostname !== "github.com" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== expectedPath
  ) {
    throw new Error("The source release URL does not match the immutable release.");
  }
  return url.href;
}

function validPlatformKeys(value) {
  return (
    Array.isArray(value) &&
    value.length >= 4 &&
    value.length <= 16 &&
    new Set(value).size === value.length &&
    value.every(
      (key) =>
        typeof key === "string" &&
        /^(?:darwin-(?:aarch64|universal|x86_64)|linux-x86_64|windows-x86_64)(?:-[a-z0-9]+)?$/u.test(
          key
        )
    )
  );
}

export function validateUpdatePromotion(release, manifest, options) {
  const { channel, repository, tag, version } = options;
  if (!validRepository(repository)) {
    throw new Error("The promotion repository identity is invalid.");
  }
  if (tag !== `v${version}` || releaseUpdateChannel(version) !== channel) {
    throw new Error("The promotion tag, version, and channel do not agree.");
  }

  exactFields(release, ["isDraft", "isPrerelease", "tagName", "url"], "The source release");
  if (release.isDraft !== false || release.tagName !== tag) {
    throw new Error("Only the exact published immutable release can be promoted.");
  }
  const expectedPrerelease = channel !== "stable";
  if (release.isPrerelease !== expectedPrerelease) {
    throw new Error("The source release pre-release state does not match its channel.");
  }
  const sourceReleaseUrl = validateReleaseUrl(release.url, repository, tag);

  exactFields(
    manifest,
    [
      "channel",
      "apiAssetUrlCount",
      "manifestSha256",
      "platformKeys",
      "product",
      "releaseTag",
      "releaseVersion",
      "repository",
      "schemaVersion"
    ],
    "The updater manifest report"
  );
  if (
    manifest.schemaVersion !== 1 ||
    manifest.product !== "Tüfekci Paperworks" ||
    manifest.releaseTag !== tag ||
    manifest.releaseVersion !== version ||
    manifest.channel !== channel ||
    manifest.repository !== repository ||
    !validPlatformKeys(manifest.platformKeys) ||
    !Number.isSafeInteger(manifest.apiAssetUrlCount) ||
    manifest.apiAssetUrlCount < 0 ||
    manifest.apiAssetUrlCount > manifest.platformKeys.length ||
    !/^[A-F0-9]{64}$/u.test(manifest.manifestSha256)
  ) {
    throw new Error("The updater manifest report does not match the promotion source.");
  }

  return {
    schemaVersion: 1,
    product: manifest.product,
    repository,
    sourceReleaseTag: tag,
    sourceReleaseUrl,
    releaseVersion: version,
    channel,
    channelTag: `updates-${channel}`,
    platformKeys: manifest.platformKeys,
    apiAssetUrlCount: manifest.apiAssetUrlCount,
    manifestSha256: manifest.manifestSha256
  };
}

async function readBoundedJson(sourcePath, label) {
  const metadata = await stat(sourcePath);
  if (!metadata.isFile() || metadata.size < 2 || metadata.size > 256 * 1024) {
    throw new Error(`${label} is not an ordinary file within the size limit.`);
  }
  try {
    return JSON.parse(await readFile(sourcePath, "utf8"));
  } catch {
    throw new Error(`${label} is not valid UTF-8 JSON.`);
  }
}

function parseArguments(argv) {
  const options = { releasePath: argv[0], manifestReportPath: argv[1] };
  for (let index = 2; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!value || !["--channel", "--report", "--repository", "--tag", "--version"].includes(flag)) {
      throw new Error("The update promotion command arguments are invalid.");
    }
    options[flag.slice(2)] = value;
  }
  for (const field of [
    "channel",
    "manifestReportPath",
    "releasePath",
    "report",
    "repository",
    "tag",
    "version"
  ]) {
    if (!options[field]) {
      throw new Error(`The update promotion command requires ${field}.`);
    }
  }
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const release = await readBoundedJson(options.releasePath, "The source release metadata");
  const manifest = await readBoundedJson(
    options.manifestReportPath,
    "The updater manifest report"
  );
  const report = validateUpdatePromotion(release, manifest, options);
  await mkdir(path.dirname(path.resolve(options.report)), { recursive: true });
  await writeFile(options.report, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  process.stdout.write(
    `Update promotion verified from ${report.sourceReleaseTag} to ${report.channelTag}.\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
