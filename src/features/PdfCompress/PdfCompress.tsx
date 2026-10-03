// Core
import { useState } from "react";
// Components
import DropOverlay from "@/components/DropOverlay/DropOverlay";
import DropZone from "@/components/DropZone/DropZone";
import FileList from "@/components/FileList/FileList";
import Icon from "@/components/Icon/Icon";
import Notice from "@/components/Notice/Notice";
import ToolBody from "@/components/ToolBody/ToolBody";
import ToolHeader from "@/components/ToolHeader/ToolHeader";
import PdfJobStatus from "./PdfJobStatus";
import PdfSettings from "./PdfSettings";
// Hooks
import useDragHover from "@/hooks/useDragHover";
import useFileIntake from "@/hooks/useFileIntake";
import usePdfJobs from "./usePdfJobs";
// Types
import type { ToolPanelProps } from "@/features/toolPanel";
import type { PdfOptions } from "@/services/fileforgeApi";
import type { PresetId } from "./pdfPresets";
// Styles
import "./PdfCompress.css";
// Utils
import formatBytes from "@/utils/formatBytes";
import formatReduction from "@/utils/formatReduction";
import { optionsKey } from "./pdfPresets";
// Consts
import messages from "@/messages/messages";
import { DEFAULT_PRESET, PRESETS } from "./pdfPresets";

function PdfCompress({ tool, active }: ToolPanelProps) {
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const jobs = usePdfJobs(intake.files);
  const [preset, setPreset] = useState<PresetId>(DEFAULT_PRESET);
  const [options, setOptions] = useState<PdfOptions>(PRESETS[DEFAULT_PRESET]);
  const { files } = intake;

  const choosePreset = (next: PresetId) => {
    setPreset(next);
    if (next !== "custom") setOptions(PRESETS[next]);
    else if (!options.images) setOptions(PRESETS.balanced);
  };
  const editCustom = (next: PdfOptions) => {
    setPreset("custom");
    setOptions(next);
  };

  const done = [...jobs.jobs.values()].flatMap((job) => (job.status === "done" ? [job] : []));
  const savable = done.filter((job) => !job.report.keptOriginal).length;
  const totalBefore = done.reduce((sum, job) => sum + job.report.originalSize, 0);
  const totalAfter = done.reduce((sum, job) => sum + job.report.outputSize, 0);
  const outdated = done.some((job) => job.optionsKey !== optionsKey(options));
  const notices = [...intake.notice, ...(jobs.notice ? [jobs.notice] : [])];

  return (
    <>
      <ToolHeader
        title={messages.tools.pdfCompress.title}
        description={messages.tools.pdfCompress.description}
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
            <PdfSettings
              preset={preset}
              options={options}
              disabled={jobs.busy}
              onPreset={choosePreset}
              onCustom={editCustom}
            />
            <section className="pdf-compress__files" aria-label={messages.intake.filesHeading(files.length)}>
              <div className="pdf-compress__summary">
                <span>{messages.intake.filesHeading(files.length)}</span>
                <span className="pdf-compress__total">
                  {messages.intake.totalSize(formatBytes(files.reduce((sum, file) => sum + file.size, 0)))}
                </span>
              </div>
              <FileList
                files={files}
                onRemove={intake.remove}
                locked={jobs.busy}
                renderDetails={(file) => (
                  <PdfJobStatus
                    id={file.id}
                    job={jobs.jobs.get(file.id)}
                    busy={jobs.busy}
                    saving={jobs.saving.has(file.id)}
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
        <footer className="pdf-compress__actions">
          <div className="pdf-compress__outcome" aria-live="polite">
            {outdated && !jobs.running ? (
              <span className="pdf-compress__outdated">{messages.pdf.outdated}</span>
            ) : done.length > 0 ? (
              <>
                <span className="pdf-compress__outcome-sizes">
                  {messages.pdf.total(formatBytes(totalBefore), formatBytes(totalAfter))}
                </span>
                <span className="pdf-job__reduction">
                  {messages.pdf.reduction(formatReduction(totalBefore, totalAfter))}
                </span>
              </>
            ) : null}
          </div>
          {savable > 0 ? (
            <button type="button" className="button" disabled={jobs.busy} onClick={() => void jobs.saveAll()}>
              {jobs.savingAll ? messages.pdf.saving : messages.pdf.saveAll(savable)}
            </button>
          ) : null}
          {jobs.running ? (
            <button type="button" className="button" disabled={jobs.cancelling} onClick={jobs.cancel}>
              {jobs.cancelling ? messages.pdf.cancelling : messages.pdf.cancel}
            </button>
          ) : null}
          <button
            type="button"
            className="button button--primary"
            disabled={jobs.busy}
            onClick={() => void jobs.compressAll(options)}
          >
            {jobs.progress
              ? messages.pdf.compressing(jobs.progress.current, jobs.progress.total)
              : messages.pdf.compress(files.length)}
          </button>
        </footer>
      ) : null}
    </>
  );
}

export default PdfCompress;
