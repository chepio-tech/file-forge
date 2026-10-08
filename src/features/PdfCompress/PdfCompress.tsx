// Core
import { useState } from "react";
// Components
import CompressionPanel from "@/components/CompressionPanel/CompressionPanel";
import PdfSettings from "./PdfSettings";
// Hooks
import useDragHover from "@/hooks/useDragHover";
import useFileIntake from "@/hooks/useFileIntake";
import usePdfJobs, { PDF_ENGINE } from "./usePdfJobs";
// Types
import type { JobOutcome } from "@/components/JobStatus/JobStatus";
import type { ToolPanelProps } from "@/features/toolPanel";
import type { DoneJob } from "@/hooks/useCompressionJobs";
import type { PdfOptions, PdfReport } from "@/services/fileforgeApi";
import type { PresetId } from "./pdfPresets";
// Consts
import messages from "@/messages/messages";
import { DEFAULT_PRESET, PRESETS } from "./pdfPresets";

/** Sizes, or "already optimal" when the original is kept; what was removed goes into the details. */
export function describePdf(job: DoneJob<PdfReport>): JobOutcome {
  const { report } = job;
  if (report.keptOriginal) {
    // Never larger wins over removal (ADR-0012): say plainly that the metadata is still there.
    const kept = job.removalRequested
      ? messages.compression.alreadyOptimalNothingRemoved
      : messages.compression.alreadyOptimal;
    return { kept, originalSize: report.originalSize, outputSize: report.outputSize, details: "" };
  }
  const removed = messages.pdf.removed;
  const details = [
    messages.pdf.details(report.pages, report.imagesRecompressed, report.imagesDownsampled, report.duplicatesMerged),
    ...(report.metadataRemoved ? [removed.metadata] : []),
    ...(report.metadataKeptForStandard ? [removed.keptForStandard] : []),
    ...(report.thumbnailsRemoved > 0 ? [removed.thumbnails(report.thumbnailsRemoved)] : []),
    ...(report.editingDataRemoved > 0 ? [removed.editingData] : []),
  ].join(" · ");
  return { kept: null, originalSize: report.originalSize, outputSize: report.outputSize, details };
}

function PdfCompress({ tool, active }: ToolPanelProps) {
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const jobs = usePdfJobs(intake.files);
  const [preset, setPreset] = useState<PresetId>(DEFAULT_PRESET);
  const [options, setOptions] = useState<PdfOptions>(PRESETS[DEFAULT_PRESET]);

  const choosePreset = (next: PresetId) => {
    setPreset(next);
    if (next !== "custom") setOptions({ ...options, images: PRESETS[next].images });
    else if (!options.images) setOptions({ ...options, images: PRESETS.balanced.images });
  };
  const editCustom = (next: PdfOptions) => {
    setPreset("custom");
    setOptions(next);
  };

  return (
    <CompressionPanel
      tool={tool}
      title={messages.tools.pdfCompress.title}
      description={messages.tools.pdfCompress.description}
      intake={intake}
      jobs={jobs}
      engine={PDF_ENGINE}
      options={options}
      hovering={hovering}
      describe={describePdf}
      settings={
        <PdfSettings
          preset={preset}
          options={options}
          disabled={jobs.busy}
          onPreset={choosePreset}
          onCustom={editCustom}
          onOptions={setOptions}
        />
      }
    />
  );
}

export default PdfCompress;
