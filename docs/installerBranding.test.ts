// Core
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const config = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
const bundle = config.bundle;
const readAsset = (path: string) => readFileSync(resolve(root, "src-tauri", path));

function bitmap(path: string, width: number, height: number) {
  const bytes = readAsset(path);
  expect(bytes.toString("ascii", 0, 2), path).toBe("BM");
  expect(bytes.readUInt32LE(2), path).toBe(bytes.length);
  expect(bytes.readInt32LE(18), path).toBe(width);
  expect(bytes.readInt32LE(22), path).toBe(height);
  expect(bytes.readUInt16LE(26), path).toBe(1);
  expect(bytes.readUInt16LE(28), path).toBe(24);
  expect(bytes.readUInt32LE(30), path).toBe(0); // Native installers need uncompressed RGB bitmaps.
  const offset = bytes.readUInt32LE(10);
  const stride = Math.ceil(width * 3 / 4) * 4;
  expect(bytes.length).toBe(offset + stride * height);
  const pixel = (x: number, y: number) => {
    const start = offset + (height - 1 - y) * stride + x * 3;
    return [...bytes.subarray(start, start + 3)].reverse();
  };
  return { pixel };
}

function tiffRepresentations(path: string) {
  const bytes = readAsset(path);
  const byteOrder = bytes.toString("ascii", 0, 2);
  expect(["II", "MM"], path).toContain(byteOrder);
  const read16 = (offset: number) => byteOrder === "II" ? bytes.readUInt16LE(offset) : bytes.readUInt16BE(offset);
  const read32 = (offset: number) => byteOrder === "II" ? bytes.readUInt32LE(offset) : bytes.readUInt32BE(offset);
  expect(read16(2), path).toBe(42);
  const representations = [];
  let offset = read32(4);
  while (offset !== 0) {
    expect(representations.length).toBeLessThan(2);
    const count = read16(offset);
    const fields = new Map<number, number>();
    for (let index = 0; index < count; index++) {
      const entry = offset + 2 + index * 12;
      if (read32(entry + 4) !== 1) continue;
      const type = read16(entry + 2);
      const value = read32(entry + 8);
      if (type === 3) fields.set(read16(entry), read16(entry + 8));
      if (type === 4) fields.set(read16(entry), value);
      if (type === 5) fields.set(read16(entry), read32(value) / read32(value + 4));
    }
    representations.push({
      width: fields.get(256), height: fields.get(257),
      dpiX: fields.get(282), dpiY: fields.get(283), resolutionUnit: fields.get(296),
    });
    offset = read32(offset + 2 + count * 12);
  }
  return representations;
}

describe("native installer branding", () => {
  it("uses the application ICO for setup and uninstall with 32-bit frames for standard and high DPI", () => {
    const { installerIcon, uninstallerIcon } = bundle.windows.nsis;
    expect(bundle.icon).toContain(installerIcon);
    expect(uninstallerIcon).toBe(installerIcon);
    const bytes = readAsset(installerIcon);
    expect(bytes.readUInt16LE(0)).toBe(0);
    expect(bytes.readUInt16LE(2)).toBe(1);
    const count = bytes.readUInt16LE(4);
    expect(count).toBeGreaterThanOrEqual(6);
    const sizes = [];
    for (let index = 0; index < count; index++) {
      const entry = 6 + index * 16;
      const width = bytes[entry] || 256;
      const height = bytes[entry + 1] || 256;
      expect(height).toBe(width);
      expect(bytes.readUInt16LE(entry + 6)).toBe(32);
      const length = bytes.readUInt32LE(entry + 8);
      const offset = bytes.readUInt32LE(entry + 12);
      expect(offset).toBeGreaterThanOrEqual(6 + count * 16);
      expect(offset + length).toBeLessThanOrEqual(bytes.length);
      if (bytes.toString("ascii", offset + 1, offset + 4) === "PNG") {
        expect(bytes.readUInt32BE(offset + 16)).toBe(width);
        expect(bytes.readUInt32BE(offset + 20)).toBe(height);
        expect([bytes[offset + 24], bytes[offset + 25]]).toEqual([8, 6]);
      } else {
        expect(bytes.readInt32LE(offset + 4)).toBe(width);
        expect(bytes.readInt32LE(offset + 8)).toBe(height * 2); // ICO DIBs include the transparency mask.
        expect(bytes.readUInt16LE(offset + 14)).toBe(32);
      }
      sizes.push(width);
    }
    expect(sizes).toEqual(expect.arrayContaining([16, 24, 32, 48, 64, 256]));
  });

  it("uses 2× bitmaps in the native NSIS and WiX proportions with uncompressed RGB encoding", () => {
    bitmap(bundle.windows.nsis.headerImage, 300, 114);
    bitmap(bundle.windows.nsis.sidebarImage, 328, 628);
    bitmap(bundle.windows.wix.bannerPath, 986, 116);
    bitmap(bundle.windows.wix.dialogImagePath, 986, 624);
    const hooks = readAsset(bundle.windows.nsis.installerHooks).toString();
    expect(hooks).toContain("!define MUI_HEADERIMAGE_RIGHT");
    for (const setting of ["MUI_HEADERIMAGE_BITMAP_STRETCH", "MUI_HEADERIMAGE_UNBITMAP_STRETCH", "MUI_WELCOMEFINISHPAGE_BITMAP_STRETCH"]) {
      expect(hooks).toContain(`!define ${setting} "AspectFitHeight"`);
    }
    expect(bundle.windows.nsis.template).toBeUndefined();
    expect(bundle.windows.wix.template).toBeUndefined();
  });

  it("keeps the native MSI title and welcome text areas clear of artwork", () => {
    const banner = bitmap(bundle.windows.wix.bannerPath, 986, 116);
    const dialog = bitmap(bundle.windows.wix.dialogImagePath, 986, 624);
    const verifyWhiteArea = (image: ReturnType<typeof bitmap>, left: number, top: number, right: number, bottom: number) => {
      let coloredPixels = 0;
      let firstColoredPixel: { x: number; y: number } | undefined;
      for (let y = top; y < bottom; y++) {
        for (let x = left; x < right; x++) {
          const [red, green, blue] = image.pixel(x, y);
          if (red !== 255 || green !== 255 || blue !== 255) {
            coloredPixels++;
            firstColoredPixel ??= { x, y };
          }
        }
      }
      expect({ coloredPixels, firstColoredPixel }).toEqual({ coloredPixels: 0, firstColoredPixel: undefined });
    };
    verifyWhiteArea(banner, 0, 0, 660, 116);
    verifyWhiteArea(dialog, 328, 0, 986, 624);
    expect(bundle.publisher).toBe("Chepio.tech");
    expect(bundle.homepage).toBe("https://chepio.tech");
  });

  it("keeps clean Windows signatures inside their margins without a masked tagline", () => {
    for (const path of [bundle.windows.nsis.headerImage, bundle.windows.nsis.sidebarImage,
      bundle.windows.wix.bannerPath, bundle.windows.wix.dialogImagePath]) {
      const svg = readAsset(path.replace(/\.bmp$/, ".svg")).toString();
      const source = new DOMParser().parseFromString(svg, "image/svg+xml");
      const canvas = source.documentElement;
      const signature = source.querySelector("svg > svg")!;
      const x = Number(signature.getAttribute("x"));
      const y = Number(signature.getAttribute("y"));
      const width = Number(signature.getAttribute("width"));
      const height = Number(signature.getAttribute("height"));
      expect(signature.children.length, path).toBe(2); // Wordmark and frame, with no tagline or cover rectangle.
      expect(signature.querySelectorAll("path").length, path).toBe(2);
      expect(width, path).toBe(120);
      expect(width / height, path).toBeCloseTo(339 / 58, 3);
      expect(x, path).toBeGreaterThanOrEqual(15);
      expect(x + width, path).toBeLessThanOrEqual(Number(canvas.getAttribute("width")) - 15);
      expect(y, path).toBeGreaterThanOrEqual(18);
      expect(y + height, path).toBeLessThanOrEqual(Number(canvas.getAttribute("height")) - 15);
      if (path === bundle.windows.nsis.sidebarImage) expect(y).toBe(264);
      if (path === bundle.windows.wix.dialogImagePath) expect(y).toBe(262);
      const canvasWidth = Number(canvas.getAttribute("width"));
      const canvasHeight = Number(canvas.getAttribute("height"));
      const image = bitmap(path, canvasWidth * 2, canvasHeight * 2);
      const bluePixels = [];
      // Sidebar app icons sit above this area; headers contain only the signature.
      for (let row = canvasHeight > 100 ? 400 : 0; row < canvasHeight * 2; row++) {
        for (let column = 0; column < canvasWidth * 2; column++) {
          const [red, , blue] = image.pixel(column, row);
          if (blue! - red! > 60) bluePixels.push({ x: column, y: row });
        }
      }
      expect(bluePixels.length, path).toBeGreaterThan(100);
      expect(bluePixels.every((pixel) => pixel.x >= Math.floor(x * 2) && pixel.x < Math.ceil((x + width) * 2)
        && pixel.y >= Math.floor(y * 2) && pixel.y < Math.ceil((y + height) * 2)), path).toBe(true);
    }
  });

  it("uses the current application icon in both Windows welcome panels", () => {
    const icon = readFileSync(resolve(root, "public/app-icon.png"));
    for (const path of [bundle.windows.nsis.sidebarImage, bundle.windows.wix.dialogImagePath]) {
      const svg = readAsset(path.replace(/\.bmp$/, ".svg")).toString();
      const source = new DOMParser().parseFromString(svg, "image/svg+xml");
      const embedded = source.querySelector("image")?.getAttribute("href");
      expect(embedded, path).toMatch(/^data:image\/png;base64,/);
      expect(Buffer.from(embedded!.split(",")[1]!, "base64").equals(icon), path).toBe(true);
    }
  });

  it("matches the DMG background to its window and keeps the drag targets above the signature", () => {
    const dmg = bundle.macOS.dmg;
    expect(dmg.background).toMatch(/\.tiff$/);
    expect(tiffRepresentations(dmg.background)).toEqual([1, 2].map((scale) => ({
      width: dmg.windowSize.width * scale, height: dmg.windowSize.height * scale,
      dpiX: 72 * scale, dpiY: 72 * scale, resolutionUnit: 2,
    })));
    for (const position of [dmg.appPosition, dmg.applicationFolderPosition]) {
      expect(position.x).toBeGreaterThan(100);
      expect(position.x).toBeLessThan(dmg.windowSize.width - 100);
      expect(position.y).toBeGreaterThan(100);
      expect(position.y).toBeLessThan(250);
    }
    expect(dmg.applicationFolderPosition.x - dmg.appPosition.x).toBeGreaterThan(200);
    const workflow = readFileSync(resolve(root, ".github/workflows/release.yml"), "utf8");
    expect(workflow).toMatch(/run: pnpm tauri build[^\n]*\n\s*env:\n(?:\s*#[^\n]*\n)*\s*TAURI_BUNDLER_DMG_IGNORE_CI: "true"/);
  });

  it("keeps a smaller vector signature clear of the Finder title bar and bottom edge", () => {
    const svg = readAsset("branding/installer-branding/dmg-background.svg").toString();
    const source = new DOMParser().parseFromString(svg, "image/svg+xml");
    const signature = source.querySelector("svg > svg")!;
    const x = Number(signature.getAttribute("x"));
    const y = Number(signature.getAttribute("y"));
    const width = Number(signature.getAttribute("width"));
    const height = Number(signature.getAttribute("height"));
    expect(signature.querySelector("path")).not.toBeNull();
    expect(signature.querySelector("image")).toBeNull();
    expect(width).toBeGreaterThanOrEqual(150);
    expect(width).toBeLessThanOrEqual(180);
    expect(x + width).toBe(bundle.macOS.dmg.windowSize.width - 32);
    expect(y).toBeGreaterThanOrEqual(312);
    expect(y).toBeLessThanOrEqual(325);
    // Finder's 400-point window includes its 32-point title bar.
    expect(y + height).toBeLessThanOrEqual(bundle.macOS.dmg.windowSize.height - 32 - 10);
  });
});
