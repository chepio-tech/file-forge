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
  saving: boolean;
  onSave: (id: FileId) => void;
  onReveal: (id: FileId) => void;
}

/** The per-file cells of the PDF list: stage progress, result size and ratio, save actions. */
function PdfJobStatus({ id, job, busy, saving, onSave, onReveal }: PdfJobStatusProps) {
  if (!job) return null;
  switch (job.status) {
    case "waiting":
      return <span className="pdf-job pdf-job--muted">{messages.pdf.waiting}</span>;
    case "working": {
      const { progress } = job;
      return (
        <span className="pdf-job pdf-job--muted">
          <Spinner />
          {progress
            ? messages.pdf.stageProgress(messages.pdf.stages[progress.stage], progress.done, progress.total)
            : messages.pdf.working}
        </span>
      );
    }
    case "cancelled":
      return <span className="pdf-job pdf-job--muted">{messages.pdf.cancelled}</span>;
    case "error":
      return (
        <span className="pdf-job pdf-job--error" title={job.message}>
          {job.message}
        </span>
      );
    case "done": {
      const { report } = job;
      if (report.keptOriginal) {
        // Never larger wins over removal (ADR-0012): say plainly that the metadata is still there.
        const text = job.removalRequested ? messages.pdf.alreadyOptimalNothingRemoved : messages.pdf.alreadyOptimal;
        return <span className="pdf-job pdf-job--muted">{text}</span>;
      }
      const removed = messages.pdf.removed;
      const details = [
        messages.pdf.details(report.pages, report.imagesRecompressed, report.imagesDownsampled, report.duplicatesMerged),
        ...(report.metadataRemoved ? [removed.metadata] : []),
        ...(report.metadataKeptForStandard ? [removed.keptForStandard] : []),
        ...(report.thumbnailsRemoved > 0 ? [removed.thumbnails(report.thumbnailsRemoved)] : []),
        ...(report.editingDataRemoved > 0 ? [removed.editingData] : []),
      ].join(" · ");
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
              {saving ? messages.pdf.saving : messages.pdf.save}
            </button>
          )}
        </span>
      );
    }
  }
}

export default PdfJobStatus;
