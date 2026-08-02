import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState
} from "react";
import {
  DEFAULT_LOCALE,
  formatDate,
  formatList,
  formatNumber,
  LOCALE_STORAGE_KEY,
  resolveSupportedLocale,
  translate,
  type SupportedLocale,
  type Translate
} from "./i18n";

type I18nContextValue = {
  formatDate: (value: Date | number, options?: Intl.DateTimeFormatOptions) => string;
  formatList: (values: string[], options?: Intl.ListFormatOptions) => string;
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) => string;
  locale: SupportedLocale;
  setLocale: (locale: SupportedLocale) => void;
  t: Translate;
};

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<SupportedLocale>(readLocalePreference);

  const setLocale = useCallback((nextLocale: SupportedLocale) => {
    const resolved = resolveSupportedLocale(nextLocale);
    setLocaleState(resolved);
    writeLocalePreference(resolved);
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = "ltr";
  }, [locale]);

  const t = useCallback<Translate>(
    (key, values) => translate(locale, key, values),
    [locale]
  );
  const formatLocaleNumber = useCallback(
    (value: number, options?: Intl.NumberFormatOptions) => formatNumber(locale, value, options),
    [locale]
  );
  const formatLocaleDate = useCallback(
    (value: Date | number, options?: Intl.DateTimeFormatOptions) =>
      formatDate(locale, value, options),
    [locale]
  );
  const formatLocaleList = useCallback(
    (values: string[], options?: Intl.ListFormatOptions) => formatList(locale, values, options),
    [locale]
  );
  const context = useMemo<I18nContextValue>(
    () => ({
      formatDate: formatLocaleDate,
      formatList: formatLocaleList,
      formatNumber: formatLocaleNumber,
      locale,
      setLocale,
      t
    }),
    [formatLocaleDate, formatLocaleList, formatLocaleNumber, locale, setLocale, t]
  );

  return <I18nContext.Provider value={context}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used inside I18nProvider.");
  }
  return context;
}

function readLocalePreference(): SupportedLocale {
  if (typeof window === "undefined") {
    return DEFAULT_LOCALE;
  }
  try {
    return resolveSupportedLocale(window.localStorage.getItem(LOCALE_STORAGE_KEY));
  } catch {
    return DEFAULT_LOCALE;
  }
}

function writeLocalePreference(locale: SupportedLocale) {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // A blocked preference store must not prevent local document work.
  }
}
