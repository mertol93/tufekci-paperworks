import type { Translate, TranslationKey } from "./i18n";

export type MergeActionErrorCode =
  | "add-sources-failed"
  | "cancel-failed"
  | "start-failed";

const actionErrorKeys: Record<MergeActionErrorCode, TranslationKey> = {
  "add-sources-failed": "merge.error.addSources",
  "cancel-failed": "merge.error.cancel",
  "start-failed": "merge.error.start"
};

const exactWarningKeys: Readonly<Record<string, TranslationKey>> = {
  "The combined copy uses AES-256 opening and administrator passwords. Reader permissions are advisory and may not be honoured by every PDF application.":
    "merge.warning.protected"
};

type SourceWarning = {
  key: TranslationKey;
  pattern: RegExp;
};

const sourceWarnings: readonly SourceWarning[] = [
  {
    key: "merge.warning.sourceEncryptedProtected",
    pattern:
      /^(.{1,512}) was encrypted\. Its source security settings are replaced by the new AES-256 output passwords\.$/u
  },
  {
    key: "merge.warning.sourceEncryptedUnprotected",
    pattern: /^(.{1,512}) was encrypted\. The combined output is not password-protected\.$/u
  },
  {
    key: "merge.warning.certificate",
    pattern:
      /^(.{1,512}) contains a certificate signature that is invalidated by merging or extraction\.$/u
  },
  {
    key: "merge.warning.forms",
    pattern: /^(.{1,512}) contains form fields\. Check their appearances in the combined output\.$/u
  },
  {
    key: "merge.warning.bookmarksNotCopied",
    pattern:
      /^(.{1,512}) contains bookmarks\. Source bookmarks are not copied into the combined output\.$/u
  },
  {
    key: "merge.warning.bookmarkEmptyRoot",
    pattern: /^(.{1,512}): The PDF has an empty bookmark root\.$/u
  },
  {
    key: "merge.warning.bookmarkTitlesReplaced",
    pattern:
      /^(.{1,512}): One or more empty, invalid, or oversized bookmark titles were replaced for safe editing\.$/u
  },
  {
    key: "merge.warning.bookmarkTitlesShortened",
    pattern:
      /^(.{1,512}): One or more unusually long bookmark titles were shortened for safe editing\.$/u
  },
  {
    key: "merge.warning.bookmarkRepeated",
    pattern:
      /^(.{1,512}): bookmarks for repeated source pages point to the first copied occurrence\.$/u
  }
];

export function mergeActionErrorTranslationKey(code: MergeActionErrorCode) {
  return actionErrorKeys[code];
}

export function localiseMergeWarnings(
  warnings: string[],
  t: Translate,
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string
) {
  const localised = warnings.map((warning) => {
    const exactKey = exactWarningKeys[warning];
    if (exactKey) {
      return t(exactKey);
    }

    const omitted = warning.match(
      /^(.{1,512}): (\d{1,9}) bookmarks? could not be preserved because the destination was unresolved or outside the selected pages\.$/u
    );
    if (omitted) {
      const name = safeSourceName(omitted[1]);
      const count = Number(omitted[2]);
      if (name && Number.isSafeInteger(count) && count > 0) {
        return t(
          count === 1
            ? "merge.warning.bookmarkOmitted.one"
            : "merge.warning.bookmarkOmitted.other",
          { count: formatNumber(count), name }
        );
      }
    }

    for (const sourceWarning of sourceWarnings) {
      const match = warning.match(sourceWarning.pattern);
      const name = safeSourceName(match?.[1]);
      if (name) {
        return t(sourceWarning.key, { name });
      }
    }

    return t("merge.warning.generic");
  });

  return [...new Set(localised)];
}

function safeSourceName(value?: string) {
  if (
    !value ||
    /[\\/\u0000-\u001F\u007F]/u.test(value) ||
    new TextEncoder().encode(value).length > 512
  ) {
    return null;
  }
  return value.trim() || null;
}
