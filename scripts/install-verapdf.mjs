import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFile, chmod, lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const veraPdfRelease = Object.freeze({
  archiveBytes: 32_923_960,
  archiveSha256: "6CC6341CB1AF644044054B81F00A6590A7918ABB18F762243DE115258BCAD838",
  archiveUrl: "https://software.verapdf.org/rel/1.30/verapdf-greenfield-1.30.2-installer.zip",
  version: "1.30.2"
});

export function createAutomatedInstallation(installPath) {
  if (
    typeof installPath !== "string" ||
    installPath.length === 0 ||
    installPath.length > 2_048 ||
    /[\0\r\n]/u.test(installPath)
  ) {
    throw new Error("The veraPDF installation path is invalid.");
  }
  const escaped = installPath
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
  return `<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<AutomatedInstallation langpack="eng">
  <com.izforge.izpack.panels.htmlhello.HTMLHelloPanel id="welcome"/>
  <com.izforge.izpack.panels.target.TargetPanel id="install_dir">
    <installpath>${escaped}</installpath>
  </com.izforge.izpack.panels.target.TargetPanel>
  <com.izforge.izpack.panels.packs.PacksPanel id="sdk_pack_select">
    <pack index="0" name="veraPDF GUI" selected="false"/>
    <pack index="1" name="veraPDF CLI" selected="true"/>
    <pack index="2" name="veraPDF Documentation" selected="false"/>
    <pack index="3" name="veraPDF Sample Plugins" selected="false"/>
  </com.izforge.izpack.panels.packs.PacksPanel>
  <com.izforge.izpack.panels.install.InstallPanel id="install"/>
  <com.izforge.izpack.panels.finish.FinishPanel id="finish"/>
</AutomatedInstallation>
`;
}

export function verifyVeraPdfArchive(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length !== veraPdfRelease.archiveBytes) {
    throw new Error("The veraPDF installer size does not match the pinned release.");
  }
  const digest = createHash("sha256").update(bytes).digest("hex").toUpperCase();
  if (digest !== veraPdfRelease.archiveSha256) {
    throw new Error("The veraPDF installer does not match its pinned SHA-256 digest.");
  }
  return digest;
}

export async function installVeraPdf() {
  const root = path.join(
    process.env.RUNNER_TEMP || os.tmpdir(),
    `paperworks-verapdf-${veraPdfRelease.version}-${process.pid}-${Date.now()}`
  );
  const unpacked = path.join(root, "unpacked");
  const installPath = path.join(root, "installed");
  await mkdir(unpacked, { recursive: true });

  const configuredArchive = process.env.PAPERWORKS_VERAPDF_INSTALLER;
  let archiveBytes;
  if (configuredArchive) {
    const metadata = await lstat(configuredArchive);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error("PAPERWORKS_VERAPDF_INSTALLER must name an ordinary installer archive.");
    }
    archiveBytes = await readFile(configuredArchive);
  } else {
    const response = await fetch(veraPdfRelease.archiveUrl, {
      redirect: "follow",
      signal: AbortSignal.timeout(180_000)
    });
    if (!response.ok) {
      throw new Error(`The pinned veraPDF installer download returned HTTP ${response.status}.`);
    }
    archiveBytes = Buffer.from(await response.arrayBuffer());
  }
  verifyVeraPdfArchive(archiveBytes);
  const archivePath = path.join(root, "verapdf-installer.zip");
  await writeFile(archivePath, archiveBytes, { flag: "wx" });
  runCommand("tar", ["-xf", archivePath, "-C", unpacked], "veraPDF installer extraction", 60_000);

  const installerDirectory = path.join(
    unpacked,
    `verapdf-greenfield-${veraPdfRelease.version}`
  );
  const installerJar = path.join(
    installerDirectory,
    `verapdf-izpack-installer-${veraPdfRelease.version}.jar`
  );
  const installerMetadata = await lstat(installerJar);
  if (!installerMetadata.isFile() || installerMetadata.isSymbolicLink()) {
    throw new Error("The verified veraPDF archive did not contain the expected installer JAR.");
  }
  const configPath = path.join(root, "auto-install.xml");
  await writeFile(configPath, createAutomatedInstallation(installPath), {
    encoding: "utf8",
    flag: "wx"
  });
  runCommand("java", ["-jar", installerJar, configPath], "veraPDF unattended installation", 180_000);

  const launcher = path.join(installPath, process.platform === "win32" ? "verapdf.bat" : "verapdf");
  const launcherMetadata = await lstat(launcher);
  if (!launcherMetadata.isFile() || launcherMetadata.isSymbolicLink()) {
    throw new Error("The veraPDF installation did not create the expected CLI launcher.");
  }
  if (process.platform !== "win32") {
    await chmod(launcher, 0o755);
  }

  if (process.env.GITHUB_ENV) {
    await appendFile(process.env.GITHUB_ENV, `PAPERWORKS_VERAPDF=${launcher}\n`, "utf8");
  }
  if (process.env.GITHUB_PATH) {
    await appendFile(process.env.GITHUB_PATH, `${installPath}\n`, "utf8");
  }
  return { installPath, launcher, version: veraPdfRelease.version };
}

function runCommand(command, args, label, timeout) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
    timeout,
    windowsHide: true
  });
  if (result.error) {
    throw new Error(`${label} could not start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    throw new Error(`${label} failed with exit code ${result.status}.`);
  }
}

async function main() {
  const installed = await installVeraPdf();
  process.stdout.write(`Installed verified veraPDF ${installed.version} for release evidence.\n`);
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
