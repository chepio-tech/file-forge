// Core
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import Sidebar from "./Sidebar";
// Consts
import { featureCatalog } from "@/features/featureCatalog";
// Utils
import renderWithI18n from "@/test/renderWithI18n";

describe("Sidebar", () => {
  it("groups tools by file type", () => {
    renderWithI18n(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={() => {}} />);

    for (const group of ["Documents", "Images", "Video", "Audio"]) {
      expect(screen.getByRole("heading", { name: group })).toBeInTheDocument();
      expect(screen.getByRole("list", { name: group })).toBeInTheDocument();
    }
  });

  it("marks the active tool as the current page", () => {
    renderWithI18n(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={() => {}} />);

    expect(screen.getByRole("button", { name: "Compress PDF" })).toHaveAttribute("aria-current", "page");
  });

  it("selects ready tools and keeps planned ones inert", async () => {
    const onSelect = vi.fn();
    renderWithI18n(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={onSelect} />);

    await userEvent.click(screen.getByRole("button", { name: "Compress PDF" }));
    expect(onSelect).toHaveBeenCalledWith("pdfCompress");

    expect(screen.queryByRole("button", { name: /Compress video/ })).not.toBeInTheDocument();
    expect(screen.getByText("Compress video").parentElement).toHaveTextContent("Soon");
  });

  it("switches the interface language", async () => {
    renderWithI18n(<Sidebar groups={featureCatalog} activeTool="pdfCompress" onSelect={() => {}} />);

    await userEvent.click(screen.getByRole("button", { name: "RU" }));

    expect(screen.getByRole("button", { name: "Сжать PDF" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "RU" })).toHaveAttribute("aria-pressed", "true");
    expect(document.documentElement.lang).toBe("ru");
  });
});
