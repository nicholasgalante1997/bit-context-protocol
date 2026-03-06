import { createBaseConfig } from "./utils";

export default createBaseConfig({
  entrypoints: ['src/index.ts'],
  external: ['debug', 'supports-color'],
  outdir: "./dist",
  packages: "external",
  target: "bun",
  naming: {
    entry: 'bcp-mcp-server.bun.js'
  }
}) as Bun.BuildConfig;
