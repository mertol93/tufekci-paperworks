import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  applyUpdateDownloadEvent,
  updateChannelLabel,
  updateProgressPercentage
} from "../src/appUpdates.ts";
import { translate } from "../src/i18n.ts";

const dialog = readFileSync(new URL("../src/UpdateDialog.tsx", import.meta.url), "utf8");
const british = (key, values) => translate("en-GB", key, values);

test("accumulates bounded updater download progress", () => {
  let progress = applyUpdateDownloadEvent(
    { downloaded: 400, total: null },
    { event: "Started", data: { contentLength: 1_000 } }
  );
  assert.deepEqual(progress, { downloaded: 0, total: 1_000 });
  progress = applyUpdateDownloadEvent(progress, {
    event: "Progress",
    data: { chunkLength: 425 }
  });
  assert.equal(updateProgressPercentage(progress), 43);
  progress = applyUpdateDownloadEvent(progress, { event: "Finished" });
  assert.deepEqual(progress, { downloaded: 1_000, total: 1_000 });
  assert.equal(updateProgressPercentage(progress), 100);
});

test("handles unknown lengths and rejects invalid byte counts", () => {
  let progress = applyUpdateDownloadEvent(
    { downloaded: 0, total: null },
    { event: "Started", data: { contentLength: null } }
  );
  progress = applyUpdateDownloadEvent(progress, {
    event: "Progress",
    data: { chunkLength: -2 }
  });
  assert.deepEqual(progress, { downloaded: 0, total: null });
  assert.equal(updateProgressPercentage(progress), null);
});

test("uses explicit release-channel labels", () => {
  assert.equal(updateChannelLabel("alpha", british), "Alpha");
  assert.equal(updateChannelLabel("beta", british), "Beta");
  assert.equal(updateChannelLabel("stable", british), "Stable");
  assert.equal(updateChannelLabel("nightly", british), "Unknown");
  assert.equal(
    updateChannelLabel("stable", (key, values) => translate("tr-TR", key, values)),
    "Kararlı"
  );
});

test("keeps update checks user-triggered and signature verification non-bypassable", () => {
  assert.match(dialog, /onClick=\{\(\) => void checkForUpdate\(\)\}/u);
  assert.doesNotMatch(dialog, /useEffect\([\s\S]{0,800}checkForUpdate\(/u);
  assert.match(dialog, /t\("update\.assurance"\)/u);
  assert.doesNotMatch(dialog, /Failed verification is never bypassed/u);
  assert.match(dialog, /closeDisabled = stage === "installing" \|\| stage === "restarting"/u);
  assert.match(dialog, /disabled=\{closeDisabled\}[\s\S]{0,180}onClick=\{onClose\}/u);
});

test("routes iPhone and iPad builds to the App Store without invoking the desktop updater", () => {
  assert.match(dialog, /result\.managedByStore\s*\?\s*"store"/u);
  assert.match(dialog, /stage === "store" \? t\("update\.assurance\.store"\)/u);
  assert.match(dialog, /case "store":/u);
  assert.match(dialog, /t\("update\.status\.store\.heading"\)/u);
  assert.match(dialog, /t\("update\.channel\.appStore"\)/u);
});
