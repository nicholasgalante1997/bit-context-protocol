import type { ToolResult } from "../types.ts";
import { runBcpCli } from "../bcp-cli.ts";

export const handleEncodeBcpFile = async (
  args: Record<string, unknown>
): Promise<ToolResult> => {
  const manifestPath = args["manifest_path"];
  if (typeof manifestPath !== "string" || manifestPath.length === 0) {
    return {
      content: [{ type: "text", text: "Error: 'manifest_path' parameter is required and must be a string" }],
      isError: true
    };
  }

  const outputPath = args["output_path"];
  if (typeof outputPath !== "string" || outputPath.length === 0) {
    return {
      content: [{ type: "text", text: "Error: 'output_path' parameter is required and must be a string" }],
      isError: true
    };
  }

  const manifestFile = Bun.file(manifestPath);
  if (!(await manifestFile.exists())) {
    return {
      content: [{ type: "text", text: `Error: Manifest file not found: ${manifestPath}` }],
      isError: true
    };
  }

  const cliArgs: Array<string> = ["encode", manifestPath, "-o", outputPath];

  if (args["compress"] === true) {
    cliArgs.push("--compress-blocks");
  }

  const encodeResult = await runBcpCli(cliArgs);

  if (encodeResult.exitCode !== 0) {
    return {
      content: [{ type: "text", text: encodeResult.stderr || encodeResult.stdout || "bcp encode failed" }],
      isError: true
    };
  }

  const statsResult = await runBcpCli(["stats", outputPath]);
  const output = statsResult.exitCode === 0
    ? `Encoded successfully: ${outputPath}\n\n${statsResult.stdout}`
    : `Encoded successfully: ${outputPath}\n${encodeResult.stdout}`;

  return {
    content: [{ type: "text", text: output }]
  };
};
