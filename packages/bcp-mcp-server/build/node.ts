import { createBaseConfig } from "./utils";

export default createBaseConfig({
  entrypoints: ['src/index.ts'],
  external: ['debug', 'supports-color'],
  outdir: "./dist",
  packages: "external",
  target: "node",
  naming: {
    entry: 'bcp-mcp-server.js'
  }
}) as Bun.BuildConfig;
