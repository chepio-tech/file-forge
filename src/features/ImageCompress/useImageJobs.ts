// Hooks
import useCompressionJobs from "@/hooks/useCompressionJobs";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { CompressionEngine, CompressionJobs } from "@/hooks/useCompressionJobs";
import type { FileInfo, RasterOptions, RasterReport } from "@/services/fileforgeApi";
// Utils
import { optionsKey } from "./imagePresets";

export type ImageJobs = CompressionJobs<RasterOptions, RasterReport>;

export const IMAGE_ENGINE: CompressionEngine<RasterOptions, RasterReport> = {
  compress: (id, options, onProgress) => fileforgeApi.compressImage(id, options, onProgress),
  optionsKey,
  removalRequested: (options) => options.stripMetadata === true,
  keptOriginal: (report) => report.kept !== null,
  sizes: (report) => report,
};

/** Compression state of the image tool's file list (see `useCompressionJobs`). */
export function useImageJobs(files: FileInfo[]): ImageJobs {
  return useCompressionJobs(files, IMAGE_ENGINE);
}

export default useImageJobs;
