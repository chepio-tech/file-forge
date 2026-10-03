// Core
import { Channel, invoke } from "@tauri-apps/api/core";
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

export interface ImageOptions {
  /** 30–95, see `JPEG_QUALITY_RANGE` in `crates/fileforge-core/src/pdf/options.rs`. */
  jpegQuality: number;
  /** 72–600 or `null` to keep every image's resolution. */
  maxDpi: number | null;
}

/** `images: null` is the lossless preset. */
export interface PdfOptions {
  images: ImageOptions | null;
}

export interface PdfReport {
  originalSize: number;
  outputSize: number;
  /** The rewrite was not smaller; the result is the original file byte for byte. */
  keptOriginal: boolean;
  pages: number;
  imagesRecompressed: number;
  imagesDownsampled: number;
  duplicatesMerged: number;
  unusedObjectsRemoved: number;
}

/** Pipeline stages in the order the engine runs them; `images` only runs for lossy presets. */
export type PdfStage = "loading" | "structure" | "images" | "streams" | "saving" | "verifying";

/** `total: 0` means the stage is not counted; otherwise `done` grows from 0 to `total`. */
export interface PdfProgress {
  stage: PdfStage;
  done: number;
  total: number;
}

export interface SavedFile {
  id: FileId;
  name: string;
}

export type AppErrorCode =
  | "unknownFile"
  | "noResult"
  | "originalTarget"
  | "notAFile"
  | "pdfTooLarge"
  | "pdfEncrypted"
  | "pdfSigned"
  | "pdfMalformed"
  | "invalidOptions"
  | "cancelled"
  | "io"
  | "internal";

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

  /** Forgets the file and its unsaved result. */
  removeFile: (id: FileId) => invoke<void>("remove_file", { id }),

  /**
   * Compresses into a temp result; the original is only read. `onProgress` receives throttled stage updates.
   * Rejects with `cancelled` after `cancelCompression`.
   */
  compressPdf: (id: FileId, options: PdfOptions, onProgress: (progress: PdfProgress) => void = () => {}) => {
    const channel = new Channel<PdfProgress>(onProgress);
    return invoke<PdfReport>("compress_pdf", { id, options, onProgress: channel });
  },

  /** Stops every compression already started at its next checkpoint; a no-op when nothing runs. */
  cancelCompression: () => invoke<void>("cancel_compression"),

  /** Native save dialog next to the original; resolves with the saved file name, or `null` when cancelled. */
  saveResult: (id: FileId) => invoke<string | null>("save_result", { id }),

  /** Native folder picker; saves each result as `<name>-compressed.pdf` without overwriting. `null` when cancelled. */
  saveResultsToFolder: (ids: FileId[]) => invoke<SavedFile[] | null>("save_results_to_folder", { ids }),

  /** Shows the last saved copy in the system file manager. */
  revealResult: (id: FileId) => invoke<void>("reveal_result", { id }),

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
