// Core
import { act, renderHook, waitFor } from "@testing-library/react";
// Hooks
import useFileIntake from "./useFileIntake";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
import type { FolderScan } from "@/services/fileforgeApi";
// Utils
import { createApiMock, file } from "@/test/mockFileforgeApi";

const mock = vi.hoisted(() => ({ current: null as ReturnType<typeof createApiMock> | null }));

vi.mock("@/services/fileforgeApi", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/services/fileforgeApi")>();
  const { createApiMock } = await import("@/test/mockFileforgeApi");
  mock.current = createApiMock();
  return { ...original, default: mock.current.api };
});

const tool: ToolDefinition = { id: "pdfCompress", icon: "compress", accepts: ["pdf"], formats: "PDF", status: "ready" };

function api() {
  if (!mock.current) throw new Error("api mock not initialised");
  return mock.current;
}

async function shownIntake() {
  const hook = renderHook(() => useFileIntake(tool, true));
  await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
  return hook;
}

function scan(overrides: Partial<FolderScan>): FolderScan {
  return { folders: 1, added: 0, ignored: 0, truncated: false, ...overrides };
}

describe("useFileIntake folder drops", () => {
  beforeEach(() => vi.clearAllMocks());

  it("tells Rust which kinds dropped folders contribute once the tool is shown", async () => {
    const { rerender } = renderHook(({ active }) => useFileIntake(tool, active), { initialProps: { active: false } });
    expect(api().api.setDropKinds).not.toHaveBeenCalled();

    rerender({ active: true });

    await waitFor(() => expect(api().api.setDropKinds).toHaveBeenCalledWith(["pdf"]));
  });

  it("adds the files found in dropped folders and counts what was left out", async () => {
    const { result } = await shownIntake();

    act(() =>
      api().dropFiles({
        files: [file(1, "a.pdf"), file(2, "b.pdf")],
        skipped: [],
        folders: scan({ added: 2, ignored: 3 }),
      }),
    );

    expect(result.current.files.map((entry) => entry.name)).toEqual(["a.pdf", "b.pdf"]);
    expect(result.current.notice).toEqual(["3 files of other types in the dropped folder were not added."]);
  });

  it("says when the dropped folders hold no accepted files", async () => {
    const { result } = await shownIntake();

    act(() => api().dropFiles({ files: [], skipped: [], folders: scan({ folders: 2, ignored: 1 }) }));

    expect(result.current.notice).toEqual([
      "No PDF files found in the dropped folders.",
      "1 file of another type in the dropped folders was not added.",
    ]);
  });

  it("reports a stopped search instead of claiming nothing was found", async () => {
    const { result } = await shownIntake();

    act(() => api().dropFiles({ files: [], skipped: [], folders: scan({ truncated: true }) }));

    expect(result.current.notice).toEqual([
      "The dropped folder is too large or too deeply nested to add at once, so some files were not added. Drop smaller folders.",
    ]);
  });

  it("stays quiet when every file in the dropped folders was added", async () => {
    const { result } = await shownIntake();

    act(() => api().dropFiles({ files: [file(1, "a.pdf")], skipped: [], folders: scan({ added: 1 }) }));

    expect(result.current.files).toHaveLength(1);
    expect(result.current.notice).toEqual([]);
  });
});
