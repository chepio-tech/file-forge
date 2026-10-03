// Core
import { useId } from "react";
// Styles
import "./Checkbox.css";

interface CheckboxProps {
  label: string;
  /** One line under the label explaining the consequence; announced as the description. */
  hint?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

/** Native checkbox with a label and an optional hint, so it looks and behaves like the platform's own. */
function Checkbox({ label, hint, checked, disabled, onChange }: CheckboxProps) {
  const labelId = useId();
  const hintId = useId();

  return (
    <label className={disabled ? "checkbox checkbox--disabled" : "checkbox"}>
      <input
        className="checkbox__input"
        type="checkbox"
        checked={checked}
        disabled={disabled}
        // The whole row is clickable, but only the label names the control; the hint describes it.
        aria-labelledby={labelId}
        aria-describedby={hint ? hintId : undefined}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="checkbox__text">
        <span id={labelId} className="checkbox__label">
          {label}
        </span>
        {hint ? (
          <span id={hintId} className="checkbox__hint">
            {hint}
          </span>
        ) : null}
      </span>
    </label>
  );
}

export default Checkbox;
