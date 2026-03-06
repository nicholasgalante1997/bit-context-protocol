import { describe, test, expect, beforeAll } from 'bun:test';
import { createDriver } from '../src/index.ts';
import type { BcpDriver } from '../src/types.ts';
import { resolve } from 'node:path';

const GOLDEN_DIR = resolve(
  import.meta.dir,
  '../../../crates/bcp-tests/tests/golden'
);

const fixture = (name: string) => resolve(GOLDEN_DIR, name, 'payload.bcp');

describe('BcpDriver (WASM backend)', () => {
  let driver: BcpDriver;
  let isAvailable: boolean;

  beforeAll(async () => {
    driver = createDriver({ backend: 'wasm' });
    isAvailable = await driver.available();
  });

  describe('available()', () => {
    test('returns true when WASM module is built', () => {
      expect(isAvailable).toBe(true);
    });
  });

  describe('inspect()', () => {
    test('simple_code: 1 block', async () => {
      const result = await driver.inspect(fixture('simple_code'));
      expect(result.blockCount).toBe(1);
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0]!.type).toBe('Code');
      expect(result.blocks[0]!.size).toBe(42);
    });

    test('all_block_types: 11 blocks', async () => {
      const result = await driver.inspect(fixture('all_block_types'));
      expect(result.blockCount).toBe(11);
    });
  });

  describe('validate()', () => {
    test('valid file returns valid: true', async () => {
      const result = await driver.validate(fixture('simple_code'));
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    test('all golden files are valid', async () => {
      const files = [
        'simple_code',
        'all_block_types',
        'mixed_blocks',
        'conversation',
        'with_summaries'
      ];
      for (const name of files) {
        const result = await driver.validate(fixture(name));
        expect(result.valid).toBe(true);
      }
    });
  });

  describe('decode()', () => {
    test('simple_code in xml mode', async () => {
      const result = await driver.decode(fixture('simple_code'), {
        mode: 'xml'
      });
      expect(result.text).toContain('<context>');
      expect(result.text).toContain('Hello, BCP!');
    });

    test('simple_code in markdown mode', async () => {
      const result = await driver.decode(fixture('simple_code'), {
        mode: 'markdown'
      });
      expect(result.text).toContain('Hello, BCP!');
    });

    test('simple_code in minimal mode', async () => {
      const result = await driver.decode(fixture('simple_code'), {
        mode: 'minimal'
      });
      expect(result.text).toContain('Hello, BCP!');
    });

    test('decode with token budget', async () => {
      const result = await driver.decode(fixture('all_block_types'), {
        mode: 'xml',
        tokenBudget: 100
      });
      expect(result.text.length).toBeGreaterThan(0);
    });
  });

  describe('encode()', () => {
    test('encodes manifest to .bcp and validates round-trip', async () => {
      const manifestPath = resolve(GOLDEN_DIR, 'simple_code', 'manifest.json');
      const outputPath = `/tmp/bcp-wasm-test-${Date.now()}.bcp`;

      await driver.encode(manifestPath, outputPath);

      const validation = await driver.validate(outputPath);
      expect(validation.valid).toBe(true);

      const inspection = await driver.inspect(outputPath);
      expect(inspection.blockCount).toBe(1);

      const { unlinkSync } = await import('node:fs');
      unlinkSync(outputPath);
    });
  });

  describe('auto-detect', () => {
    test('createDriver() without backend auto-resolves', async () => {
      const auto = createDriver();
      const available = await auto.available();
      expect(available).toBe(true);

      const result = await auto.decode(fixture('simple_code'), { mode: 'xml' });
      expect(result.text).toContain('Hello, BCP!');
    });
  });
});
