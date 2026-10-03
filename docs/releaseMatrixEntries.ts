// Core
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

/**
 * The entries of one list field of the release matrix (`.github/workflows/release.yml`), inline or folded (`>-`),
 * each split at `=`: `installers` gives `[glob, asset]`, `updates` gives `[target, glob, asset]`.
 */
export default function releaseMatrixEntries(key: "installers" | "updates"): string[][] {
  const lines = readFileSync(resolve(process.cwd(), ".github/workflows/release.yml"), "utf8").split("\n");
  const indentOf = (line: string) => line.length - line.trimStart().length;
  const entries: string[] = [];
  lines.forEach((line, index) => {
    const match = line.match(new RegExp(`^\\s+${key}: (.+)$`));
    if (!match) return;
    if (match[1] !== ">-") {
      entries.push(...match[1].trim().split(/\s+/));
      return;
    }
    for (const next of lines.slice(index + 1)) {
      if (!next.trim() || indentOf(next) <= indentOf(line)) break;
      entries.push(...next.trim().split(/\s+/));
    }
  });
  return entries.map((entry) => entry.split("="));
}
