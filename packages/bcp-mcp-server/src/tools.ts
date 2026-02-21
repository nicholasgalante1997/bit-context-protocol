import type { ToolDefinition } from "./types.ts";

export const TOOLS: ReadonlyArray<ToolDefinition> = [
  {
    name: "read_bcp_file",
    description:
      "Read and decode a .bcp (Bit Context Protocol) file, returning " +
      "the rendered content as model-ready text. Supports XML, Markdown, " +
      "and Minimal output modes.",
    inputSchema: {
      type: "object",
      properties: {
        path: {
          type: "string",
          description: "Absolute path to the .bcp file"
        },
        mode: {
          type: "string",
          description: "Output render mode",
          enum: ["xml", "markdown", "minimal"],
          default: "xml"
        },
        budget: {
          type: "number",
          description:
            "Optional token budget. When set, low-priority blocks " +
            "are summarized to fit within the budget."
        }
      },
      required: ["path"]
    }
  },
  {
    name: "inspect_bcp_file",
    description:
      "Inspect a .bcp file and return a summary of its blocks, sizes, " +
      "and structure without rendering the full content.",
    inputSchema: {
      type: "object",
      properties: {
        path: {
          type: "string",
          description: "Absolute path to the .bcp file"
        }
      },
      required: ["path"]
    }
  },
  {
    name: "encode_bcp_file",
    description:
      "Encode a JSON manifest into a .bcp file. The manifest describes " +
      "blocks to include (code, conversation, tool results, etc.).",
    inputSchema: {
      type: "object",
      properties: {
        manifest_path: {
          type: "string",
          description: "Absolute path to the JSON manifest file"
        },
        output_path: {
          type: "string",
          description: "Absolute path for the output .bcp file"
        },
        compress: {
          type: "boolean",
          description: "Enable per-block zstd compression",
          default: false
        }
      },
      required: ["manifest_path", "output_path"]
    }
  }
];
