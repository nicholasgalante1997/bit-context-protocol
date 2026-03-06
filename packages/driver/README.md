# @bit-context-protocol/driver

TypeScript driver for the Bit Context Protocol — encode, decode, inspect, and validate BCP streams with pluggable backends.

> **Status:** Pre-release (0.1.0). CLI and WASM backends are implemented. Context Builder API is stable. Auto-detection prefers WASM and falls back to CLI.

## Install

```bash
bun add @bit-context-protocol/driver
```

## Quick Start

```typescript
import { createDriver } from '@bit-context-protocol/driver';

const driver = createDriver(); // auto-detects best backend

// Decode a .bcp file to model-ready text
const result = await driver.decode('./context.bcp', {
  mode: 'markdown',       // 'xml' | 'markdown' | 'minimal'
  verbosity: 'adaptive',  // 'full' | 'summary' | 'adaptive'
  tokenBudget: 8000       // low-priority blocks summarized to fit
});
console.log(result.text);

// Encode a JSON manifest into .bcp
await driver.encode('./manifest.json', './output.bcp', {
  compressBlocks: true,    // per-block zstd compression
  compressPayload: true,   // payload-level compression
  dedup: true              // content deduplication
});

// Inspect file structure without decoding
const info = await driver.inspect('./context.bcp');
console.log(`${info.blockCount} blocks, ${info.totalSize} bytes`);
for (const block of info.blocks) {
  console.log(`  [${block.index}] ${block.type} — ${block.size} bytes`);
}

// Validate file integrity
const validation = await driver.validate('./context.bcp');
if (!validation.valid) {
  console.error('Errors:', validation.errors);
}
```

## Backends

### Auto-detection (default)

When no backend is specified, the driver tries WASM first (no binary dependency), then falls back to CLI:

```typescript
const driver = createDriver(); // auto-detects
```

### CLI Backend

Uses the `bcp` binary. Must be on PATH or specified explicitly:

```typescript
const driver = createDriver({ backend: 'cli' });
const driver = createDriver({ binaryPath: '/usr/local/bin/bcp', timeoutMs: 30000 });
```

Install the binary via Cargo:

```bash
cargo install bcp-cli
```

### WASM Backend (experimental)

Runs the BCP encoder/decoder in-process via WebAssembly. No external binary required:

```typescript
const driver = createDriver({ backend: 'wasm' });
```

The WASM backend ships with the package in the `wasm/` directory.

## Context Builder

Build BCP context programmatically with typed block methods:

```typescript
import { createContextBuilder, createDriver } from '@bit-context-protocol/driver';

const builder = createContextBuilder({
  driver: createDriver(),
  description: 'Project context for code review'
});

// Add source files (language auto-detected from extension)
builder.addFile('src/index.ts', sourceCode);
builder.addFile('src/types.ts', typesCode, { priority: 'critical' });

// Add code with explicit language
builder.addCode('config/schema.ts', schemaCode, 'typescript', {
  priority: 'high',
  summary: 'Zod validation schema'
});

// Add conversation context
builder.addConversation('user', 'Implement the new auth module');
builder.addConversation('assistant', 'I will implement...', { priority: 'low' });

// Add file tree
builder.addFileTree('src/', [
  { name: 'index.ts', kind: 'file', size: 1200 },
  { name: 'types.ts', kind: 'file', size: 800 },
  { name: 'utils/', kind: 'dir', children: [
    { name: 'helpers.ts', kind: 'file', size: 400 }
  ]}
]);

// Add tool results
builder.addToolResult('git diff', diffOutput, 'ok', { priority: 'high' });

// Add documents
builder.addDocument('API Spec', specContent, {
  format: 'markdown',
  priority: 'critical'
});

// Add structured data
builder.addStructuredData('json', configJson, { priority: 'normal' });

// Add diffs
builder.addDiff('src/index.ts', [
  { old_start: 10, new_start: 10, lines: '@@ ... @@\n-old line\n+new line' }
]);

// Add annotations
builder.addAnnotation(0, 'review', 'This block needs attention');

// Build to .bcp file
const result = await builder.build({ compressBlocks: true });
console.log(`${result.blockCount} blocks -> ${result.bcpPath}`);

// Or build and decode in one step
const decoded = await builder.buildAndDecode(
  { compressBlocks: true },
  { mode: 'xml', verbosity: 'adaptive', tokenBudget: 8000 }
);
console.log(decoded.text);
```

### Block Types

| Type | Method | Description |
|------|--------|-------------|
| `code` | `addCode()` / `addFile()` | Source code with language and path |
| `conversation` | `addConversation()` | Chat messages (system/user/assistant/tool) |
| `file_tree` | `addFileTree()` | Directory structure |
| `tool_result` | `addToolResult()` | Command/tool output with status |
| `document` | `addDocument()` | Markdown, text, or other documents |
| `structured_data` | `addStructuredData()` | JSON, YAML, TOML, etc. |
| `diff` | `addDiff()` | Code diffs with hunks |
| `annotation` | `addAnnotation()` | Metadata attached to other blocks |

### Priority Levels

Every block accepts an optional `priority` that controls adaptive rendering:

| Priority | Behavior with budget |
|----------|---------------------|
| `critical` | Always included in full |
| `high` | Included in full unless budget is very tight |
| `normal` | Included or summarized based on budget |
| `low` | Summarized or omitted first |
| `background` | Omitted first when budget is constrained |

## API Reference

### `createDriver(options?): BcpDriver`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `backend` | `'cli' \| 'wasm'` | auto | Backend selection |
| `binaryPath` | `string` | `$PATH` lookup | Path to `bcp` binary (CLI only) |
| `timeoutMs` | `number` | — | CLI operation timeout |

### `BcpDriver` Methods

#### `decode(filePath, config?): Promise<DecodeResult>`

| Config Field | Type | Default | Description |
|-------------|------|---------|-------------|
| `mode` | `'xml' \| 'markdown' \| 'minimal'` | `'xml'` | Output render format |
| `targetModel` | `'claude' \| 'gpt' \| 'gemini' \| 'generic'` | — | Model-specific optimizations |
| `tokenBudget` | `number` | — | Target token count |
| `verbosity` | `'full' \| 'summary' \| 'adaptive'` | `'full'` | Rendering verbosity |
| `include` | `string[]` | — | Block type filter |

Returns `{ text: string; tokenEstimate?: number }`.

#### `encode(manifestPath, outputPath, options?): Promise<void>`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `compressBlocks` | `boolean` | `false` | Per-block zstd compression |
| `compressPayload` | `boolean` | `false` | Full payload compression |
| `dedup` | `boolean` | `false` | Content deduplication |

#### `inspect(filePath): Promise<InspectResult>`

Returns `{ blockCount, totalSize, blocks: Array<{ index, type, size, summary? }>, raw }`.

#### `validate(filePath): Promise<ValidateResult>`

Returns `{ valid: boolean; errors: string[]; raw: string }`.

#### `available(): Promise<boolean>`

Check if the selected backend is available and functional.

### `createContextBuilder(options): ContextBuilder`

| Option | Type | Description |
|--------|------|-------------|
| `driver` | `BcpDriver` | Driver instance for encoding |
| `description` | `string` | Optional manifest description |

### `ContextBuilder` Properties & Methods

| Method | Description |
|--------|-------------|
| `addCode(path, content, lang, opts?)` | Add source code block |
| `addFile(path, content, opts?)` | Add file (language auto-detected) |
| `addConversation(role, content, opts?)` | Add chat message |
| `addFileTree(root, entries, opts?)` | Add directory tree |
| `addToolResult(name, content, status?, opts?)` | Add tool output |
| `addDocument(title, content, opts?)` | Add document |
| `addStructuredData(format, content, opts?)` | Add JSON/YAML/etc. |
| `addDiff(path, hunks, opts?)` | Add code diff |
| `addAnnotation(targetBlockId, kind, value, opts?)` | Add block annotation |
| `addBlock(block)` | Add raw manifest block |
| `blockCount` | Current number of blocks |
| `toManifest()` | Get the manifest object |
| `toJSON()` | Serialize manifest as JSON string |
| `build(opts?)` | Encode to .bcp file |
| `buildAndDecode(buildOpts?, decodeConfig?)` | Encode then decode in one step |

Language auto-detection supports 30+ extensions (TypeScript, Rust, Python, Go, Java, C/C++, Ruby, and more).

## Exports

```typescript
// Main entry
import { createDriver, createContextBuilder, createCliBackend, createWasmDriver, createDriverFromBackend } from '@bit-context-protocol/driver';

// Types
import type { BcpDriver, DriverConfig, DecodeResult, InspectResult, ValidateResult, EncodeOptions, CreateDriverOptions } from '@bit-context-protocol/driver';
import type { ContextBuilder, ContextBuilderOptions, BuildOptions, BuildResult } from '@bit-context-protocol/driver';
import type { Manifest, ManifestBlock, CodeBlock, ConversationBlock, FileTreeBlock, ToolResultBlock, DocumentBlock, StructuredDataBlock, DiffBlock, AnnotationBlock } from '@bit-context-protocol/driver';
import type { Priority, Role, Status, OutputMode, ModelFamily, Verbosity } from '@bit-context-protocol/driver';

// Constants
import { OUTPUT_MODES, MODEL_FAMILIES, VERBOSITIES, PRIORITIES, ROLES, STATUSES } from '@bit-context-protocol/driver';
```

## License

MIT
