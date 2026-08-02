import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  classifySignatureVaultError,
  localiseSignatureVaultError,
  localiseSignatureVaultStatus,
  signatureVaultPassphraseIsValid
} from "../src/signatureVaultOutcomes.ts";
import { translate } from "../src/i18n.ts";

const t = (locale) => (key, values) => translate(locale, key, values);

test("accepts only allow-listed native vault outcomes", () => {
  assert.equal(
    classifySignatureVaultError("passphrase-rejected", "unlock"),
    "passphrase-rejected"
  );
  assert.equal(
    classifySignatureVaultError("private native vault detail", "unlock"),
    "unlock-failed"
  );
  assert.equal(
    classifySignatureVaultError(new Error("private cryptography detail"), "save"),
    "save-failed"
  );
  assert.equal(classifySignatureVaultError(null, "list"), "library-unavailable");
});

test("validates vault passphrases by characters, UTF-8 bytes, and control safety", () => {
  assert.equal(signatureVaultPassphraseIsValid("too short"), false);
  assert.equal(signatureVaultPassphraseIsValid("correct horse battery staple"), true);
  assert.equal(signatureVaultPassphraseIsValid("twelve chars\n"), false);
  assert.equal(signatureVaultPassphraseIsValid("ü".repeat(513)), false);
});

test("localises stable vault errors and statuses in the selected locale", () => {
  assert.equal(
    localiseSignatureVaultError("passphrase-rejected", t("en-GB")),
    "The passphrase is incorrect, or the stored visual mark has been altered."
  );
  assert.match(
    localiseSignatureVaultError("entry-invalid", t("de-DE")),
    /beschädigt/u
  );
  assert.match(
    localiseSignatureVaultStatus({ code: "unlocked", name: "Ana imza" }, t("tr-TR")),
    /Ana imza/u
  );
});

test("keeps native vault details behind a typed command boundary", async () => {
  const [component, nativeVault] = await Promise.all([
    readFile(new URL("../src/SignatureVault.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/signature_vault.rs", import.meta.url), "utf8")
  ]);

  assert.match(component, /useState<SignatureVaultErrorCode \| null>/u);
  assert.match(component, /classifySignatureVaultError\(reason, "unlock"\)/u);
  assert.doesNotMatch(component, /reason\.message|String\(reason\)|function errorMessage/u);
  assert.match(nativeVault, /pub enum SignatureVaultErrorCode/u);
  assert.match(
    nativeVault,
    /Result<UnlockedSignature, SignatureVaultErrorCode>/u
  );
  const commandBoundary = nativeVault.slice(
    nativeVault.indexOf("pub async fn unlock_signature_vault"),
    nativeVault.indexOf("pub fn delete_signature_vault")
  );
  assert.doesNotMatch(commandBoundary, /Result<UnlockedSignature, String>/u);
});
