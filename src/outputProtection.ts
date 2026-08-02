export type OutputProtectionDraft = {
  enabled: boolean;
  openPassword: string;
  openPasswordConfirmation: string;
  ownerPassword: string;
  ownerPasswordConfirmation: string;
};

export type PdfOutputProtection = {
  openPassword: string;
  ownerPassword: string;
};

export function createOutputProtectionDraft(enabled = false): OutputProtectionDraft {
  return {
    enabled,
    openPassword: "",
    openPasswordConfirmation: "",
    ownerPassword: "",
    ownerPasswordConfirmation: ""
  };
}

export function outputProtectionIsValid(
  draft: OutputProtectionDraft,
  qpdfAvailable: boolean
) {
  if (!draft.enabled) {
    return true;
  }
  return (
    qpdfAvailable &&
    validPdfPassword(draft.openPassword) &&
    draft.openPassword === draft.openPasswordConfirmation &&
    validPdfPassword(draft.ownerPassword) &&
    draft.ownerPassword === draft.ownerPasswordConfirmation &&
    draft.ownerPassword !== draft.openPassword
  );
}

export function toPdfOutputProtection(
  draft: OutputProtectionDraft,
  qpdfAvailable: boolean
): PdfOutputProtection | null {
  if (!draft.enabled) {
    return null;
  }
  if (!outputProtectionIsValid(draft, qpdfAvailable)) {
    throw new Error("Enter valid opening and administrator passwords before protecting the output.");
  }
  return {
    openPassword: draft.openPassword,
    ownerPassword: draft.ownerPassword
  };
}

export function validPdfPassword(value: string) {
  return (
    value.length >= 8 &&
    new TextEncoder().encode(value).length <= 127 &&
    !/[\r\n\0]/u.test(value)
  );
}
