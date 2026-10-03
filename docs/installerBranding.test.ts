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

describe("native installer branding", () => {
  it("uses bitmaps with the dimensions and encoding required by NSIS and WiX", () => {
    bitmap(bundle.windows.nsis.headerImage, 150, 57);
    bitmap(bundle.windows.nsis.sidebarImage, 164, 314);
    bitmap(bundle.windows.wix.bannerPath, 493, 58);
    bitmap(bundle.windows.wix.dialogImagePath, 493, 312);
    expect(readAsset(bundle.windows.nsis.installerHooks).toString()).toContain("!define MUI_HEADERIMAGE_RIGHT");
    expect(bundle.windows.nsis.template).toBeUndefined();
    expect(bundle.windows.wix.template).toBeUndefined();
  });

  it("keeps the native MSI title and welcome text areas clear of artwork", () => {
    const banner = bitmap(bundle.windows.wix.bannerPath, 493, 58);
    const dialog = bitmap(bundle.windows.wix.dialogImagePath, 493, 312);
    for (let y = 0; y < 58; y++) {
      for (let x = 0; x < 330; x++) expect(banner.pixel(x, y)).toEqual([255, 255, 255]);
    }
    for (let y = 0; y < 312; y++) {
      for (let x = 164; x < 493; x++) expect(dialog.pixel(x, y)).toEqual([255, 255, 255]);
    }
    expect(bundle.publisher).toBe("Chepio.tech");
    expect(bundle.homepage).toBe("https://chepio.tech");
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
    const bytes = readAsset(dmg.background);
    expect([...bytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
    expect(bytes.readUInt32BE(16)).toBe(dmg.windowSize.width);
    expect(bytes.readUInt32BE(20)).toBe(dmg.windowSize.height);
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
});
