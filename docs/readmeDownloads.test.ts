// Core
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const readme = readFileSync(resolve(root, "README.md"), "utf8");
const releaseWorkflow = readFileSync(resolve(root, ".github/workflows/release.yml"), "utf8");
const downloadSection = readme.split("## Download\n")[1]?.split("## About\n")[0] ?? "";
const document = new DOMParser().parseFromString(downloadSection, "text/html");
const installersUrl = "https://github.com/chepio-tech/file-forge/raw/main/installers/";

describe("README downloads", () => {
  it("places downloading before the app description and features", () => {
    expect(readme.match(/^## .+$/m)?.[0]).toBe("## Download");
    expect(readme.indexOf("## Download")).toBeLessThan(readme.indexOf("Desktop PDF compression"));
    expect(downloadSection).toContain("Until the release workflow commits the first build");
  });

  it("downloads each platform's installer directly with accessible, equally sized local images", () => {
    const links = [...document.querySelectorAll("a")];
    expect(links).toHaveLength(3);
    expect(links.map((link) => link.querySelector("img")?.alt)).toEqual([
      "Download for macOS", "Download for Windows", "Download for Linux",
    ]);
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      `${installersUrl}FileForge-macOS-AppleSilicon.dmg`,
      `${installersUrl}FileForge-Windows-x64-setup.exe`,
      `${installersUrl}FileForge-Linux-x64.AppImage`,
    ]);
    for (const link of links) {
      const image = link.querySelector("img")!;
      expect(image.getAttribute("width")).toBe("260");
      expect(image.getAttribute("height")).toBe("80");
      expect(image.getAttribute("src")).toMatch(/^docs\/assets\/download-[a-z]+\/download-[a-z]+\.png$/);
      const path = resolve(root, image.getAttribute("src")!);
      const bytes = readFileSync(path);
      expect([...bytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
      expect(bytes.toString("ascii", 12, 16)).toBe("IHDR");
      expect(bytes.readUInt32BE(16)).toBe(780);
      expect(bytes.readUInt32BE(20)).toBe(240);
      expect(bytes[25]).toBe(6); // RGBA corners work in both GitHub themes.
      expect(bytes.length).toBeLessThan(150_000);
    }
  });

  it("links exactly the installers the release workflow commits", () => {
    const committed = [...releaseWorkflow.matchAll(/=(FileForge-[\w.-]+)/g)].map((match) => match[1]);
    expect(committed).toHaveLength(7);
    expect(new Set(committed).size).toBe(committed.length);
    const linkPattern = new RegExp(`${installersUrl.replace(/[.]/g, "\\.")}([\\w.-]+)\\)`, "g");
    const linked = [...downloadSection.matchAll(linkPattern)].map((match) => match[1]);
    expect(new Set(linked)).toEqual(new Set(committed));
    expect(linked).toHaveLength(committed.length);
  });

  it("uses static image links that survive GitHub Markdown sanitization", () => {
    expect(document.querySelector("script, style, button, svg, iframe")).toBeNull();
    expect(document.querySelectorAll("img")).toHaveLength(3);
    for (const element of document.querySelectorAll("*")) {
      for (const attribute of element.attributes) {
        expect(attribute.name).not.toMatch(/^(on|style$)/i);
      }
    }
  });
});
