// Core
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import ChepioTechFooter from "./ChepioTechFooter";

const openExternal = vi.hoisted(() => vi.fn(async (_url: string) => {}));

vi.mock("@/services/fileforgeApi", () => ({ default: { openExternal } }));

describe("ChepioTechFooter", () => {
  it("links to chepio.tech with the required attributes", () => {
    render(<ChepioTechFooter />);
    const link = screen.getByRole("link", { name: "Developed by Chepio" });

    expect(link).toHaveAttribute("href", "https://chepio.tech");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
    expect(screen.getByAltText("chepio.tech")).toHaveAttribute("src", "/images/chepio-tech/logo_designed.svg");
  });

  it("opens the site in the system browser instead of the webview", async () => {
    render(<ChepioTechFooter />);

    await userEvent.click(screen.getByRole("link", { name: "Developed by Chepio" }));

    expect(openExternal).toHaveBeenCalledWith("https://chepio.tech");
  });
});
