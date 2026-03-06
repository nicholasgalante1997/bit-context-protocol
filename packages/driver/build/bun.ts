import { createBaseConfig } from "./utils";

export default createBaseConfig({
    entrypoints: ['src/index.ts'],
    external: [],
    outdir: "./dist",
    packages: "external",
    target: "bun",
    naming: {
        entry: 'bcp-driver.bun.js'
    }
}) as Bun.BuildConfig;