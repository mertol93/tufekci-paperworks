import { access, mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspace = path.dirname(fileURLToPath(import.meta.url));
const application = path.join(
  workspace,
  "src-tauri",
  "target",
  "debug",
  process.platform === "win32" ? "tufekci-paperworks.exe" : "tufekci-paperworks"
);
const evidenceDirectory = path.join(workspace, "e2e-evidence");
const captureBackendLogs = process.env.PAPERWORKS_E2E_DEBUG === "1";

export const config = {
  runner: "local",
  specs: ["./e2e/**/*.e2e.mjs"],
  maxInstances: 1,
  logLevel: "warn",
  bail: 0,
  waitforTimeout: 30_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 1,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    timeout: 180_000
  },
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application
      }
    }
  ],
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: application,
        captureBackendLogs,
        captureFrontendLogs: false,
        driverProvider: "embedded",
        embeddedPort: 4_445,
        statusPollTimeout: 15_000
      }
    ]
  ],
  onPrepare: async () => {
    await access(application);
    await rm(evidenceDirectory, { force: true, recursive: true });
    await mkdir(evidenceDirectory, { recursive: true });
  }
};
