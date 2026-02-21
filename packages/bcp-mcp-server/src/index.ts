import { readMessages, sendMessage } from "./transports/stdio.ts";
import { createHttpServer } from "./transports/http.ts";
import { createRouter } from "./router.ts";
import type { RouterOptions } from "./router.ts";
import { createSessionState } from "./lifecycle.ts";
import { validateBcpAvailable } from "./bcp-cli.ts";
import { McpProxy } from "./proxy/proxy.ts";
import { parseProxyConfig } from "./proxy/config.ts";
import type { ProxyConfig } from "./proxy/config.ts";
import { log } from "./logger.ts";

type CliArgs = {
  transport: "stdio" | "http";
  mode: "reader" | "proxy";
  port: number;
  host: string;
  allowedOrigins: ReadonlyArray<string>;
};

const parseArgs = (argv: ReadonlyArray<string>): CliArgs => {
  const args: CliArgs = {
    transport: "stdio",
    mode: "reader",
    port: 3333,
    host: "127.0.0.1",
    allowedOrigins: ["localhost", "127.0.0.1"]
  };

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const next = argv[i + 1];

    switch (arg) {
      case "--transport":
        if (next === "stdio" || next === "http") args.transport = next;
        i++;
        break;
      case "--mode":
        if (next === "reader" || next === "proxy") args.mode = next;
        i++;
        break;
      case "--port":
        if (next) args.port = Number(next);
        i++;
        break;
      case "--host":
        if (next) args.host = next;
        i++;
        break;
      case "--allowed-origins":
        if (next) args.allowedOrigins = next.split(",");
        i++;
        break;
    }
  }

  return args;
};

const startProxy = async (argv: ReadonlyArray<string>): Promise<{ proxy: McpProxy; proxyConfig: ProxyConfig; routerOptions: RouterOptions }> => {
  const proxyConfig = parseProxyConfig(argv);

  if (!proxyConfig.downstream.command) {
    log("error", "Proxy mode requires --downstream-command");
    process.exit(1);
  }

  const proxy = new McpProxy();
  await proxy.start(proxyConfig);

  await proxy.initialize({
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "bcp-mcp-server-proxy", version: "0.1.0" }
  });

  proxy.sendNotification("notifications/initialized");

  const downstreamTools = await proxy.getDownstreamTools();
  log("info", `Downstream provides ${downstreamTools.length} tools`);

  const { TOOLS } = await import("./tools.ts");
  const mergedTools = proxy.mergeToolLists(TOOLS);
  log("info", `Merged tool list: ${mergedTools.map((t) => t.name).join(", ")}`);

  return {
    proxy,
    proxyConfig,
    routerOptions: { proxy, proxyConfig, mergedTools }
  };
};

const runStdio = async (config: CliArgs, argv: ReadonlyArray<string>): Promise<void> => {
  const session = createSessionState();
  let routerOptions: RouterOptions = {};

  if (config.mode === "proxy") {
    const proxySetup = await startProxy(argv);
    routerOptions = proxySetup.routerOptions;

    process.on("SIGINT", () => {
      proxySetup.proxy.stop();
      process.exit(0);
    });
    process.on("SIGTERM", () => {
      proxySetup.proxy.stop();
      process.exit(0);
    });
  }

  const router = createRouter(session, routerOptions);

  for await (const message of readMessages(Bun.stdin.stream())) {
    const response = await router(message);
    if (response) {
      sendMessage(response);
    }
  }

  if (config.mode === "proxy" && routerOptions.proxy) {
    (routerOptions.proxy as McpProxy).stop();
  }

  log("info", "stdin closed, shutting down");
};

const runHttp = async (config: CliArgs, argv: ReadonlyArray<string>): Promise<void> => {
  let routerOptions: RouterOptions = {};
  let proxy: McpProxy | null = null;

  if (config.mode === "proxy") {
    const proxySetup = await startProxy(argv);
    routerOptions = proxySetup.routerOptions;
    proxy = proxySetup.proxy;
  }

  const httpServer = createHttpServer({
    port: config.port,
    host: config.host,
    allowedOrigins: config.allowedOrigins,
    router: async (session, message) => {
      const routerFn = createRouter(session, routerOptions);
      return routerFn(message);
    }
  });

  const shutdown = () => {
    httpServer.stop();
    if (proxy) proxy.stop();
    process.exit(0);
  };

  process.on("SIGINT", () => {
    log("info", "SIGINT received, shutting down");
    shutdown();
  });

  process.on("SIGTERM", () => {
    log("info", "SIGTERM received, shutting down");
    shutdown();
  });
};

const main = async (): Promise<void> => {
  const argv = Bun.argv.slice(2);
  const config = parseArgs(argv);

  const bcpAvailable = await validateBcpAvailable();
  if (!bcpAvailable) {
    log("warn", "bcp binary not found on PATH. Set BCP_CLI_PATH or install bcp-cli.");
    log("warn", "Tool calls will fail until the binary is available.");
  }

  log("info", `Starting bcp-mcp-server (transport=${config.transport}, mode=${config.mode})`);

  if (config.transport === "http") {
    await runHttp(config, argv);
  } else {
    await runStdio(config, argv);
  }
};

main().catch((err) => {
  log("error", "Fatal error:", err);
  process.exit(1);
});
