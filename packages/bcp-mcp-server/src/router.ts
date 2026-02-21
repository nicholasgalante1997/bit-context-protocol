import type {
  JsonRpcMessage,
  JsonRpcResponse,
  ToolCallParams,
  ToolResult,
  ToolDefinition,
  InitializeParams
} from "./types.ts";
import {
  METHOD_NOT_FOUND,
  INTERNAL_ERROR,
  INVALID_PARAMS,
  SERVER_NOT_INITIALIZED
} from "./types.ts";
import type { SessionState } from "./lifecycle.ts";
import { handleInitialize } from "./lifecycle.ts";
import { TOOLS } from "./tools.ts";
import { handleReadBcpFile } from "./handlers/read-bcp-file.ts";
import { handleInspectBcpFile } from "./handlers/inspect-bcp-file.ts";
import { handleEncodeBcpFile } from "./handlers/encode-bcp-file.ts";
import type { McpProxy } from "./proxy/proxy.ts";
import type { ProxyConfig } from "./proxy/config.ts";
import { log } from "./logger.ts";

type ToolHandler = (args: Record<string, unknown>) => Promise<ToolResult>;

const LOCAL_TOOL_HANDLERS: Record<string, ToolHandler> = {
  read_bcp_file: handleReadBcpFile,
  inspect_bcp_file: handleInspectBcpFile,
  encode_bcp_file: handleEncodeBcpFile
};

export type RouterOptions = {
  proxy?: McpProxy;
  proxyConfig?: ProxyConfig;
  mergedTools?: ReadonlyArray<ToolDefinition>;
};

const isRequest = (msg: JsonRpcMessage): msg is JsonRpcMessage & { id: string | number } =>
  "id" in msg;

export const createRouter = (session: SessionState, options: RouterOptions = {}) => {
  const { proxy, proxyConfig, mergedTools } = options;

  return async (message: JsonRpcMessage): Promise<JsonRpcResponse | null> => {
    const id = isRequest(message) ? (message as { id: string | number }).id : null;

    try {
      switch (message.method) {
        case "initialize": {
          const params = message.params as InitializeParams | undefined;
          if (!params) {
            return {
              jsonrpc: "2.0",
              id: id!,
              error: { code: INVALID_PARAMS, message: "Missing initialize params" }
            };
          }
          const result = handleInitialize(params);
          session.initialized = true;
          session.clientInfo = params.clientInfo;
          log("info", `Initialized by ${params.clientInfo.name} v${params.clientInfo.version}`);
          return { jsonrpc: "2.0", id: id!, result };
        }

        case "notifications/initialized":
          return null;

        case "ping":
          return { jsonrpc: "2.0", id: id!, result: {} };

        case "tools/list": {
          if (!session.initialized) {
            return {
              jsonrpc: "2.0",
              id: id!,
              error: { code: SERVER_NOT_INITIALIZED, message: "Server not initialized" }
            };
          }
          const tools = mergedTools ?? TOOLS;
          return { jsonrpc: "2.0", id: id!, result: { tools } };
        }

        case "tools/call": {
          if (!session.initialized) {
            return {
              jsonrpc: "2.0",
              id: id!,
              error: { code: SERVER_NOT_INITIALIZED, message: "Server not initialized" }
            };
          }

          const callParams = message.params as ToolCallParams | undefined;
          if (!callParams?.name) {
            return {
              jsonrpc: "2.0",
              id: id!,
              error: { code: INVALID_PARAMS, message: "Missing tool name" }
            };
          }

          const localHandler = LOCAL_TOOL_HANDLERS[callParams.name];
          if (localHandler) {
            const toolResult = await localHandler(callParams.arguments ?? {});
            return { jsonrpc: "2.0", id: id!, result: toolResult };
          }

          if (proxy && proxyConfig) {
            const toolResult = await proxy.forwardToolCall(
              callParams.name,
              callParams.arguments ?? {},
              proxyConfig
            );
            return { jsonrpc: "2.0", id: id!, result: toolResult };
          }

          return {
            jsonrpc: "2.0",
            id: id!,
            error: { code: METHOD_NOT_FOUND, message: `Unknown tool: ${callParams.name}` }
          };
        }

        default:
          if (id !== null) {
            return {
              jsonrpc: "2.0",
              id: id!,
              error: { code: METHOD_NOT_FOUND, message: `Method not found: ${message.method}` }
            };
          }
          return null;
      }
    } catch (err) {
      log("error", `Handler error for ${message.method}:`, err);
      if (id !== null) {
        return {
          jsonrpc: "2.0",
          id: id!,
          error: {
            code: INTERNAL_ERROR,
            message: err instanceof Error ? err.message : "Internal error"
          }
        };
      }
      return null;
    }
  };
};
