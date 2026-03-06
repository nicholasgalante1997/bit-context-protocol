import type { ToolResult } from "../types.ts";
import { runBcpCli } from "../bcp-cli.ts";

export const handleInspectBcpFile = async (
  args: Record<string, unknown>
): Promise<ToolResult> => {
  const path = args["path"];
  if (typeof path !== "string" || path.length === 0) {
    return {
      content: [{ type: "text", text: "Error: 'path' parameter is required and must be a string" }],
      isError: true
    };
  }

  if (!path.endsWith(".bcp")) {
    return {
      content: [{ type: "text", text: `Error: File must have .bcp extension, got: ${path}` }],
      isError: true
    };
  }

  const file = Bun.file(path);
  if (!(await file.exists())) {
    return {
      content: [{ type: "text", text: `Error: File not found: ${path}` }],
      isError: true
    };
  }

  const result = await runBcpCli(["inspect", path]);

  if (result.exitCode !== 0) {
    return {
      content: [{ type: "text", text: result.stderr || result.stdout || "bcp inspect failed" }],
      isError: true
    };
  }

  return {
    content: [{ type: "text", text: result.stdout }]
  };
};
