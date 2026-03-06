import type { BcpBackend } from './backend.ts';
import type {
  BcpDriver,
  DecodeResult,
  InspectResult,
  ValidateResult
} from './types.ts';

const parseInspectOutput = (raw: string): InspectResult => {
  const blocks: InspectResult['blocks'] = [];
  let totalSize = 0;

  const lines = raw.split('\n');
  let currentIndex = 0;

  for (const line of lines) {
    // Match lines like "Block 0: CODE [rust] path="src/main.rs" (42 bytes)"
    const blockMatch = line.match(
      /Block\s+(\d+):\s+(\w[\w_]*)\s+.*?\((\d+)\s+bytes?\)/i
    );
    if (blockMatch) {
      const index = Number(blockMatch[1]);
      const type = blockMatch[2]!;
      const size = Number(blockMatch[3]);
      blocks.push({ index, type, size });
      totalSize += size;
      currentIndex = index + 1;
    }

    // Match summary lines that follow a block line
    const summaryMatch = line.match(/^\s+Summary:\s*(.+)/);
    if (summaryMatch && blocks.length > 0) {
      const lastBlock = blocks[blocks.length - 1];
      if (lastBlock) {
        lastBlock.summary = summaryMatch[1]!.trim();
      }
    }
  }

  return {
    blockCount: blocks.length || currentIndex,
    totalSize,
    blocks,
    raw
  };
};

const parseValidateOutput = (
  stdout: string,
  stderr: string,
  exitCode: number
): ValidateResult => {
  const valid = exitCode === 0;
  const errors: string[] = [];

  if (!valid) {
    const output = stderr || stdout;
    for (const line of output.split('\n')) {
      const trimmed = line.trim();
      if (trimmed && !trimmed.startsWith('Usage:')) {
        errors.push(trimmed);
      }
    }
  }

  return { valid, errors, raw: stdout || stderr };
};

export const createDriverFromBackend = (backend: BcpBackend): BcpDriver => {
  const assertAvailable = async (): Promise<void> => {
    const ok = await backend.available();
    if (!ok) {
      throw new Error(
        'bcp backend is not available. Install the bcp CLI or set BCP_CLI_PATH.'
      );
    }
  };

  return {
    async decode(filePath, config) {
      await assertAvailable();

      const args: string[] = ['decode', filePath, '--no-color'];

      if (config?.mode) {
        args.push('--mode', config.mode);
      }
      if (config?.verbosity) {
        args.push('--verbosity', config.verbosity);
      }
      if (config?.tokenBudget !== undefined) {
        args.push('--budget', String(config.tokenBudget));
      }
      if (config?.include && config.include.length > 0) {
        args.push('--include', config.include.join(','));
      }

      const result = await backend.exec(args);

      if (result.exitCode !== 0) {
        throw new Error(`bcp decode failed: ${result.stderr || result.stdout}`);
      }

      return { text: result.stdout } satisfies DecodeResult;
    },

    async encode(manifestPath, outputPath, options) {
      await assertAvailable();

      const args: string[] = [
        'encode',
        manifestPath,
        '--output',
        outputPath,
        '--no-color'
      ];

      if (options?.compressBlocks) args.push('--compress-blocks');
      if (options?.compressPayload) args.push('--compress-payload');
      if (options?.dedup) args.push('--dedup');

      const result = await backend.exec(args);

      if (result.exitCode !== 0) {
        throw new Error(
          `bcp encode failed: ${result.stderr || result.stdout}`
        );
      }
    },

    async inspect(filePath) {
      await assertAvailable();

      const args = ['inspect', filePath, '--no-color'];
      const result = await backend.exec(args);

      if (result.exitCode !== 0) {
        throw new Error(
          `bcp inspect failed: ${result.stderr || result.stdout}`
        );
      }

      return parseInspectOutput(result.stdout);
    },

    async validate(filePath) {
      await assertAvailable();

      const args = ['validate', filePath, '--no-color'];
      const result = await backend.exec(args);

      return parseValidateOutput(result.stdout, result.stderr, result.exitCode);
    },

    async available() {
      return backend.available();
    }
  };
};
