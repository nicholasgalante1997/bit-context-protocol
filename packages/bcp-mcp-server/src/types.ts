export type JsonRpcRequest = {
  jsonrpc: "2.0";
  id: string | number;
  method: string;
  params?: Record<string, unknown>;
};

export type JsonRpcResponse = {
  jsonrpc: "2.0";
  id: string | number;
  result?: unknown;
  error?: JsonRpcError;
};

export type JsonRpcNotification = {
  jsonrpc: "2.0";
  method: string;
  params?: Record<string, unknown>;
};

export type JsonRpcError = {
  code: number;
  message: string;
  data?: unknown;
};

export type JsonRpcMessage = JsonRpcRequest | JsonRpcNotification;

export type ToolDefinition = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
};

export type ToolResult = {
  content: Array<{ type: "text"; text: string }>;
  isError?: boolean;
};

export type ToolCallParams = {
  name: string;
  arguments?: Record<string, unknown>;
};

export type InitializeParams = {
  protocolVersion: string;
  capabilities: Record<string, unknown>;
  clientInfo: { name: string; version: string };
};

export type McpCapabilities = {
  tools: { listChanged: boolean };
};

export type McpServerInfo = {
  name: string;
  version: string;
};

export type InitializeResult = {
  protocolVersion: string;
  capabilities: McpCapabilities;
  serverInfo: McpServerInfo;
};

export const MCP_PROTOCOL_VERSION = "2025-11-25";

export const SERVER_INFO: McpServerInfo = {
  name: "bcp-mcp-server",
  version: "0.1.0"
};

export const SERVER_CAPABILITIES: McpCapabilities = {
  tools: { listChanged: false }
};

export const PARSE_ERROR = -32700;
export const INVALID_REQUEST = -32600;
export const METHOD_NOT_FOUND = -32601;
export const INVALID_PARAMS = -32602;
export const INTERNAL_ERROR = -32603;
export const SERVER_NOT_INITIALIZED = -32002;
