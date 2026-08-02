import type { FormFieldKind } from "./formDraft";
import type { Translate, TranslationKey } from "./i18n";

const formKindKeys = {
  button: "form.kind.button",
  checkbox: "form.kind.checkbox",
  choice: "form.kind.choice",
  radio: "form.kind.radio",
  signature: "form.kind.signature",
  text: "form.kind.text",
  unsupported: "form.kind.unsupported"
} as const satisfies Record<FormFieldKind, TranslationKey>;

export function localiseFormKind(kind: FormFieldKind, t: Translate) {
  return t(formKindKeys[kind]);
}

export function localiseFormDraftError(
  error: string | undefined,
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  if (!error) {
    return undefined;
  }
  const maximum = error.match(/^Use at most (\d+) characters\.$/u);
  if (maximum) {
    return t("form.validation.maximum", {
      count: formatNumber(Number(maximum[1]))
    });
  }
  const keys: Record<string, TranslationKey> = {
    "Choose a listed option.": "form.validation.listed",
    "Choose one value.": "form.validation.oneValue",
    "This field does not allow multiple lines.": "form.validation.singleLine",
    "This required field cannot be empty.": "form.validation.required"
  };
  return keys[error] ? t(keys[error]) : t("form.validation.generic");
}

export function localiseFormWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const substituted = warning.match(
      /^(\d+) form appearances? contained characters outside the built-in Windows Latin font\. The full Unicode value remains in editable fields, while unsupported appearance glyphs use question marks\.$/u
    );
    if (substituted) {
      return t(
        Number(substituted[1]) === 1
          ? "form.warning.substituted.one"
          : "form.warning.substituted.other",
        { count: formatNumber(Number(substituted[1])) }
      );
    }

    const flattened = warning.match(
      /^(\d+) supported fields? were flattened into static page content and are no longer editable\. Signature fields, push buttons, unsupported fields, and fields without complete widget geometry remain interactive\.$/u
    );
    if (flattened) {
      return t(
        Number(flattened[1]) === 1
          ? "form.warning.flattened.one"
          : "form.warning.flattened.other",
        { count: formatNumber(Number(flattened[1])) }
      );
    }

    const exactKeys: Record<string, TranslationKey> = {
      "All AcroForm fields were removed from the flattened copy.":
        "form.warning.allFlattened",
      "Filling or flattening this form rewrites the certificate-signed PDF and invalidates its existing signatures.":
        "form.warning.signatureReview",
      "Form editing changed the PDF and invalidated its existing certificate signatures.":
        "form.warning.signatureInvalidated",
      "The completed form copy is not password-protected. Use Protect to apply new encryption.":
        "form.warning.unprotected",
      "The completed form copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "form.warning.protected",
      "This PDF contains XFA data. XFA forms are not edited or flattened because their dynamic behaviour cannot be reproduced safely.":
        "form.warning.xfa",
      "This PDF does not contain any AcroForm fields.": "form.warning.noFields"
    };
    return exactKeys[warning] ? t(exactKeys[warning]) : t("form.warning.generic");
  });

  return [...new Set(localised)];
}
