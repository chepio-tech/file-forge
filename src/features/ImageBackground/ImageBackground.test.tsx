// Core
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import ImageBackground from "./ImageBackground";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
// Utils
import { createApiMock, file } from "@/test/mockFileforgeApi";
// Consts
import { findTool } from "@/features/featureCatalog";

const mock = vi.hoisted(() => ({ current: null as ReturnType<typeof createApiMock> | null }));
vi.mock("@/services/fileforgeApi", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/services/fileforgeApi")>();
  const { createApiMock } = await import("@/test/mockFileforgeApi");
  mock.current = createApiMock();
  return { ...original, default: mock.current.api };
});
const tool = findTool("imageBackground") as ToolDefinition;
function api() { if (!mock.current) throw new Error("mock"); return mock.current; }
async function withFiles() {
  render(<ImageBackground tool={tool} active />);
  await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
  act(() => api().dropFiles({ files: [file(1, "photo.jpg", "image"), file(2, "other.png", "image")], skipped: [] }));
  await screen.findByRole("button", { name: "Pick subject" });
}
beforeEach(() => { vi.clearAllMocks(); api().api.backgroundModelStatus.mockResolvedValue({ installed: true, downloadBytes: 82537778 }); });

it("scopes intake to this tool and previews originals before inference", async () => {
  await withFiles();
  expect(api().api.setDropKinds).toHaveBeenCalledWith(["image"], "background");
  await userEvent.click(screen.getByRole("button", { name: "Add files…" }));
  expect(api().api.pickFiles).toHaveBeenCalledWith(["image"], "JPEG, PNG, WebP files", "background");
  expect(api().api.backgroundPreview).toHaveBeenCalledWith(1);
  expect(api().api.removeBackground).not.toHaveBeenCalled();
});
it("processes a batch, shows sizes without savings and saves all", async () => {
  await withFiles();
  await userEvent.click(screen.getByRole("button", { name: "Remove background from 2 files" }));
  await screen.findByRole("button", { name: "Save 2 results to folder…" });
  expect(api().api.removeBackground.mock.calls.map(([id]) => id)).toEqual([1, 2]);
  expect(screen.queryByText(/−\d+%/)).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save 2 results to folder…" }));
  expect(api().api.saveResultsToFolder).toHaveBeenCalledWith([1, 2]);
});
it("picks an off-center subject with keyboard coordinates and reuses one file", async () => {
  await withFiles();
  const x = screen.getByLabelText("Subject X (%)");
  await userEvent.clear(x); await userEvent.type(x, "25");
  await userEvent.click(screen.getByRole("button", { name: "Pick at coordinates" }));
  await screen.findByRole("button", { name: "Save 1 result to folder…" });
  expect(api().api.removeBackground).toHaveBeenCalledWith(1, { format: "png", background: null, crop: false, point: [0.25, 0.5] }, expect.any(Function));
  expect(api().api.removeBackground).toHaveBeenCalledTimes(1);
});
it("downloads only on click and disables processing until ready", async () => {
  api().api.backgroundModelStatus.mockResolvedValue({ installed: false, downloadBytes: 82537778 });
  await withFiles();
  expect(screen.getByRole("button", { name: "Remove background from 2 files" })).toBeDisabled();
  expect(api().api.downloadBackgroundModel).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByRole("button", { name: /Download model/ }));
  await screen.findByText("Ready for offline use");
  expect(api().api.downloadBackgroundModel).toHaveBeenCalledTimes(1);
});
it("keeps a failed download retryable and preserves previews", async () => {
  api().api.backgroundModelStatus.mockResolvedValue({ installed: false, downloadBytes: 82537778 });
  api().api.downloadBackgroundModel.mockRejectedValueOnce({ code: "modelDownload" });
  await withFiles();
  await userEvent.click(await screen.findByRole("button", { name: /Download model/ }));
  await screen.findByText(/The model could not be downloaded/);
  expect(screen.getByRole("button", { name: /Download model/ })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Pick subject" })).toBeDisabled();
});
it("applies output and crop settings and warns when they change", async () => {
  await withFiles();
  await userEvent.selectOptions(screen.getByLabelText("Output format"), "webp");
  await userEvent.click(screen.getByLabelText("Crop to subject"));
  await userEvent.click(screen.getByRole("button", { name: "Remove background from 2 files" }));
  await screen.findByRole("button", { name: "Save 2 results to folder…" });
  expect(api().api.removeBackground.mock.calls[0]?.[1]).toMatchObject({ format: "webp", crop: true });
  await userEvent.selectOptions(screen.getByLabelText("Background"), "solid");
  expect(screen.getByText("Output settings changed. Run again to apply them.")).toBeInTheDocument();
});

it("retains each file's selected subject across settings and batch reruns", async () => {
  await withFiles();
  await userEvent.clear(screen.getByLabelText("Subject X (%)"));
  await userEvent.type(screen.getByLabelText("Subject X (%)"), "25");
  await userEvent.click(screen.getByRole("button", { name: "Pick at coordinates" }));
  await screen.findByRole("button", { name: "Save 1 result to folder…" });
  await userEvent.click(screen.getByRole("button", { name: "Preview other.png" }));
  await waitFor(() => expect(screen.getByLabelText("Subject X (%)")).toHaveValue(50));
  await userEvent.clear(screen.getByLabelText("Subject X (%)"));
  await userEvent.type(screen.getByLabelText("Subject X (%)"), "75");
  await userEvent.click(screen.getByRole("button", { name: "Pick at coordinates" }));
  await screen.findByRole("button", { name: "Save 2 results to folder…" });
  await userEvent.click(screen.getByLabelText("Crop to subject"));
  await userEvent.click(screen.getByRole("button", { name: "Remove background from 2 files" }));
  await waitFor(() => expect(api().api.removeBackground).toHaveBeenCalledTimes(4));
  expect(api().api.removeBackground.mock.calls.slice(-2).map(([id, options]) => [id, options.point, options.crop])).toEqual([[1, [0.25, 0.5], true], [2, [0.75, 0.5], true]]);
  await userEvent.click(screen.getByRole("button", { name: "Automatic subject" }));
  await waitFor(() => expect(api().api.removeBackground).toHaveBeenCalledTimes(5));
  expect(api().api.removeBackground.mock.calls[4]?.[1].point).toBeNull();
});
it("retries a failed model check without being stuck checking", async () => {
  api().api.backgroundModelStatus.mockRejectedValueOnce({ code: "io" });
  render(<ImageBackground tool={tool} active />);
  await userEvent.click(await screen.findByRole("button", { name: "Retry model check" }));
  await screen.findByText("Ready for offline use");
  expect(api().api.backgroundModelStatus).toHaveBeenCalledTimes(2);
});
it("retries a failed original preview for the same selected file", async () => {
  api().api.backgroundPreview.mockRejectedValueOnce({ code: "io" });
  render(<ImageBackground tool={tool} active />);
  await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
  act(() => api().dropFiles({ files: [file(1, "photo.jpg", "image")], skipped: [] }));
  await userEvent.click(await screen.findByRole("button", { name: "Retry preview" }));
  await screen.findByRole("button", { name: "Pick subject" });
  expect(api().api.backgroundPreview).toHaveBeenCalledTimes(2);
});
