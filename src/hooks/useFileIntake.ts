// Core
import { useCallback, useEffect, useEffectEvent, useState } from "react";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { ToolDefinition } from "@/features/featureCatalog";
import type { FileId, FileInfo, RegisterOutcome } from "@/services/fileforgeApi";
// Utils
import { errorMessage, listNames } from "@/utils/errorMessage";
// Consts
import messages from "@/messages/messages";

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
 * the tool is `active`; dropped folders are searched for its kinds), are filtered to the kinds the tool accepts, and
 * de-duplicated by id.
 */
export function useFileIntake(tool: ToolDefinition, active: boolean): FileIntake {
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [notice, setNotice] = useState<string[]>([]);

  const accept = useCallback(
    (outcome: RegisterOutcome) => {
      const accepted = outcome.files.filter((file) => tool.accepts.includes(file.kind));
      const rejected = outcome.files.filter((file) => !tool.accepts.includes(file.kind));
      for (const file of rejected) void fileforgeApi.removeFile(file.id);

      setFiles((current) => {
        const known = new Set(current.map((file) => file.id));
        return [...current, ...accepted.filter((file) => !known.has(file.id))];
      });

      const lines = [];
      if (rejected.length > 0) {
        lines.push(messages.intake.wrongKind(tool.formats, listNames(rejected.map((file) => file.name))));
      }
      if (outcome.skipped.length > 0) lines.push(messages.intake.skipped(listNames(outcome.skipped)));
      const folders = outcome.folders;
      if (folders) {
        if (folders.added === 0 && !folders.truncated) {
          lines.push(messages.intake.noneInFolders(tool.formats, folders.folders));
        }
        if (folders.ignored > 0) lines.push(messages.intake.ignoredInFolders(folders.ignored, folders.folders));
        if (folders.truncated) lines.push(messages.intake.foldersTruncated(folders.folders));
      }
      setNotice(lines);
    },
    [tool],
  );

  const onDropped = useEffectEvent(accept);

  useEffect(() => {
    // Without it dropped folders add nothing; directly dropped files still arrive, so a failure stays quiet.
    if (active) fileforgeApi.setDropKinds(tool.accepts).catch(() => {});
  }, [active, tool]);

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
      accept(await fileforgeApi.pickFiles(tool.accepts, messages.intake.filterName(tool.formats)));
    } catch (error) {
      setNotice([errorMessage(error)]);
    }
  }, [accept, tool]);

  const remove = useCallback((id: FileId) => {
    setFiles((current) => current.filter((file) => file.id !== id));
    void fileforgeApi.removeFile(id);
  }, []);

  const clear = useCallback(() => {
    for (const file of files) void fileforgeApi.removeFile(file.id);
    setFiles([]);
    setNotice([]);
  }, [files]);

  return { files, notice, dismissNotice: () => setNotice([]), pick, remove, clear };
}

export default useFileIntake;
