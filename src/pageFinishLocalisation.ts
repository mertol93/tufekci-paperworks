import type { Translate, TranslationKey } from "./i18n";

type NumberFormatter = (
  value: number,
  options?: Intl.NumberFormatOptions
) => string;

const paperNameKeys: Readonly<Record<string, TranslationKey>> = {
  a3: "finish.paper.a3",
  a4: "finish.paper.a4",
  a5: "finish.paper.a5",
  custom: "finish.paper.custom",
  legal: "finish.paper.legal",
  letter: "finish.paper.letter"
};

export function localiseFinishPaperName(
  id: string,
  fallback: string,
  t: Translate
) {
  const key = paperNameKeys[id];
  return key ? t(key) : fallback;
}

export function localiseFinishRangeError(error: string | null, t: Translate) {
  if (!error) {
    return null;
  }
  if (error === "The PDF does not contain any pages.") {
    return t("finish.range.error.noPages");
  }
  if (error === "The page selection contains an empty item.") {
    return t("finish.range.error.emptyItem");
  }
  const bounds = error.match(
    /^The selection must stay between pages 1 and ([0-9]+)\.$/u
  );
  if (bounds) {
    return t("finish.range.error.bounds", { count: bounds[1] });
  }
  if (error.endsWith(" is not a valid page or range.")) {
    return t("finish.range.error.invalid");
  }
  return t("finish.range.error.generic");
}

export function localisePageFinishWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: NumberFormatter
) {
  return [
    ...new Set(
      warnings.map((warning) =>
        localisePageFinishWarning(warning, t, formatNumber)
      )
    )
  ];
}

function localisePageFinishWarning(
  warning: string,
  t: Translate,
  formatNumber: NumberFormatter
) {
  const exactKeys: Readonly<Record<string, TranslationKey>> = {
    "The finished copy will be unlocked. Use Protect afterwards to apply new AES-256 encryption.":
      "finish.warning.review.unlocked",
    "Page finishing rewrites the PDF and invalidates its existing certificate signatures.":
      "finish.warning.review.certificate",
    "Interactive form structures are preserved. Resizing carries widget rectangles with their pages, but the finished copy should be reviewed.":
      "finish.warning.review.forms",
    "Bookmarks are preserved. Page resizing can change specialist destination coordinates, so navigation should be reviewed.":
      "finish.warning.review.bookmarks",
    "Cropping changes the visible page box only. Hidden content remains in the PDF and this is not redaction.":
      "finish.warning.crop",
    "Interactive form fields were preserved after resizing; review their appearance and behaviour in the finished copy.":
      "finish.warning.forms",
    "Bookmarks were preserved, but specialist zoom and coordinate destinations may still refer to their original view.":
      "finish.warning.bookmarks",
    "Watermarks, headers, footers, and Bates numbers are visual page content. They do not authenticate the document or replace a certificate signature.":
      "finish.warning.marks",
    "The finished copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
      "finish.warning.protected",
    "The finished copy is not password-protected. Use Protect to apply new AES-256 encryption.":
      "finish.warning.unprotected",
    "Page finishing changed the PDF and invalidated its existing certificate signatures.":
      "finish.warning.certificate"
  };
  const exactKey = exactKeys[warning];
  if (exactKey) {
    return t(exactKey);
  }

  const applied = warning.match(
    /^Page finishing was applied to ([0-9]+) selected pages?\. The source PDF was not changed\.$/u
  );
  if (applied) {
    const count = Number(applied[1]);
    return t(
      count === 1 ? "finish.warning.applied.one" : "finish.warning.applied.other",
      { count: formatNumber(count) }
    );
  }

  const resized = warning.match(
    /^([0-9]+) pages? were fitted to the selected paper size\. Standard annotation and form-widget coordinates were adjusted with the page content\.$/u
  );
  if (resized) {
    const count = Number(resized[1]);
    return t(
      count === 1 ? "finish.warning.resized.one" : "finish.warning.resized.other",
      { count: formatNumber(count) }
    );
  }

  const substitutions = warning.match(
    /^([0-9]+) page-mark appearances? contained characters outside the built-in Windows Latin font\. Unsupported glyphs were replaced with question marks\.$/u
  );
  if (substitutions) {
    const count = Number(substitutions[1]);
    return t(
      count === 1
        ? "finish.warning.substitutions.one"
        : "finish.warning.substitutions.other",
      { count: formatNumber(count) }
    );
  }
  return t("finish.warning.generic");
}
