import { describe, test, expect } from 'bun:test';
import type { BcpBackend } from '../src/backend.ts';
import { createDriverFromBackend } from '../src/driver.ts';

const mockBackend = (
  responses: Record<string, { stdout: string; stderr: string; exitCode: number }>
): BcpBackend => ({
  async exec(args) {
    const key = args[0] ?? 'unknown';
    return responses[key] ?? { stdout: '', stderr: 'unknown command', exitCode: 1 };
  },
  async available() {
    return true;
  }
});

describe('inspect output parsing', () => {
  test('parses single block', async () => {
    const backend = mockBackend({
      inspect: {
        stdout: [
          'Header: BCP v1.0, flags=0x00, 1 block',
          'Block 0: CODE [rust] path="src/main.rs" (42 bytes)',
          '---',
          'END sentinel at offset 73'
        ].join('\n'),
        stderr: '',
        exitCode: 0
      }
    });

    const driver = createDriverFromBackend(backend);
    const result = await driver.inspect('/test.bcp');

    expect(result.blockCount).toBe(1);
    expect(result.blocks[0]!.index).toBe(0);
    expect(result.blocks[0]!.type).toBe('CODE');
    expect(result.blocks[0]!.size).toBe(42);
    expect(result.totalSize).toBe(42);
  });

  test('parses multiple blocks', async () => {
    const backend = mockBackend({
      inspect: {
        stdout: [
          'Header: BCP v1.0, flags=0x00, 3 blocks',
          'Block 0: CODE [go] path="main.go" (12 bytes)',
          'Block 1: CONVERSATION [system] (28 bytes)',
          'Block 2: DOCUMENT title="README" (5 bytes)',
          '---',
          'END sentinel at offset 200'
        ].join('\n'),
        stderr: '',
        exitCode: 0
      }
    });

    const driver = createDriverFromBackend(backend);
    const result = await driver.inspect('/test.bcp');

    expect(result.blockCount).toBe(3);
    expect(result.totalSize).toBe(45);
    expect(result.blocks.map((b) => b.type)).toEqual([
      'CODE',
      'CONVERSATION',
      'DOCUMENT'
    ]);
  });
});

describe('validate output parsing', () => {
  test('parses valid output', async () => {
    const backend = mockBackend({
      validate: {
        stdout: '✓ Header: valid\n✓ Blocks: 1 block parsed successfully\n',
        stderr: '',
        exitCode: 0
      }
    });

    const driver = createDriverFromBackend(backend);
    const result = await driver.validate('/test.bcp');

    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  test('parses invalid output', async () => {
    const backend = mockBackend({
      validate: {
        stdout: '',
        stderr: 'error: invalid magic bytes\nerror: expected BCP header\n',
        exitCode: 1
      }
    });

    const driver = createDriverFromBackend(backend);
    const result = await driver.validate('/test.bcp');

    expect(result.valid).toBe(false);
    expect(result.errors).toHaveLength(2);
    expect(result.errors[0]).toContain('invalid magic bytes');
  });
});

describe('decode argument building', () => {
  test('passes mode and budget args', async () => {
    let capturedArgs: readonly string[] = [];
    const backend: BcpBackend = {
      async exec(args) {
        capturedArgs = args;
        return { stdout: 'decoded text', stderr: '', exitCode: 0 };
      },
      async available() {
        return true;
      }
    };

    const driver = createDriverFromBackend(backend);
    await driver.decode('/test.bcp', {
      mode: 'markdown',
      verbosity: 'full',
      tokenBudget: 500,
      include: ['code', 'document']
    });

    expect(capturedArgs).toContain('--mode');
    expect(capturedArgs).toContain('markdown');
    expect(capturedArgs).toContain('--verbosity');
    expect(capturedArgs).toContain('full');
    expect(capturedArgs).toContain('--budget');
    expect(capturedArgs).toContain('500');
    expect(capturedArgs).toContain('--include');
    expect(capturedArgs).toContain('code,document');
  });

  test('omits optional args when not provided', async () => {
    let capturedArgs: readonly string[] = [];
    const backend: BcpBackend = {
      async exec(args) {
        capturedArgs = args;
        return { stdout: 'decoded text', stderr: '', exitCode: 0 };
      },
      async available() {
        return true;
      }
    };

    const driver = createDriverFromBackend(backend);
    await driver.decode('/test.bcp');

    expect(capturedArgs).toContain('decode');
    expect(capturedArgs).toContain('/test.bcp');
    expect(capturedArgs).toContain('--no-color');
    expect(capturedArgs).not.toContain('--mode');
    expect(capturedArgs).not.toContain('--budget');
    expect(capturedArgs).not.toContain('--include');
  });
});

describe('encode argument building', () => {
  test('passes compression and dedup flags', async () => {
    let capturedArgs: readonly string[] = [];
    const backend: BcpBackend = {
      async exec(args) {
        capturedArgs = args;
        return { stdout: '', stderr: '', exitCode: 0 };
      },
      async available() {
        return true;
      }
    };

    const driver = createDriverFromBackend(backend);
    await driver.encode('/manifest.json', '/output.bcp', {
      compressBlocks: true,
      compressPayload: true,
      dedup: true
    });

    expect(capturedArgs).toContain('encode');
    expect(capturedArgs).toContain('/manifest.json');
    expect(capturedArgs).toContain('--output');
    expect(capturedArgs).toContain('/output.bcp');
    expect(capturedArgs).toContain('--compress-blocks');
    expect(capturedArgs).toContain('--compress-payload');
    expect(capturedArgs).toContain('--dedup');
  });
});
