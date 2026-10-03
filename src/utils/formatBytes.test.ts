// Utils
import formatBytes from "./formatBytes";

describe("formatBytes", () => {
  it("keeps bytes whole", () => {
    expect(formatBytes(0)).toBe("0 bytes");
    expect(formatBytes(1)).toBe("1 byte");
    expect(formatBytes(999)).toBe("999 bytes");
  });

  it("uses decimal units with three significant digits", () => {
    expect(formatBytes(1_000)).toBe("1.00 kB");
    expect(formatBytes(15_360)).toBe("15.4 kB");
    expect(formatBytes(4_200_000)).toBe("4.20 MB");
    expect(formatBytes(123_456_789)).toBe("123 MB");
    expect(formatBytes(2_500_000_000)).toBe("2.50 GB");
  });

  it("renders a dash for impossible values", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});
