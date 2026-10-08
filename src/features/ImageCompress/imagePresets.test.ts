// Utils
import { JPEG_QUALITY, optionsKey, PNG_LEVEL, PRESETS } from "./imagePresets";

describe("imagePresets", () => {
  it("keeps JPEG pixels in Lossless and stays inside the engine's ranges", () => {
    expect(PRESETS.lossless.jpegQuality).toBeNull();
    for (const preset of Object.values(PRESETS)) {
      const quality = preset.jpegQuality ?? JPEG_QUALITY.min;
      expect(quality).toBeGreaterThanOrEqual(JPEG_QUALITY.min);
      expect(quality).toBeLessThanOrEqual(JPEG_QUALITY.max);
      expect(preset.pngLevel).toBeGreaterThanOrEqual(PNG_LEVEL.min);
      expect(preset.pngLevel).toBeLessThanOrEqual(PNG_LEVEL.max);
      expect(preset.stripMetadata).toBe(false);
    }
  });

  it("gives equal settings equal keys and different settings different keys", () => {
    const keys = Object.values(PRESETS).map(optionsKey);
    expect(new Set(keys).size).toBe(keys.length);
    expect(optionsKey({ ...PRESETS.balanced })).toBe(optionsKey(PRESETS.balanced));
    expect(optionsKey({ ...PRESETS.balanced, stripMetadata: true })).not.toBe(optionsKey(PRESETS.balanced));
    expect(optionsKey({ ...PRESETS.maximum, pngZopfli: false })).not.toBe(optionsKey(PRESETS.maximum));
  });
});
