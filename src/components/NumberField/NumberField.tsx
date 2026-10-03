// Core
import { useEffect, useId, useState } from "react";
// Styles
import "./NumberField.css";

interface NumberFieldProps {
  label: string;
  value: number | null;
  min: number;
  max: number;
  /** Shown when the value is empty; empty is allowed only with `placeholder`. */
  placeholder?: string;
  disabled?: boolean;
  onCommit: (value: number | null) => void;
}

/** Numeric input that edits freely and commits a clamped value on blur or Enter, so typing "1" on the way to "150"
 * is never rejected mid-keystroke. */
function NumberField({ label, value, min, max, placeholder, disabled, onCommit }: NumberFieldProps) {
  const id = useId();
  const [draft, setDraft] = useState(value?.toString() ?? "");

  useEffect(() => setDraft(value?.toString() ?? ""), [value]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed === "" && placeholder !== undefined) {
      onCommit(null);
      return;
    }
    const parsed = Number(trimmed);
    if (!Number.isFinite(parsed)) {
      setDraft(value?.toString() ?? "");
      return;
    }
    const clamped = Math.min(max, Math.max(min, Math.round(parsed)));
    setDraft(clamped.toString());
    onCommit(clamped);
  };

  return (
    <div className="number-field">
      <label className="number-field__label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="number-field__input"
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        value={draft}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
        }}
      />
    </div>
  );
}

export default NumberField;
