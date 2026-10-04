// Core
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import PdfCompress from "./PdfCompress";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
// Utils
import { createApiMock, file, report } from "@/test/mockFileforgeApi";

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

describe("PdfCompress intake", () => {
  beforeEach(() => vi.clearAllMocks());

  it("starts with an empty state that offers the dialog", async () => {
    render(<PdfCompress tool={tool} active />);

    expect(screen.getByText("Drop PDF files here")).toBeInTheDocument();
    api().api.pickFiles.mockResolvedValueOnce({ files: [file(1, "report.pdf")], skipped: [] });

    await userEvent.click(screen.getByRole("button", { name: "Choose files…" }));

    expect(api().api.pickFiles).toHaveBeenCalledWith(["pdf"], "PDF files");
    expect(await screen.findByText("report.pdf")).toBeInTheDocument();
    expect(screen.getByText("1 file")).toBeInTheDocument();
    expect(screen.getByText("1.00 MB total")).toBeInTheDocument();
  });

  it("accepts dropped PDFs, de-duplicates them and rejects other kinds", async () => {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().isListeningForDrops()).toBe(true));

    act(() => api().dropFiles({ files: [file(1, "a.pdf"), file(2, "photo.jpg", "image")], skipped: ["Scans"] }));
    act(() => api().dropFiles({ files: [file(1, "a.pdf")], skipped: [] }));

    expect(screen.getAllByText("a.pdf")).toHaveLength(1);
    expect(screen.queryByText("photo.jpg")).not.toBeInTheDocument();
    expect(api().api.removeFile).toHaveBeenCalledWith(2);
  });

  it("explains skipped and rejected files in a dismissible notice", async () => {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().isListeningForDrops()).toBe(true));

    act(() => api().dropFiles({ files: [file(2, "photo.jpg", "image")], skipped: ["Scans"] }));

    const notice = screen.getByRole("status");
    expect(notice).toHaveTextContent("Only PDF files are accepted here. Skipped: photo.jpg");
    expect(notice).toHaveTextContent("Skipped (not a file): Scans");

    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });


  it("removes files from the list and from the registry", async () => {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
    act(() => api().dropFiles({ files: [file(1, "a.pdf"), file(3, "b.pdf")], skipped: [] }));

    await userEvent.click(screen.getByRole("button", { name: "Remove a.pdf" }));

    expect(screen.queryByText("a.pdf")).not.toBeInTheDocument();
    expect(api().api.removeFile).toHaveBeenCalledWith(1);

    await userEvent.click(screen.getByRole("button", { name: "Clear list" }));
    expect(api().api.removeFile).toHaveBeenCalledWith(3);
    expect(screen.getByText("Drop PDF files here")).toBeInTheDocument();
  });

  it("ignores drops while hidden", async () => {
    render(<PdfCompress tool={tool} active={false} />);

    expect(api().isListeningForDrops()).toBe(false);
    expect(api().api.onFilesAdded).not.toHaveBeenCalled();
  });

  it("shows the drop overlay while files hover over the window", async () => {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().api.onDragHover).toHaveBeenCalled());
    const overlay = screen.getByText("Release to add files").closest(".drop-overlay");

    act(() => api().dragHover(true));
    expect(overlay).toHaveClass("drop-overlay--visible");

    act(() => api().dragHover(false));
    expect(overlay).not.toHaveClass("drop-overlay--visible");
  });

  it("explains a failed dialog in plain words, without the technical detail", async () => {
    render(<PdfCompress tool={tool} active />);
    api().api.pickFiles.mockRejectedValueOnce({ code: "io", detail: "permission denied" });

    await userEvent.click(screen.getByRole("button", { name: "Choose files…" }));

    const notice = await screen.findByRole("status");
    expect(notice).toHaveTextContent("The file could not be read or written.");
    expect(notice).not.toHaveTextContent("permission denied");
  });
});

describe("PdfCompress compression", () => {
  beforeEach(() => vi.clearAllMocks());

  async function withFiles(...files: ReturnType<typeof file>[]) {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
    act(() => api().dropFiles({ files, skipped: [] }));
  }

  it("starts lossless, with the image settings disabled", async () => {
    await withFiles(file(1, "a.pdf"));

    expect(screen.getByRole("radio", { name: "Lossless" })).toBeChecked();
    expect(screen.getByLabelText("JPEG quality")).toBeDisabled();
    expect(screen.getByText(/stay bit-identical/)).toBeInTheDocument();
  });

  it("compresses files one after another with the chosen preset and shows the savings", async () => {
    await withFiles(file(1, "a.pdf"), file(2, "b.pdf"));
    let release: () => void = () => {};
    api().api.compressPdf.mockImplementationOnce(
      (id) => new Promise((resolve) => (release = () => resolve(report(id)))),
    );

    await userEvent.click(screen.getByRole("radio", { name: "Balanced" }));
    await userEvent.click(screen.getByRole("button", { name: "Compress 2 files" }));

    expect(screen.getByRole("button", { name: "Compressing 1 of 2…" })).toBeDisabled();
    expect(screen.getByText("Waiting")).toBeInTheDocument();
    expect(api().api.compressPdf).toHaveBeenCalledTimes(1);
    expect(api().api.compressPdf).toHaveBeenCalledWith(
      1,
      { images: { jpegQuality: 85, maxDpi: 200 }, stripMetadata: false, stripEditingData: false },
      expect.any(Function),
    );

    await act(async () => release());

    await waitFor(() => expect(api().api.compressPdf).toHaveBeenCalledTimes(2));
    await screen.findByRole("button", { name: "Compress 2 files" });
    expect(screen.getAllByText("→ 400 kB")).toHaveLength(2);
    expect(screen.getByText("2.00 MB → 800 kB")).toBeInTheDocument();
    expect(screen.getAllByText("−60%")).toHaveLength(3);
  });

  it("explains a failed file and still compresses the rest", async () => {
    await withFiles(file(1, "locked.pdf"), file(2, "ok.pdf"));
    api().api.compressPdf.mockRejectedValueOnce({ code: "pdfEncrypted" });

    await userEvent.click(screen.getByRole("button", { name: "Compress 2 files" }));

    expect(await screen.findByText(/Password-protected PDF/)).toBeInTheDocument();
    expect(await screen.findByText("→ 400 kB")).toBeInTheDocument();
  });

  it("reports files that could not be made smaller and leaves them out of bulk saving", async () => {
    await withFiles(file(1, "tight.pdf"), file(2, "big.pdf"));
    api().api.compressPdf.mockResolvedValueOnce(report(1, { keptOriginal: true, outputSize: 1_000_000 }));

    await userEvent.click(screen.getByRole("button", { name: "Compress 2 files" }));
    await screen.findByText("Already optimal");
    await userEvent.click(screen.getByRole("button", { name: "Save 1 result to folder…" }));

    expect(api().api.saveResultsToFolder).toHaveBeenCalledWith([2]);
  });

  it("saves a result and then shows where it went", async () => {
    await withFiles(file(1, "a.pdf"));
    api().api.saveResult.mockResolvedValueOnce("a-compressed.pdf");
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    await userEvent.click(await screen.findByRole("button", { name: "Save…" }));
    const saved = await screen.findByRole("button", { name: "Saved" });
    expect(saved).toHaveAttribute("title", expect.stringContaining("a-compressed.pdf"));

    await userEvent.click(saved);
    expect(api().api.revealResult).toHaveBeenCalledWith(1);
  });

  it("keeps results when saving fails and says why", async () => {
    await withFiles(file(1, "a.pdf"));
    api().api.saveResult.mockRejectedValueOnce({ code: "io", detail: "disk full" });
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    await userEvent.click(await screen.findByRole("button", { name: "Save…" }));

    expect(await screen.findByRole("status")).toHaveTextContent("The file could not be read or written.");
    expect(screen.getByRole("button", { name: "Save…" })).toBeEnabled();
  });

  it("locks result-changing actions during a save and unlocks them after cancellation", async () => {
    await withFiles(file(1, "a.pdf"));
    let cancel: () => void = () => {};
    api().api.saveResult.mockImplementationOnce(() => new Promise((resolve) => (cancel = () => resolve(null))));
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    await userEvent.click(await screen.findByRole("button", { name: "Save…" }));

    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Compress 1 file" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Clear list" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove a.pdf" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "Balanced" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Saving…" }));
    expect(api().api.saveResult).toHaveBeenCalledTimes(1);

    await act(async () => cancel());

    expect(screen.getByRole("button", { name: "Save…" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Compress 1 file" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Saved" })).not.toBeInTheDocument();
  });

  it("explains why an original cannot be used as the save destination and allows another attempt", async () => {
    await withFiles(file(1, "a.pdf"));
    api().api.saveResult.mockRejectedValueOnce({ code: "originalTarget" });
    api().api.saveResult.mockResolvedValueOnce("safe-copy.pdf");
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    await userEvent.click(await screen.findByRole("button", { name: "Save…" }));

    expect(await screen.findByRole("status")).toHaveTextContent("Original files cannot be overwritten.");
    await userEvent.click(screen.getByRole("button", { name: "Save…" }));

    expect(await screen.findByRole("button", { name: "Saved" })).toHaveAttribute(
      "title",
      expect.stringContaining("safe-copy.pdf"),
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("shows a reveal error while preserving the saved result", async () => {
    await withFiles(file(1, "a.pdf"));
    api().api.saveResult.mockResolvedValueOnce("a-compressed.pdf");
    api().api.revealResult.mockRejectedValueOnce({ code: "io", detail: "file manager failed" });
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    await userEvent.click(await screen.findByRole("button", { name: "Save…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Saved" }));

    expect(await screen.findByRole("status")).toHaveTextContent("The file could not be read or written.");
    expect(screen.getByRole("button", { name: "Saved" })).toBeEnabled();
  });

  it("marks every successfully saved batch result and releases the busy state", async () => {
    await withFiles(file(1, "a.pdf"), file(2, "b.pdf"));
    let finish: () => void = () => {};
    api().api.saveResultsToFolder.mockImplementationOnce(
      () => new Promise((resolve) => (finish = () => resolve([
          { id: 1, name: "a-compressed.pdf" },
          { id: 2, name: "b-compressed.pdf" },
        ]))),
    );
    await userEvent.click(screen.getByRole("button", { name: "Compress 2 files" }));
    await userEvent.click(await screen.findByRole("button", { name: "Save 2 results to folder…" }));

    expect(screen.getByRole("button", { name: "Compress 2 files" })).toBeDisabled();
    expect(screen.getAllByRole("button", { name: "Saving…" })).toHaveLength(3);
    await act(async () => finish());

    expect(screen.getAllByRole("button", { name: "Saved" })).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Compress 2 files" })).toBeEnabled();
  });

  it("offers a Screen preset for the smallest files and says what it costs", async () => {
    await withFiles(file(1, "a.pdf"));

    await userEvent.click(screen.getByRole("radio", { name: "Screen" }));
    expect(screen.getByLabelText("JPEG quality")).toHaveValue(65);
    expect(screen.getByLabelText("Max DPI")).toHaveValue(100);
    expect(screen.getByText(/JPEG 2000 images saved as JPEG at quality 65/)).toBeInTheDocument();
    expect(screen.getByText(/For reading on screen/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    expect(api().api.compressPdf).toHaveBeenLastCalledWith(
      1,
      { images: { jpegQuality: 65, maxDpi: 100 }, stripMetadata: false, stripEditingData: false },
      expect.any(Function),
    );

    await userEvent.click(screen.getByRole("radio", { name: "Maximum" }));
    expect(screen.queryByText(/For reading on screen/)).not.toBeInTheDocument();
  });

  it("explains Maximum photo conversion, sends its option and removes it on other presets", async () => {
    await withFiles(file(1, "a.pdf"));
    await userEvent.click(screen.getByRole("radio", { name: "Maximum" }));
    expect(screen.getByText(/Losslessly stored photos may also become JPEG/)).toBeInTheDocument();
    expect(screen.getByText(/at least 20% savings/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    expect(api().api.compressPdf).toHaveBeenLastCalledWith(
      1,
      { images: { jpegQuality: 70, maxDpi: 150, compressFlatePhotos: true }, stripMetadata: false, stripEditingData: false },
      expect.any(Function),
    );
    await screen.findByText("→ 400 kB");
    for (const preset of ["Balanced", "Screen", "Lossless"]) {
      await userEvent.click(screen.getByRole("radio", { name: preset }));
      expect(screen.queryByText(/Losslessly stored photos may also become JPEG/)).not.toBeInTheDocument();
    }
  });

  it("switches to Custom when a number is edited and flags results as outdated", async () => {
    await withFiles(file(1, "a.pdf"));
    await userEvent.click(screen.getByRole("radio", { name: "Maximum" }));
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    await screen.findByText("→ 400 kB");

    const quality = screen.getByLabelText("JPEG quality");
    await userEvent.clear(quality);
    await userEvent.type(quality, "200{Enter}");

    expect(screen.getByRole("radio", { name: "Custom" })).toBeChecked();
    expect(quality).toHaveValue(95);
    expect(screen.getByText(/Losslessly stored photos may also become JPEG/)).toBeInTheDocument();
    expect(screen.getByText(/Settings changed since the last run/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    expect(api().api.compressPdf).toHaveBeenLastCalledWith(
      1,
      { images: { jpegQuality: 95, maxDpi: 150, compressFlatePhotos: true }, stripMetadata: false, stripEditingData: false },
      expect.any(Function),
    );
  });

  it("lets an empty Max DPI keep image resolution", async () => {
    await withFiles(file(1, "a.pdf"));
    await userEvent.click(screen.getByRole("radio", { name: "Balanced" }));

    await userEvent.clear(screen.getByLabelText("Max DPI"));
    await userEvent.tab();
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    expect(api().api.compressPdf).toHaveBeenCalledWith(
      1,
      { images: { jpegQuality: 85, maxDpi: null }, stripMetadata: false, stripEditingData: false },
      expect.any(Function),
    );
    expect(screen.getByText(/Image resolution is kept/)).toBeInTheDocument();
  });
});

describe("PdfCompress progress and cancellation", () => {
  beforeEach(() => vi.clearAllMocks());

  type ProgressHandler = Parameters<ReturnType<typeof createApiMock>["api"]["compressPdf"]>[2];

  async function withFiles(...files: ReturnType<typeof file>[]) {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
    act(() => api().dropFiles({ files, skipped: [] }));
  }

  /** The next `compressPdf` call stays pending until the test settles it. */
  function pendingCompression() {
    const call = {
      progress: (() => {}) as NonNullable<ProgressHandler>,
      finish: () => {},
      fail: (_error: unknown) => {},
    };
    api().api.compressPdf.mockImplementationOnce(
      (id, _options, onProgress) =>
        new Promise((resolve, reject) => {
          call.progress = onProgress ?? (() => {});
          call.finish = () => resolve(report(id));
          call.fail = reject;
        }),
    );
    return call;
  }

  it("shows the stage of the file being compressed, with counts where known", async () => {
    await withFiles(file(1, "scan.pdf"));
    const call = pendingCompression();
    expect(screen.queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    expect(screen.getByText("Compressing…")).toBeInTheDocument();

    act(() => call.progress({ stage: "images", done: 3, total: 12 }));
    expect(screen.getByText("Images 3 of 12")).toBeInTheDocument();
    act(() => call.progress({ stage: "saving", done: 0, total: 0 }));
    expect(screen.getByText("Writing…")).toBeInTheDocument();

    await act(async () => call.finish());
    act(() => call.progress({ stage: "verifying", done: 0, total: 0 }));

    expect(screen.getByText("→ 400 kB")).toBeInTheDocument();
    expect(screen.queryByText("Verifying…")).not.toBeInTheDocument();
  });

  it("cancels the run: keeps finished results, marks the current file and leaves later files untouched", async () => {
    await withFiles(file(1, "a.pdf"), file(2, "b.pdf"), file(3, "c.pdf"));
    api().api.compressPdf.mockResolvedValueOnce(report(1));
    const second = pendingCompression();
    let stopped: () => void = () => {};
    api().api.cancelCompression.mockImplementationOnce(async () => {
      stopped = () => second.fail({ code: "cancelled" });
    });

    await userEvent.click(screen.getByRole("button", { name: "Compress 3 files" }));
    await screen.findByRole("button", { name: "Compressing 2 of 3…" });
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(api().api.cancelCompression).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
    await act(async () => stopped());

    expect(await screen.findByText("Cancelled")).toBeInTheDocument();
    expect(screen.getByText("→ 400 kB")).toBeInTheDocument();
    expect(screen.queryByText("Waiting")).not.toBeInTheDocument();
    expect(api().api.compressPdf).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Cancel/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Compress 3 files" })).toBeEnabled();
  });

  it("stops after the current file even when the cancel arrives too late or fails", async () => {
    await withFiles(file(1, "a.pdf"), file(2, "b.pdf"));
    const first = pendingCompression();
    api().api.cancelCompression.mockRejectedValueOnce({ code: "io" });

    await userEvent.click(screen.getByRole("button", { name: "Compress 2 files" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => first.finish());

    expect(await screen.findByText("→ 400 kB")).toBeInTheDocument();
    expect(api().api.compressPdf).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Waiting")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("The file could not be read or written.");
    expect(screen.getByRole("button", { name: "Compress 2 files" })).toBeEnabled();
  });
});

describe("PdfCompress metadata and editing data", () => {
  beforeEach(() => vi.clearAllMocks());

  async function withFiles(...files: ReturnType<typeof file>[]) {
    render(<PdfCompress tool={tool} active />);
    await waitFor(() => expect(api().isListeningForDrops()).toBe(true));
    act(() => api().dropFiles({ files, skipped: [] }));
  }

  it("keeps removal off by default, sends it with any preset and keeps it across preset changes", async () => {
    await withFiles(file(1, "poster.pdf"));
    const metadata = screen.getByRole("checkbox", { name: "Remove metadata and thumbnails" });
    const editing = screen.getByRole("checkbox", { name: "Remove editing data" });
    expect(metadata).not.toBeChecked();
    expect(editing).not.toBeChecked();
    expect(editing).toHaveAccessibleDescription(/can no longer edit the file natively/);

    await userEvent.click(metadata);
    await userEvent.click(screen.getByRole("radio", { name: "Maximum" }));
    await userEvent.click(screen.getByRole("radio", { name: "Lossless" }));
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));

    expect(screen.getByRole("radio", { name: "Lossless" })).toBeChecked();
    expect(api().api.compressPdf).toHaveBeenCalledWith(
      1,
      { images: null, stripMetadata: true, stripEditingData: false },
      expect.any(Function),
    );
  });

  it("flags results as outdated when a removal option changes", async () => {
    await withFiles(file(1, "poster.pdf"));
    await userEvent.click(screen.getByRole("button", { name: "Compress 1 file" }));
    await screen.findByText("→ 400 kB");

    await userEvent.click(screen.getByRole("checkbox", { name: "Remove editing data" }));

    expect(screen.getByText(/Settings changed since the last run/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Lossless" })).toBeChecked();
  });

  it("lists what was removed or kept, and says when a kept original removed nothing", async () => {
    await withFiles(file(1, "poster.pdf"), file(2, "tight.pdf"));
    api().api.compressPdf.mockResolvedValueOnce(
      report(1, { metadataRemoved: true, metadataKeptForStandard: true, thumbnailsRemoved: 2, editingDataRemoved: 3 }),
    );
    api().api.compressPdf.mockResolvedValueOnce(report(2, { keptOriginal: true, outputSize: 1_000_000 }));

    await userEvent.click(screen.getByRole("checkbox", { name: "Remove metadata and thumbnails" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "Remove editing data" }));
    await userEvent.click(screen.getByRole("button", { name: "Compress 2 files" }));

    expect(await screen.findByText("Already optimal: original kept, nothing removed")).toBeInTheDocument();
    const details = screen.getByText("→ 400 kB").getAttribute("title");
    expect(details).toContain("metadata removed");
    expect(details).toContain("document metadata kept for PDF/A, PDF/UA or PDF/X");
    expect(details).toContain("2 thumbnails removed");
    expect(details).toContain("editing data removed");
  });
});
