import { describe, test, expect, afterEach } from "bun:test";
import { handleEncodeBcpFile } from "../../src/handlers/encode-bcp-file.ts";
import { resolve } from "node:path";
import { unlinkSync } from "node:fs";
import { tmpdir } from "node:os";

const GOLDEN_DIR = resolve(import.meta.dir, "../../../..", "crates/bcp-tests/tests/golden");

const tempFiles: Array<string> = [];

const tempPath = (): string => {
  const path = resolve(tmpdir(), `bcp-test-${crypto.randomUUID()}.bcp`);
  tempFiles.push(path);
  return path;
};

afterEach(() => {
  for (const f of tempFiles) {
    try { unlinkSync(f); } catch { /* ignore */ }
  }
  tempFiles.length = 0;
});

describe("handleEncodeBcpFile", () => {
  test("encodes from golden manifest", async () => {
    const output = tempPath();
    const result = await handleEncodeBcpFile({
      manifest_path: resolve(GOLDEN_DIR, "simple_code/manifest.json"),
      output_path: output
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text).toContain("Encoded successfully");

    const file = Bun.file(output);
    expect(await file.exists()).toBe(true);
    expect(file.size).toBeGreaterThan(0);
  });

  test("encodes with compression", async () => {
    const output = tempPath();
    const result = await handleEncodeBcpFile({
      manifest_path: resolve(GOLDEN_DIR, "simple_code/manifest.json"),
      output_path: output,
      compress: true
    });
    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text).toContain("Encoded successfully");
  });

  test("returns error for missing manifest_path", async () => {
    const result = await handleEncodeBcpFile({ output_path: "/tmp/out.bcp" });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain("manifest_path");
  });

  test("returns error for missing output_path", async () => {
    const result = await handleEncodeBcpFile({
      manifest_path: resolve(GOLDEN_DIR, "simple_code/manifest.json")
    });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain("output_path");
  });

  test("returns error for nonexistent manifest", async () => {
    const result = await handleEncodeBcpFile({
      manifest_path: "/nonexistent/manifest.json",
      output_path: "/tmp/out.bcp"
    });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toContain("not found");
  });
});
