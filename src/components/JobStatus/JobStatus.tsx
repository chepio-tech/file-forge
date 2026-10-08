// Components
import Icon from "@/components/Icon/Icon";
import Spinner from "@/components/Spinner/Spinner";
// Types
import type { CompressionJob, DoneJob } from "@/hooks/useCompressionJobs";
import type { FileId } from "@/services/fileforgeApi";
// Styles
import "./JobStatus.css";
// Utils
import formatBytes from "@/utils/formatBytes";
import formatReduction from "@/utils/formatReduction";
// Consts
import messages from "@/messages/messages";

/** How a tool presents a finished job. */
export interface JobOutcome {
  /** A sentence instead of sizes when the result is the original file byte for byte. */
  kept: string | null;
  originalSize: number;
  outputSize: number;
  /** What the engine did, shown on hover. */
  details: string;
}

interface JobStatusProps<Report> {
  id: FileId;
  job: CompressionJob<Report> | undefined;
  busy: boolean;
  saving: boolean;
  describe: (job: DoneJob<Report>) => JobOutcome;
  onSave: (id: FileId) => void;
  onReveal: (id: FileId) => void;
}

/** The per-file cells of a compression list: stage progress, result size and ratio, save actions. */
function JobStatus<Report>({ id, job, busy, saving, describe, onSave, onReveal }: JobStatusProps<Report>) {
  if (!job) return null;
  const text = messages.compression;
  switch (job.status) {
    case "waiting":
      return <span className="job-status job-status--muted">{text.waiting}</span>;
    case "working": {
      const { progress } = job;
      return (
        <span className="job-status job-status--muted">
          <Spinner />
          {progress ? text.stageProgress(text.stages[progress.stage], progress.done, progress.total) : text.working}
        </span>
      );
    }
    case "cancelled":
      return <span className="job-status job-status--muted">{text.cancelled}</span>;
    case "error":
      return (
        <span className="job-status job-status--error" title={job.message}>
          {job.message}
        </span>
      );
    case "done": {
      const outcome = describe(job);
      if (outcome.kept !== null) {
        return (
          <span className="job-status job-status--muted" title={outcome.details}>
            {outcome.kept}
          </span>
        );
      }
      return (
        <span className="job-status">
          <span className="job-status__result" title={outcome.details}>
            → {formatBytes(outcome.outputSize)}
          </span>
          <span className="job-status__reduction">
            {text.reduction(formatReduction(outcome.originalSize, outcome.outputSize))}
          </span>
          {job.savedName ? (
            <button
              type="button"
              className="button button--ghost job-status__saved"
              title={text.savedAs(job.savedName)}
              onClick={() => onReveal(id)}
            >
              <Icon name="check" />
              {text.saved}
            </button>
          ) : (
            <button type="button" className="button" disabled={busy} onClick={() => onSave(id)}>
              {saving ? text.saving : text.save}
            </button>
          )}
        </span>
      );
    }
  }
}

export default JobStatus;
