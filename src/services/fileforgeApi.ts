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

/** What searching the dropped folders of one drop did (ADR-0017). */
export interface FolderScan {
  /** Folders dropped, not counting their subfolders. */
  folders: number;
  /** Files found in them and registered. */
  added: number;
  /** Files and app/document packages of kinds the active tool does not accept. */
  ignored: number;
  /** A limit (depth, entries or files per drop) stopped the search, so some files were not added. */
  truncated: boolean;
}

export interface RegisterOutcome {
  files: FileInfo[];
  /** Names of entries that are not readable regular files (broken links, unreadable folders, packages). */
  skipped: string[];
  /** Present only when the drop contained folders. */
  folders?: FolderScan;
}

export interface BackgroundOptions {
  format: "png" | "webp";
  background: [number, number, number] | null;
  crop: boolean;
  point: [number, number] | null;
}

export interface BackgroundReport {
  originalSize: number;
  outputSize: number;
  width: number;
  height: number;
  originalPreview: string;
  preview: string;
}
export interface ModelStatus { installed: boolean; downloadBytes: number }
export interface DownloadProgress { downloaded: number; total: number }

function imageData(bytes: number[]): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return `data:image/png;base64,${btoa(binary)}`;
}

export interface ImageOptions {
  /** 30–95, see `JPEG_QUALITY_RANGE` in `crates/fileforge-core/src/pdf/options.rs`. */
  jpegQuality: number;
  /** 72–600 or `null` to keep every image's resolution. */
  maxDpi: number | null;
  /** Suitable Flate photos become JPEG only with at least 20% savings; omitted means false (ADR-0016). */
  compressFlatePhotos?: boolean;
}

/** `images: null` is the lossless preset. The removal options work with every preset (ADR-0012). */
export interface PdfOptions {
  images: ImageOptions | null;
  /** Info, XMP and page thumbnails; files declaring PDF/A, PDF/UA or PDF/X keep their document metadata. */
  stripMetadata: boolean;
  /** `/PieceInfo`: Illustrator/Photoshop private data. */
  stripEditingData: boolean;
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
  metadataRemoved: boolean;
  /** Removal was asked for, but the file declares a standard that requires its document metadata. */
  metadataKeptForStandard: boolean;
  thumbnailsRemoved: number;
  editingDataRemoved: number;
}

/** How to compress JPEG, PNG and WebP files; see `crates/fileforge-core/src/raster/options.rs`. */
export interface RasterOptions {
  /** 30–95: re-encode JPEGs at this quality when that is at least 2% smaller. `null` keeps JPEG pixels exactly. */
  jpegQuality: number | null;
  /** 30–95: re-encode lossy WebPs likewise. `null` keeps their pixels; lossless WebPs always stay lossless. */
  webpQuality: number | null;
  /** oxipng level 0–6. PNGs are always lossless. */
  pngLevel: number;
  /** Finish small PNGs with Zopfli; omitted means false. */
  pngZopfli?: boolean;
  /** EXIF except orientation, XMP, IPTC, comments and text chunks; color profiles stay. Omitted means false. */
  stripMetadata?: boolean;
}

export type RasterFormat = "jpeg" | "png" | "webp";

/** Why an image result is the original file byte for byte. */
export type RasterKept = "notSmaller" | "extraData" | "signed" | "animated" | "unsupportedEncoding" | "lossyEncoding";

export interface RasterReport {
  format: RasterFormat;
  width: number;
  height: number;
  originalSize: number;
  outputSize: number;
  /** Set when the result is the original file byte for byte. */
  kept: RasterKept | null;
  /** Re-encoded at the requested JPEG or WebP quality; otherwise the pixels are exactly the original ones. */
  reencoded: boolean;
  metadataRemoved: boolean;
}

/**
 * Pipeline stages in the order the engines run them. PDFs: `loading`, `structure`, `images` (lossy presets only),
 * `streams`, `saving`, `verifying`. Images: `loading`, `encoding`, `verifying`.
 */
export type Stage = "loading" | "structure" | "images" | "streams" | "encoding" | "saving" | "verifying";

/** `total: 0` means the stage is not counted; otherwise `done` grows from 0 to `total`. */
export interface Progress {
  stage: Stage;
  done: number;
  total: number;
}

export interface SavedFile {
  id: FileId;
  name: string;
}

export interface UpdateStatus {
  currentVersion: string;
  /** The newer version the release manifest offers; `null` when this one is current. */
  availableVersion: string | null;
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
  | "imageTooLarge"
  | "imageUnsupported"
  | "imageMalformed"
  | "invalidOptions"
  | "cancelled"
  | "busy"
  | "unsavedResults"
  | "update"
  | "backgroundTooLarge"
  | "backgroundUnsupported"
  | "noSubject"
  | "modelMissing"
  | "modelDownload"
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
  pickFiles: (kinds: FileKind[], filterName: string, scope?: "background") =>
    invoke<RegisterOutcome>("pick_files", { kinds, filterName, scope }),

  /** Kinds that dropped folders contribute; the shown tool sets them. Directly dropped files are not filtered. */
  setDropKinds: (kinds: FileKind[], scope?: "background") => invoke<void>("set_drop_kinds", { kinds, scope }),

  /** Forgets the file and its unsaved result. */
  removeFile: (id: FileId) => invoke<void>("remove_file", { id }),

  /**
   * Compresses into a temp result; the original is only read. `onProgress` receives throttled stage updates.
   * Rejects with `cancelled` after `cancelCompression`.
   */
  compressPdf: (id: FileId, options: PdfOptions, onProgress: (progress: Progress) => void = () => {}) => {
    const channel = new Channel<Progress>(onProgress);
    return invoke<PdfReport>("compress_pdf", { id, options, onProgress: channel });
  },

  /** Like `compressPdf`, for one JPEG, PNG or WebP; the format is detected from the content and kept. */
  compressImage: (id: FileId, options: RasterOptions, onProgress: (progress: Progress) => void = () => {}) => {
    const channel = new Channel<Progress>(onProgress);
    return invoke<RasterReport>("compress_image", { id, options, onProgress: channel });
  },

  backgroundPreview: async (id: FileId) => imageData(await invoke<number[]>("background_preview", { id })),
  backgroundModelStatus: () => invoke<ModelStatus>("background_model_status"),
  downloadBackgroundModel: (onProgress: (progress: DownloadProgress) => void) =>
    invoke<ModelStatus>("download_background_model", { onProgress: new Channel<DownloadProgress>(onProgress) }),
  removeBackgroundModel: () => invoke<void>("remove_background_model"),
  releaseBackgroundModel: () => invoke<void>("release_background_model"),
  removeBackground: async (id: FileId, options: BackgroundOptions): Promise<BackgroundReport> => {
    const report = await invoke<Omit<BackgroundReport, "preview" | "originalPreview"> & { preview: number[]; originalPreview: number[] }>("remove_background", { id, options });
    return { ...report, preview: imageData(report.preview), originalPreview: imageData(report.originalPreview) };
  },

  /** Stops every compression already started at its next checkpoint; a no-op when nothing runs. */
  cancelCompression: () => invoke<void>("cancel_compression"),

  /** Native save dialog next to the original; resolves with the saved file name, or `null` when cancelled. */
  saveResult: (id: FileId) => invoke<string | null>("save_result", { id }),

  /** Native folder picker; saves each result as `<name>-compressed.<ext>` without overwriting. `null` when cancelled. */
  saveResultsToFolder: (ids: FileId[]) => invoke<SavedFile[] | null>("save_results_to_folder", { ids }),

  /** Shows the last saved copy in the system file manager. */
  revealResult: (id: FileId) => invoke<void>("reveal_result", { id }),

  /** Asks the release manifest whether a newer version exists; nothing is downloaded yet (ADR-0013). */
  checkForUpdate: () => invoke<UpdateStatus>("check_for_update"),

  /**
   * Downloads, verifies and installs the update found by the last check, then restarts the app, so it settles only
   * on failure. Rejects with `busy` while work runs and with `unsavedResults` unless `discardUnsaved`.
   */
  installUpdate: (discardUnsaved: boolean) => invoke<void>("install_update", { discardUnsaved }),

  /** Files dropped on the window, already registered by Rust; dropped folders arrive expanded. */
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
