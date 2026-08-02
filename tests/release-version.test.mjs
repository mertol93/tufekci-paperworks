import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  releaseUpdateChannel,
  validateReleaseVersionSet,
  validateWindowsMsiVersion
} from "../scripts/check-release-version.mjs";

test("accepts one exact alpha version and tag across release metadata", () => {
  assert.deepEqual(
    validateReleaseVersionSet(
      {
        Cargo: "0.1.0-alpha.1",
        npm: "0.1.0-alpha.1",
        Tauri: "0.1.0-alpha.1"
      },
      "v0.1.0-alpha.1"
    ),
    {
      channel: "alpha",
      prerelease: true,
      tag: "v0.1.0-alpha.1",
      version: "0.1.0-alpha.1"
    }
  );
});

test("rejects version drift, malformed versions, and mismatched tags", () => {
  assert.throws(
    () =>
      validateReleaseVersionSet(
        { Cargo: "0.1.0-alpha.1", npm: "0.1.0-alpha.2" },
        "v0.1.0-alpha.1"
      ),
    /Release versions do not match/u
  );
  assert.throws(
    () => validateReleaseVersionSet({ npm: "01.0.0" }, "v01.0.0"),
    /valid semantic version/u
  );
  assert.throws(
    () => validateReleaseVersionSet({ npm: "0.1.0-alpha.1" }, "v0.1.0"),
    /Release tag must be exactly v0\.1\.0-alpha\.1/u
  );
});

test("derives stable and pre-release states from complete semantic versions", () => {
  assert.equal(validateReleaseVersionSet({ npm: "1.2.3" }, "v1.2.3").prerelease, false);
  assert.equal(
    validateReleaseVersionSet({ npm: "1.2.3-beta.7+build.4" }, "v1.2.3-beta.7+build.4")
      .prerelease,
    true
  );
});

test("derives an explicit updater channel from the semantic version", () => {
  assert.equal(releaseUpdateChannel("0.1.0-alpha.1"), "alpha");
  assert.equal(releaseUpdateChannel("0.1.0-beta.2"), "beta");
  assert.equal(releaseUpdateChannel("0.1.0-rc.3"), "beta");
  assert.equal(releaseUpdateChannel("1.0.0"), "stable");
  assert.throws(() => releaseUpdateChannel("1.0.0-nightly.1"), /alpha, beta, or rc/u);
});

test("maps semantic pre-releases to the explicit numeric Windows MSI version", () => {
  assert.equal(validateWindowsMsiVersion("0.1.0-alpha.1", "0.1.0.1"), "0.1.0.1");
  assert.equal(validateWindowsMsiVersion("1.2.3", "1.2.3"), "1.2.3");
  assert.throws(
    () => validateWindowsMsiVersion("0.1.0-alpha.1", "0.1.0.2"),
    /Windows MSI version must be exactly 0\.1\.0\.1/u
  );
  assert.throws(
    () => validateWindowsMsiVersion("0.1.0-alpha", "0.1.0"),
    /must end with a numeric Windows Installer sequence/u
  );
});

test("validates the repository versions through structured Cargo metadata", () => {
  const script = fileURLToPath(
    new URL("../scripts/check-release-version.mjs", import.meta.url)
  );
  const result = spawnSync(process.execPath, [script, "v0.1.0-alpha.1"], {
    encoding: "utf8",
    windowsHide: true
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /contract passed for v0\.1\.0-alpha\.1 \(pre-release; Windows MSI 0\.1\.0\.1\)/u
  );
});

test("gates every platform release build on the derived pre-release identity", () => {
  const workflow = readFileSync(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8"
  );

  assert.match(workflow, /^  preflight:$/mu);
  assert.match(workflow, /run: npm run release:version -- "\$GITHUB_REF_NAME"/u);
  assert.match(workflow, /^    needs: preflight$/mu);
  assert.match(workflow, /prerelease: \$\{\{ needs\.preflight\.outputs\.prerelease \}\}/u);
  assert.doesNotMatch(workflow, /prerelease:\s*false/u);
});
