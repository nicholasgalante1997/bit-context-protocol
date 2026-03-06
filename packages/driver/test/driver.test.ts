import { describe, test, expect, beforeAll } from 'bun:test';
import { createDriver } from '../src/index.ts';
import type { BcpDriver } from '../src/types.ts';
import { resolve } from 'node:path';

const GOLDEN_DIR = resolve(
  import.meta.dir,
  '../../../crates/bcp-tests/tests/golden'
);

const fixture = (name: string) => resolve(GOLDEN_DIR, name, 'payload.bcp');

describe('BcpDriver (CLI backend)', () => {
  let driver: BcpDriver;
  let isAvailable: boolean;

  beforeAll(async () => {
    driver = createDriver({
      binaryPath: resolve(
        import.meta.dir,
        '../../../target/debug/bcp'
      )
    });
    isAvailable = await driver.available();
  });

  describe('available()', () => {
    test('returns true when binary exists', () => {
      expect(isAvailable).toBe(true);
    });

    test('returns false for non-existent binary', async () => {
      const bad = createDriver({ binaryPath: '/nonexistent/bcp' });
      expect(await bad.available()).toBe(false);
    });
  });

  describe('inspect()', () => {
    test('simple_code: 1 block', async () => {
      const result = await driver.inspect(fixture('simple_code'));
      expect(result.blockCount).toBe(1);
      expect(result.blocks).toHaveLength(1);
      expect(result.blocks[0]!.type).toBe('CODE');
      expect(result.blocks[0]!.size).toBe(42);
      expect(result.totalSize).toBe(42);
    });

    test('all_block_types: 11 blocks', async () => {
      const result = await driver.inspect(fixture('all_block_types'));
      expect(result.blockCount).toBe(11);
      expect(result.blocks).toHaveLength(11);

      const types = result.blocks.map((b) => b.type);
      expect(types).toContain('CODE');
      expect(types).toContain('CONVERSATION');
      expect(types).toContain('FILE_TREE');
      expect(types).toContain('DOCUMENT');
    });

    test('mixed_blocks: multiple blocks', async () => {
      const result = await driver.inspect(fixture('mixed_blocks'));
      expect(result.blockCount).toBeGreaterThan(1);
      expect(result.totalSize).toBeGreaterThan(0);
    });

    test('includes raw output', async () => {
      const result = await driver.inspect(fixture('simple_code'));
      expect(result.raw).toContain('Block 0');
      expect(result.raw).toContain('CODE');
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
      expect(result.text).toContain('<code');
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

    test('decode with include filter', async () => {
      const result = await driver.decode(fixture('all_block_types'), {
        mode: 'xml',
        include: ['code']
      });
      expect(result.text).toContain('<code');
      // Should not contain other block types
      expect(result.text).not.toContain('<conversation');
    });

    test('decode with token budget', async () => {
      const result = await driver.decode(fixture('all_block_types'), {
        mode: 'xml',
        tokenBudget: 100
      });
      expect(result.text.length).toBeGreaterThan(0);
    });
  });

  describe('error handling', () => {
    test('inspect non-existent file throws', async () => {
      await expect(driver.inspect('/nonexistent.bcp')).rejects.toThrow(
        'bcp inspect failed'
      );
    });

    test('decode non-existent file throws', async () => {
      await expect(driver.decode('/nonexistent.bcp')).rejects.toThrow(
        'bcp decode failed'
      );
    });

    test('validate non-existent file returns invalid', async () => {
      const result = await driver.validate('/nonexistent.bcp');
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });

    test('unavailable backend throws on decode', async () => {
      const bad = createDriver({ binaryPath: '/nonexistent/bcp' });
      await expect(bad.decode('/any.bcp')).rejects.toThrow(
        'bcp backend is not available'
      );
    });
  });
});
