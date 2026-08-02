import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

export const E2E_CASE_IDS = Object.freeze([
  "native-shell-readiness",
  "workflow-keyboard-navigation",
  "modal-focus-management",
  "pdf-page-operations",
  "pdf-search-and-rendering",
  "print-preparation-and-dialogue",
  "page-content-editing",
  "bookmark-contents-publication",
  "merge-navigation-preservation",
  "signature-definition-and-placement",
  "interface-localisation-switching"
]);

const product = "Tüfekci Paperworks";
const releaseVersion = "0.1.0-alpha.1";
const platformDetails = Object.freeze({
  darwin: { platform: "macos", webview: "WKWebView" },
  linux: { platform: "linux", webview: "WebKitGTK" },
  win32: { platform: "windows", webview: "WebView2" }
});
const supportedArchitectures = new Set(["arm64", "x64"]);

export function currentE2eIdentity(nodePlatform = process.platform, architecture = process.arch) {
  const details = platformDetails[nodePlatform];
  if (!details) {
    throw new Error(`Native end-to-end evidence is unsupported on ${nodePlatform}.`);
  }
  if (!supportedArchitectures.has(architecture)) {
    throw new Error(`Native end-to-end evidence is unsupported on ${architecture}.`);
  }
  return { architecture, ...details };
}

export function evidenceFileName(identity = currentE2eIdentity()) {
  return `e2e-report-${identity.platform}-${identity.architecture}.json`;
}

export function createE2eReport({ caseIds, viewport }, identity = currentE2eIdentity()) {
  if (
    !Array.isArray(caseIds) ||
    caseIds.length !== E2E_CASE_IDS.length ||
    caseIds.some((id, index) => id !== E2E_CASE_IDS[index])
  ) {
    throw new Error("caseIds must contain every native end-to-end case once, in contract order.");
  }
  const report = {
    architecture: identity.architecture,
    appMode: "desktop",
    cases: E2E_CASE_IDS.map((id) => ({ id, outcome: "passed" })),
    driverProvider: "embedded",
    platform: identity.platform,
    product,
    releaseVersion,
    schemaVersion: 1,
    testBoundary: "cargo-feature-e2e",
    viewport: {
      height: viewport.height,
      width: viewport.width
    },
    webview: identity.webview
  };
  validateE2eReport(report, identity);
  return report;
}

export function validateE2eReport(report, expectedIdentity = currentE2eIdentity()) {
  requireExactKeys(report, [
    "architecture",
    "appMode",
    "cases",
    "driverProvider",
    "platform",
    "product",
    "releaseVersion",
    "schemaVersion",
    "testBoundary",
    "viewport",
    "webview"
  ], "report");

  requireEqual(report.schemaVersion, 1, "schemaVersion");
  requireEqual(report.product, product, "product");
  requireEqual(report.releaseVersion, releaseVersion, "releaseVersion");
  requireEqual(report.platform, expectedIdentity.platform, "platform");
  requireEqual(report.architecture, expectedIdentity.architecture, "architecture");
  requireEqual(report.webview, expectedIdentity.webview, "webview");
  requireEqual(report.appMode, "desktop", "appMode");
  requireEqual(report.driverProvider, "embedded", "driverProvider");
  requireEqual(report.testBoundary, "cargo-feature-e2e", "testBoundary");

  requireExactKeys(report.viewport, ["height", "width"], "viewport");
  requireIntegerInRange(report.viewport.width, 960, 4_096, "viewport.width");
  requireIntegerInRange(report.viewport.height, 640, 2_160, "viewport.height");

  if (!Array.isArray(report.cases) || report.cases.length !== E2E_CASE_IDS.length) {
    throw new Error(`cases must contain exactly ${E2E_CASE_IDS.length} records.`);
  }
  report.cases.forEach((testCase, index) => {
    requireExactKeys(testCase, ["id", "outcome"], `cases[${index}]`);
    requireEqual(testCase.id, E2E_CASE_IDS[index], `cases[${index}].id`);
    requireEqual(testCase.outcome, "passed", `cases[${index}].outcome`);
  });

  return report;
}

export async function writeE2eEvidence({ caseIds, outputDirectory, viewport }) {
  const identity = currentE2eIdentity();
  const report = createE2eReport({ caseIds, viewport }, identity);
  await mkdir(outputDirectory, { recursive: true });
  const outputPath = path.join(outputDirectory, evidenceFileName(identity));
  await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx"
  });
  return outputPath;
}

function requireExactKeys(value, expectedKeys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw new Error(`${label} contains missing or unknown fields.`);
  }
}

function requireEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label} did not match the native end-to-end evidence contract.`);
  }
}

function requireIntegerInRange(value, minimum, maximum, label) {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${label} must be an integer from ${minimum} to ${maximum}.`);
  }
}
