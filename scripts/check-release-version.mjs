import { appendFile, readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const prereleaseIdentifier =
  "(?:0|[1-9]\\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)";
const semverPattern = new RegExp(
  `^(?<major>0|[1-9]\\d*)\\.(?<minor>0|[1-9]\\d*)\\.(?<patch>0|[1-9]\\d*)(?:-(?<prerelease>${prereleaseIdentifier}(?:\\.${prereleaseIdentifier})*))?(?:\\+(?<build>[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*))?$`,
  "u"
);

export function validateReleaseVersionSet(versions, tag) {
  const entries = Object.entries(versions);
  if (entries.length === 0) {
    throw new Error("No project versions were supplied for release validation.");
  }

  for (const [source, version] of entries) {
    if (typeof version !== "string" || !semverPattern.test(version)) {
      throw new Error(`${source} does not contain a valid semantic version.`);
    }
  }

  const uniqueVersions = new Set(entries.map(([, version]) => version));
  if (uniqueVersions.size !== 1) {
    throw new Error(
      `Release versions do not match: ${entries
        .map(([source, version]) => `${source}=${version}`)
        .join(", ")}.`
    );
  }

  const version = entries[0][1];
  const expectedTag = `v${version}`;
  if (typeof tag !== "string" || /[\0\r\n]/u.test(tag) || tag !== expectedTag) {
    throw new Error(`Release tag must be exactly ${expectedTag}.`);
  }

  const match = semverPattern.exec(version);
  return {
    channel: releaseUpdateChannel(version),
    prerelease: Boolean(match?.groups?.prerelease),
    tag: expectedTag,
    version
  };
}

export function releaseUpdateChannel(version) {
  const match = typeof version === "string" ? semverPattern.exec(version) : null;
  if (!match) {
    throw new Error("The updater channel requires a valid semantic version.");
  }
  const prerelease = match.groups?.prerelease;
  if (!prerelease) {
    return "stable";
  }
  const label = prerelease.split(".")[0].toLowerCase();
  if (label === "alpha") {
    return "alpha";
  }
  if (label === "beta" || label === "rc") {
    return "beta";
  }
  throw new Error("A pre-release updater version must begin with alpha, beta, or rc.");
}

export function validateWindowsMsiVersion(version, msiVersion) {
  const match = semverPattern.exec(version);
  if (!match) {
    throw new Error("The application version is not valid semantic versioning.");
  }
  const { major, minor, patch, prerelease } = match.groups;
  const numericParts = [major, minor, patch].map(Number);
  if (numericParts[0] > 255 || numericParts[1] > 255 || numericParts[2] > 65_535) {
    throw new Error("The application version is outside Windows Installer version limits.");
  }

  let expected = `${major}.${minor}.${patch}`;
  if (prerelease) {
    const sequence = prerelease.split(".").at(-1);
    if (!sequence || !/^(0|[1-9]\d*)$/u.test(sequence) || Number(sequence) > 65_535) {
      throw new Error(
        "A pre-release must end with a numeric Windows Installer sequence from 0 to 65,535."
      );
    }
    expected = `${expected}.${sequence}`;
  }
  if (msiVersion !== expected) {
    throw new Error(`Windows MSI version must be exactly ${expected}.`);
  }
  return expected;
}

export async function readProjectReleaseVersions(workspace) {
  const packageJson = JSON.parse(await readFile(path.join(workspace, "package.json"), "utf8"));
  const packageLock = JSON.parse(
    await readFile(path.join(workspace, "package-lock.json"), "utf8")
  );
  const tauriConfig = JSON.parse(
    await readFile(path.join(workspace, "src-tauri", "tauri.conf.json"), "utf8")
  );
  const cargoMetadata = runCargoMetadata(workspace);
  const cargoPackage = cargoMetadata.packages.find(
    (candidate) => candidate.name === "tufekci-paperworks"
  );
  if (!cargoPackage) {
    throw new Error("Cargo metadata does not contain the Tüfekci Paperworks package.");
  }

  return {
    versions: {
      "Cargo package": cargoPackage.version,
      "npm lock root": packageLock.packages?.[""]?.version,
      "npm lockfile": packageLock.version,
      "npm package": packageJson.version,
      "Tauri config": tauriConfig.version
    },
    windowsMsiVersion: tauriConfig.bundle?.windows?.wix?.version
  };
}

function runCargoMetadata(workspace) {
  const result = spawnSync(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--locked",
      "--manifest-path",
      path.join(workspace, "src-tauri", "Cargo.toml"),
      "--no-deps"
    ],
    {
      cwd: workspace,
      encoding: "utf8",
      windowsHide: true
    }
  );
  if (result.status !== 0) {
    throw new Error(
      `Cargo release metadata could not be read: ${boundedDiagnostic(
        result.stderr || result.stdout || "No diagnostic output was returned."
      )}`
    );
  }
  try {
    return JSON.parse(result.stdout);
  } catch {
    throw new Error("Cargo release metadata was not valid JSON.");
  }
}

function boundedDiagnostic(value) {
  const normalised = String(value).replace(/[\0\r]/gu, "").trim();
  return normalised.length <= 2_000
    ? normalised
    : `${normalised.slice(0, 2_000)}\n... diagnostic output truncated ...`;
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME;
  const project = await readProjectReleaseVersions(workspace);
  const result = validateReleaseVersionSet(project.versions, tag);
  const windowsMsiVersion = validateWindowsMsiVersion(
    result.version,
    project.windowsMsiVersion
  );
  const releaseKind = result.prerelease ? "pre-release" : "stable release";
  process.stdout.write(
    `Release version contract passed for ${result.tag} (${releaseKind}; Windows MSI ${windowsMsiVersion}).\n`
  );

  if (process.env.GITHUB_OUTPUT) {
    await appendFile(
      process.env.GITHUB_OUTPUT,
      `version=${result.version}\nprerelease=${result.prerelease}\nchannel=${result.channel}\n`,
      "utf8"
    );
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
