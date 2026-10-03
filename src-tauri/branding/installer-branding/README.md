# Installer artwork

The company logo source is `public/images/chepio-tech/main_logo.svg`. The SVG layouts in this folder embed that
vector and omit its small tagline for legibility at native installer sizes. The welcome/completion panels embed
the generated application icon from `public/app-icon.png`. File Forge is the application name;
Chepio.tech is its publisher. No artwork depends on a network URL or an installed Windows font at runtime.

| Asset | Native use | Size |
|---|---|---|
| `nsis-header.bmp` | Windows EXE page header, aligned right by `installerBranding.nsh` | 300 × 114 (2× the 150 × 57 layout) |
| `nsis-sidebar.bmp` | Windows EXE welcome and completion sidebar | 328 × 628 (2× the 164 × 314 layout) |
| `wix-banner.bmp` | Windows MSI page header | 986 × 116 (2× the 493 × 58 layout) |
| `wix-dialog.bmp` | Windows MSI welcome and completion; right side reserved for native text | 986 × 624 (2× the 493 × 312 layout) |
| `dmg-background.tiff` | macOS DMG with a company signature below the drag targets | 660 × 400 at 72 dpi + 1320 × 800 at 144 dpi |

BMP files are uncompressed 24-bit RGB and rendered directly from SVG at 2× resolution, preserving the native
layout proportions while providing detail at up to 200% Windows scaling. NSIS uses `AspectFitHeight` for header,
uninstaller header and welcome/finish images so custom DPI/font settings do not stretch the artwork out of
proportion; MSI scales its bitmap to the native control. The Windows wordmarks are 120 layout pixels wide;
sidebar signatures sit 10 pixels higher than the original artwork. Remove the tagline path from the embedded
vector rather than covering it with a rectangle, which can leave a dotted fringe after rasterization.
The DMG background is a losslessly
compressed TIFF with 1× and 2× representations of the same 660 × 400-point canvas, so Finder renders sharp artwork
on both standard and Retina displays. Render both representations directly from the vector layout; enlarging the
1× raster does not add detail. The signature is 170 points wide at `(458, 325)`, with room below it even though
Finder's 400-point window includes its title bar. The sibling SVG files preserve the layouts for deterministic
regeneration. These images are bundle artwork, not files shipped in the webview.

To regenerate with existing local librsvg and ImageMagick tools, from this folder:

```sh
for name in nsis-header nsis-sidebar wix-banner wix-dialog; do
  rsvg-convert --zoom 2 "$name.svg" -o "/tmp/fileforge-$name-2x.png"
  magick "/tmp/fileforge-$name-2x.png" -strip -alpha off -type TrueColor "BMP3:$name.bmp"
done
rsvg-convert dmg-background.svg -o /tmp/fileforge-dmg-1x.png
rsvg-convert --zoom 2 dmg-background.svg -o /tmp/fileforge-dmg-2x.png
magick /tmp/fileforge-dmg-1x.png -strip -alpha off -type TrueColor -units PixelsPerInch -density 72 -compress Zip /tmp/fileforge-dmg-1x.tiff
magick /tmp/fileforge-dmg-2x.png -strip -alpha off -type TrueColor -units PixelsPerInch -density 144 -compress Zip /tmp/fileforge-dmg-2x.tiff
magick /tmp/fileforge-dmg-1x.tiff /tmp/fileforge-dmg-2x.tiff -compress Zip dmg-background.tiff
```

If the company logo or application icon changes, update their embedded artwork in the SVG layouts first. Preserve the whitespace in the
MSI text area and the positions defined in `src-tauri/tauri.conf.json`. Run
`pnpm vitest run docs/installerBranding.test.ts` from the repository root, build on each native runner, and inspect
the installation windows. Linux installation interfaces belong to the user's package manager; AppImage runs
without a wizard.
