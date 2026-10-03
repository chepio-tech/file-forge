// Core
import { render, type RenderResult } from "@testing-library/react";
import type { ReactElement } from "react";
// Components
import I18nProvider from "@/i18n/I18nProvider";
// Types
import type { Locale } from "@/i18n/locale";

export function renderWithI18n(ui: ReactElement, locale: Locale = "en"): RenderResult {
  return render(<I18nProvider initialLocale={locale}>{ui}</I18nProvider>);
}

export default renderWithI18n;
