// Core
import { useContext } from "react";
// Types
import type { I18nValue } from "@/i18n/I18nContext";
// Utils
import { I18nContext } from "@/i18n/I18nContext";

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n must be used inside <I18nProvider>");
  return value;
}

export default useI18n;
