import type { Translate, TranslationKey } from "./i18n";

export function localiseRedactionSearchError(error: string, t: Translate) {
  const keys: Record<string, TranslationKey> = {
    "Enter a wildcard pattern. Use * for a short run, ? for one character, or # for one digit.":
      "redaction.search.error.patternRequired",
    "Enter text or a name to find.": "redaction.search.error.textRequired",
    "Search text can contain at most 512 characters.": "redaction.search.error.maximum",
    "A wildcard pattern cannot contain only asterisks.":
      "redaction.search.error.asterisksOnly"
  };
  return t(keys[error] ?? "redaction.search.error.generic");
}

export function localiseRedactionWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const flattened = warning.match(
      /^(\d+) pages? (?:was|were) flattened to reviewed raster artwork with (\d+) native-applied permanent redaction regions?\.$/u
    );
    if (flattened) {
      const pages = Number(flattened[1]);
      const regions = Number(flattened[2]);
      return t(
        pages === 1
          ? regions === 1
            ? "redaction.warning.flattened.onePageOneRegion"
            : "redaction.warning.flattened.onePageManyRegions"
          : regions === 1
            ? "redaction.warning.flattened.manyPagesOneRegion"
            : "redaction.warning.flattened.manyPagesManyRegions",
        {
          pages: formatNumber(pages),
          regions: formatNumber(regions)
        }
      );
    }

    const exactKeys: Record<string, TranslationKey> = {
      "Permanent redaction rasterises every marked page. Searchable text, links, form controls, comments, and accessibility tagging on those pages will be removed.":
        "redaction.warning.rasterises",
      "The exported copy is privacy-cleaned: metadata, actions, attachments, annotations, forms, bookmarks, named destinations, thumbnails, and document structure are removed throughout the PDF.":
        "redaction.warning.privacyCleaned",
      "Choose AES-256 output protection during export if the redacted copy must remain encrypted.":
        "redaction.warning.chooseProtection",
      "Redaction rewrites this certificate-signed PDF and invalidates its existing signatures.":
        "redaction.warning.signatureReview",
      "Searchable text and accessibility information are intentionally absent from redacted pages. Run OCR only if you accept recreating text outside the covered regions.":
        "redaction.warning.ocr",
      "Privacy cleaning removed interactive and hidden document structures from the entire exported copy.":
        "redaction.warning.cleaned",
      "The redacted copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "redaction.warning.protected",
      "The redacted copy is not password-protected. Use Protect to apply new encryption.":
        "redaction.warning.unprotected",
      "Redaction changed the PDF and invalidated its existing certificate signatures.":
        "redaction.warning.signatureInvalidated"
    };
    return exactKeys[warning] ? t(exactKeys[warning]) : t("redaction.warning.generic");
  });

  return [...new Set(localised)];
}
