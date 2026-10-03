// Core
import { useCallback, useEffect, useRef, useState } from "react";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { FileId, FileInfo, PdfOptions, PdfReport } from "@/services/fileforgeApi";
// Utils
import { errorMessage } from "@/utils/errorMessage";
import { optionsKey } from "./pdfPresets";

export type Job =
  | { status: "waiting" }
  | { status: "working" }
  | { status: "done"; report: PdfReport; optionsKey: string; savedName?: string }
  | { status: "error"; message: string };

export interface PdfJobs {
  jobs: ReadonlyMap<FileId, Job>;
  running: boolean;
  /** 1-based index of the file being compressed, for "Compressing 2 of 5". */
  progress: { current: number; total: number } | null;
  /** Why the last save failed; results stay intact and can be saved again. */
  notice: string | null;
  dismissNotice: () => void;
  compressAll: (options: PdfOptions) => Promise<void>;
  save: (id: FileId) => Promise<void>;
  saveAll: () => Promise<void>;
  reveal: (id: FileId) => void;
}

/**
 * Compression state of a tool's file list. Files are compressed one after another (the shell allows one at a
 * time anyway, and memory stays bounded); a file removed mid-run is skipped.
 */
export function usePdfJobs(files: FileInfo[]): PdfJobs {
  const [jobs, setJobs] = useState<Map<FileId, Job>>(new Map());
  const [progress, setProgress] = useState<PdfJobs["progress"]>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const present = useRef(new Set<FileId>());

  // Track the current list for the running loop, and forget jobs of files that left it.
  useEffect(() => {
    present.current = new Set(files.map((file) => file.id));
    setJobs((current) => {
      const kept = [...current].filter(([id]) => present.current.has(id));
      return kept.length === current.size ? current : new Map(kept);
    });
  }, [files]);

  const update = useCallback((id: FileId, job: Job) => {
    setJobs((current) => new Map(current).set(id, job));
  }, []);

  const compressAll = useCallback(
    async (options: PdfOptions) => {
      const ids = files.map((file) => file.id);
      setNotice(null);
      setJobs(new Map(ids.map((id) => [id, { status: "waiting" } as Job])));
      const key = optionsKey(options);
      for (const [index, id] of ids.entries()) {
        if (!present.current.has(id)) continue;
        setProgress({ current: index + 1, total: ids.length });
        update(id, { status: "working" });
        try {
          const report = await fileforgeApi.compressPdf(id, options);
          update(id, { status: "done", report, optionsKey: key });
        } catch (error) {
          update(id, { status: "error", message: errorMessage(error) });
        }
      }
      setProgress(null);
    },
    [files, update],
  );

  const markSaved = useCallback((id: FileId, name: string) => {
    setJobs((current) => {
      const job = current.get(id);
      return job?.status === "done" ? new Map(current).set(id, { ...job, savedName: name }) : current;
    });
  }, []);

  const save = useCallback(
    async (id: FileId) => {
      try {
        const name = await fileforgeApi.saveResult(id);
        if (name) markSaved(id, name);
      } catch (error) {
        setNotice(errorMessage(error));
      }
    },
    [markSaved],
  );

  const saveAll = useCallback(async () => {
    const ids = [...jobs].filter(([, job]) => job.status === "done" && !job.report.keptOriginal).map(([id]) => id);
    if (ids.length === 0) return;
    try {
      const saved = await fileforgeApi.saveResultsToFolder(ids);
      for (const file of saved ?? []) markSaved(file.id, file.name);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [jobs, markSaved]);

  const reveal = useCallback((id: FileId) => {
    void fileforgeApi.revealResult(id);
  }, []);

  return {
    jobs,
    running: progress !== null,
    progress,
    notice,
    dismissNotice: () => setNotice(null),
    compressAll,
    save,
    saveAll,
    reveal,
  };
}

export default usePdfJobs;
