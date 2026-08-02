import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  currentE2eIdentity,
  evidenceFileName,
  validateE2eReport
} from "./e2e-evidence.mjs";

const maximumReportBytes = 64 * 1024;

export async function checkE2eEvidenceDirectory(directory) {
  const root = path.resolve(directory);
  const directoryStatus = await lstat(root);
  if (!directoryStatus.isDirectory() || directoryStatus.isSymbolicLink()) {
    throw new Error("The native end-to-end evidence location must be an ordinary directory.");
  }

  const identity = currentE2eIdentity();
  const expectedName = evidenceFileName(identity);
  const entries = await readdir(root, { withFileTypes: true });
  if (entries.length !== 1 || entries[0].name !== expectedName || !entries[0].isFile()) {
    throw new Error(`The evidence directory must contain only ${expectedName}.`);
  }

  const reportPath = path.join(root, expectedName);
  const reportStatus = await lstat(reportPath);
  if (
    !reportStatus.isFile() ||
    reportStatus.isSymbolicLink() ||
    reportStatus.size < 2 ||
    reportStatus.size > maximumReportBytes
  ) {
    throw new Error("The native end-to-end evidence report has an invalid type or size.");
  }

  const text = await readFile(reportPath, "utf8");
  if (!text.endsWith("\n") || text.includes("\r")) {
    throw new Error("The native end-to-end evidence report must use UTF-8 LF text.");
  }
  const report = validateE2eReport(JSON.parse(text), identity);
  return { report, reportPath };
}

async function main() {
  const directory = process.argv[2];
  if (!directory || process.argv.length !== 3) {
    throw new Error("Usage: node scripts/check-e2e-evidence.mjs <evidence-directory>");
  }
  const { report } = await checkE2eEvidenceDirectory(directory);
  process.stdout.write(
    `Verified ${report.cases.length} native end-to-end cases on ${report.platform} ${report.architecture}.\n`
  );
}

const entryUrl = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (import.meta.url === entryUrl) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
