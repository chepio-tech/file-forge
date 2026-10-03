const UNITS = ["byte", "kilobyte", "megabyte", "gigabyte", "terabyte"] as const;

const LOCALE = "en-US";

/**
 * Formats a byte count with decimal (SI) units, the convention macOS Finder and most file managers use, so the
 * numbers match what the user sees next to the file. Three significant digits above 1 kB.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return new Intl.NumberFormat(LOCALE, {
    style: "unit",
    unit: UNITS[unit],
    // "999 bytes": the short form ("999 byte") reads like a typo.
    unitDisplay: unit === 0 ? "long" : "short",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

export default formatBytes;
