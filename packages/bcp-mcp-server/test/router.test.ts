import { describe, test, expect, beforeEach } from "bun:test";
import { createRouter } from "../src/router.ts";
import { createSessionState } from "../src/lifecycle.ts";
import type { SessionState } from "../src/lifecycle.ts";
import type { JsonRpcMessage } from "../src/types.ts";
import { METHOD_NOT_FOUND, SERVER_NOT_INITIALIZED } from "../src/types.ts";

describe("router", () => {
  let session: SessionState;
  let router: (message: JsonRpcMessage) => Promise<{ jsonrpc: "2.0"; id: string | number; result?: unknown; error?: { code: number; message: string; data?: unknown } } | null>;

  beforeEach(() => {
    session = createSessionState();
    router = createRouter(session);
  });

  test("initialize sets session state", async () => {
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "test", version: "1.0.0" }
      }
    });

    expect(response).not.toBeNull();
    expect(response!.result).toBeTruthy();
    expect(session.initialized).toBe(true);
    expect(session.clientInfo?.name).toBe("test");
  });

  test("notifications/initialized returns null", async () => {
    const response = await router({
      jsonrpc: "2.0",
      method: "notifications/initialized"
    });
    expect(response).toBeNull();
  });

  test("ping returns empty result", async () => {
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "ping"
    });
    expect(response).not.toBeNull();
    expect(response!.result).toEqual({});
  });

  test("tools/list returns tools after initialization", async () => {
    session.initialized = true;
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/list"
    });

    expect(response).not.toBeNull();
    const result = response!.result as { tools: Array<{ name: string }> };
    expect(result.tools).toHaveLength(3);
    expect(result.tools.map((t) => t.name)).toEqual([
      "read_bcp_file",
      "inspect_bcp_file",
      "encode_bcp_file"
    ]);
  });

  test("tools/list before initialization returns error", async () => {
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/list"
    });

    expect(response).not.toBeNull();
    expect(response!.error?.code).toBe(SERVER_NOT_INITIALIZED);
  });

  test("tools/call before initialization returns error", async () => {
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: "read_bcp_file", arguments: { path: "/tmp/test.bcp" } }
    });

    expect(response!.error?.code).toBe(SERVER_NOT_INITIALIZED);
  });

  test("unknown method returns METHOD_NOT_FOUND", async () => {
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "nonexistent/method"
    });

    expect(response).not.toBeNull();
    expect(response!.error?.code).toBe(METHOD_NOT_FOUND);
  });

  test("unknown tool returns METHOD_NOT_FOUND", async () => {
    session.initialized = true;
    const response = await router({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: "nonexistent_tool", arguments: {} }
    });

    expect(response!.error?.code).toBe(METHOD_NOT_FOUND);
  });

  test("unknown notification returns null", async () => {
    const response = await router({
      jsonrpc: "2.0",
      method: "unknown/notification"
    });
    expect(response).toBeNull();
  });
});
