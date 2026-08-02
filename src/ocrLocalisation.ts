import type { Translate, TranslationKey } from "./i18n";

type OcrReadinessLike = {
  languageAvailable: boolean;
  ocrMyPdf: { available: boolean };
  ready: boolean;
  tesseract: { available: boolean };
};

type OcrReviewWarningSource = {
  lowConfidenceCount: number;
  lowConfidenceWords: unknown[];
  malformedRows: number;
  wordCount: number;
};

const ocrLanguageKeys: Readonly<Record<string, TranslationKey>> = {
  ara: "ocr.language.ara",
  chi_sim: "ocr.language.chi_sim",
  chi_tra: "ocr.language.chi_tra",
  deu: "ocr.language.deu",
  eng: "ocr.language.eng",
  fra: "ocr.language.fra",
  ita: "ocr.language.ita",
  jpn: "ocr.language.jpn",
  kor: "ocr.language.kor",
  nld: "ocr.language.nld",
  osd: "ocr.language.osd",
  pol: "ocr.language.pol",
  por: "ocr.language.por",
  rus: "ocr.language.rus",
  spa: "ocr.language.spa",
  tur: "ocr.language.tur"
};

export function localiseOcrLanguage(code: string, fallbackName: string, t: Translate): string {
  if (code.includes("+")) {
    return code
      .split("+")
      .map((part) => localiseOcrLanguage(part, part, t))
      .join(" + ");
  }
  const key = ocrLanguageKeys[code];
  return key ? t(key) : fallbackName || code;
}

export function describeOcrReadiness(
  report: OcrReadinessLike | null,
  t: Translate
): string {
  if (!report) return t("ocr.engine.desktopOnly");
  if (report.ready) return t("ocr.engine.readyDetail");
  if (!report.ocrMyPdf.available) return t("ocr.engine.ocrMyPdfMissing");
  if (!report.tesseract.available) return t("ocr.engine.tesseractMissing");
  if (!report.languageAvailable) return t("ocr.engine.languageMissing");
  return t("ocr.engine.requiredDetail");
}

export function localiseSearchableOcrWarnings(
  warnings: string[],
  t: Translate
): string[] {
  return unique(
    warnings.map((warning) => {
      const normalised = warning.toLocaleLowerCase("en-GB");
      if (normalised.startsWith("ocr completed")) return t("ocr.warning.pagesWithoutText");
      if (normalised.includes("not password-protected")) return t("ocr.warning.unprotected");
      if (normalised.includes("invalidated its existing certificate")) {
        return t("ocr.warning.signatureInvalidated");
      }
      if (normalised.includes("aes-256 opening")) return t("ocr.warning.encrypted");
      return t("ocr.warning.generic");
    })
  );
}

export function localiseOcrReviewWarnings(
  result: OcrReviewWarningSource,
  t: Translate
): string[] {
  const warnings: string[] = [];
  if (result.wordCount === 0) warnings.push(t("ocrReview.warning.noWords"));
  if (result.malformedRows > 0) {
    warnings.push(
      t(
        result.malformedRows === 1
          ? "ocrReview.warning.malformed.one"
          : "ocrReview.warning.malformed.other",
        { count: result.malformedRows }
      )
    );
  }
  if (result.lowConfidenceCount > result.lowConfidenceWords.length) {
    warnings.push(
      t("ocrReview.warning.truncated", { count: result.lowConfidenceWords.length })
    );
  }
  return warnings;
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}
