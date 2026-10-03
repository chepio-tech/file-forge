// Components
import Icon from "@/components/Icon/Icon";
import Spinner from "@/components/Spinner/Spinner";
// Types
import type { FileId } from "@/services/fileforgeApi";
import type { Job } from "./usePdfJobs";
// Utils
import formatBytes from "@/utils/formatBytes";
import formatReduction from "@/utils/formatReduction";
// Consts
import messages from "@/messages/messages";

interface PdfJobStatusProps {
  id: FileId;
  job: Job | undefined;
  busy: boolean;
  onSave: (id: FileId) => void;
  onReveal: (id: FileId) => void;
}

/** The per-file cells of the PDF list: progress, result size and ratio, save actions. */
function PdfJobStatus({ id, job, busy, onSave, onReveal }: PdfJobStatusProps) {
  if (!job) return null;
  switch (job.status) {
    case "waiting":
      return <span className="pdf-job pdf-job--muted">{messages.pdf.waiting}</span>;
    case "working":
      return (
        <span className="pdf-job pdf-job--muted">
          <Spinner />
          {messages.pdf.working}
        </span>
      );
    case "error":
      return (
        <span className="pdf-job pdf-job--error" title={job.message}>
          {job.message}
        </span>
      );
    case "done": {
      const { report } = job;
      if (report.keptOriginal) {
        return <span className="pdf-job pdf-job--muted">{messages.pdf.alreadyOptimal}</span>;
      }
      const details = messages.pdf.details(
        report.pages,
        report.imagesRecompressed,
        report.imagesDownsampled,
        report.duplicatesMerged,
      );
      return (
        <span className="pdf-job">
          <span className="pdf-job__result" title={details}>
            → {formatBytes(report.outputSize)}
          </span>
          <span className="pdf-job__reduction">
            {messages.pdf.reduction(formatReduction(report.originalSize, report.outputSize))}
          </span>
          {job.savedName ? (
            <button
              type="button"
              className="button button--ghost pdf-job__saved"
              title={messages.pdf.savedAs(job.savedName)}
              onClick={() => onReveal(id)}
            >
              <Icon name="check" />
              {messages.pdf.saved}
            </button>
          ) : (
            <button type="button" className="button" disabled={busy} onClick={() => onSave(id)}>
              {messages.pdf.save}
            </button>
          )}
        </span>
      );
    }
  }
}

export default PdfJobStatus;
