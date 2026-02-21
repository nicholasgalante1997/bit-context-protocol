import type { JsonRpcMessage, JsonRpcResponse } from "../types.ts";
import { SERVER_NOT_INITIALIZED } from "../types.ts";
import type { SessionState } from "../lifecycle.ts";
import { createSessionState } from "../lifecycle.ts";
import { log } from "../logger.ts";

export type HttpTransportConfig = {
  port: number;
  host: string;
  allowedOrigins: ReadonlyArray<string>;
  router: (session: SessionState, message: JsonRpcMessage) => Promise<JsonRpcResponse | null>;
};

type ManagedSession = {
  id: string;
  state: SessionState;
};

export const createHttpServer = (config: HttpTransportConfig) => {
  const sessions = new Map<string, ManagedSession>();

  const validateOrigin = (request: Request): boolean => {
    const origin = request.headers.get("Origin");
    if (!origin) return true; // No origin header is OK for non-browser clients
    return config.allowedOrigins.some(
      (allowed) => origin === allowed || origin === `http://${allowed}` || origin === `https://${allowed}`
    );
  };

  const getOrCreateSession = (request: Request, isInitialize: boolean): ManagedSession | null => {
    const sessionId = request.headers.get("MCP-Session-Id");

    if (isInitialize) {
      const id = crypto.randomUUID();
      const session: ManagedSession = { id, state: createSessionState() };
      sessions.set(id, session);
      return session;
    }

    if (!sessionId) return null;
    return sessions.get(sessionId) ?? null;
  };

  const formatSseEvent = (id: string, data: unknown): string => {
    return `id: ${id}\ndata: ${JSON.stringify(data)}\n\n`;
  };

  const handlePost = async (request: Request): Promise<Response> => {
    if (!validateOrigin(request)) {
      return new Response("Forbidden", { status: 403 });
    }

    let body: unknown;
    try {
      body = await request.json();
    } catch {
      return new Response(
        JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "Parse error" } }),
        { status: 400, headers: { "Content-Type": "application/json" } }
      );
    }

    if (typeof body !== "object" || body === null) {
      return new Response(
        JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32600, message: "Invalid request" } }),
        { status: 400, headers: { "Content-Type": "application/json" } }
      );
    }

    const message = body as JsonRpcMessage;
    const isInitialize = message.method === "initialize";
    const isNotification = !("id" in message);

    // Session management
    const managedSession = getOrCreateSession(request, isInitialize);
    if (!isInitialize && !managedSession) {
      return new Response(
        JSON.stringify({
          jsonrpc: "2.0",
          id: ("id" in message) ? (message as { id: unknown }).id : null,
          error: { code: SERVER_NOT_INITIALIZED, message: "Invalid or missing session" }
        }),
        { status: 404, headers: { "Content-Type": "application/json" } }
      );
    }

    if (!managedSession) {
      return new Response("Internal error", { status: 500 });
    }

    // Notifications get 202 Accepted
    if (isNotification) {
      await config.router(managedSession.state, message);
      return new Response(null, { status: 202 });
    }

    const response = await config.router(managedSession.state, message);
    if (!response) {
      return new Response(null, { status: 202 });
    }

    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (isInitialize) {
      headers["MCP-Session-Id"] = managedSession.id;
    }

    return new Response(JSON.stringify(response), { status: 200, headers });
  };

  const handleGet = (_request: Request): Response => {
    // Sub-phase A: no server-initiated messages
    return new Response("Method Not Allowed", { status: 405 });
  };

  const handleDelete = (request: Request): Response => {
    const sessionId = request.headers.get("MCP-Session-Id");
    if (sessionId && sessions.has(sessionId)) {
      sessions.delete(sessionId);
      log("info", `Session ${sessionId} terminated`);
      return new Response(null, { status: 200 });
    }
    return new Response("Not Found", { status: 404 });
  };

  const server = Bun.serve({
    port: config.port,
    hostname: config.host,
    fetch(request: Request): Response | Promise<Response> {
      const url = new URL(request.url);
      if (url.pathname !== "/mcp") {
        return new Response("Not Found", { status: 404 });
      }

      switch (request.method) {
        case "POST":
          return handlePost(request);
        case "GET":
          return handleGet(request);
        case "DELETE":
          return handleDelete(request);
        default:
          return new Response("Method Not Allowed", { status: 405 });
      }
    }
  });

  log("info", `HTTP server listening on ${config.host}:${config.port}`);

  return {
    server,
    stop: async () => {
      await server.stop();
      sessions.clear();
    },
    formatSseEvent
  };
};
