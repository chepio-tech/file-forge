// Core
import { vi } from "vitest";
// Types
import type { FileInfo, RegisterOutcome } from "@/services/fileforgeApi";

/**
 * In-memory stand-in for `@/services/fileforgeApi`. Tests call `dropFiles` / `dragHover` to simulate what the Rust
 * shell would emit.
 */
export function createApiMock() {
  let filesAdded: ((outcome: RegisterOutcome) => void) | null = null;
  let hover: ((hovering: boolean) => void) | null = null;

  const api = {
    pickFiles: vi.fn<(kinds: string[], filterName: string) => Promise<RegisterOutcome>>(async () => ({
      files: [],
      skipped: [],
    })),
    removeFile: vi.fn(async (_id: number) => {}),
    onFilesAdded: vi.fn(async (handler: (outcome: RegisterOutcome) => void) => {
      filesAdded = handler;
      return () => {
        if (filesAdded === handler) filesAdded = null;
      };
    }),
    onDragHover: vi.fn(async (handler: (hovering: boolean) => void) => {
      hover = handler;
      return () => {
        if (hover === handler) hover = null;
      };
    }),
    openExternal: vi.fn(async (_url: string) => {}),
  };

  return {
    api,
    dropFiles: (outcome: RegisterOutcome) => filesAdded?.(outcome),
    dragHover: (hovering: boolean) => hover?.(hovering),
    isListeningForDrops: () => filesAdded !== null,
  };
}

export function file(id: number, name: string, kind: FileInfo["kind"] = "pdf", size = 1_000_000): FileInfo {
  return { id, name, kind, size };
}
