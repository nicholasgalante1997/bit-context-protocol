# @bit-context-protocol/mcp

MCP server for the Bit Context Protocol — read, inspect, and encode `.bcp` files from any MCP-compatible AI client. Supports stdio and HTTP transports with transparent proxy mode for wrapping downstream MCP servers.

> **Status:** Pre-release (0.1.0). Stdio transport, HTTP transport, all three tool handlers, and proxy mode with tool merging are implemented. Implements MCP protocol version `2025-11-25`.

## Install

```bash
bun add -g @bit-context-protocol/mcp
```

Requires the `bcp` CLI binary:

```bash
cargo install bcp-cli
```

## Quick Start

### Claude Code Integration

Add to your Claude Code MCP config (`.claude/mcp.json`):

```json
{
  "mcpServers": {
    "bcp": {
      "command": "bunx",
      "args": ["@bit-context-protocol/mcp"]
    }
  }
}
```

Once configured, Claude Code gains three new tools for working with `.bcp` files directly.

### HTTP Transport

For non-stdio clients:

```bash
bcp-mcp-server --transport http --port 3333 --host 127.0.0.1
```

### Proxy Mode

Wrap any downstream MCP server, merging its tools with BCP tools into a single unified tool list:

```bash
bcp-mcp-server --mode proxy \
  --downstream-command "npx" \
  --downstream-args "-y @modelcontextprotocol/server-filesystem /"
```

In proxy mode:
- BCP tools are handled locally
- Unknown tool calls are forwarded to the downstream server
- The merged tool list is returned on `tools/list`
- Downstream server lifecycle is managed automatically

## Tools

### `read_bcp_file`

Decode a `.bcp` file and return the rendered content as model-ready text.

```json
{
  "name": "read_bcp_file",
  "arguments": {
    "path": "/absolute/path/to/context.bcp",
    "mode": "xml",
    "budget": 8000
  }
}
```

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Absolute path to the .bcp file |
| `mode` | string | no | `"xml"` | Output render mode: `xml`, `markdown`, `minimal` |
| `budget` | number | no | — | Token budget. Low-priority blocks are summarized to fit |

**Output modes:**
- `xml` — Structured XML tags around each block (best for Claude)
- `markdown` — Markdown-formatted output with headers and code fences
- `minimal` — Compact text output, minimal formatting

When `budget` is set, the decoder uses adaptive verbosity — critical and high-priority blocks are rendered in full, while normal/low/background blocks are progressively summarized or omitted to fit within the token budget.

### `inspect_bcp_file`

Inspect a `.bcp` file and return a summary of its blocks, sizes, and structure without rendering content.

```json
{
  "name": "inspect_bcp_file",
  "arguments": {
    "path": "/absolute/path/to/context.bcp"
  }
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | yes | Absolute path to the .bcp file |

Returns block count, total size, and per-block metadata (index, type, size, summary).

### `encode_bcp_file`

Encode a JSON manifest into a `.bcp` file.

```json
{
  "name": "encode_bcp_file",
  "arguments": {
    "manifest_path": "/absolute/path/to/manifest.json",
    "output_path": "/absolute/path/to/output.bcp",
    "compress": true
  }
}
```

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `manifest_path` | string | yes | — | Absolute path to the JSON manifest file |
| `output_path` | string | yes | — | Absolute path for the output .bcp file |
| `compress` | boolean | no | `false` | Enable per-block zstd compression |

The manifest format matches the BCP specification — see the [driver documentation](../driver/README.md) for block types and schema.

## CLI Options

| Flag | Description | Default |
|------|-------------|---------|
| `--transport <stdio\|http>` | Transport protocol | `stdio` |
| `--mode <reader\|proxy>` | Operating mode | `reader` |
| `--port <number>` | HTTP server port | `3333` |
| `--host <address>` | HTTP bind address | `127.0.0.1` |
| `--allowed-origins <list>` | Comma-separated allowed origins (HTTP CORS) | `localhost,127.0.0.1` |
| `--downstream-command <cmd>` | Downstream MCP server command (proxy mode) | — |
| `--downstream-args <args>` | Arguments for downstream server | — |

## Protocol

The server implements the [Model Context Protocol](https://modelcontextprotocol.io) specification:

| Method | Supported | Description |
|--------|-----------|-------------|
| `initialize` | yes | Handshake with client info and capability negotiation |
| `notifications/initialized` | yes | Post-initialization acknowledgment |
| `ping` | yes | Health check |
| `tools/list` | yes | Returns tool definitions (merged in proxy mode) |
| `tools/call` | yes | Execute a tool (local BCP tools or forwarded to downstream) |

**Server capabilities:** `{ tools: { listChanged: false } }`

**Error codes:** Standard JSON-RPC error codes plus `SERVER_NOT_INITIALIZED (-32002)` for calls before initialization.

## Transports

### Stdio

Default transport. Reads newline-delimited JSON-RPC messages from stdin and writes responses to stdout. Compatible with Claude Code, Cursor, and other MCP clients that spawn server processes.

### HTTP

Stateful HTTP transport with session management. Each request carries session state. Supports CORS with configurable allowed origins.

## Proxy Architecture

In proxy mode, the server:

1. Spawns the downstream MCP server as a child process
2. Sends `initialize` + `notifications/initialized` to establish the downstream session
3. Fetches the downstream tool list via `tools/list`
4. Merges BCP tools with downstream tools (BCP tools take precedence on name collision)
5. Routes `tools/call` — local tools handled in-process, unknown tools forwarded to downstream
6. Manages downstream process lifecycle (SIGINT/SIGTERM cleanup)

This allows a single MCP server entry in your config to provide both BCP tools and any other MCP server's tools.

## Architecture

```
packages/bcp-mcp-server/
  src/
    index.ts                Server entry point (arg parsing, transport selection)
    router.ts               JSON-RPC method routing (initialize, tools/list, tools/call)
    lifecycle.ts            MCP session state and initialization
    tools.ts                Tool definitions (read, inspect, encode)
    types.ts                MCP protocol types and constants
    logger.ts               Debug logging (via debug package)
    bcp-cli.ts              BCP binary availability check

    handlers/               Tool implementations
      read-bcp-file.ts      Decode and render .bcp content
      inspect-bcp-file.ts   Inspect .bcp structure
      encode-bcp-file.ts    Encode manifest to .bcp

    transports/
      stdio.ts              Stdin/stdout JSON-RPC transport
      http.ts               HTTP server with session management

    proxy/
      proxy.ts              McpProxy — downstream server management
      config.ts             Proxy configuration parsing
      detector.ts           Downstream capability detection
      manifest.ts           Tool list merging logic
```

## Requirements

- [Bun](https://bun.sh) v1.3.9+
- `bcp` binary on PATH (`cargo install bcp-cli`)

## License

MIT
