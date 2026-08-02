import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

const featureStatusUrl = new URL("../docs/FEATURE_STATUS.md", import.meta.url);
const featureStatus = readFileSync(featureStatusUrl, "utf8");
const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
const releasePlan = readFileSync(new URL("../docs/RELEASE_PLAN.md", import.meta.url), "utf8");
const security = readFileSync(new URL("../SECURITY.md", import.meta.url), "utf8");

test("publishes complete, experimental, and unavailable capability classes", () => {
  assert.match(featureStatus, /^# Feature Status$/mu);
  assert.match(featureStatus, /^## Complete Workflows$/mu);
  assert.match(featureStatus, /^## Experimental Workflows$/mu);
  assert.match(featureStatus, /^## Unavailable Workflows$/mu);
  assert.match(featureStatus, /\b0\.1\.0-alpha\.1\b/u);
  assert.match(featureStatus, /\| Page-content editing \|/u);
  assert.match(featureStatus, /exact, unshared, page-level content/iu);
  assert.match(featureStatus, /dedicated existing-PDF Recognise Text workspace/iu);
  assert.match(featureStatus, /optional AES-256 output/iu);
  assert.match(featureStatus, /optional linked A4 contents pages/iu);
  assert.match(featureStatus, /physical output page numbers and are not tagged/iu);

  for (const unavailable of [
    "Full layout reflow",
    "PDF/UA and PDF/X conversion",
    "XFA form editing"
  ]) {
    assert.match(featureStatus, new RegExp(unavailable.replaceAll("/", "\\/"), "u"));
  }
  assert.match(featureStatus, /\| PDF\/A archival \|/u);
  assert.match(featureStatus, /PDF\/A-1b, PDF\/A-2b, and PDF\/A-3b/u);
  assert.match(featureStatus, /\| Signed application updates \|/u);
  assert.match(featureStatus, /first real credential-backed three-platform build/iu);
  assert.doesNotMatch(featureStatus, /- A generated printed contents page\./u);
});

test("retains the public security boundaries and UK English", () => {
  assert.match(featureStatus, /reader permissions are advisory/iu);
  assert.match(featureStatus, /visual signature does not authenticate a signer/iu);
  assert.match(featureStatus, /OCR text can contain recognition errors/iu);
  assert.match(featureStatus, /Ordinary local and development packages may be unsigned/iu);
  assert.match(featureStatus, /configured\s+Authenticode signer and carry a timestamp/iu);
  assert.match(featureStatus, /pass Gatekeeper, and contain a valid\s+stapled notarisation ticket/iu);
  assert.match(featureStatus, /embedded public\s+key/iu);
  assert.match(featureStatus, /expire after seven days/iu);
  assert.match(featureStatus, /\bcolour\b/u);
  assert.match(featureStatus, /\blicence\b/u);
  assert.match(featureStatus, /\bnormalisation\b/u);
  assert.doesNotMatch(featureStatus, /\b(color|license|normalization|recognized)\b/iu);
});

test("keeps release-facing links and local status references valid", () => {
  assert.match(readme, /\[feature status and security boundaries\]\(docs\/FEATURE_STATUS\.md\)/u);
  assert.match(security, /\[feature status\]\(docs\/FEATURE_STATUS\.md\)/u);
  assert.match(
    releasePlan,
    /- \[x\] Public feature-status documentation that distinguishes complete, experimental,/u
  );

  const localLinks = [...featureStatus.matchAll(/\[[^\]]+\]\((?!https?:)([^)#]+)(?:#[^)]+)?\)/gu)];
  assert.ok(localLinks.length >= 5);
  for (const [, target] of localLinks) {
    assert.ok(existsSync(new URL(target, featureStatusUrl)), `Missing local status link: ${target}`);
  }
});
