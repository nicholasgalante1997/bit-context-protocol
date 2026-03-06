import { describe, test, expect } from "bun:test";
import { handleInitialize, createSessionState } from "../src/lifecycle.ts";

describe("lifecycle", () => {
  test("createSessionState returns initial state", () => {
    const state = createSessionState();
    expect(state.initialized).toBe(false);
    expect(state.clientInfo).toBeNull();
  });

  test("handleInitialize returns correct result", () => {
    const result = handleInitialize({
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "test-client", version: "1.0.0" }
    });

    expect(result.protocolVersion).toBe("2025-11-25");
    expect(result.capabilities.tools.listChanged).toBe(false);
    expect(result.serverInfo.name).toBe("bcp-mcp-server");
    expect(result.serverInfo.version).toBe("0.1.0");
  });

  test("handleInitialize works with older protocol version", () => {
    const result = handleInitialize({
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "old-client", version: "0.1.0" }
    });

    expect(result.protocolVersion).toBe("2025-11-25");
  });
});
