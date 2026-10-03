// Core
import type { ReactNode } from "react";
// Styles
import "./ToolBody.css";

interface ToolBodyProps {
  children: ReactNode;
}

/** Scrollable content area of a tool panel; positions the drop overlay. */
function ToolBody({ children }: ToolBodyProps) {
  return <div className="tool-body">{children}</div>;
}

export default ToolBody;
