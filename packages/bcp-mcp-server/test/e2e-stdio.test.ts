import { describe, test, expect } from "bun:test";
import { resolve } from "node:path";
import { readMessages } from "../src/transports/stdio.ts";
import { createRouter } from "../src/router.ts";
import { createSessionState } from "../src/lifecycle.ts";
import type { JsonRpcResponse } from "../src/types.ts";

const GOLDEN_DIR = resolve(import.meta.dir, "../../..", "crates/bcp-tests/tests/golden");

const createStream = (text: string): ReadableStream<Uint8Array> => {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(text));
      controller.close();
    }
  });
};

const runSession = async (messages: Array<Record<string, unknown>>): Promise<Array<JsonRpcResponse>> => {
  const input = messages.map((m) => JSON.stringify(m)).join("\n") + "\n";
  const stream = createStream(input);
  const session = createSessionState();
  const router = createRouter(session);
  const responses: Array<JsonRpcResponse> = [];

  for await (const message of readMessages(stream)) {
    const response = await router(message);
    if (response) {
      responses.push(response);
    }
  }

  return responses;
};

describe("E2E stdio", () => {
  test("full handshake and tools/list", async () => {
    const responses = await runSession([
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "e2e-test", version: "1.0.0" }
        }
      },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/list" }
    ]);

    expect(responses.length).toBeGreaterThanOrEqual(2);

    const initResponse = responses.find((r) => r.id === 1);
    expect(initResponse).toBeTruthy();
    expect(initResponse!.result).toBeTruthy();

    const toolsResponse = responses.find((r) => r.id === 2);
    expect(toolsResponse).toBeTruthy();
    const result = toolsResponse!.result as { tools: Array<{ name: string }> };
    expect(result.tools).toHaveLength(3);
  });

  test("tools/call read_bcp_file", async () => {
    const bcpPath = resolve(GOLDEN_DIR, "simple_code/payload.bcp");
    const responses = await runSession([
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "e2e-test", version: "1.0.0" }
        }
      },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: {
          name: "read_bcp_file",
          arguments: { path: bcpPath }
        }
      }
    ]);

    const toolResponse = responses.find((r) => r.id === 2);
    expect(toolResponse).toBeTruthy();
    const result = toolResponse!.result as { content: Array<{ text: string }> };
    expect(result.content[0]?.text).toContain("rust");
  });

  test("tools/call inspect_bcp_file", async () => {
    const bcpPath = resolve(GOLDEN_DIR, "all_block_types/payload.bcp");
    const responses = await runSession([
      {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "e2e-test", version: "1.0.0" }
        }
      },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      {
        jsonrpc: "2.0",
        id: 2,
        method: "tools/call",
        params: {
          name: "inspect_bcp_file",
          arguments: { path: bcpPath }
        }
      }
    ]);

    const toolResponse = responses.find((r) => r.id === 2);
    expect(toolResponse).toBeTruthy();
    const result = toolResponse!.result as { content: Array<{ text: string }> };
    expect(result.content[0]?.text.toLowerCase()).toContain("block");
  });

  test("ping works without initialization", async () => {
    const responses = await runSession([
      { jsonrpc: "2.0", id: 1, method: "ping" }
    ]);

    expect(responses).toHaveLength(1);
    expect(responses[0]!.result).toEqual({});
  });

  test("unknown method returns error", async () => {
    const responses = await runSession([
      { jsonrpc: "2.0", id: 1, method: "nonexistent/method" }
    ]);

    expect(responses).toHaveLength(1);
    expect(responses[0]!.error?.code).toBe(-32601);
  });
});
