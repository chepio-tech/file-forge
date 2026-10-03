// Utils
import { clamp, JPEG_QUALITY, MAX_DPI, optionsKey, PRESETS } from "./pdfPresets";

describe("pdfPresets", () => {
  it("defines lossless as no image changes and lossy presets inside the engine's ranges", () => {
    expect(PRESETS.lossless.images).toBeNull();
    for (const preset of [PRESETS.balanced, PRESETS.maximum]) {
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
    expect(optionsKey(PRESETS.balanced)).toBe(optionsKey({ images: { jpegQuality: 85, maxDpi: 200 } }));
    expect(optionsKey(PRESETS.balanced)).not.toBe(optionsKey(PRESETS.maximum));
    expect(optionsKey({ images: { jpegQuality: 85, maxDpi: null } })).toBe("85/keep");
  });
});
