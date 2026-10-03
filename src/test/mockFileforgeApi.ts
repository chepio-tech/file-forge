// Core
import { vi } from "vitest";
// Types
import type { FileInfo, PdfOptions, PdfReport, RegisterOutcome, SavedFile } from "@/services/fileforgeApi";

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
    compressPdf: vi.fn<(id: number, options: PdfOptions) => Promise<PdfReport>>(async (id) => report(id)),
    saveResult: vi.fn<(id: number) => Promise<string | null>>(async () => null),
    saveResultsToFolder: vi.fn<(ids: number[]) => Promise<SavedFile[] | null>>(async () => null),
    revealResult: vi.fn(async (_id: number) => {}),
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

/** A result that saved 60% of a 1 MB file. */
export function report(_id: number, overrides: Partial<PdfReport> = {}): PdfReport {
  return {
    originalSize: 1_000_000,
    outputSize: 400_000,
    keptOriginal: false,
    pages: 3,
    imagesRecompressed: 2,
    imagesDownsampled: 1,
    duplicatesMerged: 0,
    unusedObjectsRemoved: 4,
    ...overrides,
  };
}
