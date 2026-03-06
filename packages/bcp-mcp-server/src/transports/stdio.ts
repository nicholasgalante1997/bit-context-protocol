import type { JsonRpcMessage, JsonRpcResponse, JsonRpcNotification } from "../types.ts";
import { log } from "../logger.ts";

export async function* readMessages(
  stream: ReadableStream<Uint8Array>
): AsyncGenerator<JsonRpcMessage> {
  const decoder = new TextDecoder();
  let buffer = "";

  for await (const chunk of stream) {
    buffer += decoder.decode(chunk, { stream: true });

    let newlineIndex: number;
    while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
      const line = buffer.slice(0, newlineIndex).trim();
      buffer = buffer.slice(newlineIndex + 1);

      if (line.length === 0) continue;

      try {
        const parsed: unknown = JSON.parse(line);
        if (isJsonRpcMessage(parsed)) {
          yield parsed;
        } else {
          log("warn", "Invalid JSON-RPC message:", line);
        }
      } catch {
        log("error", "Failed to parse JSON:", line);
      }
    }
  }

  // Handle any remaining data in buffer after stream ends
  const remaining = buffer.trim();
  if (remaining.length > 0) {
    try {
      const parsed: unknown = JSON.parse(remaining);
      if (isJsonRpcMessage(parsed)) {
        yield parsed;
      }
    } catch {
      log("error", "Failed to parse remaining buffer:", remaining);
    }
  }
}

export const sendMessage = (message: JsonRpcResponse | JsonRpcNotification): void => {
  const json = JSON.stringify(message);
  process.stdout.write(json + "\n");
};

const isJsonRpcMessage = (value: unknown): value is JsonRpcMessage => {
  if (typeof value !== "object" || value === null) return false;
  const obj = value as Record<string, unknown>;
  return obj["jsonrpc"] === "2.0" && typeof obj["method"] === "string";
};
