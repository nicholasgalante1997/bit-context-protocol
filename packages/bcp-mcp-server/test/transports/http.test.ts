import { describe, test, expect, afterEach } from "bun:test";
import { createHttpServer } from "../../src/transports/http.ts";
import type { SessionState } from "../../src/lifecycle.ts";
import type { JsonRpcMessage, JsonRpcResponse } from "../../src/types.ts";

let stopFn: (() => void) | null = null;

afterEach(() => {
  if (stopFn) {
    stopFn();
    stopFn = null;
  }
});

const createTestServer = (port: number) => {
  const mockRouter = async (_session: SessionState, message: JsonRpcMessage): Promise<JsonRpcResponse | null> => {
    if (message.method === "initialize") {
      return {
        jsonrpc: "2.0",
        id: (message as { id: number }).id,
        result: {
          protocolVersion: "2025-11-25",
          capabilities: { tools: { listChanged: false } },
          serverInfo: { name: "test", version: "0.1.0" }
        }
      };
    }
    if (message.method === "notifications/initialized") return null;
    if (message.method === "ping") {
      return { jsonrpc: "2.0", id: (message as { id: number }).id, result: {} };
    }
    return null;
  };

  const httpServer = createHttpServer({
    port,
    host: "127.0.0.1",
    allowedOrigins: ["localhost", "127.0.0.1"],
    router: mockRouter
  });

  stopFn = httpServer.stop;
  return httpServer;
};

describe("HTTP transport", () => {
  test("POST /mcp with initialize returns JSON response and session ID", async () => {
    const port = 44301;
    createTestServer(port);

    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream"
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "test", version: "0.1.0" }
        }
      })
    });

    expect(res.status).toBe(200);
    expect(res.headers.get("Content-Type")).toBe("application/json");

    const sessionId = res.headers.get("MCP-Session-Id");
    expect(sessionId).toBeTruthy();

    const body = await res.json() as JsonRpcResponse;
    expect(body.jsonrpc).toBe("2.0");
    expect(body.id).toBe(1);
    expect(body.result).toBeTruthy();
  });

  test("POST /mcp notification returns 202", async () => {
    const port = 44302;
    createTestServer(port);

    // First initialize to get session
    const initRes = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Accept": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "test", version: "0.1.0" } }
      })
    });
    const sessionId = initRes.headers.get("MCP-Session-Id")!;

    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Accept": "application/json",
        "MCP-Session-Id": sessionId
      },
      body: JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })
    });

    expect(res.status).toBe(202);
  });

  test("GET /mcp returns 405", async () => {
    const port = 44303;
    createTestServer(port);

    const res = await fetch(`http://127.0.0.1:${port}/mcp`, { method: "GET" });
    expect(res.status).toBe(405);
  });

  test("POST to unknown path returns 404", async () => {
    const port = 44304;
    createTestServer(port);

    const res = await fetch(`http://127.0.0.1:${port}/unknown`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}"
    });
    expect(res.status).toBe(404);
  });

  test("DELETE /mcp terminates session", async () => {
    const port = 44305;
    createTestServer(port);

    // Initialize
    const initRes = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Accept": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "test", version: "0.1.0" } }
      })
    });
    const sessionId = initRes.headers.get("MCP-Session-Id")!;

    // Delete session
    const delRes = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "DELETE",
      headers: { "MCP-Session-Id": sessionId }
    });
    expect(delRes.status).toBe(200);

    // Subsequent request with deleted session should fail
    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Accept": "application/json",
        "MCP-Session-Id": sessionId
      },
      body: JSON.stringify({ jsonrpc: "2.0", id: 2, method: "ping" })
    });
    expect(res.status).toBe(404);
  });

  test("POST without session ID (non-initialize) returns 404", async () => {
    const port = 44306;
    createTestServer(port);

    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Accept": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "ping" })
    });
    expect(res.status).toBe(404);
  });

  test("malformed JSON returns 400", async () => {
    const port = 44307;
    createTestServer(port);

    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Accept": "application/json" },
      body: "not json"
    });
    expect(res.status).toBe(400);
  });

  test("formatSseEvent produces correct format", () => {
    const port = 44308;
    const httpServer = createTestServer(port);
    const event = httpServer.formatSseEvent("evt-1", { jsonrpc: "2.0", id: 1, result: {} });
    expect(event).toBe('id: evt-1\ndata: {"jsonrpc":"2.0","id":1,"result":{}}\n\n');
  });
});
