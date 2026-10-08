// Components
import Checkbox from "@/components/Checkbox/Checkbox";
import NumberField from "@/components/NumberField/NumberField";
import SegmentedControl from "@/components/SegmentedControl/SegmentedControl";
// Types
import type { PdfOptions } from "@/services/fileforgeApi";
import type { PresetId } from "./pdfPresets";
// Consts
import messages from "@/messages/messages";
import { JPEG_QUALITY, MAX_DPI, PRESETS } from "./pdfPresets";

interface PdfSettingsProps {
  preset: PresetId;
  options: PdfOptions;
  disabled: boolean;
  onPreset: (preset: PresetId) => void;
  /** Editing a number switches to the custom preset. */
  onCustom: (options: PdfOptions) => void;
  /** Removal options apply to any preset and do not change it. */
  onOptions: (options: PdfOptions) => void;
}

const PRESET_IDS: PresetId[] = ["lossless", "balanced", "maximum", "screen", "custom"];

function PdfSettings({ preset, options, disabled, onPreset, onCustom, onOptions }: PdfSettingsProps) {
  const lossy = options.images ?? PRESETS.balanced.images!;
  const hint = options.images
    ? messages.pdf.lossyHint(options.images.jpegQuality, options.images.maxDpi)
    : messages.pdf.losslessHint;

  return (
    <section className="compression-settings" aria-label={messages.pdf.settings}>
      <div className="compression-settings__row">
        <SegmentedControl
          label={messages.pdf.settings}
          options={PRESET_IDS.map((id) => ({ value: id, label: messages.pdf.presets[id] }))}
          value={preset}
          onChange={onPreset}
          disabled={disabled}
        />
        <div className="compression-settings__fields">
          <NumberField
            label={messages.pdf.jpegQuality}
            value={options.images ? lossy.jpegQuality : null}
            min={JPEG_QUALITY.min}
            max={JPEG_QUALITY.max}
            disabled={disabled || !options.images}
            onCommit={(jpegQuality) =>
              onCustom({ ...options, images: { ...lossy, jpegQuality: jpegQuality ?? lossy.jpegQuality } })
            }
          />
          <NumberField
            label={messages.pdf.maxDpi}
            value={options.images ? lossy.maxDpi : null}
            min={MAX_DPI.min}
            max={MAX_DPI.max}
            placeholder={messages.pdf.noLimit}
            disabled={disabled || !options.images}
            onCommit={(maxDpi) => onCustom({ ...options, images: { ...lossy, maxDpi } })}
          />
        </div>
      </div>
      <p className="compression-settings__hint">
        {hint}
        {options.images?.compressFlatePhotos && ` ${messages.pdf.flatePhotosHint}`}
        {preset === "screen" && ` ${messages.pdf.screenHint}`}
      </p>
      <div className="compression-settings__options">
        <Checkbox
          label={messages.pdf.stripMetadata}
          hint={messages.pdf.stripMetadataHint}
          checked={options.stripMetadata}
          disabled={disabled}
          onChange={(stripMetadata) => onOptions({ ...options, stripMetadata })}
        />
        <Checkbox
          label={messages.pdf.stripEditingData}
          hint={messages.pdf.stripEditingDataHint}
          checked={options.stripEditingData}
          disabled={disabled}
          onChange={(stripEditingData) => onOptions({ ...options, stripEditingData })}
        />
      </div>
    </section>
  );
}

export default PdfSettings;
