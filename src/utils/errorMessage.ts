// Types
import type { Messages } from "@/i18n/en";
// Services
import { isAppError } from "@/services/fileforgeApi";

/** Maps an error rejected by a Tauri command to a localized sentence; unknown shapes get a generic message. */
export function errorMessage(t: Messages, error: unknown): string {
  if (isAppError(error) && error.code in t.errors) {
    return t.errors[error.code as keyof Messages["errors"]];
  }
  return t.errors.unknown;
}

/** "a.pdf, b.pdf, c.pdf +2": keeps notices one line long. */
export function listNames(names: string[], max = 3): string {
  const shown = names.slice(0, max).join(", ");
  return names.length > max ? `${shown} +${names.length - max}` : shown;
}

export default errorMessage;
