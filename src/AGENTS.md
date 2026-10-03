# src — web UI (React + TypeScript)

## Rules
- Talk to Rust only through `src/services/fileforgeApi.ts`; components never import `@tauri-apps/*` directly.
- Strings come from `useI18n()`; add every key to `src/i18n/en.ts` and `src/i18n/ru.ts` (the compiler and
  `src/i18n/i18n.test.ts` enforce parity). Store notices as data and translate at render time.
- Colors, spacing, type and motion come from the tokens in `src/App/App.css`; no raw color values in components.
  Use the shared `.button` vocabulary (`button--primary`, `button--ghost`, `button--icon`).
- A new tool: entry in `src/features/featureCatalog.ts`, panel in `src/features/<Tool>/`, registration in
  `src/App/toolPanels.ts`. Canonical example: `src/features/PdfCompress/PdfCompress.tsx`.
- Tests mock the IPC layer with `src/test/mockFileforgeApi.ts` and render through `src/test/renderWithI18n.tsx`.
- Relative color syntax (`oklch(from …)`) is not used: WebKitGTK on Linux does not support it reliably.

## Checks
- `pnpm typecheck` · `pnpm vitest run src/<area>`
