// Types
import type { Messages } from "./en";
// Utils
import en from "./en";
import ru from "./ru";

export const LOCALES = ["en", "ru"] as const;

export type Locale = (typeof LOCALES)[number];

export const MESSAGES: Record<Locale, Messages> = { en, ru };

const STORAGE_KEY = "fileforge.locale";

function isLocale(value: unknown): value is Locale {
  return LOCALES.includes(value as Locale);
}

/** The user's explicit choice wins; otherwise follow the system language, falling back to English. */
export function detectLocale(systemLanguages: readonly string[] = navigator.languages): Locale {
  const saved = readSaved();
  if (saved) return saved;
  for (const language of systemLanguages) {
    const base = language.toLowerCase().split("-")[0];
    if (isLocale(base)) return base;
  }
  return "en";
}

export function saveLocale(locale: Locale): void {
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // Storage can be unavailable; the choice then lasts for this session only.
  }
}

function readSaved(): Locale | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return isLocale(value) ? value : null;
  } catch {
    return null;
  }
}
