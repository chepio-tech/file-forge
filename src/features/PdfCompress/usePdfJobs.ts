// Core
import { useCallback, useEffect, useRef, useState } from "react";
// Services
import fileforgeApi, { isAppError } from "@/services/fileforgeApi";
// Types
import type { FileId, FileInfo, PdfOptions, PdfProgress, PdfReport } from "@/services/fileforgeApi";
// Utils
import { errorMessage } from "@/utils/errorMessage";
import { optionsKey } from "./pdfPresets";

export type Job =
  | { status: "waiting" }
  | { status: "working"; progress?: PdfProgress }
  | { status: "done"; report: PdfReport; optionsKey: string; savedName?: string }
  | { status: "cancelled" }
  | { status: "error"; message: string };

export interface PdfJobs {
  jobs: ReadonlyMap<FileId, Job>;
  running: boolean;
  /** Cancel was requested; the current file stops at its next checkpoint. */
  cancelling: boolean;
  saving: ReadonlySet<FileId>;
  savingAll: boolean;
  busy: boolean;
  /** 1-based index of the file being compressed, for "Compressing 2 of 5". */
  progress: { current: number; total: number } | null;
  /** Why the last save failed; results stay intact and can be saved again. */
  notice: string | null;
  dismissNotice: () => void;
  compressAll: (options: PdfOptions) => Promise<void>;
  /** Stops the run: the current file is cancelled, files not started yet are left untouched. */
  cancel: () => void;
  save: (id: FileId) => Promise<void>;
  saveAll: () => Promise<void>;
  reveal: (id: FileId) => void;
}

/**
 * Compression state of a tool's file list. Files are compressed one after another (the shell allows one at a
 * time anyway, and memory stays bounded); a file removed mid-run is skipped. Cancelling keeps finished results.
 */
export function usePdfJobs(files: FileInfo[]): PdfJobs {
  const [jobs, setJobs] = useState<Map<FileId, Job>>(new Map());
  const [progress, setProgress] = useState<PdfJobs["progress"]>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState<ReadonlySet<FileId>>(new Set());
  const [savingAll, setSavingAll] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const operating = useRef(false);
  /** `null` outside a run; `true` once the running batch was asked to stop. */
  const cancelRequested = useRef<boolean | null>(null);
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
    setJobs((current) => (present.current.has(id) ? new Map(current).set(id, job) : current));
  }, []);

  // Progress can arrive after the command settled; it only ever refines a file that is still working.
  const showProgress = useCallback((id: FileId, progress: PdfProgress) => {
    setJobs((current) =>
      current.get(id)?.status === "working" ? new Map(current).set(id, { status: "working", progress }) : current,
    );
  }, []);

  const compressAll = useCallback(
    async (options: PdfOptions) => {
      if (operating.current || files.length === 0) return;
      operating.current = true;
      cancelRequested.current = false;
      const ids = files.map((file) => file.id);
      setNotice(null);
      setJobs(new Map(ids.map((id) => [id, { status: "waiting" } as Job])));
      const key = optionsKey(options);
      try {
        for (const [index, id] of ids.entries()) {
          if (cancelRequested.current) break;
          if (!present.current.has(id)) continue;
          setProgress({ current: index + 1, total: ids.length });
          update(id, { status: "working" });
          try {
            const report = await fileforgeApi.compressPdf(id, options, (progress) => showProgress(id, progress));
            update(id, { status: "done", report, optionsKey: key });
          } catch (error) {
            const cancelled = isAppError(error) && error.code === "cancelled";
            update(id, cancelled ? { status: "cancelled" } : { status: "error", message: errorMessage(error) });
          }
        }
        if (cancelRequested.current) {
          setJobs((current) => new Map([...current].filter(([, job]) => job.status !== "waiting")));
        }
      } finally {
        setProgress(null);
        setCancelling(false);
        cancelRequested.current = null;
        operating.current = false;
      }
    },
    [files, update, showProgress],
  );

  const cancel = useCallback(() => {
    if (cancelRequested.current !== false) return;
    cancelRequested.current = true;
    setCancelling(true);
    // Even if this fails, the run stops after the current file.
    void fileforgeApi.cancelCompression().catch((error: unknown) => setNotice(errorMessage(error)));
  }, []);

  const markSaved = useCallback((id: FileId, name: string) => {
    setJobs((current) => {
      const job = current.get(id);
      return job?.status === "done" ? new Map(current).set(id, { ...job, savedName: name }) : current;
    });
  }, []);

  const save = useCallback(
    async (id: FileId) => {
      if (operating.current) return;
      operating.current = true;
      setSaving(new Set([id]));
      setNotice(null);
      try {
        const name = await fileforgeApi.saveResult(id);
        if (name) markSaved(id, name);
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setSaving(new Set());
        operating.current = false;
      }
    },
    [markSaved],
  );

  const saveAll = useCallback(async () => {
    if (operating.current) return;
    const ids = [...jobs].filter(([, job]) => job.status === "done" && !job.report.keptOriginal).map(([id]) => id);
    if (ids.length === 0) return;
    operating.current = true;
    setSaving(new Set(ids));
    setSavingAll(true);
    setNotice(null);
    try {
      const saved = await fileforgeApi.saveResultsToFolder(ids);
      for (const file of saved ?? []) markSaved(file.id, file.name);
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setSaving(new Set());
      setSavingAll(false);
      operating.current = false;
    }
  }, [jobs, markSaved]);

  const reveal = useCallback((id: FileId) => {
    setNotice(null);
    void fileforgeApi.revealResult(id).catch((error: unknown) => setNotice(errorMessage(error)));
  }, []);

  return {
    jobs,
    running: progress !== null,
    cancelling,
    saving,
    savingAll,
    busy: progress !== null || saving.size > 0,
    progress,
    notice,
    dismissNotice: () => setNotice(null),
    compressAll,
    cancel,
    save,
    saveAll,
    reveal,
  };
}

export default usePdfJobs;
