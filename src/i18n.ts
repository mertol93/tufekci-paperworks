import { deDE } from "./locales/de-DE.ts";
import { enGB, type TranslationCatalogue, type TranslationKey } from "./locales/en-GB.ts";
import { enUS } from "./locales/en-US.ts";
import { trTR } from "./locales/tr-TR.ts";

export const SUPPORTED_LOCALES = ["en-GB", "en-US", "tr-TR", "de-DE"] as const;
export const DEFAULT_LOCALE = "en-GB" as const;
export const LOCALE_STORAGE_KEY = "tufekci-paperworks.interface-locale.v1";

export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];
export type TranslationValue = number | string;
export type TranslationValues = Readonly<Record<string, TranslationValue>>;
export type Translate = (key: TranslationKey, values?: TranslationValues) => string;

export const catalogues: Readonly<Record<SupportedLocale, TranslationCatalogue>> = {
  "de-DE": deDE,
  "en-GB": enGB,
  "en-US": enUS,
  "tr-TR": trTR
};

export function isSupportedLocale(value: unknown): value is SupportedLocale {
  return typeof value === "string" && SUPPORTED_LOCALES.includes(value as SupportedLocale);
}

export function resolveSupportedLocale(value: unknown): SupportedLocale {
  if (isSupportedLocale(value)) {
    return value;
  }
  if (typeof value === "string") {
    const normalised = value.trim().toLocaleLowerCase("en-GB");
    const match = SUPPORTED_LOCALES.find(
      (locale) => locale.toLocaleLowerCase("en-GB") === normalised
    );
    if (match) {
      return match;
    }
  }
  return DEFAULT_LOCALE;
}

export function translate(
  locale: SupportedLocale,
  key: TranslationKey,
  values: TranslationValues = {}
): string {
  const template = catalogues[locale]?.[key] ?? enGB[key];
  return template.replace(/\{([A-Za-z][A-Za-z0-9]*)\}/gu, (placeholder, name: string) =>
    Object.prototype.hasOwnProperty.call(values, name) ? String(values[name]) : placeholder
  );
}

export function translationPlaceholders(value: string): string[] {
  return [...value.matchAll(/\{([A-Za-z][A-Za-z0-9]*)\}/gu)]
    .map((match) => match[1])
    .sort((left, right) => left.localeCompare(right, "en-GB"));
}

export function formatNumber(
  locale: SupportedLocale,
  value: number,
  options?: Intl.NumberFormatOptions
): string {
  return new Intl.NumberFormat(locale, options).format(value);
}

export function formatDate(
  locale: SupportedLocale,
  value: Date | number,
  options?: Intl.DateTimeFormatOptions
): string {
  return new Intl.DateTimeFormat(locale, options).format(value);
}

export function formatList(
  locale: SupportedLocale,
  values: string[],
  options?: Intl.ListFormatOptions
): string {
  return new Intl.ListFormat(locale, options).format(values);
}

export type { TranslationCatalogue, TranslationKey };
