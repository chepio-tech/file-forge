// Core
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Typed client for the Rust shell. This file is the TypeScript half of the IPC contract; the Rust half is
 * `src-tauri/src/commands.rs` and `src-tauri/src/drag_drop.rs`. Change both sides together.
 */

export type FileKind = "pdf" | "image" | "video" | "audio" | "other";

export type FileId = number;

export interface FileInfo {
  id: FileId;
  name: string;
  size: number;
  kind: FileKind;
}

export interface RegisterOutcome {
  files: FileInfo[];
  /** Names of dropped or picked entries that are not readable regular files (folders, broken links). */
  skipped: string[];
}

export type AppErrorCode = "notAFile" | "io" | "internal";

export interface AppError {
  code: AppErrorCode;
  detail?: unknown;
}

export const FILES_ADDED_EVENT = "files-added";

export function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && typeof (value as { code?: unknown }).code === "string";
}

const fileforgeApi = {
  /** Opens the native file dialog filtered to `kinds`; resolves with an empty outcome when the user cancels. */
  pickFiles: (kinds: FileKind[], filterName: string) =>
    invoke<RegisterOutcome>("pick_files", { kinds, filterName }),

  removeFile: (id: FileId) => invoke<void>("remove_file", { id }),

  /** Files dropped on the window, already registered by Rust. */
  onFilesAdded: (handler: (outcome: RegisterOutcome) => void): Promise<UnlistenFn> =>
    listen<RegisterOutcome>(FILES_ADDED_EVENT, (event) => handler(event.payload)),

  /** `true` while files are dragged over the window. Paths are deliberately ignored here (ADR-0004). */
  onDragHover: (handler: (hovering: boolean) => void): Promise<UnlistenFn> =>
    getCurrentWebview().onDragDropEvent((event) => {
      handler(event.payload.type === "enter" || event.payload.type === "over");
    }),

  /** Opens an allow-listed URL (see `src-tauri/capabilities/default.json`) in the system browser. */
  openExternal: (url: string) => openUrl(url),
};

export default fileforgeApi;
