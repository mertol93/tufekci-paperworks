import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { validateE2eReport } from "./e2e-evidence.mjs";

const maximumReportBytes = 64 * 1024;
const fixedIdentities = new Map([
  ["e2e-report-linux-x64.json", { architecture: "x64", platform: "linux", webview: "WebKitGTK" }],
  ["e2e-report-windows-x64.json", { architecture: "x64", platform: "windows", webview: "WebView2" }],
  ["e2e-report-macos-arm64.json", { architecture: "arm64", platform: "macos", webview: "WKWebView" }],
  ["e2e-report-macos-x64.json", { architecture: "x64", platform: "macos", webview: "WKWebView" }]
]);

export async function checkE2eEvidenceMatrix(directory) {
  const root = path.resolve(directory);
  const directoryStatus = await lstat(root);
  if (!directoryStatus.isDirectory() || directoryStatus.isSymbolicLink()) {
    throw new Error("The native end-to-end matrix location must be an ordinary directory.");
  }

  const entries = await readdir(root, { withFileTypes: true });
  if (entries.length !== 3 || entries.some((entry) => !entry.isFile())) {
    throw new Error("The native end-to-end matrix must contain exactly three report files.");
  }

  const reports = [];
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name, "en-GB"))) {
    const identity = fixedIdentities.get(entry.name);
    if (!identity) {
      throw new Error(`Unexpected native end-to-end matrix report: ${entry.name}.`);
    }
    reports.push(await readReport(path.join(root, entry.name), identity));
  }

  const platforms = reports.map((report) => report.platform).sort();
  if (platforms.join(",") !== "linux,macos,windows") {
    throw new Error("The native end-to-end matrix requires one Linux, macOS, and Windows report.");
  }
  return reports;
}

async function readReport(reportPath, identity) {
  const status = await lstat(reportPath);
  if (
    !status.isFile() ||
    status.isSymbolicLink() ||
    status.size < 2 ||
    status.size > maximumReportBytes
  ) {
    throw new Error("A native end-to-end matrix report has an invalid type or size.");
  }
  const text = await readFile(reportPath, "utf8");
  if (!text.endsWith("\n") || text.includes("\r")) {
    throw new Error("Native end-to-end matrix reports must use UTF-8 LF text.");
  }
  return validateE2eReport(JSON.parse(text), identity);
}

async function main() {
  const directory = process.argv[2];
  if (!directory || process.argv.length !== 3) {
    throw new Error("Usage: node scripts/check-e2e-matrix.mjs <matrix-directory>");
  }
  const reports = await checkE2eEvidenceMatrix(directory);
  const cases = reports.reduce((total, report) => total + report.cases.length, 0);
  process.stdout.write(`Verified ${cases} native end-to-end cases across Linux, macOS, and Windows.\n`);
}

const entryUrl = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (import.meta.url === entryUrl) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
