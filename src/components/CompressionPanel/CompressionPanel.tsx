// Core
import type { ReactNode } from "react";
// Components
import DropOverlay from "@/components/DropOverlay/DropOverlay";
import DropZone from "@/components/DropZone/DropZone";
import FileList from "@/components/FileList/FileList";
import Icon from "@/components/Icon/Icon";
import JobStatus from "@/components/JobStatus/JobStatus";
import Notice from "@/components/Notice/Notice";
import ToolBody from "@/components/ToolBody/ToolBody";
import ToolHeader from "@/components/ToolHeader/ToolHeader";
// Types
import type { JobOutcome } from "@/components/JobStatus/JobStatus";
import type { ToolDefinition } from "@/features/featureCatalog";
import type { FileIntake } from "@/hooks/useFileIntake";
import type { CompressionEngine, CompressionJobs, DoneJob } from "@/hooks/useCompressionJobs";
// Styles
import "./CompressionPanel.css";
// Utils
import formatBytes from "@/utils/formatBytes";
import formatReduction from "@/utils/formatReduction";
// Consts
import messages from "@/messages/messages";

interface CompressionPanelProps<Options, Report> {
  tool: ToolDefinition;
  title: string;
  description: string;
  intake: FileIntake;
  jobs: CompressionJobs<Options, Report>;
  engine: CompressionEngine<Options, Report>;
  /** The settings the next run uses; results made with other settings are flagged as outdated. */
  options: Options;
  hovering: boolean;
  /** The tool's settings section, shown above the file list. */
  settings: ReactNode;
  describe: (job: DoneJob<Report>) => JobOutcome;
}

/** Layout shared by the compression tools: intake, settings, per-file results and the action bar. */
function CompressionPanel<Options, Report>({
  tool,
  title,
  description,
  intake,
  jobs,
  engine,
  options,
  hovering,
  settings,
  describe,
}: CompressionPanelProps<Options, Report>) {
  const text = messages.compression;
  const { files } = intake;
  const done = [...jobs.jobs.values()].flatMap((job) => (job.status === "done" ? [job] : []));
  const savable = done.filter((job) => !engine.keptOriginal(job.report)).length;
  const totalBefore = done.reduce((sum, job) => sum + engine.sizes(job.report).originalSize, 0);
  const totalAfter = done.reduce((sum, job) => sum + engine.sizes(job.report).outputSize, 0);
  const outdated = done.some((job) => job.optionsKey !== engine.optionsKey(options));
  const notices = [...intake.notice, ...(jobs.notice ? [jobs.notice] : [])];

  return (
    <>
      <ToolHeader
        title={title}
        description={description}
        actions={
          files.length > 0 ? (
            <>
              <button type="button" className="button button--ghost" onClick={intake.clear} disabled={jobs.busy}>
                {messages.intake.clear}
              </button>
              <button type="button" className="button" onClick={() => void intake.pick()}>
                <Icon name="plus" />
                {messages.intake.addMore}
              </button>
            </>
          ) : null
        }
      />
      <ToolBody>
        {notices.length > 0 ? (
          <Notice
            lines={notices}
            onDismiss={() => {
              intake.dismissNotice();
              jobs.dismissNotice();
            }}
          />
        ) : null}
        {files.length === 0 ? (
          <DropZone formats={tool.formats} onChoose={() => void intake.pick()} />
        ) : (
          <>
            {settings}
            <section className="compression-panel__files" aria-label={messages.intake.filesHeading(files.length)}>
              <div className="compression-panel__summary">
                <span>{messages.intake.filesHeading(files.length)}</span>
                <span className="compression-panel__total">
                  {messages.intake.totalSize(formatBytes(files.reduce((sum, file) => sum + file.size, 0)))}
                </span>
              </div>
              <FileList
                files={files}
                onRemove={intake.remove}
                locked={jobs.busy}
                renderDetails={(file) => (
                  <JobStatus
                    id={file.id}
                    job={jobs.jobs.get(file.id)}
                    busy={jobs.busy}
                    saving={jobs.saving.has(file.id)}
                    describe={describe}
                    onSave={(id) => void jobs.save(id)}
                    onReveal={jobs.reveal}
                  />
                )}
              />
            </section>
          </>
        )}
        <DropOverlay visible={hovering} />
      </ToolBody>
      {files.length > 0 ? (
        <footer className="compression-panel__actions">
          <div className="compression-panel__outcome" aria-live="polite">
            {outdated && !jobs.running ? (
              <span className="compression-panel__outdated">{text.outdated}</span>
            ) : done.length > 0 ? (
              <>
                <span className="compression-panel__outcome-sizes">
                  {text.total(formatBytes(totalBefore), formatBytes(totalAfter))}
                </span>
                <span className="job-status__reduction">{text.reduction(formatReduction(totalBefore, totalAfter))}</span>
              </>
            ) : null}
          </div>
          {savable > 0 ? (
            <button type="button" className="button" disabled={jobs.busy} onClick={() => void jobs.saveAll()}>
              {jobs.savingAll ? text.saving : text.saveAll(savable)}
            </button>
          ) : null}
          {jobs.running ? (
            <button type="button" className="button" disabled={jobs.cancelling} onClick={jobs.cancel}>
              {jobs.cancelling ? text.cancelling : text.cancel}
            </button>
          ) : null}
          <button
            type="button"
            className="button button--primary"
            disabled={jobs.busy}
            onClick={() => void jobs.compressAll(options)}
          >
            {jobs.progress ? text.compressing(jobs.progress.current, jobs.progress.total) : text.compress(files.length)}
          </button>
        </footer>
      ) : null}
    </>
  );
}

export default CompressionPanel;
