import { writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { randomBytes } from 'node:crypto';
import type { BcpDriver, EncodeOptions, DriverConfig, DecodeResult } from './types.ts';
import type {
  ManifestBlock,
  Manifest,
  Priority,
  Role,
  Status,
  FileTreeEntry,
  DiffHunk
} from './manifest.ts';

export interface ContextBuilderOptions {
  description?: string;
  driver: BcpDriver;
}

export interface BuildOptions extends EncodeOptions {
  outputPath?: string;
}

export interface BuildResult {
  bcpPath: string;
  blockCount: number;
  manifestPath: string;
}

const tmpPath = (ext: string): string => {
  const id = randomBytes(8).toString('hex');
  return join(tmpdir(), `bcp-${id}${ext}`);
};

export const createContextBuilder = (options: ContextBuilderOptions) => {
  const blocks: ManifestBlock[] = [];
  const description = options.description;
  const driver = options.driver;

  const self = {
    addCode(
      path: string,
      content: string,
      lang: string,
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'code',
        lang,
        path,
        content,
        ...spreadOptional(opts)
      });
      return self;
    },

    addFile(
      path: string,
      content: string,
      opts?: { lang?: string; priority?: Priority; summary?: string }
    ) {
      const lang = opts?.lang ?? inferLang(path);
      blocks.push({
        type: 'code',
        lang,
        path,
        content,
        ...spreadOptional({ priority: opts?.priority, summary: opts?.summary })
      });
      return self;
    },

    addConversation(
      role: Role,
      content: string,
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'conversation',
        role,
        content,
        ...spreadOptional(opts)
      });
      return self;
    },

    addFileTree(
      root: string,
      entries: FileTreeEntry[],
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'file_tree',
        root,
        entries,
        ...spreadOptional(opts)
      });
      return self;
    },

    addToolResult(
      name: string,
      content: string,
      status?: Status,
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'tool_result',
        name,
        content,
        ...(status ? { status } : {}),
        ...spreadOptional(opts)
      });
      return self;
    },

    addDocument(
      title: string,
      content: string,
      opts?: { format?: string; priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'document',
        title,
        content,
        ...(opts?.format ? { format: opts.format } : {}),
        ...spreadOptional({ priority: opts?.priority, summary: opts?.summary })
      });
      return self;
    },

    addStructuredData(
      format: string,
      content: string,
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'structured_data',
        format,
        content,
        ...spreadOptional(opts)
      });
      return self;
    },

    addDiff(
      path: string,
      hunks: DiffHunk[],
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'diff',
        path,
        hunks,
        ...spreadOptional(opts)
      });
      return self;
    },

    addAnnotation(
      targetBlockId: number,
      kind: string,
      value: string,
      opts?: { priority?: Priority; summary?: string }
    ) {
      blocks.push({
        type: 'annotation',
        target_block_id: targetBlockId,
        kind,
        value,
        ...spreadOptional(opts)
      });
      return self;
    },

    addBlock(block: ManifestBlock) {
      blocks.push(block);
      return self;
    },

    get blockCount() {
      return blocks.length;
    },

    toManifest(): Manifest {
      return {
        ...(description ? { description } : {}),
        blocks: [...blocks]
      };
    },

    toJSON(): string {
      return JSON.stringify(self.toManifest(), null, 2);
    },

    async build(opts?: BuildOptions): Promise<BuildResult> {
      const manifest = self.toManifest();
      const manifestPath = tmpPath('.json');
      const bcpPath = opts?.outputPath ?? tmpPath('.bcp');

      mkdirSync(dirname(manifestPath), { recursive: true });
      mkdirSync(dirname(bcpPath), { recursive: true });

      writeFileSync(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');

      await driver.encode(manifestPath, bcpPath, {
        compressBlocks: opts?.compressBlocks,
        compressPayload: opts?.compressPayload,
        dedup: opts?.dedup
      });

      return {
        bcpPath,
        blockCount: blocks.length,
        manifestPath
      };
    },

    async buildAndDecode(
      buildOpts?: BuildOptions,
      decodeConfig?: DriverConfig
    ): Promise<DecodeResult & { bcpPath: string; blockCount: number }> {
      const { bcpPath, blockCount } = await self.build(buildOpts);
      const decoded = await driver.decode(bcpPath, decodeConfig);
      return { ...decoded, bcpPath, blockCount };
    }
  };

  return self;
};

export type ContextBuilder = ReturnType<typeof createContextBuilder>;

// --- Helpers ---

const spreadOptional = (
  opts?: { priority?: Priority; summary?: string }
): Record<string, unknown> => {
  if (!opts) return {};
  const out: Record<string, unknown> = {};
  if (opts.priority) out['priority'] = opts.priority;
  if (opts.summary) out['summary'] = opts.summary;
  return out;
};

const LANG_MAP: Record<string, string> = {
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  rs: 'rust',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  java: 'java',
  c: 'c',
  cpp: 'cpp',
  h: 'c',
  hpp: 'cpp',
  cs: 'csharp',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  yml: 'yaml',
  yaml: 'yaml',
  json: 'json',
  toml: 'toml',
  md: 'markdown',
  css: 'css',
  html: 'html',
  sql: 'sql',
  zig: 'zig',
  swift: 'swift',
  kt: 'kotlin',
  lua: 'lua',
  ex: 'elixir',
  exs: 'elixir',
  erl: 'erlang',
  hs: 'haskell',
  ml: 'ocaml',
  r: 'r',
  php: 'php',
  dart: 'dart',
  scala: 'scala',
  dockerfile: 'dockerfile'
};

const inferLang = (path: string): string => {
  const lower = path.toLowerCase();

  // Handle dotfiles and special names
  const basename = lower.split('/').pop() ?? '';
  if (basename === 'dockerfile') return 'dockerfile';
  if (basename === 'makefile') return 'makefile';

  const dotIndex = basename.lastIndexOf('.');
  if (dotIndex === -1 || dotIndex === 0) return 'text';
  const ext = basename.slice(dotIndex + 1);
  return LANG_MAP[ext] ?? (ext || 'text');
};
