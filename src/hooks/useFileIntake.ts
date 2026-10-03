// Core
import { useCallback, useEffect, useEffectEvent, useState } from "react";
// Hooks
import useI18n from "@/hooks/useI18n";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
import type { Messages } from "@/i18n/en";
import type { FileId, FileInfo, RegisterOutcome } from "@/services/fileforgeApi";
// Utils
import { errorMessage, listNames } from "@/utils/errorMessage";

/** Stored as data, not text, so a language switch re-renders the notice in the new language. */
type NoticeItem =
  | { type: "wrongKind"; names: string[] }
  | { type: "skipped"; names: string[] }
  | { type: "error"; error: unknown };

function noticeText(t: Messages, tool: ToolDefinition, item: NoticeItem): string {
  switch (item.type) {
    case "wrongKind":
      return t.intake.wrongKind(tool.formats, listNames(item.names));
    case "skipped":
      return t.intake.skipped(listNames(item.names));
    case "error":
      return errorMessage(t, item.error);
  }
}

export interface FileIntake {
  files: FileInfo[];
  /** Lines explaining what the last intake skipped or why it failed; empty when there is nothing to say. */
  notice: string[];
  dismissNotice: () => void;
  pick: () => Promise<void>;
  remove: (id: FileId) => void;
  clear: () => void;
}

/**
 * The list of input files of one tool: files come from the native dialog or from drops on the window (only while
 * the tool is `active`), are filtered to the kinds the tool accepts, and de-duplicated by id.
 */
export function useFileIntake(tool: ToolDefinition, active: boolean): FileIntake {
  const { t } = useI18n();
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [notice, setNotice] = useState<NoticeItem[]>([]);

  const accept = useCallback(
    (outcome: RegisterOutcome) => {
      const accepted = outcome.files.filter((file) => tool.accepts.includes(file.kind));
      const rejected = outcome.files.filter((file) => !tool.accepts.includes(file.kind));
      for (const file of rejected) void fileforgeApi.removeFile(file.id);

      setFiles((current) => {
        const known = new Set(current.map((file) => file.id));
        return [...current, ...accepted.filter((file) => !known.has(file.id))];
      });

      const items: NoticeItem[] = [];
      if (rejected.length > 0) items.push({ type: "wrongKind", names: rejected.map((file) => file.name) });
      if (outcome.skipped.length > 0) items.push({ type: "skipped", names: outcome.skipped });
      setNotice(items);
    },
    [tool],
  );

  const onDropped = useEffectEvent(accept);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void fileforgeApi.onFilesAdded((outcome) => onDropped(outcome)).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active]);

  const pick = useCallback(async () => {
    try {
      accept(await fileforgeApi.pickFiles(tool.accepts, t.intake.filterName(tool.formats)));
    } catch (error) {
      setNotice([{ type: "error", error }]);
    }
  }, [accept, t, tool]);

  const remove = useCallback((id: FileId) => {
    setFiles((current) => current.filter((file) => file.id !== id));
    void fileforgeApi.removeFile(id);
  }, []);

  const clear = useCallback(() => {
    for (const file of files) void fileforgeApi.removeFile(file.id);
    setFiles([]);
    setNotice([]);
  }, [files]);

  return {
    files,
    notice: notice.map((item) => noticeText(t, tool, item)),
    dismissNotice: () => setNotice([]),
    pick,
    remove,
    clear,
  };
}

export default useFileIntake;
