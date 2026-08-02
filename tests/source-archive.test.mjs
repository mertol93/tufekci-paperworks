import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  inspectZipArchive,
  parseSourceDateEpoch,
  validateArchivePathSet,
  validateIndexRecord
} from "../scripts/create-source-archive.mjs";

test("accepts bounded reproducible source epochs", () => {
  assert.equal(parseSourceDateEpoch("1785000000", 1785000000), 1785000000);
  assert.throws(() => parseSourceDateEpoch("01", 10), /whole number/u);
  assert.throws(() => parseSourceDateEpoch("-1", 10), /whole number/u);
  assert.throws(() => parseSourceDateEpoch("311", 10), /five minutes/u);
  assert.throws(
    () => parseSourceDateEpoch("4102444801", 4102444801),
    /supported range/u
  );
});

test("accepts only ordinary, resolved Git index entries", () => {
  const ordinary = {
    mode: "100644",
    objectId: "a".repeat(40),
    path: "README.md",
    stage: 0
  };
  assert.equal(validateIndexRecord(ordinary), ordinary);
  assert.equal(
    validateIndexRecord({ ...ordinary, mode: "100755" }).mode,
    "100755"
  );
  assert.throws(
    () => validateIndexRecord({ ...ordinary, mode: "120000" }),
    /non-file mode/u
  );
  assert.throws(
    () => validateIndexRecord({ ...ordinary, stage: 2 }),
    /unresolved entry/u
  );
  assert.throws(
    () => validateIndexRecord({ ...ordinary, objectId: "not-an-object" }),
    /invalid object ID/u
  );
});

test("requires the archive path set to match the index exactly", () => {
  assert.deepEqual(
    validateArchivePathSet(
      ["src/App.tsx", "README.md"],
      ["README.md", "src/App.tsx"]
    ),
    ["README.md", "src/App.tsx"]
  );
  assert.throws(
    () => validateArchivePathSet(["README.md"], ["README.md", "extra.txt"]),
    /contains 2 files/u
  );
  assert.throws(
    () => validateArchivePathSet(["README.md"], ["SECURITY.md"]),
    /path set differs/u
  );
});

test("rejects malformed and unbounded ZIP archives", () => {
  assert.throws(
    () => inspectZipArchive(Buffer.alloc(21), "paperworks/"),
    /invalid bounded size/u
  );
  assert.throws(
    () => inspectZipArchive(Buffer.alloc(22), "../unsafe/"),
    /prefix is unsafe/u
  );
  assert.throws(
    () => inspectZipArchive(Buffer.alloc(22), "paperworks/"),
    /no end-of-central-directory/u
  );
});

test("keeps source auditing and archive evidence in CI and tagged releases", async () => {
  const workspace = fileURLToPath(new URL("../", import.meta.url));
  const [ciWorkflow, releaseWorkflow] = await Promise.all([
    readFile(`${workspace}.github/workflows/ci.yml`, "utf8"),
    readFile(`${workspace}.github/workflows/release.yml`, "utf8")
  ]);

  assert.match(ciWorkflow, /Audit distributable source tree[\s\S]+release:source-check/u);
  assert.match(
    releaseWorkflow,
    /preflight:[\s\S]+Audit distributable source tree[\s\S]+release:source-check/u
  );
  assert.match(
    releaseWorkflow,
    /Create and verify source archive[\s\S]+release:source-archive/u
  );
  assert.match(releaseWorkflow, /name: release-source[\s\S]+artifacts\/source-release/u);
  assert.match(
    releaseWorkflow,
    /gh release upload[\s\S]+artifacts\/source-release\/\*/u
  );
});
