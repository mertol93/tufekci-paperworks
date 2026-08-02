import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  buildCargoSbom,
  buildLicenceReport,
  buildReleaseManifest,
  collectReleaseFiles,
  formatLicenceCsv,
  formatSha256Sums,
  normaliseNpmSbom
} from "../scripts/generate-release-metadata.mjs";

test("collects only distributable files and writes sorted streaming checksums", async () => {
  const directory = await testDirectory();
  try {
    await mkdir(path.join(directory, "windows"));
    await mkdir(path.join(directory, "linux"));
    await writeFile(path.join(directory, "windows", "Paperworks.msi"), "msi");
    await writeFile(path.join(directory, "linux", "Paperworks.AppImage"), "appimage");
    await writeFile(path.join(directory, "notes.txt"), "not a release package");
    await writeFile(path.join(directory, "SHA256SUMS"), "old metadata");

    const files = await collectReleaseFiles(directory);
    const manifest = await buildReleaseManifest(files);

    assert.deepEqual(files.map((item) => item.fileName), ["Paperworks.AppImage", "Paperworks.msi"]);
    assert.deepEqual(manifest.map((item) => item.bytes), [8, 3]);
    assert.match(formatSha256Sums(manifest), /^[a-f0-9]{64}  Paperworks\.AppImage\n[a-f0-9]{64}  Paperworks\.msi\n$/);
  } finally {
    await rm(directory, { force: true, recursive: true });
  }
});

test("rejects duplicate release filenames from different platform folders", async () => {
  const directory = await testDirectory();
  try {
    await mkdir(path.join(directory, "one"));
    await mkdir(path.join(directory, "two"));
    await writeFile(path.join(directory, "one", "setup.exe"), "one");
    await writeFile(path.join(directory, "two", "SETUP.EXE"), "two");

    await assert.rejects(collectReleaseFiles(directory), /duplicate filename/i);
  } finally {
    await rm(directory, { force: true, recursive: true });
  }
});

test("builds a deterministic Cargo CycloneDX dependency graph", () => {
  const metadata = cargoFixture();
  const project = { name: "paperworks", version: "1.2.3", license: "AGPL-3.0-or-later" };
  const first = buildCargoSbom(metadata, project, "2026-01-02T03:04:05.000Z", "lock-hash");
  const second = buildCargoSbom(metadata, project, "2026-01-02T03:04:05.000Z", "lock-hash");

  assert.deepEqual(first, second);
  assert.equal(first.bomFormat, "CycloneDX");
  assert.equal(first.specVersion, "1.5");
  assert.match(first.serialNumber, /^urn:uuid:/);
  assert.deepEqual(first.components.map((item) => item.name), ["alpha", "zeta"]);
  const projectDependencies = first.dependencies.find((item) => item.ref.startsWith("pkg:generic/"));
  assert.deepEqual(projectDependencies.dependsOn, ["pkg:cargo/alpha@1.0.0", "pkg:cargo/zeta@2.0.0"]);
});

test("normalises npm SBOM identity, timestamp, components, and dependency order", () => {
  const input = {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    serialNumber: "urn:uuid:random",
    metadata: { timestamp: "now" },
    components: [
      { type: "library", name: "zeta", version: "1" },
      { type: "library", name: "alpha", version: "1" }
    ],
    dependencies: [
      { ref: "zeta", dependsOn: ["two", "one"] },
      { ref: "alpha", dependsOn: [] }
    ]
  };
  const result = normaliseNpmSbom(
    input,
    { name: "paperworks", version: "1.2.3" },
    "2026-01-02T03:04:05.000Z",
    "lock-hash"
  );

  assert.equal(result.metadata.timestamp, "2026-01-02T03:04:05.000Z");
  assert.notEqual(result.serialNumber, input.serialNumber);
  assert.deepEqual(result.components.map((item) => item.name), ["alpha", "zeta"]);
  assert.deepEqual(result.dependencies.map((item) => item.ref), ["alpha", "zeta"]);
  assert.deepEqual(result.dependencies[1].dependsOn, ["one", "two"]);
});

test("licence inventory flags missing and non-standard declarations without a verdict", () => {
  const packageLock = {
    packages: {
      "": { name: "paperworks", version: "1.2.3", license: "AGPL-3.0-or-later" },
      "node_modules/clear": {
        version: "1.0.0",
        license: "MIT",
        resolved: [
          "https://user",
          "secret@registry.example/clear.tgz?token=private"
        ].join(":")
      },
      "node_modules/missing": { version: "2.0.0" },
      "node_modules/separate": { version: "3.0.0", license: "SEE LICENSE IN TERMS" }
    }
  };
  const metadata = cargoFixture();
  const report = buildLicenceReport(
    packageLock,
    metadata,
    { name: "paperworks", version: "1.2.3", license: "AGPL-3.0-or-later" },
    "2026-01-02T03:04:05.000Z"
  );

  assert.equal(report.summary.totalDependencies, 5);
  assert.equal(report.summary.npmDependencies, 3);
  assert.equal(report.summary.cargoDependencies, 2);
  assert.equal(report.summary.reviewRequired, 2);
  assert.match(report.notice, /not legal advice/);
  const clearDependency = report.dependencies.find((item) => item.name === "clear");
  assert.equal(clearDependency.source, "https://registry.example/clear.tgz");
  assert.match(formatLicenceCsv(report), /ecosystem,name,version,licence/);
});

function cargoFixture() {
  return {
    packages: [
      {
        id: "root",
        name: "paperworks",
        version: "1.2.3",
        license: "AGPL-3.0-or-later"
      },
      {
        id: "zeta",
        name: "zeta",
        version: "2.0.0",
        license: "Apache-2.0",
        source: "registry+https://github.com/rust-lang/crates.io-index"
      },
      {
        id: "alpha",
        name: "alpha",
        version: "1.0.0",
        license: "MIT",
        source: "registry+https://github.com/rust-lang/crates.io-index"
      }
    ],
    resolve: {
      root: "root",
      nodes: [
        { id: "root", dependencies: ["zeta", "alpha"] },
        { id: "zeta", dependencies: ["alpha"] },
        { id: "alpha", dependencies: [] }
      ]
    }
  };
}

async function testDirectory() {
  return mkdtemp(path.join(tmpdir(), "paperworks-release-metadata-"));
}
