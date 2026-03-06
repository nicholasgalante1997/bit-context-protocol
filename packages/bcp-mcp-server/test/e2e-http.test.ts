import { describe, test, expect, afterEach } from "bun:test";
import { resolve } from "node:path";

const SERVER_ENTRY = resolve(import.meta.dir, "..", "src/index.ts");
const GOLDEN_DIR = resolve(import.meta.dir, "../../..", "crates/bcp-tests/tests/golden");
const BCP_CLI = resolve(import.meta.dir, "../../..", "target/release/bcp");
const TEST_PORT = 44400;

let serverProc: ReturnType<typeof Bun.spawn> | null = null;

const startServer = async (): Promise<void> => {
  serverProc = Bun.spawn(
    ["bun", "run", SERVER_ENTRY, "--transport", "http", "--port", String(TEST_PORT)],
    {
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, BCP_CLI_PATH: BCP_CLI, BCP_LOG_LEVEL: "error" }
    }
  );
  await Bun.sleep(1000);
};

afterEach(() => {
  if (serverProc) {
    serverProc.kill();
    serverProc = null;
  }
});

const post = async (body: unknown, headers?: Record<string, string>): Promise<Response> => {
  return fetch(`http://127.0.0.1:${TEST_PORT}/mcp`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Accept": "application/json, text/event-stream",
      ...headers
    },
    body: JSON.stringify(body)
  });
};

const initialize = async (): Promise<string> => {
  const res = await post({
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "e2e-http-test", version: "1.0.0" }
    }
  });
  return res.headers.get("MCP-Session-Id")!;
};

describe("E2E HTTP", () => {
  test("full lifecycle: init -> tools/list -> tools/call -> delete", async () => {
    await startServer();

    const sessionId = await initialize();
    expect(sessionId).toBeTruthy();

    const notifRes = await post(
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { "MCP-Session-Id": sessionId }
    );
    expect(notifRes.status).toBe(202);

    const listRes = await post(
      { jsonrpc: "2.0", id: 2, method: "tools/list" },
      { "MCP-Session-Id": sessionId }
    );
    expect(listRes.status).toBe(200);
    const listBody = await listRes.json() as { result: { tools: Array<{ name: string }> } };
    expect(listBody.result.tools).toHaveLength(3);

    const bcpPath = resolve(GOLDEN_DIR, "simple_code/payload.bcp");
    const callRes = await post(
      {
        jsonrpc: "2.0",
        id: 3,
        method: "tools/call",
        params: { name: "read_bcp_file", arguments: { path: bcpPath } }
      },
      { "MCP-Session-Id": sessionId }
    );
    expect(callRes.status).toBe(200);
    const callBody = await callRes.json() as { result: { content: Array<{ text: string }> } };
    expect(callBody.result.content[0]?.text).toContain("rust");

    const delRes = await fetch(`http://127.0.0.1:${TEST_PORT}/mcp`, {
      method: "DELETE",
      headers: { "MCP-Session-Id": sessionId }
    });
    expect(delRes.status).toBe(200);
  }, 15_000);
});
