# ADR-0014: The product name is "File Forge" and stays fixed once updates ship

## Status
Accepted.

## Date
2026-10-03

## Context
The app showed "FileForge". The owner chose "File Forge" for every visible place: the UI, the window title, the
macOS menu and Dock, the Windows Start menu and the installers. Those system places read `productName` in
`src-tauri/tauri.conf.json`. The bundler also derives install locations from it: the NSIS install folder and
uninstall registry key, the macOS bundle name and the Linux resource folder. Linux package names are its kebab case,
which is `file-forge` for both spellings. Release v0.1.0 shipped as "FileForge" without the updater (ADR-0013).

## Decision
Set `productName` to "File Forge" in the first release that contains the updater, together with the UI strings
(`src/messages/messages.ts`), the window title, `index.html` and the installer artwork. Keep the bundle identifier
`tech.chepio.fileforge`, the binary name `fileforge`, the release asset names and the repository name unchanged.
After that release, `productName` does not change.

## Rationale
Before the updater ships, a rename costs only a one-time manual cleanup for the few v0.1.0 installations, which
need a manual reinstall anyway. After it ships, a rename would make Windows updates install a second copy in a new
folder with a second uninstall entry. The identifier keeps app data, cache paths and macOS permissions stable.

## Alternatives considered
- Rename only inside the window: the menu, Dock, Start menu and installers would keep showing "FileForge", and
  fixing that later would hit the Windows problem above.
- Keep "FileForge": rejected by the owner.

## Consequences
- v0.1.0 installations are not replaced by a manual install of the next release: Windows keeps a separate
  "FileForge" entry and macOS a separate `FileForge.app`; users remove the old copy by hand.
- macOS copies updated in place keep their `FileForge.app` folder name but show "File Forge" in the menu and Dock.
- Build outputs contain a space (`File Forge.app`, `File Forge_<version>_x64-setup.exe`); release collection uses
  quoted globs and stable asset names, so download links do not change.

## Validation / fitness criteria
- `pnpm vitest run docs/installerBranding.test.ts src/components/UpdateStatus` passes.
- A release build produces `File Forge.app.tar.gz` with a signature accepted by `.github/scripts/updaterManifest.mjs`.

## Reconsider when
- A rename becomes unavoidable: plan a migration for Windows installs (uninstall the old entry from the new
  installer) before changing `productName`.
