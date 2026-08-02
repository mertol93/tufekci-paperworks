import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  updaterReleaseOverlay,
  validateUpdaterReleaseEnvironment,
  writeUpdaterReleaseConfig
} from "../scripts/generate-updater-config.mjs";
import { validateUpdaterManifest } from "../scripts/check-updater-manifest.mjs";
import { validateUpdatePromotion } from "../scripts/check-update-promotion.mjs";

const releaseWorkflow = readFileSync(
  new URL("../.github/workflows/release.yml", import.meta.url),
  "utf8"
);
const promotionWorkflow = readFileSync(
  new URL("../.github/workflows/promote-update.yml", import.meta.url),
  "utf8"
);

function encodedMinisignBlock(length) {
  const bytes = Buffer.alloc(length);
  bytes[0] = 0x45;
  bytes[1] = 0x64;
  return bytes.toString("base64");
}

const publicKeyDocument =
  `untrusted comment: minisign public key\n${encodedMinisignBlock(42)}`;
const publicKey = Buffer.from(publicKeyDocument, "utf8").toString("base64");
const signatureDocument = [
  "untrusted comment: signature from minisign secret key",
  encodedMinisignBlock(74),
  "trusted comment: timestamp:1785153600\tfile:paperworks-update",
  Buffer.alloc(64).toString("base64")
].join("\n");
const signature = Buffer.from(signatureDocument, "utf8").toString("base64");
const release = {
  channel: "alpha",
  repository: "tufekci/paperworks",
  tag: "v0.1.0-alpha.1",
  version: "0.1.0-alpha.1"
};

function releaseEnvironment(overrides = {}) {
  return {
    PAPERWORKS_UPDATE_CHANNEL: "alpha",
    PAPERWORKS_UPDATE_ENDPOINT:
      "https://github.com/tufekci/paperworks/releases/download/updates-alpha/latest.json",
    PAPERWORKS_UPDATE_PUBLIC_KEY: publicKey,
    TAURI_SIGNING_PRIVATE_KEY: "private signing material supplied only by CI",
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "private password",
    ...overrides
  };
}

function updaterManifest(overrides = {}) {
  const platform = (name) => ({
    signature,
    url: `https://github.com/tufekci/paperworks/releases/download/${release.tag}/paperworks-${name}.tar.gz`
  });
  return {
    version: release.version,
    notes: "Reviewed alpha update.",
    pub_date: "2026-07-27T12:00:00Z",
    platforms: {
      "darwin-aarch64": platform("darwin-aarch64"),
      "darwin-x86_64": platform("darwin-x86_64"),
      "linux-x86_64": platform("linux-x86_64"),
      "windows-x86_64": platform("windows-x86_64")
    },
    ...overrides
  };
}

test("accepts one bounded credential-backed updater release environment", () => {
  assert.deepEqual(validateUpdaterReleaseEnvironment(releaseEnvironment()), {
    channel: "alpha",
    endpoint:
      "https://github.com/tufekci/paperworks/releases/download/updates-alpha/latest.json",
    publicKey
  });
  assert.throws(
    () => validateUpdaterReleaseEnvironment(releaseEnvironment({ TAURI_SIGNING_PRIVATE_KEY: "" })),
    /private updater signing key/u
  );
  assert.throws(
    () =>
      validateUpdaterReleaseEnvironment(
        releaseEnvironment({ PAPERWORKS_UPDATE_ENDPOINT: "http://example.test/updates-alpha/latest.json" })
      ),
    /ordinary HTTPS/u
  );
  assert.throws(
    () =>
      validateUpdaterReleaseEnvironment(
        releaseEnvironment({ PAPERWORKS_UPDATE_CHANNEL: "stable" })
      ),
    /updates-stable/u
  );
  assert.throws(
    () =>
      validateUpdaterReleaseEnvironment(
        releaseEnvironment({ PAPERWORKS_UPDATE_PUBLIC_KEY: publicKeyDocument })
      ),
    /canonical base64/u
  );
  assert.throws(
    () =>
      validateUpdaterReleaseEnvironment(
        releaseEnvironment({ PAPERWORKS_UPDATE_PUBLIC_KEY: `${publicKey} ` })
      ),
    /without surrounding space/u
  );
});

test("writes a release overlay without any updater key or endpoint", async () => {
  const destination = path.join(tmpdir(), `paperworks-updater-${process.pid}.json`);
  try {
    await writeUpdaterReleaseConfig(destination, releaseEnvironment());
    const text = await readFile(destination, "utf8");
    assert.deepEqual(JSON.parse(text), updaterReleaseOverlay());
    assert.equal(JSON.parse(text).bundle.createUpdaterArtifacts, true);
    assert.doesNotMatch(text, /private|password|github\.com|dW50/u);
  } finally {
    await rm(destination, { force: true });
  }
});

test("accepts only a complete signed-update manifest for the immutable release", () => {
  const manifest = updaterManifest();
  const report = validateUpdaterManifest(manifest, release, Buffer.from("manifest"));
  assert.equal(report.channel, "alpha");
  assert.equal(report.releaseTag, release.tag);
  assert.equal(report.apiAssetUrlCount, 0);
  assert.deepEqual(report.platformKeys, [
    "darwin-aarch64",
    "darwin-x86_64",
    "linux-x86_64",
    "windows-x86_64"
  ]);
  assert.equal(report.manifestSha256.length, 64);

  assert.throws(
    () => validateUpdaterManifest(updaterManifest({ version: "0.1.0-alpha.2" }), release),
    /version does not match/u
  );
  assert.throws(
    () =>
      validateUpdaterManifest(
        updaterManifest({
          platforms: {
            ...manifest.platforms,
            "windows-x86_64": {
              signature,
              url: "https://example.test/v0.1.0-alpha.1/windows.exe"
            }
          }
        }),
        release
      ),
    /does not belong/u
  );
  assert.throws(
    () =>
      validateUpdaterManifest(
        updaterManifest({ platforms: { ...manifest.platforms, "linux-x86_64": undefined } }),
        release
      ),
    /must be an object/u
  );
  assert.throws(
    () => validateUpdaterManifest({ ...manifest, unexpected: true }, release),
    /unknown field/u
  );
  assert.throws(
    () =>
      validateUpdaterManifest(
        updaterManifest({
          platforms: {
            ...manifest.platforms,
            "linux-x86_64": {
              ...manifest.platforms["linux-x86_64"],
              signature: Buffer.from("untrusted comment: incomplete", "utf8").toString("base64")
            }
          }
        }),
        release
      ),
    /valid encoded Minisign document/u
  );
});

test("accepts installer-specific platform entries without losing the required families", () => {
  const manifest = updaterManifest();
  manifest.platforms["darwin-universal"] = {
    signature,
    url: `https://github.com/tufekci/paperworks/releases/download/${release.tag}/paperworks-universal.tar.gz`
  };
  manifest.platforms["darwin-universal-app"] = {
    signature,
    url: `https://github.com/tufekci/paperworks/releases/download/${release.tag}/paperworks-universal.tar.gz`
  };
  manifest.platforms["windows-x86_64-nsis"] = {
    signature,
    url: `https://github.com/tufekci/paperworks/releases/download/${release.tag}/paperworks-nsis.zip`
  };
  const report = validateUpdaterManifest(manifest, release);
  assert.ok(report.platformKeys.includes("windows-x86_64"));
  assert.ok(report.platformKeys.includes("windows-x86_64-nsis"));
  assert.ok(report.platformKeys.includes("darwin-universal"));
});

test("binds Tauri Action API asset URLs to the immutable release inventory", () => {
  const manifest = updaterManifest();
  manifest.platforms["windows-x86_64"].url =
    "https://api.github.com/repos/tufekci/paperworks/releases/assets/42001";
  const report = validateUpdaterManifest(manifest, {
    ...release,
    releaseAssetIds: new Set([42001])
  });
  assert.equal(report.apiAssetUrlCount, 1);
  assert.throws(
    () =>
      validateUpdaterManifest(manifest, {
        ...release,
        releaseAssetIds: new Set([99999])
      }),
    /immutable release inventory/u
  );
});

test("runs the release manifest CLI with an exact bounded asset inventory", async () => {
  const workspace = path.join(tmpdir(), `paperworks-updater-cli-${process.pid}`);
  const manifestPath = path.join(workspace, "latest.json");
  const assetsPath = path.join(workspace, "release-assets.json");
  const reportPath = path.join(workspace, "report.json");
  try {
    await mkdir(workspace, { recursive: true });
    await writeFile(manifestPath, `${JSON.stringify(updaterManifest())}\n`, "utf8");
    await writeFile(assetsPath, '{"assetIds":[42001]}\n', "utf8");
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../scripts/check-updater-manifest.mjs", import.meta.url)),
        manifestPath,
        "--version",
        release.version,
        "--tag",
        release.tag,
        "--channel",
        release.channel,
        "--repository",
        release.repository,
        "--release-assets",
        assetsPath,
        "--report",
        reportPath
      ],
      { encoding: "utf8", windowsHide: true }
    );
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const reportText = await readFile(reportPath, "utf8");
    const report = JSON.parse(reportText);
    assert.equal(report.apiAssetUrlCount, 0);
    assert.doesNotMatch(reportText, new RegExp(workspace.replaceAll("\\", "\\\\"), "u"));
  } finally {
    await rm(workspace, { force: true, recursive: true });
  }
});

test("promotes only a published immutable release with matching manifest evidence", () => {
  const manifestReport = validateUpdaterManifest(
    updaterManifest(),
    release,
    Buffer.from("manifest")
  );
  const sourceRelease = {
    isDraft: false,
    isPrerelease: true,
    tagName: release.tag,
    url: `https://github.com/${release.repository}/releases/tag/${release.tag}`
  };
  const report = validateUpdatePromotion(sourceRelease, manifestReport, release);
  assert.equal(report.channelTag, "updates-alpha");
  assert.equal(report.manifestSha256, manifestReport.manifestSha256);
  assert.throws(
    () => validateUpdatePromotion({ ...sourceRelease, isDraft: true }, manifestReport, release),
    /published immutable release/u
  );
  assert.throws(
    () =>
      validateUpdatePromotion(sourceRelease, manifestReport, {
        ...release,
        channel: "stable"
      }),
    /do not agree/u
  );
});

test("gates signed builds and channel promotion in separate approved workflows", () => {
  assert.match(releaseWorkflow, /environment: updater-signing/u);
  assert.match(releaseWorkflow, /TAURI_SIGNING_PRIVATE_KEY: \$\{\{ secrets\./u);
  assert.match(releaseWorkflow, /npm run release:updater-config/u);
  assert.match(releaseWorkflow, /tauri-apps\/tauri-action@v1/u);
  assert.match(releaseWorkflow, /uploadUpdaterJson: true/u);
  assert.match(releaseWorkflow, /uploadUpdaterSignatures: true/u);
  assert.match(releaseWorkflow, /npm run release:updater-manifest/u);
  assert.match(releaseWorkflow, /--release-assets release-updater\/release-assets\.json/u);
  assert.match(promotionWorkflow, /workflow_dispatch/u);
  assert.match(promotionWorkflow, /environment: updater-promotion/u);
  assert.match(promotionWorkflow, /npm run release:update-promotion/u);
  assert.match(promotionWorkflow, /cmp --silent/u);
});
