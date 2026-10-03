# Installer artwork

The company logo source is `public/images/chepio-tech/main_logo.svg`. The SVG layouts in this folder embed that
vector and omit its small tagline for legibility at native installer sizes. The welcome/completion panels embed
the generated application icon from `public/app-icon.png`. FileForge remains the application name;
Chepio.tech is its publisher. No artwork depends on a network URL or an installed Windows font at runtime.

| Asset | Native use | Size |
|---|---|---|
| `nsis-header.bmp` | Windows EXE page header, aligned right by `installerBranding.nsh` | 150 × 57 |
| `nsis-sidebar.bmp` | Windows EXE welcome and completion sidebar | 164 × 314 |
| `wix-banner.bmp` | Windows MSI page header | 493 × 58 |
| `wix-dialog.bmp` | Windows MSI welcome and completion; right side reserved for native text | 493 × 312 |
| `dmg-background.png` | macOS DMG with a company signature below the drag targets | 660 × 400 |

BMP files are uncompressed 24-bit RGB; NSIS and WiX require BMP rather than WebP. The DMG background is PNG because
Finder supports it. The sibling SVG files preserve the layouts for deterministic regeneration. These images are
bundle artwork, not files shipped in the webview.

To regenerate with existing local librsvg and ImageMagick tools, from this folder:

```sh
for name in nsis-header nsis-sidebar wix-banner wix-dialog; do
  rsvg-convert "$name.svg" -o "/tmp/fileforge-$name.png"
  magick "/tmp/fileforge-$name.png" -alpha off -type TrueColor "BMP3:$name.bmp"
done
rsvg-convert dmg-background.svg -o dmg-background.png
```

If the company logo or application icon changes, update their embedded artwork in the SVG layouts first. Preserve the whitespace in the
MSI text area and the positions defined in `src-tauri/tauri.conf.json`. Run
`pnpm vitest run docs/installerBranding.test.ts` from the repository root, build on each native runner, and inspect
the installation windows. Linux installation interfaces belong to the user's package manager; AppImage runs
without a wizard.
