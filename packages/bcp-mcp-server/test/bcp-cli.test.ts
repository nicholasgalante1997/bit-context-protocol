import { describe, test, expect } from "bun:test";
import { runBcpCli, validateBcpAvailable } from "../src/bcp-cli.ts";
import { resolve } from "node:path";

const GOLDEN_DIR = resolve(import.meta.dir, "../../..", "crates/bcp-tests/tests/golden");

describe("bcp-cli", () => {
  test("bcp --version returns exit code 0", async () => {
    const result = await runBcpCli(["--version"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("bcp");
  });

  test("validateBcpAvailable returns true", async () => {
    const available = await validateBcpAvailable();
    expect(available).toBe(true);
  });

  test("bcp decode works on golden file", async () => {
    const file = resolve(GOLDEN_DIR, "simple_code/payload.bcp");
    const result = await runBcpCli(["decode", file, "--mode", "xml"]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("code");
    expect(result.stdout).toContain("rust");
  });

  test("bcp inspect works on golden file", async () => {
    const file = resolve(GOLDEN_DIR, "simple_code/payload.bcp");
    const result = await runBcpCli(["inspect", file]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout.toLowerCase()).toContain("block");
  });

  test("bcp decode on nonexistent file returns error", async () => {
    const result = await runBcpCli(["decode", "/nonexistent/path.bcp"]);
    expect(result.exitCode).not.toBe(0);
  });
});
