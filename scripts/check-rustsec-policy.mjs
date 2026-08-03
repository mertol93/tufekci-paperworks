import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const expectedWarningKinds = ["notice", "unmaintained", "unsound"];
const advisoryIdPattern = /^RUSTSEC-\d{4}-\d{4,}$/u;
const versionPattern = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/u;
const isoDatePattern = /^\d{4}-\d{2}-\d{2}$/u;
const maximumReviewWindowMilliseconds = 92 * 24 * 60 * 60 * 1_000;

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function exactStringSet(values, expected) {
  return (
    Array.isArray(values) &&
    values.length === expected.length &&
    [...values].sort().every((value, index) => value === [...expected].sort()[index])
  );
}

function warningIdentity(warning) {
  return `${warning.id} (${warning.kind}: ${warning.package} ${warning.version})`;
}

function parsePolicyDate(value, label) {
  if (!isoDatePattern.test(value ?? "")) {
    throw new Error(`The RustSec policy must contain a ${label} ISO date.`);
  }
  const parsed = new Date(`${value}T00:00:00.000Z`);
  if (Number.isNaN(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== value) {
    throw new Error(`The RustSec policy contains an invalid ${label} date.`);
  }
  return parsed.getTime();
}

function readPolicyAdvisories(policy, now) {
  if (!isRecord(policy) || policy.schemaVersion !== 1) {
    throw new Error("The RustSec policy must use schema version 1.");
  }
  if (!versionPattern.test(policy.cargoAuditVersion ?? "")) {
    throw new Error("The RustSec policy must pin one cargo-audit semantic version.");
  }
  if (!(now instanceof Date) || Number.isNaN(now.getTime())) {
    throw new Error("RustSec policy validation requires a valid review time.");
  }
  const reviewedOn = parsePolicyDate(policy.reviewedOn, "reviewedOn");
  const reviewBy = parsePolicyDate(policy.reviewBy, "reviewBy");
  const today = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
  if (reviewBy <= reviewedOn || reviewBy - reviewedOn > maximumReviewWindowMilliseconds) {
    throw new Error("The RustSec policy review window must be positive and no longer than 92 days.");
  }
  if (today < reviewedOn) {
    throw new Error("The RustSec policy review date is in the future.");
  }
  if (today > reviewBy) {
    throw new Error(`The RustSec policy review expired on ${policy.reviewBy}.`);
  }
  if (!Array.isArray(policy.exemptions) || policy.exemptions.length === 0) {
    throw new Error("The RustSec policy must contain reviewed exemptions.");
  }

  const advisories = new Map();
  for (const group of policy.exemptions) {
    if (
      !isRecord(group) ||
      typeof group.reason !== "string" ||
      group.reason.length < 40 ||
      group.reason.length > 1_000 ||
      /[\0\r\n]/u.test(group.reason) ||
      !Array.isArray(group.advisories) ||
      group.advisories.length === 0
    ) {
      throw new Error("Each RustSec exemption group needs a bounded review reason and advisories.");
    }
    for (const advisory of group.advisories) {
      if (
        !isRecord(advisory) ||
        !advisoryIdPattern.test(advisory.id ?? "") ||
        !expectedWarningKinds.includes(advisory.kind) ||
        typeof advisory.package !== "string" ||
        advisory.package.length === 0 ||
        advisory.package.length > 128 ||
        !versionPattern.test(advisory.version ?? "")
      ) {
        throw new Error("A RustSec exemption contains invalid advisory metadata.");
      }
      if (advisories.has(advisory.id)) {
        throw new Error(`The RustSec policy repeats ${advisory.id}.`);
      }
      advisories.set(advisory.id, {
        id: advisory.id,
        kind: advisory.kind,
        package: advisory.package,
        reason: group.reason,
        version: advisory.version
      });
    }
  }
  return advisories;
}

function readReportWarnings(report) {
  if (!isRecord(report.warnings)) {
    throw new Error("The RustSec report does not contain a warnings object.");
  }
  const warnings = [];
  for (const [kind, entries] of Object.entries(report.warnings)) {
    if (!expectedWarningKinds.includes(kind) || !Array.isArray(entries)) {
      throw new Error(`The RustSec report contains an unsupported warning category: ${kind}.`);
    }
    for (const entry of entries) {
      const warning = {
        id: entry?.advisory?.id,
        kind: entry?.kind,
        package: entry?.package?.name,
        version: entry?.package?.version
      };
      if (
        !advisoryIdPattern.test(warning.id ?? "") ||
        warning.kind !== kind ||
        typeof warning.package !== "string" ||
        !versionPattern.test(warning.version ?? "")
      ) {
        throw new Error("The RustSec report contains malformed warning metadata.");
      }
      warnings.push(warning);
    }
  }
  return warnings;
}

export function validateRustSecReport(report, policy, now = new Date()) {
  if (!isRecord(report)) {
    throw new Error("cargo-audit did not return a JSON object.");
  }
  if (!exactStringSet(report.settings?.informational_warnings, expectedWarningKinds)) {
    throw new Error("cargo-audit must report notice, unmaintained, and unsound warnings.");
  }
  const vulnerabilityList = report.vulnerabilities?.list;
  if (!Array.isArray(vulnerabilityList)) {
    throw new Error("The RustSec report does not contain a vulnerability list.");
  }
  const vulnerabilityCount = report.vulnerabilities?.count;
  if (
    vulnerabilityCount !== vulnerabilityList.length ||
    report.vulnerabilities?.found !== (vulnerabilityList.length > 0)
  ) {
    throw new Error("The RustSec vulnerability summary is inconsistent.");
  }
  if (vulnerabilityList.length > 0) {
    const identities = vulnerabilityList
      .slice(0, 20)
      .map((entry) => `${entry?.advisory?.id ?? "unknown"} (${entry?.package?.name ?? "unknown"})`)
      .join(", ");
    throw new Error(`RustSec found ${vulnerabilityList.length} locked vulnerability finding(s): ${identities}.`);
  }
  const dependencyCount = report.lockfile?.["dependency-count"];
  const databaseAdvisoryCount = report.database?.["advisory-count"];
  if (!Number.isSafeInteger(dependencyCount) || dependencyCount < 1) {
    throw new Error("The RustSec report has no valid locked dependency count.");
  }
  if (!Number.isSafeInteger(databaseAdvisoryCount) || databaseAdvisoryCount < 1) {
    throw new Error("The RustSec report has no valid advisory database count.");
  }

  const reviewed = readPolicyAdvisories(policy, now);
  const reported = readReportWarnings(report);
  const seen = new Set();
  const unreviewed = [];
  for (const warning of reported) {
    const exemption = reviewed.get(warning.id);
    if (
      !exemption ||
      exemption.kind !== warning.kind ||
      exemption.package !== warning.package ||
      exemption.version !== warning.version
    ) {
      unreviewed.push(warningIdentity(warning));
      continue;
    }
    if (seen.has(warning.id)) {
      throw new Error(`The RustSec report repeats ${warning.id}.`);
    }
    seen.add(warning.id);
  }
  if (unreviewed.length > 0) {
    throw new Error(`RustSec returned unreviewed warning(s): ${unreviewed.join(", ")}.`);
  }

  const stale = [...reviewed.values()].filter((entry) => !seen.has(entry.id));
  if (stale.length > 0) {
    throw new Error(
      `The RustSec policy contains stale exemption(s): ${stale.map(warningIdentity).join(", ")}.`
    );
  }

  const warningCounts = Object.fromEntries(
    expectedWarningKinds.map((kind) => [
      kind,
      reported.filter((warning) => warning.kind === kind).length
    ])
  );
  return {
    databaseAdvisoryCount,
    dependencyCount,
    warningCount: reported.length,
    warningCounts
  };
}

export function parseCargoAuditVersion(value) {
  const match = String(value).match(/\b(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)\s*$/u);
  if (!match) {
    throw new Error("The installed cargo-audit version could not be read.");
  }
  return match[1];
}

function runCargoAudit(workspace, expectedVersion) {
  const versionResult = spawnSync("cargo", ["audit", "--version"], {
    cwd: workspace,
    encoding: "utf8",
    windowsHide: true
  });
  if (versionResult.status !== 0) {
    throw new Error("cargo-audit is unavailable. Install the pinned release before running this gate.");
  }
  const installedVersion = parseCargoAuditVersion(versionResult.stdout || versionResult.stderr);
  if (installedVersion !== expectedVersion) {
    throw new Error(
      `cargo-audit ${installedVersion} is installed; this policy requires ${expectedVersion}.`
    );
  }

  const result = spawnSync(
    "cargo",
    ["audit", "--file", path.join(workspace, "src-tauri", "Cargo.lock"), "--json"],
    {
      cwd: workspace,
      encoding: "utf8",
      env: { ...process.env, CARGO_TERM_COLOR: "never" },
      maxBuffer: 16 * 1024 * 1024,
      windowsHide: true
    }
  );
  if (result.error) {
    throw new Error(`cargo-audit could not start: ${result.error.message}`);
  }
  let report;
  try {
    report = JSON.parse(result.stdout);
  } catch {
    const diagnostic = String(result.stderr || result.stdout || "No diagnostic was returned.")
      .replace(/[\0\r]/gu, "")
      .trim()
      .slice(0, 2_000);
    throw new Error(`cargo-audit did not return valid JSON: ${diagnostic}`);
  }
  return { report, status: result.status };
}

async function main() {
  const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const policy = JSON.parse(
    await readFile(path.join(workspace, "security", "rustsec-policy.json"), "utf8")
  );
  const audit = runCargoAudit(workspace, policy.cargoAuditVersion);
  const summary = validateRustSecReport(audit.report, policy);
  if (audit.status !== 0) {
    throw new Error(`cargo-audit exited with status ${audit.status} after producing its report.`);
  }
  process.stdout.write(
    `RustSec policy passed for ${summary.dependencyCount.toLocaleString("en-GB")} locked dependencies: ` +
      `0 vulnerabilities and ${summary.warningCount} reviewed warning(s) ` +
      `(${summary.warningCounts.unmaintained} unmaintained, ${summary.warningCounts.unsound} unsound, ` +
      `${summary.warningCounts.notice} notice).\n`
  );
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
