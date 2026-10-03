// Core
import type { ComponentType } from "react";
// Types
import type { ToolDefinition } from "./featureCatalog";

/** Contract every ready tool's panel implements. Panels stay mounted while hidden so their lists survive. */
export interface ToolPanelProps {
  tool: ToolDefinition;
  /** Only the visible panel accepts dropped files. */
  active: boolean;
}

export type ToolPanel = ComponentType<ToolPanelProps>;
