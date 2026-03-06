import { describe, test, expect } from "bun:test";
import { handleReadBcpFile } from "../../src/handlers/read-bcp-file.ts";
import { resolve } from "node:path";

const GOLDEN_DIR = resolve(import.meta.dir, "../../../..", "crates/bcp-tests/tests/golden");

describe("handleReadBcpFile", () => {
  test("decodes simple_code in xml mode", async () => {
    const result = await handleReadBcpFile({
      path: resolve(GOLDEN_DIR, "simple_code/payload.bcp"),
      mode: "xml"
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text).toContain("code");
    expect(result.content[0]?.text).toContain("rust");
  });

  test("decodes in markdown mode", async () => {
    const result = await handleReadBcpFile({
      path: resolve(GOLDEN_DIR, "simple_code/payload.bcp"),
      mode: "markdown"
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text).toContain("rust");
  });

  test("decodes in minimal mode", async () => {
    const result = await handleReadBcpFile({
      path: resolve(GOLDEN_DIR, "simple_code/payload.bcp"),
      mode: "minimal"
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text).toContain("rust");
  });

  test("passes budget parameter", async () => {
    const result = await handleReadBcpFile({
      path: resolve(GOLDEN_DIR, "budget_constrained/payload.bcp"),
      budget: 200
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text.length).toBeGreaterThan(0);
  });

  test("returns error for missing path", async () => {
    const result = await handleReadBcpFile({});
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain("path");
  });

  test("returns error for non-.bcp file", async () => {
    const result = await handleReadBcpFile({ path: "/tmp/test.txt" });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain(".bcp");
  });

  test("returns error for nonexistent file", async () => {
    const result = await handleReadBcpFile({ path: "/nonexistent/path.bcp" });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain("not found");
  });
});
