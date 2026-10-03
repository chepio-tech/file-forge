// Core
import { useEffect, useMemo, useState, type ReactNode } from "react";
// Types
import type { I18nValue } from "./I18nContext";
// Utils
import { I18nContext } from "./I18nContext";
import { detectLocale, MESSAGES, saveLocale, type Locale } from "./locale";

interface I18nProviderProps {
  children: ReactNode;
  /** Overrides detection; used by tests. */
  initialLocale?: Locale;
}

function I18nProvider({ children, initialLocale }: I18nProviderProps) {
  const [locale, setLocaleState] = useState<Locale>(() => initialLocale ?? detectLocale());

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const value = useMemo<I18nValue>(
    () => ({
      locale,
      t: MESSAGES[locale],
      setLocale: (next) => {
        saveLocale(next);
        setLocaleState(next);
      },
    }),
    [locale],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export default I18nProvider;
