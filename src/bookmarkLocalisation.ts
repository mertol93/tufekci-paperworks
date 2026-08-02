import type { Translate, TranslationKey } from "./i18n";

const validationKeys: Readonly<Record<string, TranslationKey>> = {
  "Add a bookmark at one of the included levels before creating printed contents.":
    "bookmark.contents.validation.noEntries",
  "Choose a valid bookmark level for the printed contents.":
    "bookmark.contents.validation.level",
  "Enter a title for the printed contents pages.":
    "bookmark.contents.validation.titleRequired",
  "Printed contents can contain at most 64 pages. Include fewer bookmark levels.":
    "bookmark.contents.validation.pageLimit",
  "The contents title cannot contain control characters.":
    "bookmark.contents.validation.titleControl",
  "The contents title must contain at most 128 characters.":
    "bookmark.contents.validation.titleLength"
};

const exactWarningKeys: Readonly<Record<string, TranslationKey>> = {
  "All bookmarks were removed from the new copy.": "bookmark.warning.allRemoved",
  "Bookmark destinations in the new copy use whole-page Fit targets. Specialist zoom coordinates and external destinations are not retained.":
    "bookmark.warning.fitTargets",
  "Bookmark editing changed the PDF and invalidated its existing certificate signatures.":
    "bookmark.warning.signatureInvalidated",
  "Editing bookmarks rewrites this certificate-signed PDF and invalidates its existing signatures.":
    "bookmark.warning.signatureReview",
  "One or more empty, invalid, or oversized bookmark titles were replaced for safe editing.":
    "bookmark.warning.titlesReplaced",
  "One or more unusually long bookmark titles were shortened for safe editing.":
    "bookmark.warning.titlesShortened",
  "Printed contents pages are not tagged. Run PDF/UA checks before claiming an accessible or standards-conforming copy.":
    "bookmark.warning.contentsUntagged",
  "The bookmarked copy is not password-protected. Use Protect to apply new encryption.":
    "bookmark.warning.unprotected",
  "The bookmarked copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
    "bookmark.warning.protected",
  "The contents entries use physical output page numbers; existing custom page labels are preserved but not printed.":
    "bookmark.warning.pageLabels",
  "The PDF has an empty bookmark root.": "bookmark.warning.emptyRoot"
};

export function localisePrintedContentsValidation(
  message: string | null,
  t: Translate
) {
  return message ? t(validationKeys[message] ?? "bookmark.contents.validation.generic") : null;
}

export function localiseBookmarkWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const exactKey = exactWarningKeys[warning];
    if (exactKey) {
      return t(exactKey);
    }

    const unresolved = warning.match(
      /^(\d{1,9}) bookmarks? use an unsupported, missing, or external destination\. Assign a page before exporting the edited tree\.$/u
    );
    if (unresolved) {
      const count = Number(unresolved[1]);
      return t(
        count === 1
          ? "bookmark.warning.unresolved.one"
          : "bookmark.warning.unresolved.other",
        { count: formatNumber(count) }
      );
    }

    const contents = warning.match(
      /^Added (\d{1,9}) printed contents pages? with (\d{1,9}) linked (?:entry|entries); source pages moved forward by (\d{1,9})\.$/u
    );
    if (contents) {
      const pages = Number(contents[1]);
      const entries = Number(contents[2]);
      const shift = Number(contents[3]);
      if (pages !== shift) {
        return t("bookmark.warning.generic");
      }
      const key =
        pages === 1
          ? entries === 1
            ? "bookmark.warning.contentsAdded.onePageOneEntry"
            : "bookmark.warning.contentsAdded.onePageManyEntries"
          : entries === 1
            ? "bookmark.warning.contentsAdded.manyPagesOneEntry"
            : "bookmark.warning.contentsAdded.manyPagesManyEntries";
      return t(key, {
        entries: formatNumber(entries),
        pages: formatNumber(pages)
      });
    }

    const unsupportedCharacters = warning.match(
      /^(\d{1,9}) unsupported contents characters? were replaced with question marks in the printed pages; the bookmark titles remain unchanged\.$/u
    );
    if (unsupportedCharacters) {
      const count = Number(unsupportedCharacters[1]);
      return t(
        count === 1
          ? "bookmark.warning.unsupportedCharacters.one"
          : "bookmark.warning.unsupportedCharacters.other",
        { count: formatNumber(count) }
      );
    }

    return t("bookmark.warning.generic");
  });

  return [...new Set(localised)];
}

export function bookmarkPdfOpeningErrorKey(reason: unknown): TranslationKey {
  const name =
    reason && typeof reason === "object" && "name" in reason
      ? String(reason.name)
      : "";
  if (name === "InvalidPDFException") {
    return "bookmark.error.damaged";
  }
  if (name === "MissingPDFException" || name === "UnexpectedResponseException") {
    return "bookmark.error.read";
  }
  return "bookmark.error.review";
}
