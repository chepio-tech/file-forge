// Core
import { useState } from "react";
// Components
import CompressionPanel from "@/components/CompressionPanel/CompressionPanel";
import NumberField from "@/components/NumberField/NumberField";
import SegmentedControl from "@/components/SegmentedControl/SegmentedControl";
// Hooks
import useCompressionJobs from "@/hooks/useCompressionJobs";
import useDragHover from "@/hooks/useDragHover";
import useFileIntake from "@/hooks/useFileIntake";
// Services
import fileforgeApi from "@/services/fileforgeApi";
// Types
import type { ToolPanelProps } from "@/features/toolPanel";
import type { CompressionEngine, DoneJob } from "@/hooks/useCompressionJobs";
import type { ConversionOptions, ConversionReport, RasterFormat } from "@/services/fileforgeApi";
// Consts
import messages from "@/messages/messages";

const DEFAULT_OPTIONS: ConversionOptions = { format: "png", jpegQuality: 90, background: [255, 255, 255] };
const FORMATS: RasterFormat[] = ["png", "webp", "jpeg"];

export const CONVERSION_ENGINE: CompressionEngine<ConversionOptions, ConversionReport> = {
  compress: (id, options, onProgress) => fileforgeApi.convertImage(id, options, onProgress),
  optionsKey: (options) => options.format === "jpeg" ? `${options.format}/${options.jpegQuality}/${options.background.join(",")}` : options.format,
  removalRequested: () => true,
  keptOriginal: () => false,
  sizes: (report) => report,
};

function describe(job: DoneJob<ConversionReport>) {
  const { report } = job;
  return {
    kept: null,
    originalSize: report.originalSize,
    outputSize: report.outputSize,
    details: messages.conversion.details(messages.image.formats[report.format], report.width, report.height, report.flattened),
  };
}

function ImageConvert({ tool, active }: ToolPanelProps) {
  const intake = useFileIntake(tool, active);
  const hovering = useDragHover(active);
  const jobs = useCompressionJobs(intake.files, CONVERSION_ENGINE);
  const [options, setOptions] = useState<ConversionOptions>(DEFAULT_OPTIONS);
  const text = messages.conversion;
  return (
    <CompressionPanel
      tool={tool}
      title={messages.tools.imageConvert.title}
      description={messages.tools.imageConvert.description}
      intake={intake}
      jobs={jobs}
      engine={CONVERSION_ENGINE}
      options={options}
      hovering={hovering}
      describe={describe}
      conversion
      settings={
        <section className="compression-settings" aria-label={text.settings}>
          <div className="compression-settings__row">
            <SegmentedControl
              label={text.settings}
              options={FORMATS.map((value) => ({ value, label: messages.image.formats[value] }))}
              value={options.format}
              disabled={jobs.busy}
              onChange={(format) => setOptions({ ...options, format })}
            />
            {options.format === "jpeg" ? (
              <NumberField
                label={messages.image.jpegQuality}
                value={options.jpegQuality}
                min={30}
                max={95}
                disabled={jobs.busy}
                onCommit={(jpegQuality) => setOptions({ ...options, jpegQuality: jpegQuality ?? 90 })}
              />
            ) : null}
          </div>
          <p className="compression-settings__hint">{options.format === "jpeg" ? text.jpegHint : text.losslessHint}</p>
          {options.format === "jpeg" ? (
            <div className="compression-settings__fields">
              <span className="compression-settings__hint">{text.background}</span>
              <SegmentedControl
                label={text.background}
                options={[{ value: "white", label: text.white }, { value: "black", label: text.black }]}
                value={options.background[0] === 255 ? "white" : "black"}
                disabled={jobs.busy}
                onChange={(value) => setOptions({ ...options, background: value === "white" ? [255, 255, 255] : [0, 0, 0] })}
              />
            </div>
          ) : null}
          <p className="compression-settings__hint">{text.metadataHint}</p>
          <p className="compression-settings__hint">{text.limitsHint}</p>
        </section>
      }
    />
  );
}

export default ImageConvert;
