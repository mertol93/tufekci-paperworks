import type { Translate, TranslationKey } from "./i18n";

export const MAX_SIGNATURE_VAULT_PASSPHRASE_BYTES = 1024;
export const MIN_SIGNATURE_VAULT_PASSPHRASE_CHARACTERS = 12;

export type SignatureVaultAction = "delete" | "list" | "save" | "unlock";

export type SignatureVaultErrorCode =
  | "capacity-reached"
  | "delete-failed"
  | "entry-invalid"
  | "entry-unavailable"
  | "library-unavailable"
  | "passphrase-rejected"
  | "save-failed"
  | "unlock-failed";

export type SignatureVaultStatus =
  | { code: "deleted" | "saved" }
  | { code: "unlocked"; name: string };

const recognisedErrorCodes = new Set<SignatureVaultErrorCode>([
  "capacity-reached",
  "delete-failed",
  "entry-invalid",
  "entry-unavailable",
  "library-unavailable",
  "passphrase-rejected",
  "save-failed",
  "unlock-failed"
]);

const fallbackCodes: Record<SignatureVaultAction, SignatureVaultErrorCode> = {
  delete: "delete-failed",
  list: "library-unavailable",
  save: "save-failed",
  unlock: "unlock-failed"
};

const errorTranslationKeys: Record<SignatureVaultErrorCode, TranslationKey> = {
  "capacity-reached": "signature.vault.error.capacity",
  "delete-failed": "signature.vault.error.delete",
  "entry-invalid": "signature.vault.error.entryInvalid",
  "entry-unavailable": "signature.vault.error.entryUnavailable",
  "library-unavailable": "signature.vault.error.list",
  "passphrase-rejected": "signature.vault.error.passphraseRejected",
  "save-failed": "signature.vault.error.save",
  "unlock-failed": "signature.vault.error.unlock"
};

export function classifySignatureVaultError(
  reason: unknown,
  action: SignatureVaultAction
): SignatureVaultErrorCode {
  return typeof reason === "string" && recognisedErrorCodes.has(reason as SignatureVaultErrorCode)
    ? (reason as SignatureVaultErrorCode)
    : fallbackCodes[action];
}

export function localiseSignatureVaultError(
  code: SignatureVaultErrorCode,
  t: Translate
) {
  return t(errorTranslationKeys[code]);
}

export function localiseSignatureVaultStatus(
  status: SignatureVaultStatus,
  t: Translate
) {
  switch (status.code) {
    case "deleted":
      return t("signature.vault.status.deleted");
    case "saved":
      return t("signature.vault.status.saved");
    case "unlocked":
      return t("signature.vault.status.unlocked", { name: status.name });
  }
}

export function signatureVaultPassphraseIsValid(value: string) {
  return (
    !/[\r\n\0]/u.test(value) &&
    Array.from(value).length >= MIN_SIGNATURE_VAULT_PASSPHRASE_CHARACTERS &&
    new TextEncoder().encode(value).length <= MAX_SIGNATURE_VAULT_PASSPHRASE_BYTES
  );
}
