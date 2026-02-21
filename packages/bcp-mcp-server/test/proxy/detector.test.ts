import { describe, test, expect } from "bun:test";
import { detectContent } from "../../src/proxy/detector.ts";

describe("detectContent", () => {
  test("detects unified diff", () => {
    const text = `--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,3 @@
-fn old() {}
+fn new() {}`;
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("diff");
    expect(regions[0]?.confidence).toBeGreaterThanOrEqual(0.95);
  });

  test("detects fenced code block with language", () => {
    const text = "```typescript\nconst x = 1;\n```";
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("code");
    expect(regions[0]?.metadata.lang).toBe("typescript");
    expect(regions[0]?.confidence).toBeGreaterThanOrEqual(0.95);
  });

  test("detects fenced code block without language", () => {
    const text = "```\nsome code\n```";
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("code");
    expect(regions[0]?.confidence).toBe(0.7);
  });

  test("detects valid JSON", () => {
    const text = '{"name": "test", "version": "1.0.0"}';
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("structured_data");
    expect(regions[0]?.metadata.format).toBe("json");
    expect(regions[0]?.confidence).toBe(0.9);
  });

  test("detects JSON array", () => {
    const text = '[1, 2, 3, "test"]';
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("structured_data");
  });

  test("falls back to tool_result for plain text", () => {
    const text = "Just some regular text output from a tool.";
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("tool_result");
  });

  test("respects confidence threshold", () => {
    const text = "```\nsome code\n```";
    const regions = detectContent(text, 0.8);
    expect(regions).toHaveLength(1);
    // Bare fenced block has confidence 0.7, below 0.8 threshold
    expect(regions[0]?.blockType).toBe("tool_result");
  });

  test("detects invalid JSON as tool_result", () => {
    const text = '{"broken json';
    const regions = detectContent(text, 0.7);
    expect(regions).toHaveLength(1);
    expect(regions[0]?.blockType).toBe("tool_result");
  });
});
