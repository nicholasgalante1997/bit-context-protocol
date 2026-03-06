import { describe, test, expect } from "bun:test";
import { readMessages } from "../../src/transports/stdio.ts";

const createStream = (text: string): ReadableStream<Uint8Array> => {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(text));
      controller.close();
    }
  });
};

const createChunkedStream = (chunks: Array<string>): ReadableStream<Uint8Array> => {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk));
      }
      controller.close();
    }
  });
};

describe("readMessages", () => {
  test("parses a single JSON-RPC request", async () => {
    const stream = createStream('{"jsonrpc":"2.0","id":1,"method":"initialize"}\n');
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
    expect(messages[0]?.method).toBe("initialize");
  });

  test("parses multiple newline-delimited messages", async () => {
    const stream = createStream(
      '{"jsonrpc":"2.0","id":1,"method":"initialize"}\n' +
      '{"jsonrpc":"2.0","method":"notifications/initialized"}\n' +
      '{"jsonrpc":"2.0","id":2,"method":"tools/list"}\n'
    );
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(3);
    expect(messages[0]?.method).toBe("initialize");
    expect(messages[1]?.method).toBe("notifications/initialized");
    expect(messages[2]?.method).toBe("tools/list");
  });

  test("handles partial lines across chunks", async () => {
    const stream = createChunkedStream([
      '{"jsonrpc":"2.0",',
      '"id":1,"method"',
      ':"ping"}\n'
    ]);
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
    expect(messages[0]?.method).toBe("ping");
  });

  test("skips malformed JSON without crashing", async () => {
    const stream = createStream(
      'not json\n' +
      '{"jsonrpc":"2.0","id":1,"method":"ping"}\n'
    );
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
    expect(messages[0]?.method).toBe("ping");
  });

  test("skips empty lines", async () => {
    const stream = createStream(
      '\n\n{"jsonrpc":"2.0","id":1,"method":"ping"}\n\n'
    );
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
  });

  test("skips non-JSON-RPC objects", async () => {
    const stream = createStream(
      '{"hello":"world"}\n' +
      '{"jsonrpc":"2.0","id":1,"method":"ping"}\n'
    );
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
    expect(messages[0]?.method).toBe("ping");
  });

  test("handles message without trailing newline", async () => {
    const stream = createStream('{"jsonrpc":"2.0","id":1,"method":"ping"}');
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
  });

  test("handles unicode content in messages", async () => {
    const stream = createStream(
      '{"jsonrpc":"2.0","id":1,"method":"test","params":{"text":"Hello 世界 🌍"}}\n'
    );
    const messages = [];
    for await (const msg of readMessages(stream)) {
      messages.push(msg);
    }
    expect(messages).toHaveLength(1);
    const params = (messages[0] as { params?: { text?: string } }).params;
    expect(params?.text).toBe("Hello 世界 🌍");
  });
});
