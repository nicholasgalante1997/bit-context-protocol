# SPEC_11 — MCP Server Adapter

**Package**: `bcp-mcp-server`
**Runtime**: Bun (TypeScript)
**Phase**: 5 (Ecosystem Integration)
**Prerequisites**: SPEC_01 through SPEC_10 (complete BCP reference implementation)
**Dependencies**: `bcp-cli` binary (subprocess for encode/decode), Bun runtime ≥1.3.x

---

## Context

RFC §9.2 identifies the MCP server adapter as a key Phase 2 deliverable:
"Build an MCP server adapter: automatically wraps MCP tool results in BCP blocks."
This is the adoption wedge for BCP — a transparent proxy that sits between any
MCP client (Claude Code, Cursor, etc.) and any MCP server (ripgrep, filesystem,
database tools), compressing tool results into BCP blocks before returning them
to the client. The result: identical semantic content in fewer tokens.

The server implements the [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-11-25)
(spec revision 2025-11-25) directly over JSON-RPC 2.0 on the stdio transport.
**No existing MCP SDK or framework is used** — the JSON-RPC layer, capability
negotiation, and message routing are implemented from scratch to keep
dependencies minimal and to demonstrate that BCP can integrate with MCP at the
protocol level.

Implementation proceeds in two sub-phases:

1. **Sub-phase A — BCP File Reader**: A standalone MCP server exposing a
   `read_bcp_file` tool. Proves the consumption side: any MCP client can read
   `.bcp` files and receive rendered text. Useful immediately.

2. **Sub-phase B — Proxy with Content Detection**: The full transparent proxy.
   Forwards requests to a downstream MCP server, intercepts tool results,
   heuristically detects content types (code, file trees, structured data),
   wraps them in BCP blocks, and returns the compressed payload rendered as
   text.

---

## Architecture

```
Sub-phase A — BCP File Reader:

  MCP Client ◄──stdio──► bcp-mcp-server
  (Claude Code)               │
                              ▼
                     read_bcp_file(path)
                              │
                              ▼
                     bcp cli decode ──► rendered text
                              │
                              ▼
                     TextContent response


Sub-phase B — Transparent Proxy:

  MCP Client ◄──stdio──► bcp-mcp-server ──stdio──► Downstream MCP Server
  (Claude Code)               │                     (ripgrep, fs, etc.)
                              │
                              ▼
                     Tool result arrives as JSON
                              │
                              ▼
                     ┌────────────────────────┐
                     │   Content Detector     │
                     │                        │
                     │  Code?  ──► CODE block │
                     │  Tree?  ──► FILE_TREE  │
                     │  JSON?  ──► STRUCT_DATA│
                     │  Diff?  ──► DIFF block │
                     │  Other? ──► TOOL_RESULT│
                     └────────────────────────┘
                              │
                              ▼
                     bcp cli encode ──► .bcp binary
                              │
                              ▼
                     bcp cli decode --mode xml
                              │
                              ▼
                     TextContent response (compressed)
```

---

## Design Decisions

### DD-11-01: No MCP SDK — Raw JSON-RPC 2.0

**Question**: Should we use the official `@modelcontextprotocol/sdk` package?

**Option A**: Use `@modelcontextprotocol/sdk`
- Pros: Faster development, handles protocol edge cases. ⭐⭐⭐⭐
- Cons: Heavy dependency tree, obscures protocol mechanics, limits control over message handling. ⭐⭐

**Option B**: Implement JSON-RPC 2.0 from scratch over stdio
- Pros: Zero dependencies, full protocol understanding, minimal binary size, demonstrates BCP is framework-agnostic. ⭐⭐⭐⭐⭐
- Cons: Must handle protocol edge cases manually. ⭐⭐⭐

**Decision**: **Option B — Raw JSON-RPC 2.0**

**Rationale**: The MCP stdio transport is newline-delimited JSON-RPC 2.0 — a
trivial protocol to implement. The server only needs to handle `initialize`,
`tools/list`, and `tools/call` methods. Building from scratch keeps the
dependency count at zero (Bun built-ins only) and proves that BCP integration
requires no special framework.

### DD-11-02: BCP Encode/Decode via CLI Subprocess

**Question**: How should the TypeScript server encode/decode BCP payloads?

**Option A**: Compile `bcp-encoder`/`bcp-decoder` to WASM
- Pros: In-process, fast, no subprocess overhead. ⭐⭐⭐⭐
- Cons: WASM compilation not yet set up, adds build complexity, Rust 2024 edition WASM support is experimental. ⭐⭐

**Option B**: Shell out to `bcp` CLI binary
- Pros: Already built and tested, zero new Rust code, validates the CLI as an integration point. ⭐⭐⭐⭐⭐
- Cons: Subprocess overhead (~5-10ms per invocation), requires `bcp` binary on PATH. ⭐⭐⭐

**Option C**: Implement a minimal TypeScript BCP decoder
- Pros: No Rust dependency, pure TypeScript. ⭐⭐⭐
- Cons: Duplicates logic, must be kept in sync with Rust implementation, not a conformance-tested decoder. ⭐

**Decision**: **Option B — CLI subprocess**

**Rationale**: The `bcp` CLI already handles encode, decode, and all render
modes. Subprocess overhead is negligible for MCP tool calls (which are
inherently I/O-bound). This validates the CLI as the universal integration
point for non-Rust consumers. WASM bindings are a natural follow-up for
Phase 3.

### DD-11-03: Content Detection Strategy

**Question**: How should the proxy identify content types within tool results?

**Option A**: Regex-based heuristics
- Pros: Simple, no dependencies, covers common patterns. ⭐⭐⭐⭐
- Cons: Fragile for edge cases, may misclassify. ⭐⭐⭐

**Option B**: Tree-sitter parsing for code detection
- Pros: Accurate language detection. ⭐⭐⭐⭐⭐
- Cons: Heavy dependency, slow for large outputs. ⭐⭐

**Option C**: Pattern matching with confidence scores
- Pros: Balances accuracy and speed, can fall back to TOOL_RESULT for low confidence. ⭐⭐⭐⭐⭐
- Cons: Slightly more complex than pure regex. ⭐⭐⭐⭐

**Decision**: **Option C — Pattern matching with confidence scores**

**Rationale**: The detector runs a series of lightweight pattern checks (fenced
code blocks, file path patterns, JSON/YAML structure, unified diff headers)
and assigns a confidence score. Above a threshold, the content is wrapped in
the specific block type. Below threshold, it falls back to a generic
TOOL_RESULT block. This avoids both false positives (misclassifying plain
text as code) and false negatives (missing obvious code blocks). The
confidence threshold is configurable.

---

## Requirements

### 1. JSON-RPC 2.0 Transport Layer

The transport layer handles newline-delimited JSON-RPC 2.0 over stdio, per the
[MCP stdio transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports#stdio).

```
Message framing (stdio):

  ┌─────────────────────────────────────────────────┐
  │ {"jsonrpc":"2.0","id":1,"method":"initialize"…} │
  │ \n                                              │
  │ {"jsonrpc":"2.0","method":"notifications/init…} │
  │ \n                                              │
  └─────────────────────────────────────────────────┘

  - Messages are newline-delimited (\n)
  - Messages MUST NOT contain embedded newlines
  - All messages are UTF-8 encoded JSON
  - Server reads from stdin, writes to stdout
  - Logging/debug output goes to stderr only
```

```typescript
// JSON-RPC 2.0 message types

type JsonRpcRequest = {
  jsonrpc: "2.0";
  id: string | number;
  method: string;
  params?: Record<string, unknown>;
};

type JsonRpcResponse = {
  jsonrpc: "2.0";
  id: string | number;
  result?: unknown;
  error?: JsonRpcError;
};

type JsonRpcNotification = {
  jsonrpc: "2.0";
  method: string;
  params?: Record<string, unknown>;
};

type JsonRpcError = {
  code: number;
  message: string;
  data?: unknown;
};
```

```typescript
// Transport: read newline-delimited JSON from stdin, write to stdout

async function* readMessages(
  reader: ReadableStream<Uint8Array>
): AsyncGenerator<JsonRpcRequest | JsonRpcNotification> {
  // Buffer incoming bytes, split on \n, parse each line as JSON
  // Yield parsed JSON-RPC messages
  // Emit parse errors to stderr, do not crash
}

function sendMessage(message: JsonRpcResponse | JsonRpcNotification): void {
  // JSON.stringify (no embedded newlines) + \n to stdout
}
```

### 2. MCP Lifecycle — Initialize / Initialized

The server implements the [MCP initialization handshake](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle):

```
Handshake sequence:

  Client ──► initialize request (protocolVersion, capabilities, clientInfo)
  Server ◄── initialize response (protocolVersion, capabilities, serverInfo)
  Client ──► notifications/initialized
  Server:    enters operation phase
```

```typescript
// Server capabilities declaration

const SERVER_INFO = {
  name: "bcp-mcp-server",
  version: "0.1.0"
} as const;

const SERVER_CAPABILITIES = {
  tools: {
    listChanged: false
  }
} as const;

// Handle initialize request
function handleInitialize(params: {
  protocolVersion: string;
  capabilities: Record<string, unknown>;
  clientInfo: { name: string; version: string };
}): {
  protocolVersion: string;
  capabilities: typeof SERVER_CAPABILITIES;
  serverInfo: typeof SERVER_INFO;
} {
  // Version negotiation:
  //   - If client requests "2025-11-25", respond with "2025-11-25"
  //   - If client requests an older version, respond with "2025-11-25"
  //     (client may disconnect if unsupported)
  //   - Store negotiated version for the session
}
```

### 3. Tool Definitions

The server exposes tools via `tools/list`. Sub-phase A exposes one tool;
Sub-phase B adds the proxy's forwarded tools.

```typescript
// Sub-phase A tools

const TOOLS = [
  {
    name: "read_bcp_file",
    description:
      "Read and decode a .bcp (Bit Context Protocol) file, returning " +
      "the rendered content as model-ready text. Supports XML, Markdown, " +
      "and Minimal output modes.",
    inputSchema: {
      type: "object",
      properties: {
        path: {
          type: "string",
          description: "Absolute path to the .bcp file"
        },
        mode: {
          type: "string",
          description: "Output render mode",
          enum: ["xml", "markdown", "minimal"],
          default: "xml"
        },
        budget: {
          type: "number",
          description:
            "Optional token budget. When set, low-priority blocks " +
            "are summarized to fit within the budget."
        }
      },
      required: ["path"]
    }
  },
  {
    name: "inspect_bcp_file",
    description:
      "Inspect a .bcp file and return a summary of its blocks, sizes, " +
      "and structure without rendering the full content.",
    inputSchema: {
      type: "object",
      properties: {
        path: {
          type: "string",
          description: "Absolute path to the .bcp file"
        }
      },
      required: ["path"]
    }
  },
  {
    name: "encode_bcp_file",
    description:
      "Encode a JSON manifest into a .bcp file. The manifest describes " +
      "blocks to include (code, conversation, tool results, etc.).",
    inputSchema: {
      type: "object",
      properties: {
        manifest_path: {
          type: "string",
          description: "Absolute path to the JSON manifest file"
        },
        output_path: {
          type: "string",
          description: "Absolute path for the output .bcp file"
        },
        compress: {
          type: "boolean",
          description: "Enable per-block zstd compression",
          default: false
        }
      },
      required: ["manifest_path", "output_path"]
    }
  }
] as const;
```

### 4. Tool Execution — `read_bcp_file`

```typescript
// Execute bcp cli decode as subprocess

async function readBcpFile(args: {
  path: string;
  mode?: "xml" | "markdown" | "minimal";
  budget?: number;
}): Promise<{ content: Array<{ type: "text"; text: string }>; isError: boolean }> {
  // 1. Validate path exists and has .bcp extension
  // 2. Build command: bcp decode <path> --mode <mode> [--budget <N> --verbosity adaptive]
  // 3. Spawn subprocess via Bun.spawn()
  // 4. Capture stdout (rendered text) and stderr (errors)
  // 5. Return TextContent result or isError: true with stderr message
}
```

### 5. Tool Execution — `inspect_bcp_file`

```typescript
async function inspectBcpFile(args: {
  path: string;
}): Promise<{ content: Array<{ type: "text"; text: string }>; isError: boolean }> {
  // 1. Validate path exists and has .bcp extension
  // 2. Build command: bcp inspect <path>
  // 3. Spawn subprocess, capture stdout
  // 4. Return TextContent with block summary
}
```

### 6. Tool Execution — `encode_bcp_file`

```typescript
async function encodeBcpFile(args: {
  manifest_path: string;
  output_path: string;
  compress?: boolean;
}): Promise<{ content: Array<{ type: "text"; text: string }>; isError: boolean }> {
  // 1. Validate manifest_path exists
  // 2. Build command: bcp encode <manifest_path> -o <output_path> [--compress-blocks]
  // 3. Spawn subprocess, capture stdout/stderr
  // 4. On success: return TextContent confirming file written + bcp stats output
  // 5. On failure: return isError: true with diagnostic
}
```

### 7. Message Router

```typescript
// Central dispatcher for all JSON-RPC methods

async function handleMessage(
  message: JsonRpcRequest | JsonRpcNotification
): Promise<JsonRpcResponse | null> {
  // Notifications return null (no response)
  // Requests return a JsonRpcResponse

  switch (message.method) {
    case "initialize":
      return { jsonrpc: "2.0", id: message.id, result: handleInitialize(message.params) };

    case "notifications/initialized":
      // Mark session as ready, return null (notification)
      return null;

    case "tools/list":
      return { jsonrpc: "2.0", id: message.id, result: { tools: TOOLS } };

    case "tools/call":
      return { jsonrpc: "2.0", id: message.id, result: await handleToolCall(message.params) };

    case "ping":
      return { jsonrpc: "2.0", id: message.id, result: {} };

    default:
      // Unknown method → JSON-RPC method not found error (-32601)
      return {
        jsonrpc: "2.0",
        id: (message as JsonRpcRequest).id,
        error: { code: -32601, message: `Method not found: ${message.method}` }
      };
  }
}
```

### 8. BCP CLI Integration Layer

```typescript
// Subprocess wrapper for the bcp CLI binary

async function runBcpCli(
  args: Array<string>
): Promise<{ stdout: string; stderr: string; exitCode: number }> {
  // 1. Resolve bcp binary path (PATH lookup or BCP_CLI_PATH env var)
  // 2. Spawn: Bun.spawn(["bcp", ...args])
  // 3. Collect stdout and stderr as text
  // 4. Return { stdout, stderr, exitCode }
  // 5. Timeout after 30 seconds (configurable via BCP_CLI_TIMEOUT_MS)
}
```

### 9. Sub-phase B — Proxy Transport

The proxy spawns a downstream MCP server as a child process and forwards
messages bidirectionally.

```
Proxy message flow:

  Client stdin ──► bcp-mcp-server ──► Downstream stdin
                        │
  Client stdout ◄──     │         ◄── Downstream stdout
                        │
                   intercept tools/call responses
                   detect content types
                   wrap in BCP blocks
                   render and return
```

```typescript
// Proxy configuration (via environment variables or CLI flags)

type ProxyConfig = {
  downstream: {
    command: string;       // e.g. "npx" or "uvx"
    args: Array<string>;   // e.g. ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
  };
  detection: {
    enabled: boolean;           // Enable content detection (default: true)
    confidenceThreshold: number; // 0.0-1.0, default: 0.7
  };
  render: {
    mode: "xml" | "markdown" | "minimal";  // Default: "xml"
    budget?: number;                        // Optional token budget
  };
};
```

```typescript
// Proxy lifecycle

class McpProxy {
  private downstream: Subprocess;
  private pendingRequests: Map<string | number, { resolve: Function; reject: Function }>;

  async start(config: ProxyConfig): Promise<void> {
    // 1. Spawn downstream server: Bun.spawn(config.downstream.command, config.downstream.args)
    // 2. Pipe downstream stdout through message reader
    // 3. Forward initialize/initialized to downstream, merge capabilities
  }

  async forwardToolCall(params: {
    name: string;
    arguments: Record<string, unknown>;
  }): Promise<ToolResult> {
    // 1. Send tools/call to downstream via its stdin
    // 2. Await response from downstream stdout
    // 3. Run content detection on result
    // 4. If BCP-wrappable: encode → decode → return compressed text
    // 5. If not wrappable or below confidence: pass through unchanged
  }

  mergeToolLists(
    localTools: Array<Tool>,
    downstreamTools: Array<Tool>
  ): Array<Tool> {
    // Combine local BCP tools with downstream tools
    // Prefix downstream tools if namespace collision occurs
  }
}
```

### 10. Content Detection Engine

The detector analyzes tool result text and identifies BCP-wrappable content
regions.

```
Detection patterns:

  ┌──────────────────────┬────────────────────────────────────────┐
  │ Pattern              │ Detection Rule                         │
  ├──────────────────────┼────────────────────────────────────────┤
  │ Fenced code block    │ ```lang\n...\n``` → CODE block         │
  │                      │ Confidence: 0.95 if lang present,      │
  │                      │ 0.7 if bare ```                        │
  ├──────────────────────┼────────────────────────────────────────┤
  │ File path + content  │ Lines matching path patterns followed  │
  │                      │ by code → CODE block with path         │
  │                      │ Confidence: 0.8                        │
  ├──────────────────────┼────────────────────────────────────────┤
  │ Directory listing    │ Tree-like indentation with file        │
  │                      │ extensions → FILE_TREE block           │
  │                      │ Confidence: 0.85                       │
  ├──────────────────────┼────────────────────────────────────────┤
  │ JSON/YAML/TOML       │ Starts with { or [ (JSON), or has     │
  │                      │ YAML frontmatter/indentation, or       │
  │                      │ TOML [section] headers                 │
  │                      │ → STRUCTURED_DATA block                │
  │                      │ Confidence: 0.9 for valid JSON,        │
  │                      │ 0.75 for YAML/TOML heuristic           │
  ├──────────────────────┼────────────────────────────────────────┤
  │ Unified diff         │ Lines starting with --- a/, +++ b/,    │
  │                      │ @@ → DIFF block                        │
  │                      │ Confidence: 0.95                       │
  ├──────────────────────┼────────────────────────────────────────┤
  │ Plain text           │ No patterns matched above threshold    │
  │                      │ → TOOL_RESULT block (passthrough)      │
  │                      │ Confidence: N/A                        │
  └──────────────────────┴────────────────────────────────────────┘
```

```typescript
type DetectedRegion = {
  blockType: "code" | "file_tree" | "structured_data" | "diff" | "tool_result";
  content: string;
  confidence: number;
  metadata: {
    lang?: string;       // Detected language for code blocks
    path?: string;       // File path if detected
    format?: string;     // json | yaml | toml for structured data
  };
};

function detectContent(text: string): Array<DetectedRegion> {
  // 1. Split text into candidate regions (fenced blocks, contiguous sections)
  // 2. Run pattern matchers on each region
  // 3. Assign confidence scores
  // 4. Return regions sorted by position in original text
  // 5. Unmatched regions become tool_result with the original text
}
```

### 11. Manifest Generation for Proxy Encoding

When the proxy detects content types, it builds a JSON manifest compatible
with `bcp encode` and passes it to the CLI.

```typescript
type BcpManifest = {
  blocks: Array<{
    type: string;
    lang?: string;
    path?: string;
    format?: string;
    role?: string;
    tool_name?: string;
    status?: string;
    content: string;
    summary?: string;
  }>;
};

function buildManifest(
  toolName: string,
  regions: Array<DetectedRegion>
): BcpManifest {
  // Map detected regions to BCP block definitions
  // Wrap remaining text as TOOL_RESULT with tool_name
  // Generate summaries for large blocks (>500 chars)
}
```

### 12. Entry Point

```typescript
// Main entry point: bcp-mcp-server

async function main(): Promise<void> {
  // 1. Parse mode from argv: --mode reader | --mode proxy
  // 2. If proxy mode, parse downstream config from argv/env
  // 3. Initialize message reader on process.stdin
  // 4. Enter message loop:
  //    for await (const message of readMessages(Bun.stdin.stream()))
  //      const response = await handleMessage(message);
  //      if (response) sendMessage(response);
  // 5. On stdin close, clean up (kill downstream if proxy mode)
}
```

---

## File Structure

```
packages/bcp-mcp-server/
├── package.json
├── tsconfig.json
├── src/
│   ├── index.ts              # Entry point, stdin/stdout message loop
│   ├── transport.ts          # JSON-RPC 2.0 reader/writer over stdio
│   ├── router.ts             # Method dispatcher (initialize, tools/list, tools/call)
│   ├── lifecycle.ts          # MCP initialize/initialized handshake
│   ├── tools.ts              # Tool definitions (read_bcp_file, inspect, encode)
│   ├── handlers/
│   │   ├── read-bcp-file.ts  # read_bcp_file tool handler
│   │   ├── inspect-bcp-file.ts # inspect_bcp_file tool handler
│   │   └── encode-bcp-file.ts  # encode_bcp_file tool handler
│   ├── bcp-cli.ts            # Subprocess wrapper for bcp binary
│   ├── proxy/
│   │   ├── proxy.ts          # McpProxy class (Sub-phase B)
│   │   ├── detector.ts       # Content detection engine
│   │   ├── manifest.ts       # BCP manifest builder from detected regions
│   │   └── config.ts         # Proxy configuration parsing
│   └── types.ts              # Shared type definitions (JSON-RPC, MCP, etc.)
└── test/
    ├── transport.test.ts     # JSON-RPC message parsing tests
    ├── router.test.ts        # Method dispatch tests
    ├── lifecycle.test.ts     # Initialize handshake tests
    ├── handlers/
    │   ├── read-bcp-file.test.ts
    │   ├── inspect-bcp-file.test.ts
    │   └── encode-bcp-file.test.ts
    ├── bcp-cli.test.ts       # CLI subprocess integration tests
    └── proxy/
        ├── detector.test.ts  # Content detection unit tests
        ├── manifest.test.ts  # Manifest generation tests
        └── proxy.test.ts     # End-to-end proxy tests
```

---

## Configuration

### Claude Code MCP Config (`~/.claude.json` or project `.mcp.json`)

```json
{
  "mcpServers": {
    "bcp": {
      "command": "bun",
      "args": ["run", "/path/to/packages/bcp-mcp-server/src/index.ts", "--mode", "reader"]
    }
  }
}
```

### Proxy Mode

```json
{
  "mcpServers": {
    "bcp-proxy": {
      "command": "bun",
      "args": [
        "run", "/path/to/packages/bcp-mcp-server/src/index.ts",
        "--mode", "proxy",
        "--downstream-command", "npx",
        "--downstream-args", "-y,@modelcontextprotocol/server-filesystem,/tmp",
        "--render-mode", "xml"
      ]
    }
  }
}
```

### Environment Variables

```
BCP_CLI_PATH        Path to the bcp binary (default: resolved from PATH)
BCP_CLI_TIMEOUT_MS  Subprocess timeout in milliseconds (default: 30000)
BCP_RENDER_MODE     Default render mode: xml | markdown | minimal (default: xml)
BCP_DETECT_THRESHOLD  Content detection confidence threshold (default: 0.7)
BCP_LOG_LEVEL       Log verbosity: debug | info | warn | error (default: info)
```

---

## Acceptance Criteria

### Sub-phase A — BCP File Reader

- [ ] Server starts and completes MCP initialization handshake over stdio
- [ ] `tools/list` returns `read_bcp_file`, `inspect_bcp_file`, and `encode_bcp_file`
- [ ] `read_bcp_file` with a valid `.bcp` path returns rendered text as `TextContent`
- [ ] `read_bcp_file` with `--mode markdown` returns markdown-rendered output
- [ ] `read_bcp_file` with `--budget 500` returns summarized output for low-priority blocks
- [ ] `read_bcp_file` with a non-existent path returns `isError: true` with diagnostic
- [ ] `read_bcp_file` with a non-`.bcp` file returns `isError: true` with message
- [ ] `inspect_bcp_file` returns block summary table matching `bcp inspect` output
- [ ] `encode_bcp_file` creates a valid `.bcp` file from a JSON manifest
- [ ] `encode_bcp_file` with `compress: true` produces compressed output
- [ ] Unknown method returns JSON-RPC error code -32601
- [ ] Malformed JSON input does not crash the server (error logged to stderr)
- [ ] `ping` method returns empty result
- [ ] Server shuts down cleanly when stdin is closed

### Sub-phase B — Proxy

- [ ] Proxy spawns downstream MCP server and completes initialization
- [ ] `tools/list` returns merged tools (local BCP tools + downstream tools)
- [ ] `tools/call` for a downstream tool forwards the request and returns the result
- [ ] Tool results containing fenced code blocks are detected and wrapped as CODE blocks
- [ ] Tool results containing directory listings are detected as FILE_TREE blocks
- [ ] Tool results containing valid JSON are detected as STRUCTURED_DATA blocks
- [ ] Tool results containing unified diffs are detected as DIFF blocks
- [ ] Tool results below the confidence threshold are passed through unmodified
- [ ] Proxy encodes detected blocks via `bcp encode`, then decodes via `bcp decode`
- [ ] Rendered proxy output uses fewer tokens than the raw tool result text
- [ ] Downstream server crash is handled gracefully (error returned, not server crash)
- [ ] Proxy shuts down downstream server when stdin is closed

### Cross-cutting

- [ ] Zero runtime dependencies beyond Bun built-ins
- [ ] All TypeScript files pass strict mode (`noImplicitAny`, `noImplicitReturns`)
- [ ] `bun test` passes with zero failures
- [ ] No `any` types in source code (use `unknown` with narrowing)
- [ ] Named exports only (no default exports)
- [ ] Logging output goes to stderr only (stdout reserved for JSON-RPC)

---

## Verification

```bash
# Prerequisites: bcp binary must be on PATH
cargo build --release -p bcp-cli
export PATH="$PWD/target/release:$PATH"

# Install dependencies (should be none beyond Bun built-ins)
cd packages/bcp-mcp-server

# Type check
bun run tsc --noEmit

# Run tests
bun test

# Manual smoke test — reader mode
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_bcp_file","arguments":{"path":"../../crates/bcp-tests/tests/golden/simple_code.bcp"}}}' \
  | bun run src/index.ts --mode reader

# Manual smoke test — proxy mode (requires a downstream server)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | bun run src/index.ts --mode proxy \
      --downstream-command npx \
      --downstream-args "-y,@anthropic/mcp-echo-server"
```

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `bcp` binary not on PATH | High | Blocks all functionality | Clear error message at startup with install instructions; `BCP_CLI_PATH` env var override |
| Content detection misclassifies text as code | Medium | Low (wrong block type, but still readable) | Confidence threshold prevents low-quality matches; passthrough fallback is always safe |
| Downstream MCP server uses non-stdio transport | Medium | Blocks proxy mode | Document stdio-only requirement; future phase adds HTTP transport |
| Large tool results cause subprocess timeout | Low | Medium | Configurable timeout; stream processing in future phase |
| JSON-RPC edge cases (batch requests, notifications) | Low | Low | MCP spec does not use batch requests; notification handling is explicit |
| Bun subprocess API differences across platforms | Low | Medium | Test on Linux and macOS; use `Bun.spawn()` which is stable API |

---

## Rollback Plan

### Sub-phase A Rollback
- Delete `packages/bcp-mcp-server/` directory
- Remove from root `package.json` workspaces (if added)
- No Rust code is modified; all existing crates remain functional
- Clean rollback: `git revert`

### Sub-phase B Rollback
- Remove `src/proxy/` directory
- Remove proxy-related CLI flags from entry point
- Sub-phase A tools continue to work independently
- Reader mode remains fully functional without proxy code
