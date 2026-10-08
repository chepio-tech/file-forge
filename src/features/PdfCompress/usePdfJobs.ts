// Hooks
import useCompressionJobs from "@/hooks/useCompressionJobs";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { CompressionEngine, CompressionJob, CompressionJobs } from "@/hooks/useCompressionJobs";
import type { FileInfo, PdfOptions, PdfReport } from "@/services/fileforgeApi";
// Utils
import { optionsKey } from "./pdfPresets";

export type Job = CompressionJob<PdfReport>;
export type PdfJobs = CompressionJobs<PdfOptions, PdfReport>;

export const PDF_ENGINE: CompressionEngine<PdfOptions, PdfReport> = {
  compress: (id, options, onProgress) => fileforgeApi.compressPdf(id, options, onProgress),
  optionsKey,
  removalRequested: (options) => options.stripMetadata || options.stripEditingData,
  keptOriginal: (report) => report.keptOriginal,
  sizes: (report) => report,
};

/** Compression state of the PDF tool's file list (see `useCompressionJobs`). */
export function usePdfJobs(files: FileInfo[]): PdfJobs {
  return useCompressionJobs(files, PDF_ENGINE);
}

export default usePdfJobs;
