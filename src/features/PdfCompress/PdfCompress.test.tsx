// Core
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import PdfCompress from "./PdfCompress";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
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
