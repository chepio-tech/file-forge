// Consts
import messages from "./messages";

/** Flattens nested message objects into `[dotted.key, value]` pairs. */
function entries(value: object, prefix = ""): [string, unknown][] {
  return Object.entries(value).flatMap(([key, child]) =>
    typeof child === "object" && child !== null ? entries(child as object, `${prefix}${key}.`) : [[`${prefix}${key}`, child]],
  );
}

describe("messages", () => {
  it("has no empty strings", () => {
    for (const [key, value] of entries(messages)) {
      if (typeof value === "string") expect(value.trim(), key).not.toBe("");
    }
  });

  it("formats counts and lists", () => {
    expect(messages.intake.filesHeading(1)).toBe("1 file");
    expect(messages.intake.filesHeading(3)).toBe("3 files");
    expect(messages.intake.wrongKind("PDF", "a.jpg")).toBe("Only PDF files are accepted here. Skipped: a.jpg");
  });
});
