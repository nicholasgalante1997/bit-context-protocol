import type { InitializeParams, InitializeResult } from "./types.ts";
import {
  MCP_PROTOCOL_VERSION,
  SERVER_INFO,
  SERVER_CAPABILITIES
} from "./types.ts";

export type SessionState = {
  initialized: boolean;
  clientInfo: { name: string; version: string } | null;
};

export const createSessionState = (): SessionState => ({
  initialized: false,
  clientInfo: null
});

export const handleInitialize = (_params: InitializeParams): InitializeResult => ({
  protocolVersion: MCP_PROTOCOL_VERSION,
  capabilities: SERVER_CAPABILITIES,
  serverInfo: SERVER_INFO
});
