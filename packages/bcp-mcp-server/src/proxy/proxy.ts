import type { Subprocess } from "bun";
import type { JsonRpcMessage, JsonRpcResponse, ToolResult, ToolDefinition } from "../types.ts";
import type { ProxyConfig } from "./config.ts";
import { detectContent } from "./detector.ts";
import { buildManifest } from "./manifest.ts";
import { runBcpCli } from "../bcp-cli.ts";
import { log } from "../logger.ts";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { unlinkSync } from "node:fs";

type PendingRequest = {
  resolve: (value: JsonRpcResponse) => void;
  reject: (reason: Error) => void;
};

export class McpProxy {
  private downstream: Subprocess<"pipe", "pipe", "pipe"> | null = null;
  private pendingRequests = new Map<string | number, PendingRequest>();
  private downstreamTools: Array<ToolDefinition> = [];
  private nextId = 1;
  private buffer = "";

  async start(config: ProxyConfig): Promise<void> {
    if (!config.downstream.command) {
      throw new Error("Downstream command is required for proxy mode");
    }

    log("info", `Starting downstream: ${config.downstream.command} ${config.downstream.args.join(" ")}`);

    this.downstream = Bun.spawn(
      [config.downstream.command, ...config.downstream.args],
      { stdin: "pipe", stdout: "pipe", stderr: "pipe" }
    );

    this.readDownstreamOutput();
  }

  async initialize(params: Record<string, unknown>): Promise<JsonRpcResponse> {
    return this.sendToDownstream({
      jsonrpc: "2.0",
      id: this.nextId++,
      method: "initialize",
      params
    });
  }

  async getDownstreamTools(): Promise<Array<ToolDefinition>> {
    const response = await this.sendToDownstream({
      jsonrpc: "2.0",
      id: this.nextId++,
      method: "tools/list"
    });

    if (response.result) {
      const result = response.result as { tools: Array<ToolDefinition> };
      this.downstreamTools = result.tools ?? [];
    }

    return this.downstreamTools;
  }

  mergeToolLists(localTools: ReadonlyArray<ToolDefinition>): Array<ToolDefinition> {
    const localNames = new Set(localTools.map((t) => t.name));
    const merged = [...localTools];

    for (const tool of this.downstreamTools) {
      if (localNames.has(tool.name)) {
        merged.push({ ...tool, name: `downstream_${tool.name}` });
      } else {
        merged.push(tool);
      }
    }

    return merged;
  }

  async forwardToolCall(
    toolName: string,
    args: Record<string, unknown>,
    config: ProxyConfig
  ): Promise<ToolResult> {
    const actualName = toolName.startsWith("downstream_")
      ? toolName.slice("downstream_".length)
      : toolName;

    const response = await this.sendToDownstream({
      jsonrpc: "2.0",
      id: this.nextId++,
      method: "tools/call",
      params: { name: actualName, arguments: args }
    });

    if (response.error) {
      return {
        content: [{ type: "text", text: `Downstream error: ${response.error.message}` }],
        isError: true
      };
    }

    const result = response.result as ToolResult;
    if (!config.detection.enabled) return result;

    const textContent = result.content
      .filter((c) => c.type === "text")
      .map((c) => c.text)
      .join("\n");

    if (textContent.length < 100) return result;

    try {
      return await this.wrapInBcp(actualName, textContent, config);
    } catch (err) {
      log("warn", "BCP wrapping failed, passing through original:", err);
      return result;
    }
  }

  sendNotification(method: string, params?: Record<string, unknown>): void {
    if (!this.downstream) return;
    const message = params
      ? { jsonrpc: "2.0" as const, method, params }
      : { jsonrpc: "2.0" as const, method };
    const json = JSON.stringify(message) + "\n";
    this.downstream.stdin.write(json);
  }

  stop(): void {
    if (this.downstream) {
      this.downstream.kill();
      this.downstream = null;
    }
    for (const [, pending] of this.pendingRequests) {
      pending.reject(new Error("Proxy shutting down"));
    }
    this.pendingRequests.clear();
  }

  private async wrapInBcp(
    toolName: string,
    text: string,
    config: ProxyConfig
  ): Promise<ToolResult> {
    const regions = detectContent(text, config.detection.confidenceThreshold);

    const allToolResult = regions.every((r) => r.blockType === "tool_result");
    if (allToolResult) {
      return { content: [{ type: "text", text }] };
    }

    const manifest = buildManifest(toolName, regions);
    const manifestPath = resolve(tmpdir(), `bcp-proxy-${crypto.randomUUID()}.json`);
    const bcpPath = resolve(tmpdir(), `bcp-proxy-${crypto.randomUUID()}.bcp`);

    try {
      await Bun.write(manifestPath, JSON.stringify(manifest));

      const encodeResult = await runBcpCli(["encode", manifestPath, "-o", bcpPath]);
      if (encodeResult.exitCode !== 0) {
        throw new Error(`Encode failed: ${encodeResult.stderr}`);
      }

      const decodeArgs = ["decode", bcpPath, "--mode", config.render.mode];
      if (config.render.budget) {
        decodeArgs.push("--budget", String(config.render.budget), "--verbosity", "adaptive");
      }

      const decodeResult = await runBcpCli(decodeArgs);
      if (decodeResult.exitCode !== 0) {
        throw new Error(`Decode failed: ${decodeResult.stderr}`);
      }

      return { content: [{ type: "text", text: decodeResult.stdout }] };
    } finally {
      try { unlinkSync(manifestPath); } catch { /* ignore */ }
      try { unlinkSync(bcpPath); } catch { /* ignore */ }
    }
  }

  private sendToDownstream(message: JsonRpcMessage & { id: number }): Promise<JsonRpcResponse> {
    if (!this.downstream) {
      return Promise.reject(new Error("Downstream not started"));
    }

    return new Promise<JsonRpcResponse>((res, reject) => {
      this.pendingRequests.set(message.id, { resolve: res, reject });
      const json = JSON.stringify(message) + "\n";
      this.downstream!.stdin.write(json);
    });
  }

  private async readDownstreamOutput(): Promise<void> {
    if (!this.downstream?.stdout) return;

    const decoder = new TextDecoder();
    for await (const chunk of this.downstream.stdout) {
      this.buffer += decoder.decode(chunk, { stream: true });

      let newlineIndex: number;
      while ((newlineIndex = this.buffer.indexOf("\n")) !== -1) {
        const line = this.buffer.slice(0, newlineIndex).trim();
        this.buffer = this.buffer.slice(newlineIndex + 1);

        if (line.length === 0) continue;

        try {
          const parsed = JSON.parse(line) as Record<string, unknown>;
          if ("id" in parsed && parsed["id"] !== undefined) {
            const pending = this.pendingRequests.get(parsed["id"] as string | number);
            if (pending) {
              this.pendingRequests.delete(parsed["id"] as string | number);
              pending.resolve(parsed as unknown as JsonRpcResponse);
            }
          }
        } catch {
          log("error", "Failed to parse downstream response:", line);
        }
      }
    }
  }
}
