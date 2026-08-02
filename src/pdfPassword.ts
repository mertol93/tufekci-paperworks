import type { TranslationKey } from "./i18n.ts";

export const MAX_PDF_OPENING_PASSWORD_BYTES = 1024;

export type PdfOpenErrorCode =
  | "cancelled"
  | "changed"
  | "invalid"
  | "password"
  | "unreadable"
  | "unknown";

export type PdfPasswordRequest = {
  incorrect: boolean;
  cancel: () => void;
  submit: (password: string) => void;
};

const errorTranslationKeys: Record<PdfOpenErrorCode, TranslationKey> = {
  cancelled: "app.document.error.cancelled",
  changed: "app.document.error.changed",
  invalid: "app.document.error.invalid",
  password: "app.document.error.password",
  unreadable: "app.document.error.unreadable",
  unknown: "app.document.openFailedDetail"
};

export function classifyPdfOpenError(
  error: unknown,
  rangeFailure: PdfOpenErrorCode | null = null
): PdfOpenErrorCode {
  if (rangeFailure) {
    return rangeFailure;
  }
  const name = error instanceof Error ? error.name : "";
  if (name === "InvalidPDFException") {
    return "invalid";
  }
  if (name === "PasswordException") {
    return "password";
  }
  if (name === "ResponseException" || name === "MissingPDFException") {
    return "unreadable";
  }
  return "unknown";
}

export function classifyPdfRangeFailure(reason: unknown): PdfOpenErrorCode {
  return typeof reason === "string" && reason.toLowerCase().includes("changed on disk")
    ? "changed"
    : "unreadable";
}

export function pdfOpenErrorTranslationKey(code: PdfOpenErrorCode): TranslationKey {
  return errorTranslationKeys[code];
}

export function validPdfOpeningPasswordInput(value: string) {
  return (
    !/[\r\n\0]/u.test(value) &&
    new TextEncoder().encode(value).length <= MAX_PDF_OPENING_PASSWORD_BYTES
  );
}
