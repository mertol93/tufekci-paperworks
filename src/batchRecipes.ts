export type BatchPrivacyOptions = {
  removeActiveContent: boolean;
  removeAnnotationsAndForms: boolean;
  removeAttachments: boolean;
  removeMetadata: boolean;
  removeThumbnails: boolean;
};

export type BatchRecipeSettings = {
  archiveProfile: "pdfa-1b" | "pdfa-2b" | "pdfa-3b" | null;
  cleanPrivacy: boolean;
  compress: boolean;
  jpegQuality: number;
  ocrLanguage: string;
  privacyOptions: BatchPrivacyOptions;
  recogniseText: boolean;
  straighten: boolean;
};

export type BatchRecipe = BatchRecipeSettings & {
  custom: boolean;
  description: string;
  id: string;
  name: string;
  outputSuffix: string;
};

export type BatchRecipeInputOrigin =
  | "document"
  | "image-scan"
  | "connected-scanner";

export type VerifiedScanBatchSeed = Readonly<{
  origin: "image-scan" | "connected-scanner";
  path: string;
}>;

export const BATCH_RECIPE_STORAGE_KEY = "paperworks.batch-recipes.v1";
export const MAX_CUSTOM_BATCH_RECIPES = 20;

export function createVerifiedScanBatchSeed(
  outputPath: string,
  connectedScanner: boolean
): VerifiedScanBatchSeed {
  if (
    !outputPath.trim() ||
    outputPath.length > 32_768 ||
    /[\r\n\0]/u.test(outputPath) ||
    !outputPath.toLocaleLowerCase("en-GB").endsWith(".pdf")
  ) {
    throw new Error("The verified scan output path is invalid.");
  }
  return {
    origin: connectedScanner ? "connected-scanner" : "image-scan",
    path: outputPath
  };
}

export function batchRecipeInputOriginLabel(origin: BatchRecipeInputOrigin) {
  switch (origin) {
    case "connected-scanner":
      return "Connected scanner intake";
    case "image-scan":
      return "Reviewed image scan";
    case "document":
      return "PDF document";
  }
}

export const DEFAULT_BATCH_PRIVACY_OPTIONS: BatchPrivacyOptions = {
  removeActiveContent: true,
  removeAnnotationsAndForms: false,
  removeAttachments: true,
  removeMetadata: true,
  removeThumbnails: true
};

export const BUILT_IN_BATCH_RECIPES: readonly BatchRecipe[] = [
  {
    archiveProfile: null,
    cleanPrivacy: false,
    compress: true,
    custom: false,
    description: "Create smaller copies when image recompression produces a genuine saving.",
    id: "built-in-smaller-sharing",
    jpegQuality: 78,
    name: "Smaller sharing copies",
    ocrLanguage: "eng",
    outputSuffix: "smaller",
    privacyOptions: { ...DEFAULT_BATCH_PRIVACY_OPTIONS },
    recogniseText: false,
    straighten: false
  },
  {
    archiveProfile: null,
    cleanPrivacy: true,
    compress: true,
    custom: false,
    description: "Remove common sharing risks, then reduce compatible images where worthwhile.",
    id: "built-in-private-sharing",
    jpegQuality: 78,
    name: "Private sharing copies",
    ocrLanguage: "eng",
    outputSuffix: "private",
    privacyOptions: { ...DEFAULT_BATCH_PRIVACY_OPTIONS },
    recogniseText: false,
    straighten: false
  },
  {
    archiveProfile: null,
    cleanPrivacy: true,
    compress: false,
    custom: false,
    description: "Rewrite privacy-clean copies without changing image quality.",
    id: "built-in-privacy-clean",
    jpegQuality: 78,
    name: "Privacy-clean copies",
    ocrLanguage: "eng",
    outputSuffix: "clean",
    privacyOptions: { ...DEFAULT_BATCH_PRIVACY_OPTIONS },
    recogniseText: false,
    straighten: false
  },
  {
    archiveProfile: null,
    cleanPrivacy: false,
    compress: true,
    custom: false,
    description: "Deskew scanned pages, add searchable text, then reduce compatible images where worthwhile.",
    id: "built-in-searchable-archive",
    jpegQuality: 84,
    name: "Searchable copies",
    ocrLanguage: "eng",
    outputSuffix: "searchable",
    privacyOptions: { ...DEFAULT_BATCH_PRIVACY_OPTIONS },
    recogniseText: true,
    straighten: true
  },
  {
    archiveProfile: "pdfa-2b",
    cleanPrivacy: false,
    compress: false,
    custom: false,
    description: "Deskew scanned pages, add searchable text, then require independent PDF/A-2b validation.",
    id: "built-in-pdfa-archive",
    jpegQuality: 84,
    name: "PDF/A-2b archive copies",
    ocrLanguage: "eng",
    outputSuffix: "pdfa-2b",
    privacyOptions: { ...DEFAULT_BATCH_PRIVACY_OPTIONS },
    recogniseText: true,
    straighten: true
  }
];

export function cloneBatchRecipeSettings(recipe: BatchRecipeSettings): BatchRecipeSettings {
  return {
    archiveProfile: recipe.archiveProfile,
    cleanPrivacy: recipe.cleanPrivacy,
    compress: recipe.compress,
    jpegQuality: recipe.jpegQuality,
    ocrLanguage: recipe.ocrLanguage,
    privacyOptions: { ...recipe.privacyOptions },
    recogniseText: recipe.recogniseText,
    straighten: recipe.straighten
  };
}

export function batchRecipeSettingsError(settings: BatchRecipeSettings): string | null {
  if (
    !settings.cleanPrivacy &&
    !settings.compress &&
    !settings.recogniseText &&
    settings.archiveProfile === null
  ) {
    return "Enable searchable OCR, privacy cleaning, compression, PDF/A conversion, or a combination.";
  }
  if (settings.straighten && !settings.recogniseText) {
    return "Enable searchable OCR before deskewing scanned pages.";
  }
  if (settings.recogniseText && !validOcrLanguage(settings.ocrLanguage)) {
    return "Choose a valid installed OCR language.";
  }
  if (
    settings.archiveProfile !== null &&
    !["pdfa-1b", "pdfa-2b", "pdfa-3b"].includes(settings.archiveProfile)
  ) {
    return "Choose a supported PDF/A archive profile.";
  }
  if (
    settings.cleanPrivacy &&
    !Object.values(settings.privacyOptions).some(Boolean)
  ) {
    return "Select at least one privacy category.";
  }
  if (
    settings.compress &&
    (!Number.isInteger(settings.jpegQuality) ||
      settings.jpegQuality < 40 ||
      settings.jpegQuality > 95)
  ) {
    return "Image quality must be between 40 and 95.";
  }
  return null;
}

export function createCustomBatchRecipe(
  id: string,
  name: string,
  settings: BatchRecipeSettings
): BatchRecipe {
  const safeName = normaliseRecipeName(name);
  if (!/^custom-[A-Za-z0-9-]{1,80}$/.test(id)) {
    throw new Error("The saved recipe identifier is invalid.");
  }
  if (!safeName) {
    throw new Error("Enter a recipe name using 1 to 60 characters.");
  }
  const settingsError = batchRecipeSettingsError(settings);
  if (settingsError) {
    throw new Error(settingsError);
  }
  return {
    ...cloneBatchRecipeSettings(settings),
    custom: true,
    description: "Saved locally on this device.",
    id,
    name: safeName,
    outputSuffix: recipeOutputSuffix(safeName)
  };
}

export function parseStoredBatchRecipes(serialised: string | null): BatchRecipe[] {
  if (!serialised || serialised.length > 100_000) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(serialised);
    if (
      !isRecord(parsed) ||
      ![1, 2, 3].includes(parsed.version as number) ||
      !Array.isArray(parsed.recipes)
    ) {
      return [];
    }
    const recipes: BatchRecipe[] = [];
    const ids = new Set<string>();
    for (const candidate of parsed.recipes) {
      if (recipes.length >= MAX_CUSTOM_BATCH_RECIPES) {
        break;
      }
      if (!isRecord(candidate)) {
        continue;
      }
      try {
        const settings = storedSettings(candidate, parsed.version as 1 | 2 | 3);
        const recipe = createCustomBatchRecipe(
          typeof candidate.id === "string" ? candidate.id : "",
          typeof candidate.name === "string" ? candidate.name : "",
          settings
        );
        if (!ids.has(recipe.id)) {
          ids.add(recipe.id);
          recipes.push(recipe);
        }
      } catch {
        // One malformed entry must not hide the other local recipes.
      }
    }
    return recipes;
  } catch {
    return [];
  }
}

export function serialiseStoredBatchRecipes(recipes: readonly BatchRecipe[]): string {
  const safeRecipes = recipes
    .filter((recipe) => recipe.custom)
    .slice(0, MAX_CUSTOM_BATCH_RECIPES)
    .map((recipe) => ({
      archiveProfile: recipe.archiveProfile,
      cleanPrivacy: recipe.cleanPrivacy,
      compress: recipe.compress,
      id: recipe.id,
      jpegQuality: recipe.jpegQuality,
      name: normaliseRecipeName(recipe.name),
      ocrLanguage: recipe.ocrLanguage,
      privacyOptions: {
        removeActiveContent: recipe.privacyOptions.removeActiveContent,
        removeAnnotationsAndForms: recipe.privacyOptions.removeAnnotationsAndForms,
        removeAttachments: recipe.privacyOptions.removeAttachments,
        removeMetadata: recipe.privacyOptions.removeMetadata,
        removeThumbnails: recipe.privacyOptions.removeThumbnails
      },
      recogniseText: recipe.recogniseText,
      straighten: recipe.straighten
    }));
  return JSON.stringify({ recipes: safeRecipes, version: 3 });
}

export function buildBatchOutputFileNames(
  sourcePaths: readonly string[],
  requestedSuffix: string
): string[] {
  const suffix = recipeOutputSuffix(requestedSuffix);
  const used = new Set<string>();
  return sourcePaths.map((sourcePath) => {
    const sourceName = sourcePath.split(/[\\/]/).pop() || "document.pdf";
    const sourceStem = sourceName.replace(/\.pdf$/i, "");
    const safeStem = sanitiseOutputStem(sourceStem);
    let sequence = 1;
    while (true) {
      const sequenceSuffix = sequence === 1 ? "" : `-${sequence}`;
      const ending = `-${suffix}${sequenceSuffix}.pdf`;
      const stem = fitUtf8(safeStem, 240 - utf8Length(ending)) || "document";
      const candidate = `${stem}${ending}`;
      const key = candidate.normalize("NFC").toLocaleLowerCase("en-GB");
      if (!used.has(key)) {
        used.add(key);
        return candidate;
      }
      sequence += 1;
    }
  });
}

export function recipeOutputSuffix(name: string): string {
  const suffix = name
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLocaleLowerCase("en-GB")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32)
    .replace(/-+$/g, "");
  return suffix || "processed";
}

function storedSettings(candidate: Record<string, unknown>, version: 1 | 2 | 3): BatchRecipeSettings {
  if (
    typeof candidate.cleanPrivacy !== "boolean" ||
    typeof candidate.compress !== "boolean" ||
    typeof candidate.jpegQuality !== "number" ||
    !isRecord(candidate.privacyOptions)
  ) {
    throw new Error("The stored recipe settings are incomplete.");
  }
  const privacyOptions = candidate.privacyOptions;
  const keys: Array<keyof BatchPrivacyOptions> = [
    "removeActiveContent",
    "removeAnnotationsAndForms",
    "removeAttachments",
    "removeMetadata",
    "removeThumbnails"
  ];
  if (keys.some((key) => typeof privacyOptions[key] !== "boolean")) {
    throw new Error("The stored privacy settings are incomplete.");
  }
  const recogniseText = version === 1 ? false : candidate.recogniseText;
  const straighten = version === 1 ? false : candidate.straighten;
  const ocrLanguage = version === 1 ? "eng" : candidate.ocrLanguage;
  const archiveProfile = version < 3 ? null : candidate.archiveProfile;
  if (
    typeof recogniseText !== "boolean" ||
    typeof straighten !== "boolean" ||
    typeof ocrLanguage !== "string"
  ) {
    throw new Error("The stored OCR settings are incomplete.");
  }
  if (
    archiveProfile !== null &&
    !["pdfa-1b", "pdfa-2b", "pdfa-3b"].includes(archiveProfile as string)
  ) {
    throw new Error("The stored archive settings are invalid.");
  }
  return {
    archiveProfile: archiveProfile as BatchRecipeSettings["archiveProfile"],
    cleanPrivacy: candidate.cleanPrivacy,
    compress: candidate.compress,
    jpegQuality: candidate.jpegQuality,
    ocrLanguage,
    privacyOptions: {
      removeActiveContent: privacyOptions.removeActiveContent as boolean,
      removeAnnotationsAndForms: privacyOptions.removeAnnotationsAndForms as boolean,
      removeAttachments: privacyOptions.removeAttachments as boolean,
      removeMetadata: privacyOptions.removeMetadata as boolean,
      removeThumbnails: privacyOptions.removeThumbnails as boolean
    },
    recogniseText,
    straighten
  };
}

function validOcrLanguage(language: string): boolean {
  if (!language || language.length > 128) {
    return false;
  }
  const codes = language.split("+");
  return (
    codes.length <= 4 &&
    codes.every((code) => /^[A-Za-z0-9_-]{1,32}$/.test(code))
  );
}

function normaliseRecipeName(name: string): string {
  const normalised = name.normalize("NFC").trim().replace(/\s+/g, " ");
  if (!normalised || normalised.length > 60 || /[\u0000-\u001f\u007f]/.test(normalised)) {
    return "";
  }
  return normalised;
}

function sanitiseOutputStem(stem: string): string {
  const safe = stem
    .normalize("NFC")
    .replace(/[\u0000-\u001f\u007f<>:"/\\|?*]+/g, "-")
    .replace(/\s+/g, " ")
    .replace(/-+/g, "-")
    .replace(/^[ .-]+|[ .-]+$/g, "");
  return safe || "document";
}

function fitUtf8(value: string, maximumBytes: number): string {
  let result = "";
  for (const character of value) {
    if (utf8Length(result + character) > maximumBytes) {
      break;
    }
    result += character;
  }
  return result.replace(/[ .-]+$/g, "");
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
