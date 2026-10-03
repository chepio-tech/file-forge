// Core
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
// Utils
import releaseMatrixEntries from "./releaseMatrixEntries";

const root = process.cwd();
const script = resolve(root, ".github/scripts/updaterManifest.mjs");
// Signed by `tauri signer` with throwaway keys that were never stored: `asset.bin.sig` binds version 0.2.0,
// `asset-unversioned.bin.sig` binds none. `other.key.pub` is an unrelated key.
const fixtures = resolve(root, ".github/scripts/testdata/updater");
const fixture = (name: string) => readFileSync(join(fixtures, name), "utf8").trim();
const workspaces: string[] = [];

afterEach(() => {
  for (const workspace of workspaces.splice(0)) rmSync(workspace, { recursive: true, force: true });
});

interface Options {
  tag?: string;
  pubkey?: string;
  signature?: string;
  asset?: string;
  targets?: string[];
}

function writeManifest({ tag = "v0.2.0", pubkey, signature, asset, targets }: Options = {}) {
  const workspace = mkdtempSync(join(tmpdir(), "fileforge-updater-"));
  workspaces.push(workspace);
  mkdirSync(join(workspace, "src-tauri"));
  mkdirSync(join(workspace, "installers"));
  mkdirSync(join(workspace, "updates"));
  const config = { plugins: { updater: { pubkey: pubkey ?? fixture("fixture.key.pub") } } };
  writeFileSync(join(workspace, "src-tauri/tauri.conf.json"), JSON.stringify(config));
  for (const target of targets ?? ["linux-x86_64-appimage", "windows-x86_64-nsis"]) {
    const name = `FileForge ${target}.bin`;
    if (asset === undefined) cpSync(join(fixtures, "asset.bin"), join(workspace, "installers", name));
    else writeFileSync(join(workspace, "installers", name), asset);
    writeFileSync(join(workspace, "updates", `${target}.asset`), `${name}\n`);
    writeFileSync(join(workspace, "updates", `${target}.sig`), signature ?? fixture("asset.bin.sig"));
  }
  const result = spawnSync("node", [script], {
    cwd: workspace,
    encoding: "utf8",
    env: { PATH: process.env.PATH, TAG: tag, GITHUB_REPOSITORY: "chepio-tech/file-forge" },
  });
  const manifestPath = join(workspace, "installers/latest.json");
  const manifest = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, "utf8")) : null;
  return { ...result, manifest };
}

describe("updater manifest", () => {
  it("lists every target with its release asset and signature", () => {
    const result = writeManifest();

    expect(result.status, result.stdout).toBe(0);
    expect(result.manifest).toEqual({
      version: "0.2.0",
      platforms: {
        "linux-x86_64-appimage": {
          url: "https://github.com/chepio-tech/file-forge/releases/download/v0.2.0/FileForge%20linux-x86_64-appimage.bin",
          signature: fixture("asset.bin.sig"),
        },
        "windows-x86_64-nsis": {
          url: "https://github.com/chepio-tech/file-forge/releases/download/v0.2.0/FileForge%20windows-x86_64-nsis.bin",
          signature: fixture("asset.bin.sig"),
        },
      },
    });
  });

  it.each([
    ["a key the app does not trust", { pubkey: fixture("other.key.pub") }, "but the app trusts key"],
    ["a changed artifact", { asset: "FileForge updater test asset, modified\n" }, "does not match the file"],
    ["another release version", { tag: "v0.2.1" }, "signed for version 0.2.0, but the release is 0.2.1"],
    ["a signature without a version", { signature: fixture("asset-unversioned.bin.sig") }, "signed for version (none)"],
    ["a malformed signature", { signature: "not a signature" }, "malformed"],
    ["a placeholder public key", { pubkey: "REPLACE_ME" }, "not a minisign public key"],
    ["no collected artifacts", { targets: [] }, "No updater artifacts"],
  ])("refuses %s and writes no manifest", (_case, options: Options, message) => {
    const result = writeManifest(options);

    expect(result.status).toBe(1);
    expect(result.stdout).toContain("::error::");
    expect(result.stdout).toContain(message);
    expect(result.manifest).toBeNull();
  });
});

describe("updater configuration", () => {
  const config = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
  const release = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.release.conf.json"), "utf8"));
  const workflow = readFileSync(resolve(root, ".github/workflows/release.yml"), "utf8");

  it("reads the manifest of the latest release and requires version-bound signatures", () => {
    const updater = config.plugins.updater;
    expect(updater.endpoints).toEqual(["https://github.com/chepio-tech/file-forge/releases/latest/download/latest.json"]);
    expect(updater.requireSignedVersion).toBe(true);
    expect(updater.windows).toEqual({ installMode: "passive" });
    const key = Buffer.from(Buffer.from(updater.pubkey, "base64").toString("utf8").trim().split("\n").at(-1)!, "base64");
    expect(key).toHaveLength(42);
    expect(key.toString("latin1", 0, 2)).toBe("Ed");
  });

  it("signs updater artifacts only in release builds, so local builds need no key", () => {
    expect(config.bundle.createUpdaterArtifacts).toBeUndefined();
    expect(release).toMatchObject({ bundle: { createUpdaterArtifacts: true } });
    expect(workflow).toContain("pnpm tauri build --config src-tauri/tauri.release.conf.json");
    expect(workflow).toContain("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}");
    expect(workflow).toContain("TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}");
    expect(workflow.indexOf("node .github/scripts/updaterManifest.mjs")).toBeGreaterThan(0);
    expect(workflow.indexOf("node .github/scripts/updaterManifest.mjs")).toBeLessThan(
      workflow.indexOf("bash .github/scripts/publishRelease.sh"),
    );
  });

  it("offers an update for every installable package in its own format", () => {
    const updates = releaseMatrixEntries("updates");
    const folders = { app: "macos", appimage: "appimage", deb: "deb", rpm: "rpm", nsis: "nsis", msi: "msi" };

    expect(updates.map(([target]) => target).sort()).toEqual([
      "darwin-aarch64-app",
      "darwin-x86_64-app",
      "linux-x86_64-appimage",
      "linux-x86_64-deb",
      "linux-x86_64-rpm",
      "windows-x86_64-msi",
      "windows-x86_64-nsis",
    ]);
    for (const [target, glob] of updates) {
      const bundleType = target!.split("-").at(-1) as keyof typeof folders;
      expect(glob, target).toMatch(new RegExp(`^${folders[bundleType]}/`));
    }
    const assets = updates.map(([, , asset]) => asset);
    expect(new Set(assets).size).toBe(assets.length);
    // Every installer except the DMG (which installs the `.app`) is itself an updater artifact.
    const installers = releaseMatrixEntries("installers").map(([, asset]) => asset);
    expect(installers.filter((asset) => !asset!.endsWith(".dmg")).every((asset) => assets.includes(asset))).toBe(true);
  });
});
