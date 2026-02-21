import type { DetectedRegion } from "./detector.ts";

export type BcpManifest = {
  blocks: Array<{
    type: string;
    lang?: string;
    path?: string;
    format?: string;
    role?: string;
    tool_name?: string;
    status?: string;
    content: string;
    summary?: string;
  }>;
};

const BLOCK_TYPE_MAP: Record<DetectedRegion["blockType"], string> = {
  code: "code",
  file_tree: "file_tree",
  structured_data: "structured_data",
  diff: "diff",
  tool_result: "tool_result"
} as const;

const generateSummary = (content: string): string | undefined => {
  if (content.length <= 500) return undefined;
  const firstLine = content.split("\n")[0] ?? "";
  return `${firstLine.slice(0, 100)}... (${content.length} chars)`;
};

export const buildManifest = (
  toolName: string,
  regions: ReadonlyArray<DetectedRegion>
): BcpManifest => {
  const blocks: BcpManifest["blocks"] = regions.map((region) => {
    const blockType = BLOCK_TYPE_MAP[region.blockType] ?? "tool_result";
    const block: BcpManifest["blocks"][number] = {
      type: blockType,
      content: region.content,
      summary: generateSummary(region.content)
    };

    if (region.blockType === "code") {
      block.lang = region.metadata.lang ?? "unknown";
      if (region.metadata.path) block.path = region.metadata.path;
    }

    if (region.blockType === "structured_data") {
      block.format = region.metadata.format ?? "json";
    }

    if (region.blockType === "tool_result") {
      block.tool_name = toolName;
      block.status = "ok";
    }

    if (region.blockType === "diff" && region.metadata.path) {
      block.path = region.metadata.path;
    }

    return block;
  });

  return { blocks };
};
