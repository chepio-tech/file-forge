// Utils
import { clamp, JPEG_QUALITY, MAX_DPI, optionsKey, PRESETS } from "./pdfPresets";

describe("pdfPresets", () => {
  it("defines lossless as no image changes and lossy presets inside the engine's ranges", () => {
    expect(PRESETS.lossless.images).toBeNull();
    for (const preset of [PRESETS.balanced, PRESETS.maximum, PRESETS.screen]) {
      const images = preset.images!;
      expect(clamp(images.jpegQuality, JPEG_QUALITY)).toBe(images.jpegQuality);
      expect(clamp(images.maxDpi!, MAX_DPI)).toBe(images.maxDpi);
    }
  });

  it("clamps and rounds user input", () => {
    expect(clamp(10, JPEG_QUALITY)).toBe(30);
    expect(clamp(99, JPEG_QUALITY)).toBe(95);
    expect(clamp(150.6, MAX_DPI)).toBe(151);
  });

  it("gives equal settings equal keys", () => {
    const keep = { stripMetadata: false, stripEditingData: false };
    expect(optionsKey(PRESETS.balanced)).toBe(optionsKey({ images: { jpegQuality: 85, maxDpi: 200 }, ...keep }));
    expect(optionsKey(PRESETS.balanced)).not.toBe(optionsKey(PRESETS.maximum));
    expect(optionsKey(PRESETS.maximum)).not.toBe(optionsKey(PRESETS.screen));
    expect(optionsKey({ images: { jpegQuality: 85, maxDpi: null }, ...keep })).toBe("85/keep");
  });

  it("keeps removal options out of every preset and in the key", () => {
    for (const preset of Object.values(PRESETS)) {
      expect([preset.stripMetadata, preset.stripEditingData]).toEqual([false, false]);
    }
    expect(optionsKey({ ...PRESETS.lossless, stripMetadata: true })).toBe("lossless+metadata");
    expect(optionsKey({ ...PRESETS.maximum, stripEditingData: true })).toBe("70/150+editing");
    expect(optionsKey({ ...PRESETS.lossless, stripMetadata: true, stripEditingData: true })).toBe(
      "lossless+metadata+editing",
    );
  });
});
