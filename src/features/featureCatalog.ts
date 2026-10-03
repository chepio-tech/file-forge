// Types
import type { IconName } from "@/components/Icon/Icon";
import type { Messages } from "@/messages/messages";
import type { FileKind } from "@/services/fileforgeApi";

/**
 * Every tool the app offers or plans, grouped by file type. Adding a tool: add an entry here, its strings under
 * `tools` in `src/messages/messages.ts`, and (when `ready`) its panel in `src/App/toolPanels.ts`.
 */

export type GroupId = keyof Messages["nav"]["groups"];

export type ToolId = keyof Messages["tools"];

export interface ToolDefinition {
  id: ToolId;
  icon: IconName;
  /** File kinds the tool accepts from the dialog and from drag & drop. */
  accepts: FileKind[];
  /** Human-readable format list for prompts and dialog filters, e.g. "PDF". */
  formats: string;
  status: "ready" | "soon";
}

export interface GroupDefinition {
  id: GroupId;
  tools: ToolDefinition[];
}

export const featureCatalog: GroupDefinition[] = [
  {
    id: "documents",
    tools: [
      { id: "pdfCompress", icon: "compress", accepts: ["pdf"], formats: "PDF", status: "ready" },
      { id: "pdfConvert", icon: "convert", accepts: ["pdf"], formats: "PDF", status: "soon" },
    ],
  },
  {
    id: "images",
    tools: [
      { id: "imageCompress", icon: "compress", accepts: ["image"], formats: "JPEG, PNG, WebP, HEIC", status: "soon" },
      { id: "imageConvert", icon: "convert", accepts: ["image"], formats: "JPEG, PNG, WebP, HEIC", status: "soon" },
    ],
  },
  {
    id: "video",
    tools: [
      { id: "videoCompress", icon: "compress", accepts: ["video"], formats: "MP4, MOV, MKV, WebM", status: "soon" },
      { id: "videoConvert", icon: "convert", accepts: ["video"], formats: "MP4, MOV, MKV, WebM", status: "soon" },
    ],
  },
  {
    id: "audio",
    tools: [{ id: "audioConvert", icon: "convert", accepts: ["audio"], formats: "MP3, AAC, FLAC, WAV", status: "soon" }],
  },
];

export function findTool(id: ToolId): ToolDefinition | undefined {
  return featureCatalog.flatMap((group) => group.tools).find((tool) => tool.id === id);
}

export function firstReadyTool(): ToolDefinition | undefined {
  return featureCatalog.flatMap((group) => group.tools).find((tool) => tool.status === "ready");
}

export default featureCatalog;
