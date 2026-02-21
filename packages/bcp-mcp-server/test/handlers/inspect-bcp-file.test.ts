import { describe, test, expect } from "bun:test";
import { handleInspectBcpFile } from "../../src/handlers/inspect-bcp-file.ts";
import { resolve } from "node:path";

const GOLDEN_DIR = resolve(import.meta.dir, "../../../..", "crates/bcp-tests/tests/golden");

describe("handleInspectBcpFile", () => {
  test("inspects simple_code golden file", async () => {
    const result = await handleInspectBcpFile({
      path: resolve(GOLDEN_DIR, "simple_code/payload.bcp")
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text.toLowerCase()).toContain("block");
  });

  test("inspects all_block_types golden file", async () => {
    const result = await handleInspectBcpFile({
      path: resolve(GOLDEN_DIR, "all_block_types/payload.bcp")
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text.length).toBeGreaterThan(0);
  });

  test("returns error for missing path", async () => {
    const result = await handleInspectBcpFile({});
    expect(result.isError).toBe(true);
  });

  test("returns error for nonexistent file", async () => {
    const result = await handleInspectBcpFile({ path: "/nonexistent/path.bcp" });
    expect(result.isError).toBe(true);
  });
});
