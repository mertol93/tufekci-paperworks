import type { Translate, TranslationKey } from "./i18n";
import type {
  BatchRecipe,
  BatchRecipeInputOrigin
} from "./batchRecipes";

const builtInRecipeKeys: Readonly<
  Record<string, { description: TranslationKey; name: TranslationKey }>
> = {
  "built-in-smaller-sharing": {
    description: "batch.recipe.builtIn.smaller.description",
    name: "batch.recipe.builtIn.smaller.name"
  },
  "built-in-private-sharing": {
    description: "batch.recipe.builtIn.private.description",
    name: "batch.recipe.builtIn.private.name"
  },
  "built-in-privacy-clean": {
    description: "batch.recipe.builtIn.clean.description",
    name: "batch.recipe.builtIn.clean.name"
  },
  "built-in-searchable-archive": {
    description: "batch.recipe.builtIn.searchable.description",
    name: "batch.recipe.builtIn.searchable.name"
  },
  "built-in-pdfa-archive": {
    description: "batch.recipe.builtIn.pdfa.description",
    name: "batch.recipe.builtIn.pdfa.name"
  }
};

const recipeErrorKeys: Readonly<Record<string, TranslationKey>> = {
  "Choose a supported PDF/A archive profile.": "batch.recipe.error.archiveProfile",
  "Choose a valid installed OCR language.": "batch.recipe.error.ocrLanguage",
  "Enable searchable OCR before deskewing scanned pages.": "batch.recipe.error.deskewRequiresOcr",
  "Enable searchable OCR, privacy cleaning, compression, PDF/A conversion, or a combination.":
    "batch.recipe.error.noSteps",
  "Image quality must be between 40 and 95.": "batch.recipe.error.quality",
  "Select at least one privacy category.": "batch.recipe.error.privacyCategory"
};

const stepKeys: Readonly<Record<string, TranslationKey>> = {
  "AES-256 protection": "batch.step.protection",
  Compression: "batch.step.compression",
  Deskew: "batch.step.deskew",
  "PDF/A-1b": "batch.step.pdfa1b",
  "PDF/A-2b": "batch.step.pdfa2b",
  "PDF/A-3b": "batch.step.pdfa3b",
  "Privacy cleaning": "batch.step.privacy",
  "Searchable OCR": "batch.step.ocr"
};

const noteKeys: Readonly<Record<string, TranslationKey>> = {
  "The prepared copy was already efficient at this quality, so the preceding recipe steps were published without an additional compression rewrite.":
    "batch.result.note.preparedEfficient",
  "The source was already efficient at this quality, so PDF/A conversion continued without an additional compression rewrite.":
    "batch.result.note.archiveEfficient",
  "The source was already efficient at this quality, so output protection was applied without an additional compression rewrite.":
    "batch.result.note.protectionEfficient"
};

const skippedKeys: Readonly<Record<string, TranslationKey>> = {
  "The source was already efficient at this quality; no duplicate copy was created.":
    "batch.result.skipped.efficient"
};

export function localiseBatchRecipeName(recipe: BatchRecipe, t: Translate) {
  const key = builtInRecipeKeys[recipe.id]?.name;
  return key ? t(key) : recipe.name;
}

export function localiseBatchRecipeDescription(recipe: BatchRecipe, t: Translate) {
  const key = builtInRecipeKeys[recipe.id]?.description;
  return t(key ?? "batch.recipe.custom.description");
}

export function localiseBatchInputOrigin(origin: BatchRecipeInputOrigin, t: Translate) {
  const keys: Record<BatchRecipeInputOrigin, TranslationKey> = {
    "connected-scanner": "batch.origin.connectedScanner",
    document: "batch.origin.document",
    "image-scan": "batch.origin.imageScan"
  };
  return t(keys[origin]);
}

export function localiseBatchRecipeSettingsError(
  error: string | null,
  t: Translate
) {
  return error ? t(recipeErrorKeys[error] ?? "batch.recipe.error.generic") : null;
}

export function batchRecipeExceptionKey(reason: unknown): TranslationKey {
  const message = reason instanceof Error ? reason.message : "";
  if (recipeErrorKeys[message]) {
    return recipeErrorKeys[message];
  }
  if (message === "Enter a recipe name using 1 to 60 characters.") {
    return "batch.recipe.error.name";
  }
  return "batch.recipe.error.save";
}

export function localiseBatchInspectionError(error: string | null | undefined, t: Translate) {
  const normalised = error?.toLocaleLowerCase("en-GB") ?? "";
  if (normalised === "incomplete") {
    return t("batch.source.error.incomplete");
  }
  if (normalised === "request") {
    return t("batch.source.error.request");
  }
  return t(
    normalised.includes("password") || normalised.includes("decrypt")
      ? "batch.source.error.password"
      : "batch.source.error.inspect"
  );
}

export function localiseBatchSteps(steps: string[], t: Translate) {
  return [
    ...new Set(steps.map((step) => t(stepKeys[step] ?? "batch.step.generic")))
  ];
}

export function localiseBatchNote(note: string | null | undefined, t: Translate) {
  return note ? t(noteKeys[note] ?? "batch.result.note.generic") : null;
}

export function localiseBatchSkippedReason(
  reason: string | null | undefined,
  t: Translate
) {
  return t(skippedKeys[reason ?? ""] ?? "batch.result.skipped.generic");
}

export function localiseBatchWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    if (
      /^OCR completed, but (?:page|pages) [0-9, and]+ contain no searchable text\. Review blank or low-confidence pages\.$/u.test(
        warning
      )
    ) {
      return t("batch.warning.ocrCoverage");
    }

    const recompressed = warning.match(
      /^(\d+) compatible raster images? will be recompressed with JPEG quality (\d+)\. Text, vectors, links, forms, and OCR text layers remain PDF content\.$/u
    );
    if (recompressed) {
      const count = Number(recompressed[1]);
      return t(
        count === 1
          ? "compression.warning.recompressed.one"
          : "compression.warning.recompressed.other",
        {
          count: formatNumber(count),
          quality: formatNumber(Number(recompressed[2]))
        }
      );
    }

    const preserved = warning.match(
      /^(\d+) images? use colour spaces, masks, filters, dimensions, or data that this preservation-first pass does not JPEG-recompress\. Lossless stream optimisation may still change their encoded representation\.$/u
    );
    if (preserved) {
      const count = Number(preserved[1]);
      return t(
        count === 1
          ? "compression.warning.preserved.one"
          : "compression.warning.preserved.other",
        { count: formatNumber(count) }
      );
    }

    const exactKeys: Readonly<Record<string, TranslationKey>> = {
      "Cleaning changes the PDF and invalidates any existing certificate signature.":
        "privacy.warning.certificate",
      "Compression rewrites the PDF and invalidates any existing certificate signature.":
        "compression.warning.certificate",
      "Interactive form structures are preserved and checked, but their appearance should be reviewed in the compressed copy.":
        "compression.warning.forms",
      "The bounded image-work limit was reached. Remaining images stay unchanged; split very large PDFs before compressing them more deeply.":
        "compression.warning.limit",
      "The cleaned copy is not password-protected. Use Protect to apply new encryption.":
        "privacy.warning.unprotected",
      "The cleaned copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "privacy.warning.protected",
      "The compressed copy is not password-protected. Use Protect to apply new AES-256 encryption.":
        "compression.warning.unprotected",
      "The compressed copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
        "compression.warning.protected",
      "The selected quality does not produce a smaller verified rewrite. Try a lower quality or keep the source.":
        "compression.warning.notSmaller"
    };
    return t(exactKeys[warning] ?? "batch.warning.generic");
  });

  return [...new Set(localised)];
}
