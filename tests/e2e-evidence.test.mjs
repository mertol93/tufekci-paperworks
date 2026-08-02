import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { checkE2eEvidenceDirectory } from "../scripts/check-e2e-evidence.mjs";
import { checkE2eEvidenceMatrix } from "../scripts/check-e2e-matrix.mjs";
import {
  E2E_CASE_IDS,
  createE2eReport,
  currentE2eIdentity,
  evidenceFileName,
  validateE2eReport
} from "../scripts/e2e-evidence.mjs";

const viewport = { height: 820, width: 1_280 };

test("native end-to-end evidence has a fixed path-free success schema", () => {
  const report = createE2eReport({ caseIds: [...E2E_CASE_IDS], viewport });
  assert.equal(validateE2eReport(report), report);
  const serialised = JSON.stringify(report);
  assert.doesNotMatch(serialised, /(?:[A-Za-z]:\\|\/Users\/|\/home\/|qa-fixtures|\.pdf)/u);
  assert.deepEqual(report.cases.map(({ id }) => id), E2E_CASE_IDS);
});

test("native end-to-end evidence rejects missing, failed, reordered, and unknown fields", () => {
  assert.throws(
    () => createE2eReport({ caseIds: E2E_CASE_IDS.slice(0, -1), viewport }),
    /caseIds/u
  );

  const failed = createE2eReport({ caseIds: [...E2E_CASE_IDS], viewport });
  failed.cases[0].outcome = "failed";
  assert.throws(() => validateE2eReport(failed), /outcome/u);

  const reordered = createE2eReport({ caseIds: [...E2E_CASE_IDS], viewport });
  reordered.cases.reverse();
  assert.throws(() => validateE2eReport(reordered), /\.id/u);

  const extra = { ...createE2eReport({ caseIds: [...E2E_CASE_IDS], viewport }), path: "private" };
  assert.throws(() => validateE2eReport(extra), /unknown fields/u);
});

test("native end-to-end evidence checker accepts only the current platform report", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "paperworks-e2e-evidence-"));
  const evidenceDirectory = path.join(temporaryRoot, "evidence");
  const identity = currentE2eIdentity();
  const report = createE2eReport({ caseIds: [...E2E_CASE_IDS], viewport }, identity);
  try {
    await mkdir(evidenceDirectory);
    await writeFile(
      path.join(evidenceDirectory, evidenceFileName(identity)),
      `${JSON.stringify(report, null, 2)}\n`,
      "utf8"
    );
    const checked = await checkE2eEvidenceDirectory(evidenceDirectory);
    assert.equal(checked.report.platform, identity.platform);

    await writeFile(path.join(evidenceDirectory, "unexpected.json"), "{}\n", "utf8");
    await assert.rejects(
      checkE2eEvidenceDirectory(evidenceDirectory),
      /must contain only/u
    );
  } finally {
    await rm(temporaryRoot, { force: true, recursive: true });
  }
});

test("release evidence requires one Linux, macOS, and Windows native report", async () => {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "paperworks-e2e-matrix-"));
  const matrixDirectory = path.join(temporaryRoot, "matrix");
  const identities = [
    { architecture: "x64", platform: "linux", webview: "WebKitGTK" },
    { architecture: "arm64", platform: "macos", webview: "WKWebView" },
    { architecture: "x64", platform: "windows", webview: "WebView2" }
  ];
  try {
    await mkdir(matrixDirectory);
    for (const identity of identities) {
      const report = createE2eReport({ caseIds: [...E2E_CASE_IDS], viewport }, identity);
      await writeFile(
        path.join(matrixDirectory, evidenceFileName(identity)),
        `${JSON.stringify(report, null, 2)}\n`,
        "utf8"
      );
    }
    const reports = await checkE2eEvidenceMatrix(matrixDirectory);
    assert.deepEqual(reports.map((report) => report.platform).sort(), ["linux", "macos", "windows"]);

    await writeFile(path.join(matrixDirectory, "unexpected.json"), "{}\n", "utf8");
    await assert.rejects(checkE2eEvidenceMatrix(matrixDirectory), /exactly three/u);
  } finally {
    await rm(temporaryRoot, { force: true, recursive: true });
  }
});
