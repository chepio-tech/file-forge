// Utils
import en from "./en";
import { detectLocale, saveLocale } from "./locale";
import { plural } from "./plural";
import ru from "./ru";

/** Flattens nested message objects into dotted keys with the value's type, e.g. `intake.remove: function`. */
function shape(messages: object, prefix = ""): string[] {
  return Object.entries(messages).flatMap(([key, value]) =>
    typeof value === "object" && value !== null
      ? shape(value as object, `${prefix}${key}.`)
      : [`${prefix}${key}: ${typeof value}`],
  );
}

describe("messages", () => {
  it("ru has exactly the keys and value types of en", () => {
    expect(shape(ru).sort()).toEqual(shape(en).sort());
  });

  it("has no empty strings", () => {
    for (const messages of [en, ru]) {
      for (const entry of shape(messages)) expect(entry).not.toMatch(/: $/);
    }
  });
});

describe("plural (ru)", () => {
  const forms = { one: "файл", few: "файла", many: "файлов" };

  it.each([
    [1, "файл"],
    [21, "файл"],
    [2, "файла"],
    [4, "файла"],
    [5, "файлов"],
    [11, "файлов"],
    [0, "файлов"],
    [1.5, "файла"],
  ])("%d → %s", (count, expected) => {
    expect(plural(count, forms)).toBe(expected);
  });

  it("is used by the files heading", () => {
    expect(ru.intake.filesHeading(3)).toBe("3 файла");
    expect(en.intake.filesHeading(1)).toBe("1 file");
  });
});

describe("detectLocale", () => {
  it("follows the first supported system language", () => {
    expect(detectLocale(["ru-RU", "en-US"])).toBe("ru");
    expect(detectLocale(["de-DE", "en-GB"])).toBe("en");
  });

  it("falls back to English", () => {
    expect(detectLocale(["ja-JP"])).toBe("en");
    expect(detectLocale([])).toBe("en");
  });

  it("prefers the saved choice over the system language", () => {
    saveLocale("ru");
    expect(detectLocale(["en-US"])).toBe("ru");
  });
});
