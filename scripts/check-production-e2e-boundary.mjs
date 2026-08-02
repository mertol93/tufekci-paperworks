import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const maximumTextBytes = 12 * 1024 * 1024;
const inspectedExtensions = new Set([".html", ".js", ".mjs"]);
const forbiddenMarkers = [
  "@wdio/",
  "__paperworksE2eBootErrors",
  "__paperworksE2eOpenPaths",
  "__paperworksE2eSavePath",
  "plugin:wdio",
  "TAURI_WEBDRIVER_PORT",
  "WDIO Tauri Plugin",
  "wdioTauri"
];

export async function checkProductionE2eBoundary(directory) {
  const root = path.resolve(directory);
  const rootStatus = await lstat(root);
  if (!rootStatus.isDirectory() || rootStatus.isSymbolicLink()) {
    throw new Error("The production bundle must be an ordinary directory.");
  }

  const files = await listInspectedFiles(root);
  if (!files.some((file) => path.basename(file) === "index.html") || files.length < 2) {
    throw new Error("The production bundle does not contain the expected HTML and script assets.");
  }

  let inspectedBytes = 0;
  for (const file of files) {
    const status = await lstat(file);
    if (!status.isFile() || status.isSymbolicLink()) {
      throw new Error("Production HTML and script assets must be ordinary files.");
    }
    inspectedBytes += status.size;
    if (inspectedBytes > maximumTextBytes) {
      throw new Error("Production HTML and script assets exceed the boundary check size limit.");
    }
    const text = await readFile(file, "utf8");
    const marker = forbiddenMarkers.find((candidate) => text.includes(candidate));
    if (marker) {
      throw new Error(`The production bundle contains an end-to-end test marker: ${marker}.`);
    }
  }
  return { files: files.length, inspectedBytes };
}

async function listInspectedFiles(root) {
  const files = [];
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.pop();
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isSymbolicLink()) {
        throw new Error("The production bundle must not contain symbolic links.");
      }
      if (entry.isDirectory()) {
        pending.push(entryPath);
      } else if (entry.isFile() && inspectedExtensions.has(path.extname(entry.name))) {
        files.push(entryPath);
      }
    }
  }
  return files.sort((left, right) => left.localeCompare(right, "en-GB"));
}

async function main() {
  const directory = process.argv[2];
  if (!directory || process.argv.length !== 3) {
    throw new Error("Usage: node scripts/check-production-e2e-boundary.mjs <production-bundle>");
  }
  const report = await checkProductionE2eBoundary(directory);
  process.stdout.write(
    `Production end-to-end boundary passed for ${report.files} assets (${report.inspectedBytes} bytes).\n`
  );
}

const entryUrl = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (import.meta.url === entryUrl) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
