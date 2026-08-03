import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  parseCargoAuditVersion,
  validateRustSecReport
} from "../scripts/check-rustsec-policy.mjs";

const policy = JSON.parse(
  readFileSync(new URL("../security/rustsec-policy.json", import.meta.url), "utf8")
);
const reviewDate = new Date("2026-08-03T12:00:00.000Z");

function policyAdvisories() {
  return policy.exemptions.flatMap((group) => group.advisories);
}

function reportFromPolicy() {
  const warnings = {
    notice: [],
    unmaintained: [],
    unsound: []
  };
  for (const advisory of policyAdvisories()) {
    warnings[advisory.kind].push({
      advisory: { id: advisory.id },
      kind: advisory.kind,
      package: { name: advisory.package, version: advisory.version }
    });
  }
  return {
    database: { "advisory-count": 1_186 },
    lockfile: { "dependency-count": 572 },
    settings: {
      informational_warnings: ["unmaintained", "unsound", "notice"]
    },
    vulnerabilities: { count: 0, found: false, list: [] },
    warnings
  };
}

test("accepts only the complete reviewed RustSec warning set", () => {
  const summary = validateRustSecReport(reportFromPolicy(), policy, reviewDate);
  assert.equal(summary.dependencyCount, 572);
  assert.equal(summary.warningCount, 17);
  assert.deepEqual(summary.warningCounts, {
    notice: 0,
    unmaintained: 16,
    unsound: 1
  });
});

test("rejects vulnerabilities, new warnings, changed versions, and stale exemptions", () => {
  const vulnerable = reportFromPolicy();
  vulnerable.vulnerabilities = {
    count: 1,
    found: true,
    list: [{ advisory: { id: "RUSTSEC-2099-0001" }, package: { name: "unsafe-pdf" } }]
  };
  assert.throws(
    () => validateRustSecReport(vulnerable, policy, reviewDate),
    /locked vulnerability/u
  );

  const added = reportFromPolicy();
  added.warnings.unsound.push({
    advisory: { id: "RUSTSEC-2099-0002" },
    kind: "unsound",
    package: { name: "new-warning", version: "1.0.0" }
  });
  assert.throws(() => validateRustSecReport(added, policy, reviewDate), /unreviewed warning/u);

  const changed = reportFromPolicy();
  changed.warnings.unsound[0].package.version = "0.18.6";
  assert.throws(
    () => validateRustSecReport(changed, policy, reviewDate),
    /unreviewed warning/u
  );

  const removed = reportFromPolicy();
  removed.warnings.unmaintained.shift();
  assert.throws(
    () => validateRustSecReport(removed, policy, reviewDate),
    /stale exemption/u
  );
});

test("requires every informational warning category and the pinned auditor version", () => {
  const report = reportFromPolicy();
  report.settings.informational_warnings = ["unmaintained"];
  assert.throws(
    () => validateRustSecReport(report, policy, reviewDate),
    /notice, unmaintained, and unsound/u
  );
  assert.equal(parseCargoAuditVersion("cargo-audit-audit 0.22.2\n"), "0.22.2");
  assert.throws(() => parseCargoAuditVersion("cargo audit unknown"), /could not be read/u);
});

test("expires reviewed warning exemptions after the bounded review window", () => {
  assert.throws(
    () =>
      validateRustSecReport(
        reportFromPolicy(),
        policy,
        new Date("2026-11-02T00:00:00.000Z")
      ),
    /review expired on 2026-11-01/u
  );
});

test("keeps CI and tagged release security checks current and fail-closed", () => {
  const workflows = [
    ".github/workflows/apple-mobile.yml",
    ".github/workflows/ci.yml",
    ".github/workflows/promote-update.yml",
    ".github/workflows/release.yml"
  ].map((file) => readFileSync(new URL(`../${file}`, import.meta.url), "utf8"));
  const combined = workflows.join("\n");
  const expectedActionMajors = new Map([
    ["actions/checkout", "v7"],
    ["actions/download-artifact", "v8"],
    ["actions/setup-java", "v5"],
    ["actions/setup-node", "v7"],
    ["actions/setup-python", "v7"],
    ["actions/upload-artifact", "v7"]
  ]);
  for (const [action, expected] of expectedActionMajors) {
    const escaped = action.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
    const references = [...combined.matchAll(new RegExp(`${escaped}@([^\\s]+)`, "gu"))].map(
      (match) => match[1]
    );
    assert.ok(references.length > 0, `${action} is not used by the workflows`);
    assert.ok(
      references.every((reference) => reference === expected),
      `${action} must use ${expected}: ${references.join(", ")}`
    );
  }
  assert.doesNotMatch(combined, /rustsec\/audit-check/u);

  for (const workflow of [workflows[1], workflows[3]]) {
    assert.match(workflow, /cargo install cargo-audit --version 0\.22\.2 --locked/u);
    assert.match(workflow, /npm run security:audit-rust/u);
  }

  const dependabot = readFileSync(
    new URL("../.github/dependabot.yml", import.meta.url),
    "utf8"
  );
  assert.match(dependabot, /package-ecosystem: github-actions/u);
});
