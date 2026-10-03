// Components
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
}

const PRESET_IDS: PresetId[] = ["lossless", "balanced", "maximum", "custom"];

function PdfSettings({ preset, options, disabled, onPreset, onCustom }: PdfSettingsProps) {
  const lossy = options.images ?? PRESETS.balanced.images!;
  const hint = options.images
    ? messages.pdf.lossyHint(options.images.jpegQuality, options.images.maxDpi)
    : messages.pdf.losslessHint;

  return (
    <section className="pdf-settings" aria-label={messages.pdf.settings}>
      <div className="pdf-settings__row">
        <SegmentedControl
          label={messages.pdf.settings}
          options={PRESET_IDS.map((id) => ({ value: id, label: messages.pdf.presets[id] }))}
          value={preset}
          onChange={onPreset}
          disabled={disabled}
        />
        <div className="pdf-settings__fields">
          <NumberField
            label={messages.pdf.jpegQuality}
            value={options.images ? lossy.jpegQuality : null}
            min={JPEG_QUALITY.min}
            max={JPEG_QUALITY.max}
            disabled={disabled || !options.images}
            onCommit={(jpegQuality) =>
              onCustom({ images: { ...lossy, jpegQuality: jpegQuality ?? lossy.jpegQuality } })
            }
          />
          <NumberField
            label={messages.pdf.maxDpi}
            value={options.images ? lossy.maxDpi : null}
            min={MAX_DPI.min}
            max={MAX_DPI.max}
            placeholder={messages.pdf.noLimit}
            disabled={disabled || !options.images}
            onCommit={(maxDpi) => onCustom({ images: { ...lossy, maxDpi } })}
          />
        </div>
      </div>
      <p className="pdf-settings__hint">{hint}</p>
    </section>
  );
}

export default PdfSettings;
