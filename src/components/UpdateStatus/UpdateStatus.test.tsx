// Core
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Components
import UpdateStatus from "./UpdateStatus";
// Utils
import { createApiMock } from "@/test/mockFileforgeApi";

const mock = vi.hoisted(() => ({ current: null as ReturnType<typeof createApiMock> | null }));

vi.mock("@/services/fileforgeApi", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/services/fileforgeApi")>();
  const { createApiMock } = await import("@/test/mockFileforgeApi");
  mock.current = createApiMock();
  return { ...original, default: mock.current.api };
});

function api() {
  if (!mock.current) throw new Error("api mock not initialised");
  return mock.current.api;
}

const current = { currentVersion: "0.1.0", availableVersion: null };
const newer = { currentVersion: "0.1.0", availableVersion: "0.2.0" };

describe("UpdateStatus", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api().checkForUpdate.mockResolvedValue(current);
  });

  it("checks once at startup and stays quiet when the app is current", async () => {
    render(<UpdateStatus />);

    expect(await screen.findByRole("button", { name: "Check for updates" })).toBeInTheDocument();
    expect(api().checkForUpdate).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/up to date/)).not.toBeInTheDocument();
  });

  it("hides a failed startup check, so an offline start shows no error", async () => {
    api().checkForUpdate.mockRejectedValueOnce({ code: "update", detail: "offline" });
    render(<UpdateStatus />);

    expect(await screen.findByRole("button", { name: "Check for updates" })).toBeInTheDocument();
    expect(screen.queryByText(/Could not check/)).not.toBeInTheDocument();
  });

  it("reports the result of a check the user asked for", async () => {
    render(<UpdateStatus />);

    await userEvent.click(await screen.findByRole("button", { name: "Check for updates" }));

    expect(await screen.findByText("FileForge 0.1.0 is up to date.")).toBeInTheDocument();
    expect(api().checkForUpdate).toHaveBeenCalledTimes(2);
  });

  it("reports a failed manual check and offers to retry it", async () => {
    render(<UpdateStatus />);
    api().checkForUpdate.mockRejectedValueOnce({ code: "update", detail: "offline" });

    await userEvent.click(await screen.findByRole("button", { name: "Check for updates" }));

    expect(
      await screen.findByText("Could not check for updates. Check your internet connection."),
    ).toBeInTheDocument();
    api().checkForUpdate.mockResolvedValueOnce(newer);
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("FileForge 0.2.0 is available.")).toBeInTheDocument();
  });

  it("installs a found update only when the user asks", async () => {
    api().checkForUpdate.mockResolvedValue(newer);
    render(<UpdateStatus />);

    const restart = await screen.findByRole("button", { name: "Restart to update" });
    expect(screen.getByText("FileForge 0.2.0 is available.")).toBeInTheDocument();
    expect(api().installUpdate).not.toHaveBeenCalled();

    await userEvent.click(restart);

    expect(api().installUpdate).toHaveBeenCalledWith(false);
    expect(screen.getByText("Downloading the update…")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("asks before discarding unsaved results and can step back", async () => {
    api().checkForUpdate.mockResolvedValue(newer);
    api().installUpdate.mockRejectedValueOnce({ code: "unsavedResults" });
    render(<UpdateStatus />);

    await userEvent.click(await screen.findByRole("button", { name: "Restart to update" }));

    expect(
      await screen.findByText("Compressed files you have not saved will be lost when FileForge restarts."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Restart to update" })).toBeInTheDocument();

    api().installUpdate.mockRejectedValueOnce({ code: "unsavedResults" });
    await userEvent.click(screen.getByRole("button", { name: "Restart to update" }));
    await userEvent.click(await screen.findByRole("button", { name: "Update anyway" }));

    expect(api().installUpdate).toHaveBeenLastCalledWith(true);
    expect(screen.getByText("Downloading the update…")).toBeInTheDocument();
  });

  it.each([
    ["busy", "Wait until the current compression or save finishes, then try again."],
    ["update", "The update could not be downloaded or installed. Nothing was changed."],
  ])("explains a refused or failed install (%s) and keeps the update available", async (code, message) => {
    api().checkForUpdate.mockResolvedValue(newer);
    api().installUpdate.mockRejectedValueOnce({ code, detail: "test" });
    render(<UpdateStatus />);

    await userEvent.click(await screen.findByRole("button", { name: "Restart to update" }));

    expect(await screen.findByText(message)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restart to update" })).toBeInTheDocument();
  });
});
