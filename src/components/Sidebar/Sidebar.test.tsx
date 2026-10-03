// Core
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import Sidebar from "./Sidebar";
// Consts
import { featureCatalog } from "@/features/featureCatalog";

describe("Sidebar", () => {
  it("groups tools by file type", () => {
    render(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={() => {}} />);

    for (const group of ["Documents", "Images", "Video", "Audio"]) {
      expect(screen.getByRole("heading", { name: group })).toBeInTheDocument();
      expect(screen.getByRole("list", { name: group })).toBeInTheDocument();
    }
  });

  it("marks the active tool as the current page", () => {
    render(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={() => {}} />);

    expect(screen.getByRole("button", { name: "Compress PDF" })).toHaveAttribute("aria-current", "page");
  });

  it("selects ready tools and keeps planned ones inert", async () => {
    const onSelect = vi.fn();
    render(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={onSelect} />);

    await userEvent.click(screen.getByRole("button", { name: "Compress PDF" }));
    expect(onSelect).toHaveBeenCalledWith("pdfCompress");

    expect(screen.queryByRole("button", { name: /Compress video/ })).not.toBeInTheDocument();
    expect(screen.getByText("Compress video").parentElement).toHaveTextContent("Soon");
  });
});
