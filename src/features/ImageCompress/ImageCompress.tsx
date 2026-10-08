// Core
import { useState } from "react";
// Components
import CompressionPanel from "@/components/CompressionPanel/CompressionPanel";
import ImageSettings from "./ImageSettings";
// Hooks
import useDragHover from "@/hooks/useDragHover";
import useFileIntake from "@/hooks/useFileIntake";
import useImageJobs, { IMAGE_ENGINE } from "./useImageJobs";
// Types
import type { JobOutcome } from "@/components/JobStatus/JobStatus";
import type { ToolPanelProps } from "@/features/toolPanel";
import type { DoneJob } from "@/hooks/useCompressionJobs";
import type { RasterOptions, RasterReport } from "@/services/fileforgeApi";
import type { PresetId } from "./imagePresets";
// Consts
import messages from "@/messages/messages";
import { DEFAULT_PRESET, PRESETS } from "./imagePresets";

/** Sizes, or why the original stays; format, size and what changed go into the details. */
export function describeImage(job: DoneJob<RasterReport>): JobOutcome {
  const { report } = job;
  const details = messages.image.details(
    messages.image.formats[report.format],
    report.width,
    report.height,
    report.reencoded,
    report.metadataRemoved,
  );
  let kept: string | null = null;
  if (report.kept === "notSmaller") {
    kept = job.removalRequested
      ? messages.compression.alreadyOptimalNothingRemoved
      : messages.compression.alreadyOptimal;
  } else if (report.kept !== null) {
    kept = messages.image.kept[report.kept];
  }
  return { kept, originalSize: report.originalSize, outputSize: report.outputSize, details };
}

function ImageCompress({ tool, active }: ToolPanelProps) {
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const jobs = useImageJobs(intake.files);
  const [preset, setPreset] = useState<PresetId>(DEFAULT_PRESET);
  const [options, setOptions] = useState<RasterOptions>(PRESETS[DEFAULT_PRESET]);

  const choosePreset = (next: PresetId) => {
    setPreset(next);
    if (next !== "custom") setOptions({ ...PRESETS[next], stripMetadata: options.stripMetadata });
  };
  const editCustom = (next: RasterOptions) => {
    setPreset("custom");
    setOptions(next);
  };

  return (
    <CompressionPanel
      tool={tool}
      title={messages.tools.imageCompress.title}
      description={messages.tools.imageCompress.description}
      intake={intake}
      jobs={jobs}
      engine={IMAGE_ENGINE}
      options={options}
      hovering={hovering}
      describe={describeImage}
      settings={
        <ImageSettings
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

export default ImageCompress;
