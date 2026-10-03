# src — web UI (React + TypeScript)

## Rules
- Talk to Rust only through `src/services/fileforgeApi.ts`; components never import `@tauri-apps/*` directly.
- The UI is English-only. Strings come from `src/messages/messages.ts` (`import messages from "@/messages/messages"`);
  no string literals for UI text in components.
- Colors, spacing, type and motion come from the tokens in `src/App/App.css`; no raw color values in components.
  Use the shared `.button` vocabulary (`button--primary`, `button--ghost`, `button--icon`).
- A new tool: entry in `src/features/featureCatalog.ts`, panel in `src/features/<Tool>/`, registration in
  `src/App/toolPanels.ts`. Canonical example: `src/features/PdfCompress/PdfCompress.tsx`.
- Tests mock the IPC layer with `src/test/mockFileforgeApi.ts`.
- Relative color syntax (`oklch(from …)`) is not used: WebKitGTK on Linux does not support it reliably.

## Checks
- `pnpm typecheck` · `pnpm vitest run src/<area>`
