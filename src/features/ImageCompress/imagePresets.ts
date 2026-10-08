// Types
import type { RasterOptions } from "@/services/fileforgeApi";

/**
 * Preset values (ADR-0019). Ranges mirror `JPEG_QUALITY_RANGE` and `PNG_LEVEL_RANGE` in
 * `crates/fileforge-core/src/raster/options.rs`, which validates them again at the IPC boundary.
 */
export const JPEG_QUALITY = { min: 30, max: 95 } as const;
export const PNG_LEVEL = { min: 0, max: 6 } as const;

export type PresetId = "lossless" | "balanced" | "maximum" | "custom";

/** Metadata removal is off in every preset; choosing a preset keeps the user's choice. */
export const PRESETS: Record<Exclude<PresetId, "custom">, RasterOptions> = {
  lossless: { jpegQuality: null, pngLevel: 2, pngZopfli: false, stripMetadata: false },
  balanced: { jpegQuality: 85, pngLevel: 4, pngZopfli: false, stripMetadata: false },
  maximum: { jpegQuality: 75, pngLevel: 6, pngZopfli: true, stripMetadata: false },
};

export const DEFAULT_PRESET = "lossless" satisfies PresetId;

/** Stable key for "were these results produced with the current settings?". */
export function optionsKey(options: RasterOptions): string {
  return [
    options.jpegQuality === null ? "lossless" : `q${options.jpegQuality}`,
    `png${options.pngLevel}`,
    options.pngZopfli ? "zopfli" : "",
    options.stripMetadata ? "metadata" : "",
  ]
    .filter(Boolean)
    .join("+");
}

export default PRESETS;
