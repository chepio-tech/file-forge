// Core
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import ImageConvert from "./ImageConvert";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
import type { ConversionReport } from "@/services/fileforgeApi";
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
const tool = findTool("imageConvert") as ToolDefinition;
function api() {
  if (!mock.current) throw new Error("mock missing");
  return mock.current;
}
async function withFiles(...names: string[]) {
  render(<ImageConvert tool={tool} active />);
  await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
  act(() => api().dropFiles({ files: names.map((name, i) => file(i + 1, name, "image")), skipped: [] }));
}

beforeEach(() => vi.clearAllMocks());

it("registers conversion files in their own scope and filters unsupported formats", async () => {
  await withFiles("photo.JPG", "logo.webp", "photo.heic");
  expect(tool.status).toBe("ready");
  expect(api().api.setDropKinds).toHaveBeenCalledWith(["image"], "conversion");
  expect(screen.getByText("photo.JPG")).toBeInTheDocument();
  expect(screen.queryByText("photo.heic")).not.toBeInTheDocument();
  expect(api().api.removeFile).toHaveBeenCalledWith(3);
  await userEvent.click(screen.getByRole("button", { name: "Add files…" }));
  expect(api().api.pickFiles).toHaveBeenCalledWith(["image"], "JPEG, PNG, WebP files", "conversion");
});

it("defaults to PNG, discloses metadata removal and saves larger results without claiming savings", async () => {
  await withFiles("photo.jpg", "logo.webp");
  expect(screen.getByRole("radio", { name: "PNG" })).toBeChecked();
  expect(screen.getByText(/Camera details, location and other metadata are removed/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Convert 2 files" }));
  await waitFor(() => expect(api().api.convertImage).toHaveBeenCalledTimes(2));
  expect(api().api.convertImage).toHaveBeenNthCalledWith(1, 1, { format: "png", jpegQuality: 90, background: [255, 255, 255] }, expect.any(Function));
  expect(await screen.findByText("2.00 MB → 4.00 MB")).toBeInTheDocument();
  expect(screen.queryByText(/%$/)).not.toBeInTheDocument();
  expect(screen.getAllByTitle("PNG 600×400 · lossless · metadata removed")).toHaveLength(2);
  await userEvent.click(screen.getByRole("button", { name: "Save 2 results to folder…" }));
  expect(api().api.saveResultsToFolder).toHaveBeenCalledWith([1, 2]);
});

it("sends JPEG quality and background, marks stale settings, and saves individual results", async () => {
  await withFiles("alpha.png");
  await userEvent.click(screen.getByRole("button", { name: "Convert 1 file" }));
  await screen.findByText("→ 2.00 MB");
  await userEvent.click(screen.getByRole("radio", { name: "JPEG" }));
  expect(screen.getByText(/Convert again to apply them/)).toBeInTheDocument();
  expect(screen.getByText(/JPEG is lossy/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("radio", { name: "Black" }));
  await userEvent.clear(screen.getByLabelText("JPEG quality"));
  await userEvent.type(screen.getByLabelText("JPEG quality"), "75{Enter}");
  await userEvent.click(screen.getByRole("button", { name: "Convert 1 file" }));
  expect(api().api.convertImage).toHaveBeenLastCalledWith(1, { format: "jpeg", jpegQuality: 75, background: [0, 0, 0] }, expect.any(Function));
  api().api.saveResult.mockResolvedValueOnce("alpha-converted.jpg");
  await userEvent.click(screen.getByRole("button", { name: "Save…" }));
  await userEvent.click(await screen.findByRole("button", { name: "Saved" }));
  expect(api().api.revealResult).toHaveBeenCalledWith(1);
});

it("cancels the batch, disables settings and does not start the next file", async () => {
  await withFiles("one.png", "two.png");
  let fail: (error: unknown) => void = () => {};
  api().api.convertImage.mockImplementationOnce(() => new Promise<ConversionReport>((_resolve, reject) => { fail = reject; }));
  await userEvent.click(screen.getByRole("button", { name: "Convert 2 files" }));
  expect(await screen.findByText("Converting…")).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "JPEG" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(api().api.cancelCompression).toHaveBeenCalledOnce();
  await act(async () => fail({ code: "cancelled" }));
  expect(await screen.findByText("Cancelled")).toBeInTheDocument();
  expect(api().api.convertImage).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("radio", { name: "JPEG" })).not.toBeDisabled();
});

it("explains rejected input and continues with remaining files", async () => {
  await withFiles("signed.png", "valid.png");
  api().api.convertImage.mockRejectedValueOnce({ code: "conversionUnsupported", detail: "private parser details" });
  await userEvent.click(screen.getByRole("radio", { name: "WebP" }));
  await userEvent.click(screen.getByRole("button", { name: "Convert 2 files" }));
  expect(await screen.findByText(/This image cannot be converted/)).toBeInTheDocument();
  expect(await screen.findByTitle("WebP 600×400 · lossless · metadata removed")).toBeInTheDocument();
  expect(screen.queryByText(/private parser/)).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Save 1 result to folder…" }));
  expect(api().api.saveResultsToFolder).toHaveBeenCalledWith([2]);
});
