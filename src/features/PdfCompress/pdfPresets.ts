// Types
import type { PdfOptions } from "@/services/fileforgeApi";

/**
 * Preset values (ADR-0003). Ranges mirror `JPEG_QUALITY_RANGE` and `MAX_DPI_RANGE` in
 * `crates/fileforge-core/src/pdf/options.rs`, which validates them again at the IPC boundary.
 */
export const JPEG_QUALITY = { min: 30, max: 95 } as const;
export const MAX_DPI = { min: 72, max: 600 } as const;

export type PresetId = "lossless" | "balanced" | "maximum" | "screen" | "custom";

/** Removal options are off in every preset; choosing a preset keeps the user's removal choices. */
export const PRESETS: Record<Exclude<PresetId, "custom">, PdfOptions> = {
  lossless: { images: null, stripMetadata: false, stripEditingData: false },
  balanced: { images: { jpegQuality: 85, maxDpi: 200 }, stripMetadata: false, stripEditingData: false },
  maximum: {
    images: { jpegQuality: 70, maxDpi: 150, compressFlatePhotos: true },
    stripMetadata: false,
    stripEditingData: false,
  },
  screen: { images: { jpegQuality: 65, maxDpi: 100 }, stripMetadata: false, stripEditingData: false },
};

export const DEFAULT_PRESET = "lossless" satisfies PresetId;

export function clamp(value: number, range: { min: number; max: number }): number {
  return Math.min(range.max, Math.max(range.min, Math.round(value)));
}

/** Stable key for "were these results produced with the current settings?". */
export function optionsKey(options: PdfOptions): string {
  const images = options.images ? `${options.images.jpegQuality}/${options.images.maxDpi ?? "keep"}` : "lossless";
  return [
    images,
    options.images?.compressFlatePhotos ? "photos" : "",
    options.stripMetadata ? "metadata" : "",
    options.stripEditingData ? "editing" : "",
  ]
    .filter(Boolean)
    .join("+");
}

export default PRESETS;
