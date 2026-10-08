// Core
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import ImageCompress from "./ImageCompress";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
// Utils
import { createApiMock, file, imageReport } from "@/test/mockFileforgeApi";
// Consts
import { findTool } from "@/features/featureCatalog";

const mock = vi.hoisted(() => ({ current: null as ReturnType<typeof createApiMock> | null }));

vi.mock("@/services/fileforgeApi", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/services/fileforgeApi")>();
  const { createApiMock } = await import("@/test/mockFileforgeApi");
  mock.current = createApiMock();
  return { ...original, default: mock.current.api };
});

const tool = findTool("imageCompress") as ToolDefinition;

function api() {
  if (!mock.current) throw new Error("api mock not initialised");
  return mock.current;
}

async function withFiles(...files: ReturnType<typeof file>[]) {
  render(<ImageCompress tool={tool} active />);
  await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
  act(() => api().dropFiles({ files, skipped: [] }));
}

const photo = (id: number, name = `IMG_${id}.JPG`) => file(id, name, "image");

describe("ImageCompress intake", () => {
  beforeEach(() => vi.clearAllMocks());

  it("is ready and asks for JPEG and PNG files", async () => {
    expect(tool.status).toBe("ready");
    render(<ImageCompress tool={tool} active />);

    expect(screen.getByText("Drop JPEG, PNG files here")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Choose files…" }));

    expect(api().api.pickFiles).toHaveBeenCalledWith(["image"], "JPEG, PNG files");
  });

  it("takes JPEG and PNG files by extension and rejects other images and kinds", async () => {
    await withFiles(photo(1), file(2, "logo.png", "image"), file(3, "IMG_3.HEIC", "image"), file(4, "a.pdf"));

    expect(screen.getByText("IMG_1.JPG")).toBeInTheDocument();
    expect(screen.getByText("logo.png")).toBeInTheDocument();
    expect(screen.queryByText("IMG_3.HEIC")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Only JPEG, PNG files are accepted here. Skipped: IMG_3.HEIC, a.pdf");
    expect(api().api.removeFile).toHaveBeenCalledWith(3);
    expect(api().api.removeFile).toHaveBeenCalledWith(4);
  });
});

describe("ImageCompress compression", () => {
  beforeEach(() => vi.clearAllMocks());

  it("starts lossless and says that pixels stay identical", async () => {
    await withFiles(photo(1));

    expect(screen.getByRole("radio", { name: "Lossless" })).toBeChecked();
    expect(screen.getByLabelText("JPEG quality")).toHaveValue(null);
    expect(screen.getByText(/JPEG pixels stay bit-identical/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    expect(api().api.compressImage).toHaveBeenCalledWith(
      1,
      { jpegQuality: null, pngLevel: 2, pngZopfli: false, stripMetadata: false },
      expect.any(Function),
    );
    expect(await screen.findByText("→ 700 kB")).toBeInTheDocument();
    expect(screen.getByTitle("JPEG 4032×3024 · pixels unchanged")).toBeInTheDocument();
  });

  it("sends the exact numbers of each preset and explains them", async () => {
    await withFiles(photo(1));

    await userEvent.click(screen.getByRole("radio", { name: "Balanced" }));
    expect(screen.getByText(/re-encoded at quality 85 when that saves at least 2%/)).toBeInTheDocument();
    expect(screen.getByText(/effort 4 of 6\./)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: "Maximum" }));
    expect(screen.getByText(/effort 6 of 6, with Zopfli for small images/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    expect(api().api.compressImage).toHaveBeenCalledWith(
      1,
      { jpegQuality: 75, pngLevel: 6, pngZopfli: true, stripMetadata: false },
      expect.any(Function),
    );
  });

  it("switches to Custom when a number is edited, allows keeping JPEG pixels, and flags old results", async () => {
    await withFiles(photo(1));
    await userEvent.click(screen.getByRole("radio", { name: "Balanced" }));
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    await screen.findByText("→ 700 kB");

    const quality = screen.getByLabelText("JPEG quality");
    await userEvent.clear(quality);
    await userEvent.type(quality, "{Enter}");

    expect(screen.getByRole("radio", { name: "Custom" })).toBeChecked();
    expect(screen.getByText(/JPEG pixels stay bit-identical/)).toBeInTheDocument();
    expect(screen.getByText("Settings changed since the last run. Compress again to apply them.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    expect(api().api.compressImage).toHaveBeenLastCalledWith(
      1,
      { jpegQuality: null, pngLevel: 4, pngZopfli: false, stripMetadata: false },
      expect.any(Function),
    );
  });

  it("keeps metadata removal across presets and sends it", async () => {
    await withFiles(photo(1));

    await userEvent.click(screen.getByRole("checkbox", { name: /Remove metadata/ }));
    await userEvent.click(screen.getByRole("radio", { name: "Maximum" }));
    expect(screen.getByRole("checkbox", { name: /Remove metadata/ })).toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    expect(api().api.compressImage).toHaveBeenCalledWith(
      1,
      expect.objectContaining({ jpegQuality: 75, stripMetadata: true }),
      expect.any(Function),
    );
  });

  it("says why an original was kept and leaves it out of bulk saving", async () => {
    await withFiles(photo(1), photo(2), photo(3), file(4, "anim.png", "image"));
    const kept = { outputSize: 1_000_000 };
    api().api.compressImage
      .mockResolvedValueOnce(imageReport(1, { kept: "signed", ...kept }))
      .mockResolvedValueOnce(imageReport(2, { kept: "notSmaller", ...kept }))
      .mockResolvedValueOnce(imageReport(3, { reencoded: true, metadataRemoved: true }))
      .mockResolvedValueOnce(imageReport(4, { kept: "animated", format: "png", ...kept }));

    await userEvent.click(screen.getByRole("button", { name: "Compress 4 files" }));

    expect(await screen.findByText("Kept as is: signed with Content Credentials")).toBeInTheDocument();
    expect(screen.getByText("Already optimal")).toBeInTheDocument();
    expect(screen.getByText("Kept as is: animated PNG")).toBeInTheDocument();
    expect(screen.getByTitle("JPEG 4032×3024 · re-encoded · metadata removed")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Save 1 result to folder…" }));
    expect(api().api.saveResultsToFolder).toHaveBeenCalledWith([3]);
  });

  it("says when a kept original removed no metadata", async () => {
    await withFiles(photo(1));
    api().api.compressImage.mockResolvedValueOnce(imageReport(1, { kept: "notSmaller", outputSize: 1_000_000 }));

    await userEvent.click(screen.getByRole("checkbox", { name: /Remove metadata/ }));
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    expect(await screen.findByText("Already optimal: original kept, nothing removed")).toBeInTheDocument();
  });

  it("shows the optimizing stage and explains unsupported or damaged files", async () => {
    await withFiles(photo(1), file(2, "fake.png", "image"), file(3, "broken.jpg", "image"));
    let finish = () => {};
    api().api.compressImage
      .mockImplementationOnce(
        (id, _options, onProgress) =>
          new Promise((resolve) => {
            onProgress?.({ stage: "encoding", done: 0, total: 0 });
            finish = () => resolve(imageReport(id));
          }),
      )
      .mockRejectedValueOnce({ code: "imageUnsupported", detail: "WebP" })
      .mockRejectedValueOnce({ code: "imageMalformed", detail: "truncated" });

    await userEvent.click(screen.getByRole("button", { name: "Compress 3 files" }));
    expect(await screen.findByText("Optimizing…")).toBeInTheDocument();
    await act(async () => finish());

    expect(await screen.findByText(/Only JPEG and PNG images can be compressed/)).toBeInTheDocument();
    expect(await screen.findByText(/damaged or not a valid JPEG or PNG image/)).toBeInTheDocument();
    expect(screen.queryByText(/WebP|truncated/)).not.toBeInTheDocument();
  });
});
