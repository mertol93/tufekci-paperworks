import { createHash } from "node:crypto";
import {
  access,
  mkdir,
  readFile,
  rename,
  rm,
  writeFile
} from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { inflateRawSync } from "node:zlib";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  auditSourceTree,
  validateCandidatePaths
} from "./check-source-tree.mjs";
import {
  readProjectReleaseVersions,
  validateReleaseVersionSet,
  validateWindowsMsiVersion
} from "./check-release-version.mjs";

const maxArchiveBytes = 64 * 1024 * 1024;
const maxFileBytes = 5 * 1024 * 1024;
const maxSourceBytes = 50 * 1024 * 1024;
const ordinaryModes = new Set(["100644", "100755"]);

export function parseSourceDateEpoch(value, now = Math.floor(Date.now() / 1_000)) {
  if (typeof value !== "string" || !/^(?:0|[1-9]\d*)$/u.test(value)) {
    throw new Error("SOURCE_DATE_EPOCH must be a whole number of seconds.");
  }
  const epoch = Number(value);
  if (!Number.isSafeInteger(epoch) || epoch > 4_102_444_800) {
    throw new Error("SOURCE_DATE_EPOCH is outside the supported range ending in 2100.");
  }
  if (epoch > now + 300) {
    throw new Error("SOURCE_DATE_EPOCH must not be more than five minutes in the future.");
  }
  return epoch;
}

export function validateIndexRecord(record) {
  if (!record || typeof record !== "object") {
    throw new Error("Git index records must be objects.");
  }
  if (!ordinaryModes.has(record.mode)) {
    throw new Error(`The source index contains a non-file mode: ${record.path}.`);
  }
  if (record.stage !== 0) {
    throw new Error(`The source index contains an unresolved entry: ${record.path}.`);
  }
  if (!/^[0-9a-f]{40}(?:[0-9a-f]{24})?$/u.test(record.objectId)) {
    throw new Error(`The source index contains an invalid object ID: ${record.path}.`);
  }
  return record;
}

export function validateArchivePathSet(expectedPaths, archivePaths) {
  const expected = [...expectedPaths].sort((left, right) =>
    left.localeCompare(right, "en-GB")
  );
  const actual = [...archivePaths].sort((left, right) =>
    left.localeCompare(right, "en-GB")
  );
  if (expected.length !== actual.length) {
    throw new Error(
      `The source archive contains ${actual.length} files; ${expected.length} were expected.`
    );
  }
  for (let index = 0; index < expected.length; index += 1) {
    if (expected[index] !== actual[index]) {
      throw new Error(
        `The source archive path set differs at ${actual[index] ?? "<missing>"}.`
      );
    }
  }
  return actual;
}

export function inspectZipArchive(bytes, prefix) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 22 || bytes.length > maxArchiveBytes) {
    throw new Error("The source ZIP has an invalid bounded size.");
  }
  if (
    typeof prefix !== "string" ||
    !/^[a-z0-9][a-z0-9._-]*\/$/u.test(prefix)
  ) {
    throw new Error("The source ZIP prefix is unsafe.");
  }

  const eocdOffset = findEndOfCentralDirectory(bytes);
  const disk = bytes.readUInt16LE(eocdOffset + 4);
  const centralDisk = bytes.readUInt16LE(eocdOffset + 6);
  const entriesOnDisk = bytes.readUInt16LE(eocdOffset + 8);
  const totalEntries = bytes.readUInt16LE(eocdOffset + 10);
  const centralBytes = bytes.readUInt32LE(eocdOffset + 12);
  const centralOffset = bytes.readUInt32LE(eocdOffset + 16);
  const commentBytes = bytes.readUInt16LE(eocdOffset + 20);

  if (
    disk !== 0 ||
    centralDisk !== 0 ||
    entriesOnDisk !== totalEntries ||
    totalEntries === 0xffff ||
    centralBytes === 0xffffffff ||
    centralOffset === 0xffffffff
  ) {
    throw new Error("Multi-disk and ZIP64 source archives are not supported.");
  }
  if (
    eocdOffset + 22 + commentBytes !== bytes.length ||
    centralOffset + centralBytes !== eocdOffset
  ) {
    throw new Error("The source ZIP central directory bounds are invalid.");
  }

  const decoder = new TextDecoder("utf-8", { fatal: true });
  const files = new Map();
  let totalUncompressedBytes = 0;
  let offset = centralOffset;

  for (let index = 0; index < totalEntries; index += 1) {
    if (offset + 46 > eocdOffset || bytes.readUInt32LE(offset) !== 0x02014b50) {
      throw new Error("The source ZIP central directory is malformed.");
    }
    const flags = bytes.readUInt16LE(offset + 8);
    const method = bytes.readUInt16LE(offset + 10);
    const expectedCrc = bytes.readUInt32LE(offset + 16);
    const compressedBytes = bytes.readUInt32LE(offset + 20);
    const uncompressedBytes = bytes.readUInt32LE(offset + 24);
    const nameBytes = bytes.readUInt16LE(offset + 28);
    const extraBytes = bytes.readUInt16LE(offset + 30);
    const entryCommentBytes = bytes.readUInt16LE(offset + 32);
    const localOffset = bytes.readUInt32LE(offset + 42);
    const nextOffset = offset + 46 + nameBytes + extraBytes + entryCommentBytes;
    if (nextOffset > eocdOffset || (flags & 0x1) !== 0) {
      throw new Error("The source ZIP contains an invalid or encrypted entry.");
    }

    let archivePath;
    try {
      archivePath = decoder.decode(bytes.subarray(offset + 46, offset + 46 + nameBytes));
    } catch {
      throw new Error("The source ZIP contains a path that is not strict UTF-8.");
    }
    validateArchiveEntryPath(archivePath, prefix);

    if (archivePath.endsWith("/")) {
      if (compressedBytes !== 0 || uncompressedBytes !== 0) {
        throw new Error(`The source ZIP contains a non-empty directory: ${archivePath}.`);
      }
      offset = nextOffset;
      continue;
    }
    if (
      uncompressedBytes === 0 ||
      uncompressedBytes > maxFileBytes ||
      totalUncompressedBytes + uncompressedBytes > maxSourceBytes
    ) {
      throw new Error(`The source ZIP contains an invalid file size: ${archivePath}.`);
    }
    if (files.has(archivePath)) {
      throw new Error(`The source ZIP contains a duplicate file: ${archivePath}.`);
    }

    const content = readZipEntry(
      bytes,
      localOffset,
      flags,
      method,
      compressedBytes,
      uncompressedBytes
    );
    if (crc32(content) !== expectedCrc) {
      throw new Error(`The source ZIP failed its CRC check: ${archivePath}.`);
    }
    files.set(archivePath.slice(prefix.length), content);
    totalUncompressedBytes += content.length;
    offset = nextOffset;
  }
  if (offset !== eocdOffset) {
    throw new Error("The source ZIP central directory has trailing data.");
  }
  return files;
}

export async function createSourceArchive(workspace, outputArgument, epochValue) {
  const outputDirectory = resolveOutputDirectory(workspace, outputArgument);
  const epoch = parseSourceDateEpoch(epochValue);
  const audit = await auditSourceTree(workspace);
  assertIndexMatchesWorktree(workspace);
  const indexEntries = readIndexEntries(workspace);
  if (indexEntries.length !== audit.candidateFiles) {
    throw new Error("The audited source candidates do not match the Git index.");
  }

  const project = await readProjectReleaseVersions(workspace);
  const packageVersion = project.versions["npm package"];
  const release = validateReleaseVersionSet(
    project.versions,
    `v${packageVersion}`
  );
  validateWindowsMsiVersion(release.version, project.windowsMsiVersion);

  const treeId = runGit(workspace, ["write-tree"]).trim();
  if (!/^[0-9a-f]{40}(?:[0-9a-f]{24})?$/u.test(treeId)) {
    throw new Error("Git returned an invalid source tree object ID.");
  }

  const archiveBase = `tufekci-paperworks-${release.version}-source`;
  const prefix = `tufekci-paperworks-${release.version}/`;
  const archiveFilename = `${archiveBase}.zip`;
  const manifestFilename = `${archiveBase}.manifest.json`;
  const checksumFilename = `${archiveBase}.sha256`;
  await mkdir(outputDirectory, { recursive: true });

  const finalPaths = [
    path.join(outputDirectory, archiveFilename),
    path.join(outputDirectory, manifestFilename),
    path.join(outputDirectory, checksumFilename)
  ];
  await Promise.all(finalPaths.map(assertAbsent));
  const partialPaths = finalPaths.map(
    (candidate) => `${candidate}.partial-${process.pid}`
  );
  await Promise.all(partialPaths.map(assertAbsent));
  const published = [];

  try {
    runGit(workspace, [
      "archive",
      "--format=zip",
      `--prefix=${prefix}`,
      `--mtime=@${epoch}`,
      `--output=${partialPaths[0]}`,
      treeId
    ]);

    const archiveBytes = await readFile(partialPaths[0]);
    const archivedFiles = inspectZipArchive(archiveBytes, prefix);
    const sourcePaths = indexEntries.map((entry) => entry.path);
    validateArchivePathSet(sourcePaths, archivedFiles.keys());
    const fileManifest = [];
    for (const entry of indexEntries) {
      const archived = archivedFiles.get(entry.path);
      const working = await readFile(path.join(workspace, ...entry.path.split("/")));
      if (!archived || !archived.equals(working)) {
        throw new Error(`The source ZIP content differs from the index: ${entry.path}.`);
      }
      fileManifest.push({
        bytes: archived.length,
        path: entry.path,
        sha256: sha256(archived)
      });
    }

    assertIndexMatchesWorktree(workspace);
    if (runGit(workspace, ["write-tree"]).trim() !== treeId) {
      throw new Error("The Git index changed while the source archive was created.");
    }

    const archiveSha256 = sha256(archiveBytes);
    const manifest = {
      schemaVersion: 1,
      product: "Tüfekci Paperworks",
      version: release.version,
      tag: release.tag,
      sourceDateEpoch: epoch,
      generatedAt: new Date(epoch * 1_000).toISOString(),
      treeId,
      audit,
      archive: {
        bytes: archiveBytes.length,
        fileCount: fileManifest.length,
        filename: archiveFilename,
        sha256: archiveSha256
      },
      files: fileManifest
    };
    await writeFile(
      partialPaths[1],
      `${JSON.stringify(manifest, null, 2)}\n`,
      { encoding: "utf8", flag: "wx" }
    );
    await writeFile(
      partialPaths[2],
      `${archiveSha256}  ${archiveFilename}\n`,
      { encoding: "utf8", flag: "wx" }
    );

    for (let index = 0; index < finalPaths.length; index += 1) {
      await rename(partialPaths[index], finalPaths[index]);
      published.push(finalPaths[index]);
    }

    return {
      ...manifest.archive,
      checksumFilename,
      manifestFilename,
      outputDirectory,
      sourceDateEpoch: epoch,
      treeId
    };
  } catch (error) {
    await Promise.allSettled(
      [...partialPaths, ...published].map((candidate) =>
        rm(candidate, { force: true })
      )
    );
    throw error;
  }
}

function assertIndexMatchesWorktree(workspace) {
  const diff = spawnSync("git", ["diff", "--quiet", "--no-ext-diff", "--"], {
    cwd: workspace,
    windowsHide: true
  });
  if (diff.status === 1) {
    throw new Error("Stage every source change before creating the source archive.");
  }
  if (diff.status !== 0) {
    throw new Error("Git could not compare the source worktree with the index.");
  }
  const untracked = runGit(
    workspace,
    ["ls-files", "--others", "--exclude-standard", "-z"],
    null
  );
  if (untracked.length !== 0) {
    throw new Error("Stage every untracked source file before creating the source archive.");
  }
}

function readIndexEntries(workspace) {
  const output = runGit(workspace, ["ls-files", "--stage", "-z"]);
  const entries = output.split("\0").filter(Boolean).map((line) => {
    const match = /^(?<mode>[0-7]{6}) (?<objectId>[0-9a-f]{40}(?:[0-9a-f]{24})?) (?<stage>[0-3])\t(?<path>.+)$/u.exec(
      line
    );
    if (!match?.groups) {
      throw new Error("Git returned a malformed source index record.");
    }
    return validateIndexRecord({
      mode: match.groups.mode,
      objectId: match.groups.objectId,
      path: match.groups.path.replaceAll("\\", "/"),
      stage: Number(match.groups.stage)
    });
  });
  const paths = validateCandidatePaths(entries.map((entry) => entry.path));
  const byPath = new Map(entries.map((entry) => [entry.path, entry]));
  return paths.map((candidate) => byPath.get(candidate));
}

function resolveOutputDirectory(workspace, outputArgument) {
  const outputDirectory = path.resolve(
    workspace,
    outputArgument || path.join("artifacts", "source-release")
  );
  const relative = path.relative(workspace, outputDirectory).replaceAll("\\", "/");
  if (
    !relative ||
    relative === ".." ||
    relative.startsWith("../") ||
    path.isAbsolute(relative) ||
    (relative !== "artifacts" && !relative.startsWith("artifacts/"))
  ) {
    throw new Error("The source archive output must be inside the ignored artifacts directory.");
  }
  return outputDirectory;
}

function resolveEpoch(workspace) {
  if (process.env.SOURCE_DATE_EPOCH) {
    return process.env.SOURCE_DATE_EPOCH;
  }
  const result = spawnSync("git", ["log", "-1", "--format=%ct"], {
    cwd: workspace,
    encoding: "utf8",
    windowsHide: true
  });
  if (result.status === 0 && result.stdout.trim()) {
    return result.stdout.trim();
  }
  throw new Error(
    "Set SOURCE_DATE_EPOCH when creating an archive before the first Git commit."
  );
}

function runGit(workspace, arguments_, encoding = "utf8") {
  const result = spawnSync("git", arguments_, {
    cwd: workspace,
    encoding,
    maxBuffer: maxArchiveBytes,
    windowsHide: true
  });
  if (result.status !== 0) {
    const diagnostic =
      encoding === null
        ? "No textual diagnostic was retained."
        : boundedDiagnostic(result.stderr || result.stdout);
    throw new Error(
      `Git source-archive command failed: ${diagnostic || "No diagnostic was returned."}`
    );
  }
  return result.stdout;
}

function validateArchiveEntryPath(archivePath, prefix) {
  if (
    typeof archivePath !== "string" ||
    !archivePath.startsWith(prefix) ||
    /[\0\r\n\\]/u.test(archivePath)
  ) {
    throw new Error("The source ZIP contains an unsafe path.");
  }
  const relative = archivePath.slice(prefix.length);
  if (
    relative.startsWith("/") ||
    relative.split("/").some((part, index, parts) =>
      part === "." || part === ".." || (!part && index !== parts.length - 1)
    )
  ) {
    throw new Error(`The source ZIP contains an unsafe path: ${archivePath}.`);
  }
}

function readZipEntry(
  archive,
  localOffset,
  expectedFlags,
  expectedMethod,
  compressedBytes,
  uncompressedBytes
) {
  if (
    localOffset + 30 > archive.length ||
    archive.readUInt32LE(localOffset) !== 0x04034b50
  ) {
    throw new Error("The source ZIP contains an invalid local entry.");
  }
  const flags = archive.readUInt16LE(localOffset + 6);
  const method = archive.readUInt16LE(localOffset + 8);
  const nameBytes = archive.readUInt16LE(localOffset + 26);
  const extraBytes = archive.readUInt16LE(localOffset + 28);
  const contentOffset = localOffset + 30 + nameBytes + extraBytes;
  const contentEnd = contentOffset + compressedBytes;
  if (
    flags !== expectedFlags ||
    method !== expectedMethod ||
    contentEnd > archive.length
  ) {
    throw new Error("The source ZIP local and central entries disagree.");
  }
  const compressed = archive.subarray(contentOffset, contentEnd);
  let content;
  if (method === 0) {
    content = Buffer.from(compressed);
  } else if (method === 8) {
    content = inflateRawSync(compressed, { maxOutputLength: maxFileBytes });
  } else {
    throw new Error(`The source ZIP uses unsupported compression method ${method}.`);
  }
  if (content.length !== uncompressedBytes) {
    throw new Error("The source ZIP entry has an invalid uncompressed size.");
  }
  return content;
}

function findEndOfCentralDirectory(bytes) {
  const earliest = Math.max(0, bytes.length - 65_557);
  for (let offset = bytes.length - 22; offset >= earliest; offset -= 1) {
    if (bytes.readUInt32LE(offset) === 0x06054b50) {
      return offset;
    }
  }
  throw new Error("The source ZIP has no end-of-central-directory record.");
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex").toUpperCase();
}

async function assertAbsent(candidate) {
  try {
    await access(candidate);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return;
    }
    throw error;
  }
  throw new Error(`Source archive output already exists: ${path.basename(candidate)}.`);
}

function boundedDiagnostic(value) {
  const text = String(value).replace(/[\0\r]/gu, "").trim();
  return text.length <= 2_000
    ? text
    : `${text.slice(0, 2_000)}\n... diagnostic output truncated ...`;
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const result = await createSourceArchive(
    workspace,
    process.argv[2],
    resolveEpoch(workspace)
  );
  process.stdout.write(
    `Verified source archive ${result.filename} (${result.fileCount} files, ${result.bytes} bytes, SHA-256 ${result.sha256}; tree ${result.treeId}; SOURCE_DATE_EPOCH ${result.sourceDateEpoch}).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
