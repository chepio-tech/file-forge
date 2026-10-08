// Components
import Checkbox from "@/components/Checkbox/Checkbox";
import NumberField from "@/components/NumberField/NumberField";
import SegmentedControl from "@/components/SegmentedControl/SegmentedControl";
// Types
import type { RasterOptions } from "@/services/fileforgeApi";
import type { PresetId } from "./imagePresets";
// Consts
import messages from "@/messages/messages";
import { JPEG_QUALITY, PNG_LEVEL, WEBP_QUALITY } from "./imagePresets";

interface ImageSettingsProps {
  preset: PresetId;
  options: RasterOptions;
  disabled: boolean;
  onPreset: (preset: PresetId) => void;
  /** Editing a number switches to the custom preset. */
  onCustom: (options: RasterOptions) => void;
  /** Metadata removal applies to any preset and does not change it. */
  onOptions: (options: RasterOptions) => void;
}

const PRESET_IDS: PresetId[] = ["lossless", "balanced", "maximum", "custom"];

function ImageSettings({ preset, options, disabled, onPreset, onCustom, onOptions }: ImageSettingsProps) {
  const text = messages.image;
  return (
    <section className="compression-settings" aria-label={text.settings}>
      <div className="compression-settings__row">
        <SegmentedControl
          label={text.settings}
          options={PRESET_IDS.map((id) => ({ value: id, label: text.presets[id] }))}
          value={preset}
          onChange={onPreset}
          disabled={disabled}
        />
        <div className="compression-settings__fields">
          <NumberField
            label={text.jpegQuality}
            value={options.jpegQuality}
            min={JPEG_QUALITY.min}
            max={JPEG_QUALITY.max}
            placeholder={text.keepPixels}
            disabled={disabled}
            onCommit={(jpegQuality) => onCustom({ ...options, jpegQuality })}
          />
          <NumberField
            label={text.webpQuality}
            value={options.webpQuality}
            min={WEBP_QUALITY.min}
            max={WEBP_QUALITY.max}
            placeholder={text.keepPixels}
            disabled={disabled}
            onCommit={(webpQuality) => onCustom({ ...options, webpQuality })}
          />
          <NumberField
            label={text.pngLevel}
            value={options.pngLevel}
            min={PNG_LEVEL.min}
            max={PNG_LEVEL.max}
            disabled={disabled}
            onCommit={(pngLevel) => onCustom({ ...options, pngLevel: pngLevel ?? options.pngLevel })}
          />
        </div>
      </div>
      <p className="compression-settings__hint">
        {text.hint(options.jpegQuality, options.webpQuality, options.pngLevel, options.pngZopfli === true)}
      </p>
      <div className="compression-settings__options">
        <Checkbox
          label={text.stripMetadata}
          hint={text.stripMetadataHint}
          checked={options.stripMetadata === true}
          disabled={disabled}
          onChange={(stripMetadata) => onOptions({ ...options, stripMetadata })}
        />
      </div>
    </section>
  );
}

export default ImageSettings;
