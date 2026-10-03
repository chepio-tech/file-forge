// Components
import PdfCompress from "@/features/PdfCompress/PdfCompress";
// Types
import type { ToolId } from "@/features/featureCatalog";
import type { ToolPanel } from "@/features/toolPanel";

/** Panels of the tools marked `ready` in the feature catalog. */
export const toolPanels: Partial<Record<ToolId, ToolPanel>> = {
  pdfCompress: PdfCompress,
};

export default toolPanels;
