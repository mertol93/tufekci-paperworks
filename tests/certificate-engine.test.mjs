import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  parseCertificateMarker,
  validateCertificateEngineReport
} from "../scripts/run-certificate-engine-corpus.mjs";

function validReport() {
  return {
    architecture: "x64",
    engines: {
      openSsl: { command: "openssl", version: "OpenSSL 3.0.15" },
      pyHanko: { command: "pyhanko", version: "pyHanko, version 0.36.2 (CLI 0.4.2)" }
    },
    platform: "win32",
    product: "Tüfekci Paperworks",
    releaseVersion: "0.1.0-alpha.1",
    scenarios: {
      encryptedSourcePreserved: true,
      incrementalSignatureCount: 2,
      integrityAndTrustSeparated: true,
      timestampTested: false,
      trustedValidation: true,
      visibleSignaturePublished: true
    },
    schemaVersion: 1,
    sourceFixtureSha256: "A".repeat(64)
  };
}

test("parses one closed native certificate evidence marker", () => {
  const marker = parseCertificateMarker(
    "test output\nPAPERWORKS_CERTIFICATE_V1\t1\t2\t1\t1\t1\t0\tpyHanko, version 0.36.2 (CLI 0.4.2)\n"
  );
  assert.equal(marker.incrementalSignatureCount, 2);
  assert.equal(marker.timestampTested, false);
  assert.throws(
    () => parseCertificateMarker("PAPERWORKS_CERTIFICATE_V1\t1\t1\t1\t1\t1\t0\t0.36.2"),
    /malformed/u
  );
  assert.throws(
    () => parseCertificateMarker("PAPERWORKS_CERTIFICATE_V1\t1\t2\t1\t1\t1\t0\t0.36.2\nPAPERWORKS_CERTIFICATE_V1\t1\t2\t1\t1\t1\t0\t0.36.2"),
    /exactly one/u
  );
});

test("validates path-free certificate engine evidence and enforces timestamp policy", () => {
  const report = validReport();
  assert.equal(validateCertificateEngineReport(report), report);
  assert.throws(
    () => validateCertificateEngineReport({ ...report, corpusPath: "C:\\Private" }),
    /unknown fields/u
  );
  assert.throws(
    () => validateCertificateEngineReport(report, { requireTimestamp: true }),
    /timestamp evidence/u
  );
  const timestamped = {
    ...report,
    scenarios: { ...report.scenarios, timestampTested: true }
  };
  assert.equal(
    validateCertificateEngineReport(timestamped, { requireTimestamp: true }),
    timestamped
  );
});

test("keeps disposable identity material out of the retained certificate report", async () => {
  const source = await readFile(
    new URL("../scripts/run-certificate-engine-corpus.mjs", import.meta.url),
    "utf8"
  );
  assert.match(source, /mkdtemp/u);
  assert.match(source, /rm\(temporaryDirectory, \{ force: true, recursive: true \}\)/u);
  assert.match(source, /COPYFILE_EXCL/u);
  assert.doesNotMatch(JSON.stringify(validReport()), /passphrase|password|privateKey|path/iu);
});

test("requires timestamped certificate evidence on every tagged desktop release", async () => {
  const workflow = await readFile(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8"
  );
  assert.match(
    workflow,
    /OCR, PDF\/A and certificate engine evidence \(\$\{\{ matrix\.platform \}\}\)[\s\S]+ubuntu-24\.04[\s\S]+macos-latest[\s\S]+windows-latest/u
  );
  assert.match(workflow, /"pyHanko==0\.36\.2"/u);
  assert.match(workflow, /"pyhanko-cli==0\.4\.2"/u);
  assert.match(workflow, /PAPERWORKS_REQUIRE_CERTIFICATE_TIMESTAMP: "1"/u);
  assert.match(workflow, /PAPERWORKS_TEST_TSA_URL: \$\{\{ secrets\.PAPERWORKS_TEST_TSA_URL \}\}/u);
  assert.match(workflow, /run: npm run qa:certificate-engine/u);
  assert.match(workflow, /pattern: release-certificate-engine-\*/u);
  assert.match(workflow, /release-certificate-engine-evidence\/\*/u);
});
