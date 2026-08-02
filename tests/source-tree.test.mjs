import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  classifySourcePath,
  sensitiveTextFinding,
  validateCandidatePaths
} from "../scripts/check-source-tree.mjs";

const requiredPaths = [
  ".gitattributes",
  ".gitignore",
  ".github/dependabot.yml",
  ".github/workflows/apple-mobile.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/promote-update.yml",
  ".github/workflows/release.yml",
  "CHANGELOG.md",
  "CONTRIBUTING.md",
  "LICENSE",
  "README.md",
  "SECURITY.md",
  "docs/E2E_TESTING.md",
  "docs/FEATURE_STATUS.md",
  "docs/PRINTING_TESTING.md",
  "docs/RELEASE_PLAN.md",
  "docs/UPDATES.md",
  "e2e/native-shell.e2e.mjs",
  "package-lock.json",
  "package.json",
  "scripts/check-e2e-evidence.mjs",
  "scripts/check-e2e-matrix.mjs",
  "scripts/check-apple-mobile.mjs",
  "scripts/check-apple-mobile-bundle.mjs",
  "scripts/check-production-e2e-boundary.mjs",
  "scripts/e2e-evidence.mjs",
  "scripts/generate-platform-signing-config.mjs",
  "src-tauri/Cargo.lock",
  "src-tauri/Cargo.toml",
  "src-tauri/Info.ios.plist",
  "src-tauri/tauri.conf.json",
  "src-tauri/tauri.e2e.conf.json",
  "src-tauri/tauri.ios.conf.json",
  "src/e2eBridgeDisabled.ts",
  "src/e2eBridgeEnabled.ts",
  "src/PageTransferDialog.tsx",
  "src/pageSelection.ts",
  "src/pageTransfer.ts",
  "src/runtimeCapabilities.ts",
  "src/PrintStudio.tsx",
  "src/print.ts",
  "src/printRenderer.ts",
  "tests/page-selection.test.mjs",
  "tests/page-transfer.test.mjs",
  "tests/print.test.mjs",
  "tests/apple-mobile.test.mjs",
  "tests/runtime-capabilities.test.mjs",
  "wdio.conf.mjs"
];

test("accepts the required source roots and known text and binary types", () => {
  assert.deepEqual(
    validateCandidatePaths([...requiredPaths].reverse()),
    [...requiredPaths].sort((left, right) => left.localeCompare(right, "en-GB"))
  );
  assert.equal(classifySourcePath(".gitignore"), "text");
  assert.equal(classifySourcePath("src-tauri/binaries/.gitkeep"), "text");
  assert.equal(classifySourcePath("src/App.tsx"), "text");
  assert.equal(classifySourcePath("src-tauri/Info.ios.plist"), "text");
  assert.equal(classifySourcePath("src-tauri/native/macos-scanner/main.m"), "text");
  assert.equal(classifySourcePath("src-tauri/icons/icon.icns"), "binary");
  assert.equal(
    classifySourcePath("src-tauri/assets/fonts/LICENSE_LIBERATION.txt"),
    "text"
  );
  assert.equal(
    classifySourcePath("src-tauri/assets/fonts/LiberationSans-Regular.ttf"),
    "binary"
  );
});

test("rejects generated, private, unsafe, duplicate, and unknown source paths", () => {
  for (const candidate of [
    ".env",
    "artifacts/release.zip",
    "node_modules/example/index.js",
    "src-tauri/target/release/app.exe",
    "private/signing-identity.p12"
  ]) {
    assert.throws(
      () => validateCandidatePaths([...requiredPaths, candidate]),
      /not allowed|Private key material/u
    );
  }
  assert.throws(
    () => validateCandidatePaths([...requiredPaths, "README.MD"]),
    /duplicate path/u
  );
  assert.throws(() => classifySourcePath("fixture.bin"), /Unsupported source file type/u);
});

test("detects representative credentials and personal home paths", () => {
  assert.equal(
    sensitiveTextFinding(["-----BEGIN ", "PRIVATE KEY-----\nprivate\n"].join("")),
    "a private-key block"
  );
  assert.equal(
    sensitiveTextFinding(["token=ghp_", "abcdefghijklmnopqrstuvwxyz1234567890"].join("")),
    "a GitHub token"
  );
  assert.equal(
    sensitiveTextFinding(["C:", "Users", "person", "Documents", "private.pdf"].join("\\")),
    "a personal absolute home path"
  );
  assert.equal(
    sensitiveTextFinding("PAPERWORKS_CERTIFICATE_PASSPHRASE='<private passphrase>'"),
    null
  );
});

test("audits the complete repository source candidate set", () => {
  const script = fileURLToPath(new URL("../scripts/check-source-tree.mjs", import.meta.url));
  const result = spawnSync(process.execPath, [script], {
    encoding: "utf8",
    windowsHide: true
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /Source tree audit passed for \d+ files \(\d+ text, \d+ binary, \d+ bytes\)/u
  );
});
