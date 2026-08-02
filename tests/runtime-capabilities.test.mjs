import assert from "node:assert/strict";
import test from "node:test";
import {
  isAppleMobileRuntime,
  parseRuntimeCapabilities
} from "../src/runtimeCapabilities.ts";

const iosCapabilities = {
  platform: "ios",
  mobile: true,
  nativeFileDialogs: true,
  localPdfEditing: true,
  localVisualMarks: true,
  imageToPdf: true,
  externalProcesses: false,
  connectedScanning: false,
  cameraCapture: false,
  searchableOcr: false,
  certificateSigning: false,
  archivalPdf: false,
  passwordProtection: false,
  directUpdates: false,
  appStoreUpdates: true
};

test("accepts the explicit iPhone and iPad capability contract", () => {
  const capabilities = parseRuntimeCapabilities(iosCapabilities);
  assert.equal(capabilities.platform, "ios");
  assert.equal(capabilities.localPdfEditing, true);
  assert.equal(capabilities.externalProcesses, false);
  assert.equal(capabilities.appStoreUpdates, true);
  assert.equal(isAppleMobileRuntime(capabilities), true);
  assert.equal(Object.isFrozen(capabilities), true);
});

test("rejects missing, malformed and internally inconsistent reports", () => {
  assert.throws(() => parseRuntimeCapabilities(null), /invalid/u);
  assert.throws(
    () => parseRuntimeCapabilities({ ...iosCapabilities, searchableOcr: "yes" }),
    /invalid/u
  );
  assert.throws(
    () => parseRuntimeCapabilities({ ...iosCapabilities, unexpected: false }),
    /invalid/u
  );
  assert.throws(
    () => parseRuntimeCapabilities({ ...iosCapabilities, externalProcesses: true }),
    /inconsistent/u
  );
  assert.throws(
    () => parseRuntimeCapabilities({ ...iosCapabilities, platform: "linux" }),
    /inconsistent/u
  );
  assert.throws(
    () =>
      parseRuntimeCapabilities({
        ...iosCapabilities,
        platform: "android",
        appStoreUpdates: true
      }),
    /inconsistent/u
  );
  assert.throws(
    () => parseRuntimeCapabilities({ ...iosCapabilities, appStoreUpdates: false }),
    /inconsistent/u
  );
});
