import { createBaseConfig } from "./utils";

export default createBaseConfig({
    entrypoints: ['src/index.ts'],
    external: [],
    outdir: "./dist",
    packages: "external",
    target: "node",
    naming: {
        entry: 'bcp-driver.js'
    }
}) as Bun.BuildConfig;