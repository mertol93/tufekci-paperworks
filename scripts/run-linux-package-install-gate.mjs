import { spawnSync } from "node:child_process";
import { chmod, lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { classifyBundleFilename, versionEquivalent } from "./check-release-bundles.mjs";

const marker = "PAPERWORKS_LINUX_INSTALL_V1";
const product = "Tüfekci Paperworks";
const packageName = "tufekci-paperworks";
const maximumOutputBytes = 16 * 1024 * 1024;
const cases = Object.freeze([
  Object.freeze({
    containerImage: "ubuntu:22.04",
    distribution: "ubuntu",
    distributionVersion: "22.04",
    format: "appimage",
    verification: "extracted-on-baseline"
  }),
  Object.freeze({
    containerImage: "debian:13-slim",
    distribution: "debian",
    distributionVersion: "13",
    format: "deb",
    verification: "installed-and-linked"
  }),
  Object.freeze({
    containerImage: "fedora:43",
    distribution: "fedora",
    distributionVersion: "43",
    format: "rpm",
    verification: "installed-and-linked"
  })
]);

export function parseLinuxInstallMarker(output, expected) {
  if (typeof output !== "string" || Buffer.byteLength(output, "utf8") > maximumOutputBytes) {
    throw new Error("Linux package-install output is missing or exceeds its safety limit.");
  }
  const records = output
    .split(/\r?\n/u)
    .filter((line) => line.startsWith(`${marker}\t`));
  if (records.length !== 1) {
    throw new Error("The Linux package-install test did not emit exactly one evidence marker.");
  }
  const fields = records[0].split("\t");
  if (
    fields.length !== 7 ||
    fields[0] !== marker ||
    fields[1] !== expected.distribution ||
    fields[2] !== expected.distributionVersion ||
    fields[3] !== expected.format ||
    fields[4].length < 1 ||
    fields[4].length > 80 ||
    fields[5] !== "x64" ||
    fields[6] !== expected.verification
  ) {
    throw new Error(`The ${expected.distribution} package-install evidence marker is invalid.`);
  }
  return {
    architecture: fields[5],
    containerImage: expected.containerImage,
    containerImageId: null,
    distribution: fields[1],
    distributionVersion: fields[2],
    format: fields[3],
    packageVersion: fields[4],
    verification: fields[6]
  };
}

export function validateLinuxInstallReport(value) {
  requireExactFields(
    value,
    ["architecture", "cases", "platform", "product", "releaseVersion", "schemaVersion"],
    "Linux package-install report"
  );
  if (
    value.schemaVersion !== 1 ||
    value.product !== product ||
    value.platform !== "linux" ||
    value.architecture !== "x64" ||
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(value.releaseVersion) ||
    !Array.isArray(value.cases) ||
    value.cases.length !== cases.length
  ) {
    throw new Error("The Linux package-install report identity is invalid.");
  }
  const seen = new Set();
  for (const entry of value.cases) {
    requireExactFields(
      entry,
      [
        "architecture",
        "containerImage",
        "containerImageId",
        "distribution",
        "distributionVersion",
        "format",
        "packageVersion",
        "verification"
      ],
      "Linux package-install case"
    );
    const expected = cases.find((candidate) => candidate.distribution === entry.distribution);
    if (
      !expected ||
      seen.has(entry.distribution) ||
      entry.architecture !== "x64" ||
      entry.containerImage !== expected.containerImage ||
      entry.distributionVersion !== expected.distributionVersion ||
      entry.format !== expected.format ||
      entry.verification !== expected.verification ||
      !/^[0-9A-F]{64}$/u.test(entry.containerImageId) ||
      !versionEquivalent(entry.packageVersion, value.releaseVersion)
    ) {
      throw new Error(`The Linux package-install case is invalid for ${entry?.distribution ?? "unknown"}.`);
    }
    seen.add(entry.distribution);
  }
  return value;
}

export async function runLinuxPackageInstallGate(workspace, bundleRoot, evidenceDirectory) {
  if (process.platform !== "linux" || process.arch !== "x64") {
    throw new Error("The Linux package-install gate must run on an x64 Linux host.");
  }
  const packageJson = JSON.parse(await readFile(path.join(workspace, "package.json"), "utf8"));
  const bundles = await findLinuxBundles(bundleRoot, packageJson.version);
  await chmod(bundles.find((entry) => entry.format === "appimage").absolutePath, 0o755);
  runCommand("docker", ["version", "--format", "{{.Server.Version}}"], "Docker readiness", 60_000);
  const reports = [];
  for (const expected of cases) {
    const script = containerScript(expected.format);
    const result = runCommand(
      "docker",
      [
        "run",
        "--rm",
        "--network",
        "bridge",
        "--mount",
        `type=bind,source=${path.resolve(bundleRoot)},target=/packages,readonly`,
        "--env",
        `PAPERWORKS_VERSION=${packageJson.version}`,
        expected.containerImage,
        expected.distribution === "fedora" ? "bash" : "sh",
        "-euc",
        script
      ],
      `${expected.distribution} ${expected.format} installation test`,
      20 * 60_000
    );
    const entry = parseLinuxInstallMarker(`${result.stdout}\n${result.stderr}`, expected);
    const imageId = runCommand(
      "docker",
      ["image", "inspect", "--format", "{{.Id}}", expected.containerImage],
      `${expected.distribution} container identity`,
      60_000
    ).stdout.trim();
    if (!/^sha256:[0-9a-f]{64}$/u.test(imageId)) {
      throw new Error(`The ${expected.distribution} container image identity is invalid.`);
    }
    entry.containerImageId = imageId.slice("sha256:".length).toUpperCase();
    if (!versionEquivalent(entry.packageVersion, packageJson.version)) {
      throw new Error(`The ${expected.distribution} package version does not match the release.`);
    }
    reports.push(entry);
  }
  if (bundles.length !== 3) throw new Error("The Linux package inventory changed during installation testing.");
  const report = {
    schemaVersion: 1,
    product,
    releaseVersion: packageJson.version,
    platform: "linux",
    architecture: "x64",
    cases: reports.sort((left, right) => left.distribution.localeCompare(right.distribution, "en-GB"))
  };
  validateLinuxInstallReport(report);
  await mkdir(evidenceDirectory, { recursive: true });
  const reportPath = path.join(evidenceDirectory, "linux-install-report-x64.json");
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  validateLinuxInstallReport(JSON.parse(await readFile(reportPath, "utf8")));
  return report;
}

async function findLinuxBundles(root, releaseVersion) {
  const found = [];
  let count = 0;
  async function visit(directory, depth) {
    if (depth > 8) throw new Error("The Linux bundle tree is too deeply nested.");
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      count += 1;
      if (count > 1_024) throw new Error("The Linux bundle tree exceeds its entry limit.");
      const absolutePath = path.join(directory, entry.name);
      const metadata = await lstat(absolutePath);
      if (metadata.isSymbolicLink()) {
        if (classifyBundleFilename(entry.name)) {
          throw new Error("A Linux package candidate must not be a symbolic link.");
        }
        continue;
      }
      if (metadata.isDirectory()) await visit(absolutePath, depth + 1);
      else {
        const format = classifyBundleFilename(entry.name);
        if (["appimage", "deb", "rpm"].includes(format)) {
          if (!entry.name.normalize("NFC").includes(releaseVersion)) {
            throw new Error("A Linux package filename does not include the release version.");
          }
          found.push({ absolutePath, format, fileName: entry.name });
        }
      }
    }
  }
  await visit(path.resolve(root), 0);
  if (found.length !== 3 || new Set(found.map((entry) => entry.format)).size !== 3) {
    throw new Error("Linux distribution testing requires exactly one AppImage, deb and rpm package.");
  }
  return found;
}

function containerScript(format) {
  if (format === "appimage") {
    return String.raw`
apt-get update >/dev/null
apt-get install -y ca-certificates >/dev/null
mkdir -p /work
cd /work
package="$(find /packages -type f -name '*.AppImage' -print -quit)"
test -n "$package"
"$package" --appimage-extract >/dev/null
test -x squashfs-root/AppRun
test -x squashfs-root/usr/bin/tufekci-paperworks
id="$(. /etc/os-release; printf '%s' "$ID")"
version="$(. /etc/os-release; printf '%s' "$VERSION_ID")"
test "$id" = ubuntu
test "$version" = 22.04
printf 'PAPERWORKS_LINUX_INSTALL_V1\tubuntu\t22.04\tappimage\t%s\tx64\textracted-on-baseline\n' "$PAPERWORKS_VERSION"
`;
  }
  if (format === "deb") {
    return String.raw`
export DEBIAN_FRONTEND=noninteractive
apt-get update >/dev/null
package="$(find /packages -type f -name '*.deb' -print -quit)"
test -n "$package"
apt-get install -y "$package" >/dev/null
test -x /usr/bin/tufekci-paperworks
! ldd /usr/bin/tufekci-paperworks | grep -q 'not found'
id="$(. /etc/os-release; printf '%s' "$ID")"
version="$(. /etc/os-release; printf '%s' "$VERSION_ID")"
test "$id" = debian
test "$version" = 13
package_version="$(dpkg-query -W -f='${"${Version}"}' tufekci-paperworks)"
printf 'PAPERWORKS_LINUX_INSTALL_V1\tdebian\t13\tdeb\t%s\tx64\tinstalled-and-linked\n' "$package_version"
`;
  }
  return String.raw`
package="$(find /packages -type f -name '*.rpm' -print -quit)"
test -n "$package"
dnf install -y "$package" >/dev/null
test -x /usr/bin/tufekci-paperworks
! ldd /usr/bin/tufekci-paperworks | grep -q 'not found'
id="$(. /etc/os-release; printf '%s' "$ID")"
version="$(. /etc/os-release; printf '%s' "$VERSION_ID")"
test "$id" = fedora
test "$version" = 43
package_version="$(rpm -q --queryformat '%{VERSION}-%{RELEASE}' tufekci-paperworks)"
printf 'PAPERWORKS_LINUX_INSTALL_V1\tfedora\t43\trpm\t%s\tx64\tinstalled-and-linked\n' "$package_version"
`;
}

function runCommand(command, args, label, timeout) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: maximumOutputBytes,
    shell: false,
    timeout,
    windowsHide: true
  });
  if (result.error) throw new Error(`${label} could not start: ${bounded(result.error.message)}`);
  if (result.status !== 0) {
    throw new Error(`${label} failed: ${bounded((result.stderr || result.stdout || "No diagnostic was returned.").trim())}`);
  }
  return { stderr: result.stderr ?? "", stdout: result.stdout ?? "" };
}

function bounded(value) {
  const text = String(value).replace(/[\0\r]/gu, "").trim();
  return text.length <= 4_096 ? text : `${text.slice(0, 4_096)} ...`;
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

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const bundleRoot = path.resolve(workspace, process.argv[2] ?? "release-linux-assets");
  const evidenceDirectory = path.resolve(workspace, process.argv[3] ?? "release-linux-install-evidence");
  runLinuxPackageInstallGate(workspace, bundleRoot, evidenceDirectory)
    .then((report) => {
      process.stdout.write(`Verified Linux package installation on ${report.cases.length} supported distribution baselines.\n`);
    })
    .catch((error) => {
      process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
      process.exitCode = 1;
    });
}
