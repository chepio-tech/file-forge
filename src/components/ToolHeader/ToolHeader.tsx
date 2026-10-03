// Core
import type { ReactNode } from "react";
// Styles
import "./ToolHeader.css";

interface ToolHeaderProps {
  title: string;
  description: string;
  actions?: ReactNode;
}

/** Title row of a tool panel; doubles as the window drag area under the macOS overlay title bar. */
function ToolHeader({ title, description, actions }: ToolHeaderProps) {
  return (
    <header className="tool-header" data-tauri-drag-region>
      <div className="tool-header__text" data-tauri-drag-region>
        <h1 className="tool-header__title" data-tauri-drag-region>
          {title}
        </h1>
        <p className="tool-header__description" data-tauri-drag-region>
          {description}
        </p>
      </div>
      {actions ? <div className="tool-header__actions">{actions}</div> : null}
    </header>
  );
}

export default ToolHeader;
