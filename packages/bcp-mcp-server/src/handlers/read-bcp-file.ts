import type { ToolResult } from "../types.ts";
import { runBcpCli } from "../bcp-cli.ts";

const VALID_MODES = ["xml", "markdown", "minimal"] as const;

export const handleReadBcpFile = async (
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

  const mode = typeof args["mode"] === "string" && VALID_MODES.includes(args["mode"] as typeof VALID_MODES[number])
    ? (args["mode"] as string)
    : "xml";

  const cliArgs: Array<string> = ["decode", path, "--mode", mode];

  const budget = args["budget"];
  if (typeof budget === "number" && budget > 0) {
    cliArgs.push("--budget", String(Math.floor(budget)), "--verbosity", "adaptive");
  }

  const result = await runBcpCli(cliArgs);

  if (result.exitCode !== 0) {
    return {
      content: [{ type: "text", text: result.stderr || result.stdout || "bcp decode failed" }],
      isError: true
    };
  }

  return {
    content: [{ type: "text", text: result.stdout }]
  };
};
