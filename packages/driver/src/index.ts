export type {
  BcpBlock,
  BcpDriver,
  CreateDriverOptions,
  DecodeResult,
  DriverConfig,
  EncodeOptions,
  InspectResult,
  ModelFamily,
  OutputMode,
  ValidateResult,
  Verbosity
} from './types.ts';

export { OUTPUT_MODES, MODEL_FAMILIES, VERBOSITIES } from './types.ts';

export type { BcpBackend } from './backend.ts';

export type {
  CodeBlock,
  ConversationBlock,
  FileTreeBlock,
  FileTreeEntry,
  ToolResultBlock,
  DocumentBlock,
  StructuredDataBlock,
  DiffBlock,
  DiffHunk,
  AnnotationBlock,
  ManifestBlock,
  Manifest,
  Priority,
  Role,
  Status
} from './manifest.ts';

export { PRIORITIES, ROLES, STATUSES } from './manifest.ts';

export type { ContextBuilder, ContextBuilderOptions, BuildOptions, BuildResult } from './context.ts';
export { createContextBuilder } from './context.ts';

export { createCliBackend } from './backends/cli.ts';
export { createWasmDriver } from './backends/wasm.ts';
export { createDriverFromBackend } from './driver.ts';

import type { BcpDriver, CreateDriverOptions } from './types.ts';
import { createCliBackend } from './backends/cli.ts';
import { createWasmDriver } from './backends/wasm.ts';
import { createDriverFromBackend } from './driver.ts';

export const createDriver = (options?: CreateDriverOptions): BcpDriver => {
  if (options?.backend === 'wasm') {
    return createWasmDriver();
  }

  if (options?.backend === 'cli' || options?.binaryPath) {
    return createDriverFromBackend(
      createCliBackend({
        binaryPath: options?.binaryPath,
        timeoutMs: options?.timeoutMs
      })
    );
  }

  // Auto-detect: prefer WASM (no binary dependency), fall back to CLI
  if (!options?.backend) {
    return createAutoDriver(options);
  }

  return createDriverFromBackend(
    createCliBackend({
      binaryPath: options?.binaryPath,
      timeoutMs: options?.timeoutMs
    })
  );
};

const createAutoDriver = (options?: CreateDriverOptions): BcpDriver => {
  const wasmDriver = createWasmDriver();
  const cliDriver = createDriverFromBackend(
    createCliBackend({
      binaryPath: options?.binaryPath,
      timeoutMs: options?.timeoutMs
    })
  );

  let resolvedDriver: BcpDriver | null = null;

  const resolve = async (): Promise<BcpDriver> => {
    if (resolvedDriver) return resolvedDriver;

    if (await wasmDriver.available()) {
      resolvedDriver = wasmDriver;
    } else {
      resolvedDriver = cliDriver;
    }
    return resolvedDriver;
  };

  return {
    async decode(filePath, config) {
      return (await resolve()).decode(filePath, config);
    },
    async encode(manifestPath, outputPath, opts) {
      return (await resolve()).encode(manifestPath, outputPath, opts);
    },
    async inspect(filePath) {
      return (await resolve()).inspect(filePath);
    },
    async validate(filePath) {
      return (await resolve()).validate(filePath);
    },
    async available() {
      return (await resolve()).available();
    }
  };
};
