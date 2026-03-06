import type { BuildConfig, BuildOutput } from "bun";

export interface BaseConfigOptions {
  external?: string[];
  outdir?: string;
  entrypoints?: string[];
  packages?: "bundle" | "external";
  target?: "node" | "bun";
  naming?: BuildConfig['naming'];
}

export function createBaseConfig(
  options: BaseConfigOptions = {},
): Partial<BuildConfig> {
  const {
    external = [],
    outdir = "./dist",
    entrypoints,
    naming,
    packages,
    target = "bun",
  } = options;

  return {
    ...(entrypoints ? { entrypoints } : {}),
    outdir,
    format: "esm",
    target,
    splitting: false,
    sourcemap: "linked",
    minify: false,
    root: "./src",
    naming,
    packages,
    external,
  };
}

export async function concurrent_build(
  ...configs: BuildConfig[]
): Promise<BuildOutput[]> {
  const start = performance.now();
  const builds = configs.map((config) =>
    Bun.build({ ...config, metafile: true }),
  );

  let outputs: BuildOutput[];

  try {
    outputs = await Promise.all(builds);
  } catch (error) {
    const elapsed = (performance.now() - start).toFixed(2);
    console.error(`Build failed after ${elapsed}ms`);
    throw error;
  }

  for (const output of outputs) {
    if (!output.success) {
      console.error("Build errors:", output.logs);
      throw new Error("Build produced errors");
    }
  }

  const elapsed = (performance.now() - start).toFixed(2);
  console.log(`Built ${configs.length} bundles in ${elapsed}ms`);
  return outputs;
}
