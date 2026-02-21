export type DetectedRegion = {
  blockType: "code" | "file_tree" | "structured_data" | "diff" | "tool_result";
  content: string;
  confidence: number;
  metadata: {
    lang?: string;
    path?: string;
    format?: string;
  };
};

const DIFF_HEADER = /^---\s+\S+[\s\S]*?\n\+\+\+\s+\S+[\s\S]*?\n@@/m;
const FENCED_CODE = /^```(\w+)?\n([\s\S]*?)^```$/m;
const TREE_LINE = /^[\s│├└─]+[\w./-]+/m;
const FILE_EXTENSION = /\.\w{1,10}$/;

const detectDiff = (text: string): DetectedRegion | null => {
  if (DIFF_HEADER.test(text)) {
    return { blockType: "diff", content: text, confidence: 0.95, metadata: {} };
  }
  return null;
};

const detectFencedCode = (text: string): DetectedRegion | null => {
  const match = FENCED_CODE.exec(text);
  if (match) {
    const lang = match[1];
    const content = match[2] ?? text;
    return {
      blockType: "code",
      content,
      confidence: lang ? 0.95 : 0.7,
      metadata: { lang }
    };
  }
  return null;
};

const detectJson = (text: string): DetectedRegion | null => {
  const trimmed = text.trim();
  if ((trimmed.startsWith("{") && trimmed.endsWith("}")) ||
      (trimmed.startsWith("[") && trimmed.endsWith("]"))) {
    try {
      JSON.parse(trimmed);
      return {
        blockType: "structured_data",
        content: trimmed,
        confidence: 0.9,
        metadata: { format: "json" }
      };
    } catch {
      return null;
    }
  }
  return null;
};

const detectYaml = (text: string): DetectedRegion | null => {
  const trimmed = text.trim();
  if (trimmed.startsWith("---\n") || /^\w+:\s+.+$/m.test(trimmed)) {
    const lines = trimmed.split("\n");
    const yamlLines = lines.filter((l) => /^\s*[\w.-]+:\s/.test(l) || /^\s*-\s/.test(l));
    if (yamlLines.length >= 2 && yamlLines.length / lines.length > 0.3) {
      return {
        blockType: "structured_data",
        content: trimmed,
        confidence: 0.75,
        metadata: { format: "yaml" }
      };
    }
  }
  return null;
};

const detectToml = (text: string): DetectedRegion | null => {
  const trimmed = text.trim();
  if (/^\[[\w.-]+\]$/m.test(trimmed)) {
    const lines = trimmed.split("\n");
    const tomlLines = lines.filter((l) => /^\[[\w.-]+\]$/.test(l.trim()) || /^\w+\s*=/.test(l.trim()));
    if (tomlLines.length >= 2 && tomlLines.length / lines.length > 0.3) {
      return {
        blockType: "structured_data",
        content: trimmed,
        confidence: 0.75,
        metadata: { format: "toml" }
      };
    }
  }
  return null;
};

const detectFileTree = (text: string): DetectedRegion | null => {
  const lines = text.trim().split("\n");
  const treeLines = lines.filter((l) => TREE_LINE.test(l) && FILE_EXTENSION.test(l.trim()));
  if (treeLines.length >= 3 && treeLines.length / lines.length > 0.5) {
    return {
      blockType: "file_tree",
      content: text.trim(),
      confidence: 0.85,
      metadata: {}
    };
  }
  return null;
};

const DETECTORS = [
  detectDiff,
  detectFencedCode,
  detectJson,
  detectFileTree,
  detectYaml,
  detectToml
] as const;

export const detectContent = (
  text: string,
  threshold: number
): ReadonlyArray<DetectedRegion> => {
  for (const detector of DETECTORS) {
    const result = detector(text);
    if (result && result.confidence >= threshold) {
      return [result];
    }
  }

  return [{
    blockType: "tool_result",
    content: text,
    confidence: 1.0,
    metadata: {}
  }];
};
