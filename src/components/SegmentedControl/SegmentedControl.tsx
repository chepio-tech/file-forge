// Core
import { useId } from "react";
// Styles
import "./SegmentedControl.css";

interface SegmentedControlProps<T extends string> {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
}

/** Native radio group styled as segments: arrow keys, focus and screen readers work without extra code. */
function SegmentedControl<T extends string>({ label, options, value, onChange, disabled }: SegmentedControlProps<T>) {
  const name = useId();
  return (
    <fieldset className="segmented" disabled={disabled}>
      <legend className="visually-hidden">{label}</legend>
      {options.map((option) => (
        <label key={option.value} className="segmented__option">
          <input
            type="radio"
            className="segmented__input"
            name={name}
            value={option.value}
            checked={option.value === value}
            onChange={() => onChange(option.value)}
          />
          <span className="segmented__label">{option.label}</span>
        </label>
      ))}
    </fieldset>
  );
}

export default SegmentedControl;
