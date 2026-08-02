import assert from "node:assert/strict";
import { readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import {
  validatePlatformSigningEnvironment,
  validatePlatformSigningOverlay,
  writePlatformSigningConfig
} from "../scripts/generate-platform-signing-config.mjs";

const certificate = Buffer.alloc(2_048, 0x42).toString("base64");
const teamId = "A1B2C3D4E5";
const privateKey = [
  ["-----BEGIN", "PRIVATE", "KEY-----"].join(" "),
  Buffer.alloc(128, 0x24).toString("base64"),
  ["-----END", "PRIVATE", "KEY-----"].join(" ")
].join("\n");

function windowsEnvironment(overrides = {}) {
  return {
    PAPERWORKS_WINDOWS_CERTIFICATE_THUMBPRINT: "a".repeat(40),
    PAPERWORKS_WINDOWS_TIMESTAMP_URL: "https://timestamp.example.test/rfc3161",
    WINDOWS_CERTIFICATE: certificate,
    WINDOWS_CERTIFICATE_PASSWORD: "private PFX password",
    ...overrides
  };
}

function macosEnvironment(overrides = {}) {
  return {
    PAPERWORKS_APPLE_TEAM_ID: teamId,
    APPLE_SIGNING_IDENTITY: `Developer ID Application: Tufekci Paperworks (${teamId})`,
    APPLE_CERTIFICATE: certificate,
    APPLE_CERTIFICATE_PASSWORD: "private P12 password",
    KEYCHAIN_PASSWORD: "temporary keychain password",
    APPLE_API_ISSUER: "12345678-1234-1234-1234-1234567890ab",
    APPLE_API_KEY: "AB12CD34EF",
    APPLE_API_PRIVATE_KEY: privateKey,
    ...overrides
  };
}

test("creates a secret-free Windows publisher-signing overlay", () => {
  const result = validatePlatformSigningEnvironment("windows", windowsEnvironment());
  assert.equal(result.expectedSignerIdentity, "A".repeat(40));
  assert.deepEqual(result.overlay.bundle.windows, {
    certificateThumbprint: "A".repeat(40),
    digestAlgorithm: "sha256",
    timestampUrl: "https://timestamp.example.test/rfc3161"
  });
  assert.equal(validatePlatformSigningOverlay(result.overlay, "windows"), "A".repeat(40));
  const text = JSON.stringify(result.overlay);
  assert.doesNotMatch(text, /private|QkJCQk|password/u);

  assert.throws(
    () =>
      validatePlatformSigningEnvironment(
        "windows",
        windowsEnvironment({ PAPERWORKS_WINDOWS_TIMESTAMP_URL: "http://timestamp.example.test" })
      ),
    /ordinary HTTPS/u
  );
  assert.throws(
    () =>
      validatePlatformSigningEnvironment(
        "windows",
        windowsEnvironment({ WINDOWS_CERTIFICATE: `${certificate}\n` })
      ),
    /one line/u
  );
});

test("binds a macOS Developer ID identity to its notarisation team", () => {
  const result = validatePlatformSigningEnvironment("macos", macosEnvironment());
  assert.equal(result.expectedSignerIdentity, teamId);
  assert.deepEqual(result.overlay.bundle.macOS, {
    hardenedRuntime: true,
    signingIdentity: `Developer ID Application: Tufekci Paperworks (${teamId})`
  });
  assert.equal(validatePlatformSigningOverlay(result.overlay, "macos"), teamId);
  const text = JSON.stringify(result.overlay);
  assert.doesNotMatch(text, /private P12|BEGIN PRIVATE|QkJCQk/u);

  assert.throws(
    () =>
      validatePlatformSigningEnvironment(
        "macos",
        macosEnvironment({ APPLE_SIGNING_IDENTITY: "Apple Development: Example (A1B2C3D4E5)" })
      ),
    /Developer ID Application/u
  );
  assert.throws(
    () =>
      validatePlatformSigningEnvironment(
        "macos",
        macosEnvironment({
          APPLE_SIGNING_IDENTITY: `Developer ID Application: Bad\tName (${teamId})`
        })
      ),
    /Developer ID Application/u
  );
  assert.throws(
    () =>
      validatePlatformSigningEnvironment(
        "macos",
        macosEnvironment({ KEYCHAIN_PASSWORD: "too-short" })
      ),
    /size limit/u
  );
  assert.throws(
    () =>
      validatePlatformSigningEnvironment(
        "macos",
        macosEnvironment({ APPLE_API_PRIVATE_KEY: ` ${privateKey}` })
      ),
    /canonical PKCS#8/u
  );
  assert.throws(
    () =>
      validatePlatformSigningOverlay(
        {
          ...result.overlay,
          bundle: {
            macOS: {
              ...result.overlay.bundle.macOS,
              signingIdentity: `Developer ID Application: Bad\tName (${teamId})`
            }
          }
        },
        "macos"
      ),
    /macOS signing settings/u
  );
});

test("keeps Linux signing configuration explicit and credential-free", () => {
  const result = validatePlatformSigningEnvironment("linux", {});
  assert.equal(result.expectedSignerIdentity, null);
  assert.deepEqual(result.overlay, {
    $schema: "https://schema.tauri.app/config/2",
    bundle: {}
  });
  assert.equal(validatePlatformSigningOverlay(result.overlay, "linux"), null);
  assert.throws(
    () => validatePlatformSigningOverlay({ ...result.overlay, privateKey: "leak" }, "linux"),
    /unknown fields/u
  );
});

test("writes and revalidates only the public platform overlay", async () => {
  const destinations = [
    ["windows", windowsEnvironment()],
    ["macos", macosEnvironment()],
    ["linux", {}]
  ];
  for (const [platform, environment] of destinations) {
    const destination = path.join(
      tmpdir(),
      `paperworks-${platform}-signing-${process.pid}.json`
    );
    try {
      await writePlatformSigningConfig(destination, platform, environment);
      const text = await readFile(destination, "utf8");
      validatePlatformSigningOverlay(JSON.parse(text), platform);
      assert.doesNotMatch(text, /private|password|BEGIN PRIVATE|QkJCQk/u);
      await assert.rejects(
        writePlatformSigningConfig(destination, platform, environment),
        /exist/iu
      );
    } finally {
      await rm(destination, { force: true });
    }
  }
});

test("gates tagged packages on ephemeral publisher credentials and strict evidence", async () => {
  const workflow = await readFile(
    new URL("../.github/workflows/release.yml", import.meta.url),
    "utf8"
  );
  assert.match(workflow, /environment: updater-signing/u);
  assert.match(workflow, /Generate Windows publisher-signing configuration/u);
  assert.match(workflow, /WINDOWS_CERTIFICATE: \$\{\{ secrets\.WINDOWS_CERTIFICATE \}\}/u);
  assert.match(workflow, /1\.3\.6\.1\.5\.5\.7\.3\.3/u);
  assert.match(workflow, /paperworks-imported-certificate-thumbprints\.txt/u);
  assert.match(workflow, /Generate macOS publisher-signing configuration/u);
  assert.match(workflow, /APPLE_API_PRIVATE_KEY: \$\{\{ secrets\.APPLE_API_PRIVATE_KEY \}\}/u);
  assert.match(workflow, /security set-key-partition-list/u);
  assert.match(workflow, /security list-keychains -d user -s/u);
  assert.match(workflow, /Remove the ephemeral Windows publisher certificate/u);
  assert.match(workflow, /Remove the ephemeral macOS signing material/u);
  assert.match(
    workflow,
    /--config src-tauri\/updater\.release\.conf\.json --config src-tauri\/platform-signing\.release\.conf\.json/u
  );
  assert.match(workflow, /--signature-policy signed-required/u);
  assert.match(workflow, /--signing-config src-tauri\/platform-signing\.release\.conf\.json/u);
  assert.doesNotMatch(workflow, /--signature-policy unsigned-allowed/u);
});
