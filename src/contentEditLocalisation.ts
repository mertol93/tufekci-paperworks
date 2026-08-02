import type { Translate, TranslationKey } from "./i18n";

export function localiseContentWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const readOnly = warning.match(
      /^(\d+) text objects? and (\d+) image objects? could not be edited safely in this release and will be preserved unchanged\.$/u
    );
    if (readOnly) {
      return t("content.warning.readOnly", {
        images: formatNumber(Number(readOnly[2])),
        text: formatNumber(Number(readOnly[1]))
      });
    }

    const exactKeys: Record<string, TranslationKey> = {
      "Editing page content rewrites this certificate-signed PDF and invalidates its existing signatures.":
        "content.warning.signatureReview",
      "Only native-reviewed page-stream objects are editable. Complex text, nested form content, shared streams, and ambiguous image placements remain visible and read-only.":
        "content.warning.reviewedOnly",
      "Text bounds are an indicative selection aid; export changes the exact reviewed PDF text-show operation.":
        "content.warning.textBounds",
      "Only the selected native-reviewed page content was changed. Unsupported content was preserved unchanged.":
        "content.warning.selectedOnly",
      "Replacement text keeps the reviewed position and font but is not reflowed. Review the edited page for overlap before sharing.":
        "content.warning.noReflow",
      "Removed or replaced image resources may remain as unreachable or unused data. Run Privacy Cleaner before sharing when hidden-data removal matters.":
        "content.warning.imagePrivacy",
      "The edited copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "content.warning.protected",
      "The edited copy is not password-protected. Enable output protection or use Protect to apply new encryption.":
        "content.warning.unprotected",
      "Page-content editing changed the PDF and invalidated its existing certificate signatures.":
        "content.warning.signatureInvalidated"
    };
    return exactKeys[warning] ? t(exactKeys[warning]) : t("content.warning.generic");
  });

  return [...new Set(localised)];
}

export function localiseContentSelectionKind(kind: "image" | "text", t: Translate) {
  return t(kind === "image" ? "content.kind.image" : "content.kind.text");
}
