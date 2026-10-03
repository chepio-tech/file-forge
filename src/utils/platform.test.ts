// Utils
import markPlatform from "./platform";

describe("markPlatform", () => {
  it("marks macOS so the overlay title bar gets room", () => {
    const root = document.createElement("html");
    markPlatform(root, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15");
    expect(root).toHaveClass("platform-mac");
  });

  it("leaves Windows and Linux alone", () => {
    for (const userAgent of ["Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/140", "Mozilla/5.0 (X11; Linux x86_64)"]) {
      const root = document.createElement("html");
      markPlatform(root, userAgent);
      expect(root).not.toHaveClass("platform-mac");
    }
  });
});
