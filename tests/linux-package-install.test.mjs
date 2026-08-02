import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  parseLinuxInstallMarker,
  validateLinuxInstallReport
} from "../scripts/run-linux-package-install-gate.mjs";

const marker = "PAPERWORKS_LINUX_INSTALL_V1";
const version = "0.1.0-alpha.1";
const imageId = "A".repeat(64);
const expectedCases = [
  {
    containerImage: "ubuntu:22.04",
    distribution: "ubuntu",
    distributionVersion: "22.04",
    format: "appimage",
    verification: "extracted-on-baseline"
  },
  {
    containerImage: "debian:13-slim",
    distribution: "debian",
    distributionVersion: "13",
    format: "deb",
    verification: "installed-and-linked"
  },
  {
    containerImage: "fedora:43",
    distribution: "fedora",
    distributionVersion: "43",
    format: "rpm",
    verification: "installed-and-linked"
  }
];

test("parses one bounded Linux installation marker", () => {
  const expected = expectedCases[1];
  const entry = parseLinuxInstallMarker(
    `installer output\n${marker}\tdebian\t13\tdeb\t${version}\tx64\tinstalled-and-linked\n`,
    expected
  );
  assert.equal(entry.distribution, "debian");
  assert.equal(entry.packageVersion, version);
  assert.equal(entry.containerImageId, null);
  assert.throws(
    () => parseLinuxInstallMarker(`${marker}\tdebian\t12\tdeb\t${version}\tx64\tinstalled-and-linked`, expected),
    /invalid/u
  );
  assert.throws(
    () => parseLinuxInstallMarker(`${marker}\tdebian\t13\tdeb\t${version}\tx64\tinstalled-and-linked\n${marker}\tdebian\t13\tdeb\t${version}\tx64\tinstalled-and-linked`, expected),
    /exactly one/u
  );
});

test("accepts only complete path-free distribution installation evidence", () => {
  const report = validReport();
  assert.equal(validateLinuxInstallReport(report), report);
  const leaked = validReport();
  leaked.cases[0].bundlePath = "/private/release.AppImage";
  assert.throws(() => validateLinuxInstallReport(leaked), /unknown fields/u);
  const stale = validReport();
  stale.cases.find((entry) => entry.distribution === "debian").packageVersion = "0.1.0-alpha.2";
  assert.throws(() => validateLinuxInstallReport(stale), /invalid for debian/u);
});

test("keeps Linux installation evidence mandatory before tagged release metadata", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [packageJson, workflow, installScript] = await Promise.all([
    readFile(`${workspace}package.json`, "utf8").then(JSON.parse),
    readFile(`${workspace}.github/workflows/release.yml`, "utf8"),
    readFile(`${workspace}scripts/run-linux-package-install-gate.mjs`, "utf8")
  ]);
  assert.match(packageJson.scripts["release:verify-bundles"] ?? "", /check-release-bundles/u);
  assert.match(packageJson.scripts["release:linux-install"] ?? "", /run-linux-package-install-gate/u);
  assert.match(workflow, /Linux package installation evidence/u);
  assert.match(installScript, /ubuntu:22\.04[\s\S]+debian:13-slim[\s\S]+fedora:43/u);
  assert.match(workflow, /metadata:[\s\S]+needs:[\s\S]+linux-package-install[\s\S]+Generate checksums/u);
});

function validReport() {
  return {
    schemaVersion: 1,
    product: "Tüfekci Paperworks",
    releaseVersion: version,
    platform: "linux",
    architecture: "x64",
    cases: expectedCases.map((entry) => ({
      architecture: "x64",
      containerImage: entry.containerImage,
      containerImageId: imageId,
      distribution: entry.distribution,
      distributionVersion: entry.distributionVersion,
      format: entry.format,
      packageVersion: entry.format === "rpm" ? `${version}-1` : version,
      verification: entry.verification
    })).sort((left, right) => left.distribution.localeCompare(right.distribution, "en-GB"))
  };
}
