// Core
import { createContext } from "react";
// Types
import type { Messages } from "./en";
import type { Locale } from "./locale";

export interface I18nValue {
  locale: Locale;
  t: Messages;
  setLocale: (locale: Locale) => void;
}

export const I18nContext = createContext<I18nValue | null>(null);

export default I18nContext;
