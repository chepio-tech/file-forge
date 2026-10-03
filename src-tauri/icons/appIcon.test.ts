// Core
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { inflateSync } from "node:zlib";
import { describe, expect, it } from "vitest";

interface AlphaImage {
  width: number;
  height: number;
  alpha: Uint8Array;
}

const root = process.cwd();
const TRANSPARENT = 8;
const OPAQUE = 247;

function paeth(left: number, up: number, upLeft: number) {
  const estimate = left + up - upLeft;
  const toLeft = Math.abs(estimate - left);
  const toUp = Math.abs(estimate - up);
  const toUpLeft = Math.abs(estimate - upLeft);
  if (toLeft <= toUp && toLeft <= toUpLeft) return left;
  return toUp <= toUpLeft ? up : upLeft;
}

/** Decodes an 8-bit, non-interlaced RGBA PNG and returns its alpha channel. */
function readAlpha(path: string): AlphaImage {
  const bytes = readFileSync(resolve(root, path));
  expect([...bytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
  expect([bytes[24], bytes[25], bytes[28]]).toEqual([8, 6, 0]);
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);

  const chunks: Buffer[] = [];
  for (let offset = 8; offset < bytes.length; ) {
    const length = bytes.readUInt32BE(offset);
    if (bytes.toString("ascii", offset + 4, offset + 8) === "IDAT") {
      chunks.push(bytes.subarray(offset + 8, offset + 8 + length));
    }
    offset += length + 12;
  }
  const raw = inflateSync(Buffer.concat(chunks));

  const stride = width * 4;
  const pixels = new Uint8Array(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const value = raw[y * (stride + 1) + 1 + x]!;
      const left = x >= 4 ? pixels[y * stride + x - 4]! : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + x]! : 0;
      const upLeft = x >= 4 && y > 0 ? pixels[(y - 1) * stride + x - 4]! : 0;
      const predicted = [0, left, up, (left + up) >> 1, paeth(left, up, upLeft)][filter!]!;
      pixels[y * stride + x] = (value + predicted) & 0xff;
    }
  }

  const alpha = new Uint8Array(width * height);
  for (let i = 0; i < alpha.length; i++) alpha[i] = pixels[i * 4 + 3]!;
  return { width, height, alpha };
}

function visibleBounds({ width, height, alpha }: AlphaImage) {
  let left = width, top = height, right = -1, bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (alpha[y * width + x] === 0) continue;
      left = Math.min(left, x);
      right = Math.max(right, x);
      top = Math.min(top, y);
      bottom = Math.max(bottom, y);
    }
  }
  return { left, top, right: width - 1 - right, bottom: height - 1 - bottom };
}

/**
 * Counts semi-transparent pixels farther than `radius` from a transparent or from an opaque pixel.
 * A clean cutout fades from opaque to transparent within a few pixels; a backdrop shadow left
 * behind by background removal is a wide soft skirt and shows up here.
 */
function softSkirtPixels({ width, height, alpha }: AlphaImage, radius: number) {
  let count = 0;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const value = alpha[y * width + x]!;
      if (value <= TRANSPARENT || value >= OPAQUE) continue;
      let nearTransparent = false;
      let nearOpaque = false;
      for (let dy = -radius; dy <= radius; dy++) {
        for (let dx = -radius; dx <= radius; dx++) {
          const nx = x + dx;
          const ny = y + dy;
          const neighbour = nx < 0 || ny < 0 || nx >= width || ny >= height ? 0 : alpha[ny * width + nx]!;
          if (neighbour <= TRANSPARENT) nearTransparent = true;
          if (neighbour >= OPAQUE) nearOpaque = true;
        }
      }
      if (!nearTransparent || !nearOpaque) count++;
    }
  }
  return count;
}

describe("app icon", () => {
  const master = readAlpha("public/icon.png");

  it("keeps a square transparent master with the artwork centred", () => {
    expect(master.width).toBe(master.height);
    expect(master.width).toBeGreaterThanOrEqual(1024);
    const margins = visibleBounds(master);
    expect(Math.min(margins.left, margins.top, margins.right, margins.bottom)).toBeGreaterThan(0);
    expect(Math.abs(margins.left - margins.right)).toBeLessThanOrEqual(1);
    expect(Math.abs(margins.top - margins.bottom)).toBeLessThanOrEqual(1);
  });

  it("leaves no backdrop or drop shadow around the artwork", () => {
    expect(softSkirtPixels(master, 3)).toBe(0);
    expect(softSkirtPixels(readAlpha("src-tauri/icons/128x128@2x.png"), 2)).toBe(0);
  });

  it("shows the generated 256 px icon in the app and in the README", () => {
    const generated = readFileSync(resolve(root, "src-tauri/icons/128x128@2x.png"));
    expect(readFileSync(resolve(root, "public/app-icon.png")).equals(generated)).toBe(true);
    expect(readFileSync(resolve(root, "docs/assets/app-icon.png")).equals(generated)).toBe(true);
  });
});
