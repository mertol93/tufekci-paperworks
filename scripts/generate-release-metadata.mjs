import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { createReadStream, existsSync } from "node:fs";
import {
  mkdir,
  readFile,
  readdir,
  stat,
  writeFile
} from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const releaseSuffixes = [
  ".appimage",
  ".deb",
  ".dmg",
  ".exe",
  ".msi",
  ".pkg",
  ".rpm",
  ".sig",
  ".tar.gz",
  ".zip"
];

const generatedNames = new Set([
  "DEPENDENCY-LICENCES.csv",
  "DEPENDENCY-LICENCES.json",
  "RELEASE-MANIFEST.json",
  "SHA256SUMS",
  "sbom-cargo.cdx.json",
  "sbom-npm.cdx.json"
]);

export async function collectReleaseFiles(root) {
  const absoluteRoot = path.resolve(root);
  const files = [];

  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const absolutePath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(absolutePath);
      } else if (
        entry.isFile() &&
        !generatedNames.has(entry.name) &&
        releaseSuffixes.some((suffix) => entry.name.toLowerCase().endsWith(suffix))
      ) {
        files.push({
          absolutePath,
          fileName: entry.name,
          relativePath: normalisePath(path.relative(absoluteRoot, absolutePath))
        });
      }
    }
  }

  await visit(absoluteRoot);
  files.sort((left, right) => left.fileName.localeCompare(right.fileName, "en"));
  const names = new Set();
  for (const file of files) {
    if (/[\0\r\n]/.test(file.fileName)) {
      throw new Error("Release artefact filenames cannot contain control characters.");
    }
    const nameKey = file.fileName.normalize("NFC").toLocaleLowerCase("en-US");
    if (names.has(nameKey)) {
      throw new Error(
        `Release artefacts contain the duplicate filename ${file.fileName}. Rename the platform packages before creating checksums.`
      );
    }
    names.add(nameKey);
  }
  return files;
}

export async function buildReleaseManifest(files) {
  const artefacts = [];
  for (const file of files) {
    const metadata = await stat(file.absolutePath);
    artefacts.push({
      bytes: metadata.size,
      fileName: file.fileName,
      relativePath: file.relativePath,
      sha256: await sha256File(file.absolutePath)
    });
  }
  return artefacts;
}

export function formatSha256Sums(artefacts) {
  return `${artefacts
    .map((artefact) => `${artefact.sha256}  ${artefact.fileName}`)
    .join("\n")}\n`;
}

export function buildCargoSbom(metadata, project, generatedAt, lockDigest) {
  const packageById = new Map(metadata.packages.map((item) => [item.id, item]));
  const rootId = metadata.resolve?.root;
  const rootPackage = packageById.get(rootId) ?? metadata.packages.find(
    (item) => item.name === project.name && item.version === project.version
  );
  const rootRef = genericProjectPurl(project);
  const componentRefs = new Map();
  const components = metadata.packages
    .filter((item) => item.id !== rootPackage?.id)
    .map((item) => {
      const bomRef = cargoPurl(item.name, item.version);
      componentRefs.set(item.id, bomRef);
      return compactObject({
        "bom-ref": bomRef,
        type: "library",
        name: item.name,
        version: item.version,
        purl: bomRef,
        licenses: licenceChoice(item.license),
        externalReferences: cargoExternalReferences(item)
      });
    })
    .sort(componentSort);

  const dependencies = [];
  if (metadata.resolve?.nodes) {
    for (const node of metadata.resolve.nodes) {
      const ref = node.id === rootId ? rootRef : componentRefs.get(node.id);
      if (!ref) {
        continue;
      }
      const dependsOn = node.dependencies
        .map((id) => (id === rootId ? rootRef : componentRefs.get(id)))
        .filter(Boolean)
        .sort();
      dependencies.push({ ref, dependsOn });
    }
  }
  dependencies.sort((left, right) => left.ref.localeCompare(right.ref, "en"));

  return {
    $schema: "https://cyclonedx.org/schema/bom-1.5.schema.json",
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    serialNumber: deterministicSerialNumber(`cargo:${project.name}:${project.version}:${lockDigest}`),
    version: 1,
    metadata: {
      timestamp: generatedAt,
      tools: {
        components: [
          {
            type: "application",
            author: "Tüfekci Paperworks contributors",
            name: "generate-release-metadata.mjs",
            version: project.version
          }
        ]
      },
      component: {
        "bom-ref": rootRef,
        type: "application",
        name: project.name,
        version: project.version,
        purl: rootRef,
        licenses: licenceChoice(project.license)
      }
    },
    components,
    dependencies
  };
}

export function normaliseNpmSbom(sbom, project, generatedAt, lockDigest) {
  const normalised = structuredClone(sbom);
  normalised.serialNumber = deterministicSerialNumber(
    `npm:${project.name}:${project.version}:${lockDigest}`
  );
  normalised.version = 1;
  normalised.metadata = normalised.metadata ?? {};
  normalised.metadata.timestamp = generatedAt;
  if (Array.isArray(normalised.components)) {
    normalised.components.sort(componentSort);
  }
  if (Array.isArray(normalised.dependencies)) {
    normalised.dependencies.sort((left, right) =>
      String(left.ref).localeCompare(String(right.ref), "en")
    );
    for (const dependency of normalised.dependencies) {
      if (Array.isArray(dependency.dependsOn)) {
        dependency.dependsOn.sort();
      }
    }
  }
  return normalised;
}

export function buildLicenceReport(packageLock, cargoMetadata, project, generatedAt) {
  const dependencies = [];
  const npmRows = new Map();
  for (const [packagePath, item] of Object.entries(packageLock.packages ?? {})) {
    if (!packagePath || !item.version) {
      continue;
    }
    const name = npmPackageName(packagePath, item.name);
    const key = `npm:${name}:${item.version}`;
    const row = licenceRow({
      ecosystem: "npm",
      name,
      version: item.version,
      licence: item.license,
      developmentOnly: Boolean(item.dev),
      source: item.resolved ?? null
    });
    const existing = npmRows.get(key);
    if (existing) {
      existing.developmentOnly = existing.developmentOnly && row.developmentOnly;
      existing.source ??= row.source;
    } else {
      npmRows.set(key, row);
    }
  }
  dependencies.push(...npmRows.values());

  const rootId = cargoMetadata.resolve?.root;
  for (const item of cargoMetadata.packages ?? []) {
    if (item.id === rootId || (item.name === project.name && item.version === project.version)) {
      continue;
    }
    dependencies.push(licenceRow({
      ecosystem: "Cargo",
      name: item.name,
      version: item.version,
      licence: item.license,
      developmentOnly: false,
      source: item.source ?? null
    }));
  }
  dependencies.sort((left, right) =>
    `${left.ecosystem}:${left.name}:${left.version}`.localeCompare(
      `${right.ecosystem}:${right.name}:${right.version}`,
      "en"
    )
  );

  const reviewRequired = dependencies.filter((item) => item.reviewRequired).length;
  return {
    schemaVersion: 1,
    generatedAt,
    project: {
      name: project.name,
      version: project.version,
      licence: project.license
    },
    summary: {
      cargoDependencies: dependencies.filter((item) => item.ecosystem === "Cargo").length,
      npmDependencies: dependencies.filter((item) => item.ecosystem === "npm").length,
      reviewRequired,
      totalDependencies: dependencies.length
    },
    notice:
      "This is an automated declaration inventory, not legal advice or a compatibility verdict. Review every flagged or changed dependency before release.",
    dependencies
  };
}

export function formatLicenceCsv(report) {
  const header = [
    "ecosystem",
    "name",
    "version",
    "licence",
    "developmentOnly",
    "reviewRequired",
    "reviewReason",
    "source"
  ];
  const rows = report.dependencies.map((item) =>
    header.map((key) => csvCell(item[key] ?? "")).join(",")
  );
  return `${header.join(",")}\n${rows.join("\n")}\n`;
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const artefactRoot = path.resolve(process.argv[2] ?? path.join(workspace, "release-assets"));
  const outputRoot = path.resolve(process.argv[3] ?? path.join(workspace, "release-metadata"));
  if (!existsSync(artefactRoot)) {
    throw new Error(`Release artefact directory does not exist: ${artefactRoot}`);
  }

  const packageJson = JSON.parse(await readFile(path.join(workspace, "package.json"), "utf8"));
  const packageLockPath = path.join(workspace, "package-lock.json");
  const cargoLockPath = path.join(workspace, "src-tauri", "Cargo.lock");
  const packageLock = JSON.parse(await readFile(packageLockPath, "utf8"));
  const project = {
    name: packageJson.name,
    version: packageJson.version,
    license: packageJson.license
  };
  const generatedAt = reproducibleTimestamp();
  const cargoLockDigest = await sha256File(cargoLockPath);
  const packageLockDigest = await sha256File(packageLockPath);
  const cargoMetadata = runJsonCommand(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--locked",
      "--manifest-path",
      path.join(workspace, "src-tauri", "Cargo.toml")
    ],
    workspace
  );
  const npmSbom = normaliseNpmSbom(
    runNpmSbom(workspace),
    project,
    generatedAt,
    packageLockDigest
  );
  const cargoSbom = buildCargoSbom(cargoMetadata, project, generatedAt, cargoLockDigest);
  const licenceReport = buildLicenceReport(packageLock, cargoMetadata, project, generatedAt);
  const releaseFiles = await collectReleaseFiles(artefactRoot);
  if (releaseFiles.length === 0) {
    throw new Error(`No distributable release artefacts were found under ${artefactRoot}.`);
  }
  const artefacts = await buildReleaseManifest(releaseFiles);

  await mkdir(outputRoot, { recursive: true });
  await writeJson(path.join(outputRoot, "sbom-npm.cdx.json"), npmSbom);
  await writeJson(path.join(outputRoot, "sbom-cargo.cdx.json"), cargoSbom);
  await writeJson(path.join(outputRoot, "DEPENDENCY-LICENCES.json"), licenceReport);
  await writeFile(
    path.join(outputRoot, "DEPENDENCY-LICENCES.csv"),
    formatLicenceCsv(licenceReport),
    "utf8"
  );
  await writeJson(path.join(outputRoot, "RELEASE-MANIFEST.json"), {
    schemaVersion: 1,
    generatedAt,
    project,
    artefacts
  });
  await writeFile(path.join(outputRoot, "SHA256SUMS"), formatSha256Sums(artefacts), "utf8");

  process.stdout.write(
    `Created release metadata for ${artefacts.length} artefact(s) and ${licenceReport.summary.totalDependencies} dependencies. ${licenceReport.summary.reviewRequired} licence declaration(s) require review.\n`
  );
}

function runNpmSbom(workspace) {
  const npmExecPath = process.env.npm_execpath;
  if (!npmExecPath) {
    throw new Error("Run release metadata through npm so its SBOM command can be located safely.");
  }
  return runJsonCommand(
    process.execPath,
    [
      npmExecPath,
      "sbom",
      "--package-lock-only",
      "--sbom-format",
      "cyclonedx",
      "--sbom-type",
      "application"
    ],
    workspace
  );
}

function runJsonCommand(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    shell: false,
    windowsHide: true
  });
  if (result.error) {
    throw new Error(`${command} could not start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = truncateDiagnostic(
      (result.stderr || result.stdout || "No diagnostic output was returned.").trim()
    );
    throw new Error(`${command} failed while generating release metadata: ${detail}`);
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`${command} returned invalid JSON: ${error instanceof Error ? error.message : error}`);
  }
}

function licenceRow({ ecosystem, name, version, licence, developmentOnly, source }) {
  const normalisedLicence = typeof licence === "string" ? licence.trim() : "";
  const opaque = /^(UNLICENSED|SEE LICEN[CS]E|UNKNOWN|NONE)/i.test(normalisedLicence)
    || /LicenseRef-/i.test(normalisedLicence);
  const reviewRequired = !normalisedLicence || opaque;
  return {
    ecosystem,
    name,
    version,
    licence: normalisedLicence || null,
    developmentOnly,
    reviewRequired,
    reviewReason: !normalisedLicence
      ? "No licence declaration was available in dependency metadata."
      : opaque
        ? "The licence declaration is non-standard or refers to separate terms."
        : null,
    source: sanitiseSource(source)
  };
}

function licenceChoice(licence) {
  return typeof licence === "string" && licence.trim()
    ? [{ expression: licence.trim() }]
    : undefined;
}

function cargoExternalReferences(item) {
  const references = [];
  if (item.repository) {
    references.push({ type: "vcs", url: item.repository });
  }
  if (item.homepage && item.homepage !== item.repository) {
    references.push({ type: "website", url: item.homepage });
  }
  return references.length > 0 ? references : undefined;
}

function cargoPurl(name, version) {
  return `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

function genericProjectPurl(project) {
  return `pkg:generic/${encodeURIComponent(project.name)}@${encodeURIComponent(project.version)}`;
}

function deterministicSerialNumber(seed) {
  const value = createHash("sha256").update(seed).digest("hex").slice(0, 32).split("");
  value[12] = "5";
  value[16] = ["8", "9", "a", "b"][Number.parseInt(value[16], 16) % 4];
  const hex = value.join("");
  return `urn:uuid:${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function componentSort(left, right) {
  return `${left.name}:${left.version}`.localeCompare(`${right.name}:${right.version}`, "en");
}

function compactObject(object) {
  return Object.fromEntries(Object.entries(object).filter(([, value]) => value !== undefined));
}

function npmPackageName(packagePath, declaredName) {
  if (declaredName) {
    return declaredName;
  }
  const marker = "node_modules/";
  const index = packagePath.lastIndexOf(marker);
  return index >= 0 ? packagePath.slice(index + marker.length) : packagePath;
}

function sanitiseSource(source) {
  if (typeof source !== "string" || !source) {
    return null;
  }
  if (/^file:/i.test(source)) {
    return "local dependency";
  }
  const prefix = source.match(/^(git\+|registry\+)/i)?.[0] ?? "";
  const candidate = prefix ? source.slice(prefix.length) : source;
  try {
    const url = new URL(candidate);
    url.username = "";
    url.password = "";
    url.search = "";
    url.hash = "";
    return `${prefix}${url.toString()}`;
  } catch {
    return source.replace(/\/\/[^/@\s]+@/g, "//[credentials]@");
  }
}

function truncateDiagnostic(value) {
  const maximum = 16 * 1024;
  return value.length <= maximum
    ? value
    : `${value.slice(0, maximum)}\n... diagnostic output truncated ...`;
}

function csvCell(value) {
  const text = String(value);
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function reproducibleTimestamp() {
  const sourceDateEpoch = Number(process.env.SOURCE_DATE_EPOCH);
  if (Number.isFinite(sourceDateEpoch) && sourceDateEpoch >= 0) {
    return new Date(sourceDateEpoch * 1000).toISOString();
  }
  return new Date().toISOString();
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  await new Promise((resolve, reject) => {
    const input = createReadStream(filePath);
    input.on("data", (chunk) => hash.update(chunk));
    input.on("error", reject);
    input.on("end", resolve);
  });
  return hash.digest("hex");
}

function normalisePath(value) {
  return value.split(path.sep).join("/");
}

async function writeJson(filePath, value) {
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
