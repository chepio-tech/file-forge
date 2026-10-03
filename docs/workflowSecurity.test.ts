// Core
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const root = process.cwd();
const script = resolve(root, ".github/scripts/publishRelease.sh");
const workspaces: string[] = [];

afterEach(() => {
  for (const workspace of workspaces.splice(0)) rmSync(workspace, { recursive: true, force: true });
});

function publish(options: { status?: string; tag?: string; assets?: boolean; createExit?: number } = {}) {
  const workspace = mkdtempSync(join(tmpdir(), "fileforge-release-"));
  workspaces.push(workspace);
  const bin = join(workspace, "bin");
  const log = join(workspace, "calls");
  mkdirSync(bin);
  writeFileSync(join(workspace, "Cargo.toml"), '[workspace.package]\nversion = "0.1.0"\n');
  if (options.assets !== false) {
    mkdirSync(join(workspace, "installers"));
    writeFileSync(join(workspace, "installers/FileForge-test.dmg"), "test installer");
    writeFileSync(join(workspace, "installers/FileForge with space.exe"), "test installer");
  }
  // A local CLI stub records calls; these tests never contact GitHub or receive credentials.
  writeFileSync(join(bin, "gh"), `#!/usr/bin/env bash
set -eu
printf '%s\\0' "$@" >> "$GH_TEST_LOG"
printf '\\n' >> "$GH_TEST_LOG"
if [ "$1" = "api" ]; then
  if [ "$GH_TEST_STATUS" = "network" ]; then exit 1; fi
  printf 'HTTP/2.0 %s Test\\n\\n' "$GH_TEST_STATUS"
  if [ "$GH_TEST_STATUS" = "200" ]; then exit 0; fi
  exit 1
fi
if [ "$1" = "release" ] && [ "$2" = "create" ]; then exit "$GH_TEST_CREATE_EXIT"; fi
exit 99
`, { mode: 0o755 });
  const result = spawnSync("bash", [script], {
    cwd: workspace,
    encoding: "utf8",
    env: {
      PATH: `${bin}:${process.env.PATH ?? "/usr/bin:/bin"}`,
      TAG: options.tag ?? "v0.1.0",
      GITHUB_REPOSITORY: "chepio-tech/file-forge",
      GH_TEST_LOG: log,
      GH_TEST_STATUS: options.status ?? "404",
      GH_TEST_CREATE_EXIT: String(options.createExit ?? 0),
    },
  });
  const calls = existsSync(log)
    ? readFileSync(log, "utf8").split("\n").filter(Boolean).map((line) => line.split("\0").filter(Boolean))
    : [];
  return { ...result, calls };
}

describe("workflow security policy", () => {
  it("pins every action and reusable workflow to a full commit SHA", () => {
    const directory = join(root, ".github/workflows");
    const workflows = readdirSync(directory).filter((name) => /\.ya?ml$/.test(name));
    expect(workflows.length).toBeGreaterThan(0);
    for (const name of workflows) {
      const source = readFileSync(join(directory, name), "utf8");
      const references = [...source.matchAll(/^\s*(?:-\s*)?uses:\s*(\S+)/gm)].map((match) => match[1]!);
      expect(references.length, name).toBeGreaterThan(0);
      for (const reference of references) {
        expect(reference, `${name}: ${reference}`).toMatch(/^[\w.-]+\/[\w./-]+@[a-f0-9]{40}$/);
      }
    }
  });
});

describe("immutable release publication", () => {
  it("creates an absent release with all assets and verifies the pushed tag", () => {
    const result = publish();
    expect(result.status, result.stderr).toBe(0);
    expect(result.calls).toHaveLength(2);
    expect(result.calls[0]).toEqual([
      "api", "--include", "--silent", "repos/chepio-tech/file-forge/releases/tags/v0.1.0",
    ]);
    expect(result.calls[1]).toEqual([
      "release", "create", "v0.1.0", "installers/FileForge with space.exe", "installers/FileForge-test.dmg",
      "--verify-tag", "--title", "FileForge v0.1.0", "--notes",
      "Installers for macOS, Windows and Linux. Builds are not signed or notarized yet; see the README for first-launch steps.",
    ]);
  });

  it("refuses to modify an existing published release or draft", () => {
    const result = publish({ status: "200" });
    expect(result.status).toBe(1);
    expect(result.stdout).toContain("already exists");
    expect(result.calls).toHaveLength(1);
    expect(result.calls[0]?.[0]).toBe("api");
  });

  it.each(["401", "403", "500", "network"])("does not create a release after an API failure (%s)", (status) => {
    const result = publish({ status });
    expect(result.status).toBe(1);
    expect(result.stdout).toContain("refusing to publish");
    expect(result.calls).toHaveLength(1);
  });

  it("rejects a tag/version mismatch before any GitHub call", () => {
    const result = publish({ tag: "v0.1.1" });
    expect(result.status).toBe(1);
    expect(result.stdout).toContain("does not match");
    expect(result.calls).toEqual([]);
  });

  it("rejects missing installers before any GitHub call", () => {
    const result = publish({ assets: false });
    expect(result.status).toBe(1);
    expect(result.stdout).toContain("No installers");
    expect(result.calls).toEqual([]);
  });

  it("reports publication failure without attempting an overwrite or deleting a release", () => {
    const result = publish({ createExit: 9 });
    expect(result.status).toBe(9);
    expect(result.calls).toHaveLength(2);
    expect(result.calls[1]?.slice(0, 2)).toEqual(["release", "create"]);
  });
});
