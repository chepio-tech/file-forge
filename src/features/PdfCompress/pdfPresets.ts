// Types
import type { PdfOptions } from "@/services/fileforgeApi";

/**
 * Preset values (ADR-0003). Ranges mirror `JPEG_QUALITY_RANGE` and `MAX_DPI_RANGE` in
 * `crates/fileforge-core/src/pdf/options.rs`, which validates them again at the IPC boundary.
 */
export const JPEG_QUALITY = { min: 30, max: 95 } as const;
export const MAX_DPI = { min: 72, max: 600 } as const;

export type PresetId = "lossless" | "balanced" | "maximum" | "custom";

export const PRESETS: Record<Exclude<PresetId, "custom">, PdfOptions> = {
  lossless: { images: null },
  balanced: { images: { jpegQuality: 85, maxDpi: 200 } },
  maximum: { images: { jpegQuality: 70, maxDpi: 150 } },
};

export const DEFAULT_PRESET = "lossless" satisfies PresetId;

export function clamp(value: number, range: { min: number; max: number }): number {
  return Math.min(range.max, Math.max(range.min, Math.round(value)));
}

/** Stable key for "were these results produced with the current settings?". */
export function optionsKey(options: PdfOptions): string {
  return options.images ? `${options.images.jpegQuality}/${options.images.maxDpi ?? "keep"}` : "lossless";
}

export default PRESETS;
