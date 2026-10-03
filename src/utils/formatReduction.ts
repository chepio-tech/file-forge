/** Share of the original size that was saved, as "74" or "0.4" (one decimal below 10%). */
export function formatReduction(originalSize: number, outputSize: number): string {
  if (originalSize <= 0 || outputSize >= originalSize) return "0";
  const percent = (1 - outputSize / originalSize) * 100;
  return percent >= 10 ? Math.floor(percent).toString() : (Math.floor(percent * 10) / 10).toString();
}

export default formatReduction;
