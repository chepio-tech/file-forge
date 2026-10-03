// Utils
import formatReduction from "./formatReduction";

describe("formatReduction", () => {
  it("rounds down so the claim is never overstated", () => {
    expect(formatReduction(1000, 260)).toBe("74");
    expect(formatReduction(1000, 999)).toBe("0.1");
    expect(formatReduction(1000, 951)).toBe("4.9");
  });

  it("reports nothing saved when the size did not shrink", () => {
    expect(formatReduction(1000, 1000)).toBe("0");
    expect(formatReduction(0, 0)).toBe("0");
  });
});
