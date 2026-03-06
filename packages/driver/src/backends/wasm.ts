import { readFileSync, writeFileSync } from 'node:fs';
import type { BcpDriver, DecodeResult, InspectResult, ValidateResult } from '../types.ts';

interface WasmModule {
  decode(payload: Uint8Array, configJson?: string | null): string;
  encode(manifestJson: string): Uint8Array;
  inspect(payload: Uint8Array): string;
  validate(payload: Uint8Array): string;
}

interface WasmInspectBlock {
  index: number;
  type: string;
  size: number;
}

interface WasmInspectResult {
  blockCount: number;
  totalSize: number;
  blocks: WasmInspectBlock[];
}

interface WasmValidateResult {
  valid: boolean;
  errors: string[];
}

interface WasmDecodeResult {
  text: string;
}

let cachedModule: WasmModule | null = null;

const loadWasm = async (): Promise<WasmModule> => {
  if (cachedModule) return cachedModule;
  try {
    // Dynamic import — the wasm/ directory is co-located with the driver package
    const mod = await import('../../wasm/bcp_wasm.js') as WasmModule;
    cachedModule = mod;
    return mod;
  } catch {
    throw new Error(
      'BCP WASM module not found. Build it with: cd crates/bcp-wasm && wasm-pack build --target bundler --out-dir ../../packages/driver/wasm'
    );
  }
};

export const createWasmDriver = (): BcpDriver => {
  return {
    async decode(filePath, config) {
      const wasm = await loadWasm();
      const payload = readFileSync(filePath);

      const configJson = config
        ? JSON.stringify({
            mode: config.mode,
            verbosity: config.verbosity,
            tokenBudget: config.tokenBudget
          })
        : undefined;

      const resultJson = wasm.decode(new Uint8Array(payload), configJson ?? null);
      const result = JSON.parse(resultJson) as WasmDecodeResult;

      return { text: result.text } satisfies DecodeResult;
    },

    async encode(manifestPath, outputPath, _options) {
      const wasm = await loadWasm();
      const manifestJson = readFileSync(manifestPath, 'utf-8');
      const bytes = wasm.encode(manifestJson);
      writeFileSync(outputPath, bytes);
    },

    async inspect(filePath) {
      const wasm = await loadWasm();
      const payload = readFileSync(filePath);
      const resultJson = wasm.inspect(new Uint8Array(payload));
      const result = JSON.parse(resultJson) as WasmInspectResult;

      return {
        blockCount: result.blockCount,
        totalSize: result.totalSize,
        blocks: result.blocks.map((b) => ({
          index: b.index,
          type: b.type,
          size: b.size
        })),
        raw: resultJson
      } satisfies InspectResult;
    },

    async validate(filePath) {
      const wasm = await loadWasm();
      const payload = readFileSync(filePath);
      const resultJson = wasm.validate(new Uint8Array(payload));
      const result = JSON.parse(resultJson) as WasmValidateResult;

      return {
        valid: result.valid,
        errors: result.errors,
        raw: resultJson
      } satisfies ValidateResult;
    },

    async available() {
      try {
        await loadWasm();
        return true;
      } catch {
        return false;
      }
    }
  };
};
