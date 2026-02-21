import { describe, test, expect } from "bun:test";
import { buildManifest } from "../../src/proxy/manifest.ts";
import type { DetectedRegion } from "../../src/proxy/detector.ts";

describe("buildManifest", () => {
  test("builds manifest from code region", () => {
    const regions: ReadonlyArray<DetectedRegion> = [{
      blockType: "code",
      content: "const x = 1;",
      confidence: 0.95,
      metadata: { lang: "typescript" }
    }];

    const manifest = buildManifest("test_tool", regions);
    expect(manifest.blocks).toHaveLength(1);
    expect(manifest.blocks[0]?.type).toBe("code");
    expect(manifest.blocks[0]?.lang).toBe("typescript");
  });

  test("builds manifest from structured_data region", () => {
    const regions: ReadonlyArray<DetectedRegion> = [{
      blockType: "structured_data",
      content: '{"key": "value"}',
      confidence: 0.9,
      metadata: { format: "json" }
    }];

    const manifest = buildManifest("test_tool", regions);
    expect(manifest.blocks).toHaveLength(1);
    expect(manifest.blocks[0]?.type).toBe("structured_data");
    expect(manifest.blocks[0]?.format).toBe("json");
  });

  test("builds manifest from tool_result region", () => {
    const regions: ReadonlyArray<DetectedRegion> = [{
      blockType: "tool_result",
      content: "plain output",
      confidence: 1.0,
      metadata: {}
    }];

    const manifest = buildManifest("my_tool", regions);
    expect(manifest.blocks).toHaveLength(1);
    expect(manifest.blocks[0]?.type).toBe("tool_result");
    expect(manifest.blocks[0]?.tool_name).toBe("my_tool");
    expect(manifest.blocks[0]?.status).toBe("ok");
  });

  test("generates summary for large content", () => {
    const longContent = "x".repeat(600);
    const regions: ReadonlyArray<DetectedRegion> = [{
      blockType: "tool_result",
      content: longContent,
      confidence: 1.0,
      metadata: {}
    }];

    const manifest = buildManifest("tool", regions);
    expect(manifest.blocks[0]?.summary).toBeTruthy();
    expect(manifest.blocks[0]?.summary).toContain("600 chars");
  });

  test("no summary for short content", () => {
    const regions: ReadonlyArray<DetectedRegion> = [{
      blockType: "tool_result",
      content: "short",
      confidence: 1.0,
      metadata: {}
    }];

    const manifest = buildManifest("tool", regions);
    expect(manifest.blocks[0]?.summary).toBeUndefined();
  });
});
