// Services
import { isAppError } from "@/services/fileforgeApi";
// Consts
import messages from "@/messages/messages";

type ErrorCode = keyof typeof messages.errors;

/** Maps an error rejected by a Tauri command to a sentence for the user; unknown shapes get a generic message. */
export function errorMessage(error: unknown): string {
  if (isAppError(error) && error.code in messages.errors) {
    return messages.errors[error.code as ErrorCode];
  }
  return messages.errors.unknown;
}

/** "a.pdf, b.pdf, c.pdf +2": keeps notices one line long. */
export function listNames(names: string[], max = 3): string {
  const shown = names.slice(0, max).join(", ");
  return names.length > max ? `${shown} +${names.length - max}` : shown;
}

export default errorMessage;
