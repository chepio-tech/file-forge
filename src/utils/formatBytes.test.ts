// Utils
import formatBytes from "./formatBytes";

describe("formatBytes", () => {
  it("keeps bytes whole", () => {
    expect(formatBytes(0, "en")).toBe("0 bytes");
    expect(formatBytes(999, "en")).toBe("999 bytes");
    expect(formatBytes(1, "ru")).toBe("1 байт");
  });

  it("uses decimal units with three significant digits", () => {
    expect(formatBytes(1_000, "en")).toBe("1.00 kB");
    expect(formatBytes(15_360, "en")).toBe("15.4 kB");
    expect(formatBytes(4_200_000, "en")).toBe("4.20 MB");
    expect(formatBytes(123_456_789, "en")).toBe("123 MB");
    expect(formatBytes(2_500_000_000, "en")).toBe("2.50 GB");
  });

  it("localizes the decimal separator and unit names", () => {
    expect(formatBytes(4_200_000, "ru")).toBe("4,20 МБ");
  });

  it("renders a dash for impossible values", () => {
    expect(formatBytes(-1, "en")).toBe("—");
    expect(formatBytes(Number.NaN, "en")).toBe("—");
  });
});
