import { describe, test, expect, beforeAll } from 'bun:test';
import { createContextBuilder } from '../src/context.ts';
import { createDriver } from '../src/index.ts';
import type { BcpDriver } from '../src/types.ts';
import { resolve } from 'node:path';
import { unlinkSync, existsSync } from 'node:fs';

const BCP_BINARY = resolve(
  import.meta.dir,
  '../../../target/debug/bcp'
);

describe('ContextBuilder', () => {
  let driver: BcpDriver;

  beforeAll(() => {
    driver = createDriver({ binaryPath: BCP_BINARY });
  });

  describe('toManifest()', () => {
    test('empty builder produces empty blocks', () => {
      const ctx = createContextBuilder({ driver });
      const manifest = ctx.toManifest();
      expect(manifest.blocks).toEqual([]);
    });

    test('description is included when provided', () => {
      const ctx = createContextBuilder({
        driver,
        description: 'Test context'
      });
      expect(ctx.toManifest().description).toBe('Test context');
    });

    test('description is omitted when not provided', () => {
      const ctx = createContextBuilder({ driver });
      expect(ctx.toManifest().description).toBeUndefined();
    });

    test('addCode produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addCode('src/main.rs', 'fn main() {}', 'rust', {
        priority: 'critical',
        summary: 'Entry point'
      });

      const manifest = ctx.toManifest();
      expect(manifest.blocks).toHaveLength(1);
      expect(manifest.blocks[0]).toEqual({
        type: 'code',
        lang: 'rust',
        path: 'src/main.rs',
        content: 'fn main() {}',
        priority: 'critical',
        summary: 'Entry point'
      });
    });

    test('addFile infers language from extension', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addFile('src/app.tsx', 'export const App = () => <div/>;');

      const block = ctx.toManifest().blocks[0]!;
      expect(block['lang']).toBe('typescript');
    });

    test('addFile with explicit lang overrides inference', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addFile('config', 'key=value', { lang: 'ini' });

      const block = ctx.toManifest().blocks[0]!;
      expect(block['lang']).toBe('ini');
    });

    test('addConversation produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addConversation('user', 'Fix the bug');

      expect(ctx.toManifest().blocks[0]).toEqual({
        type: 'conversation',
        role: 'user',
        content: 'Fix the bug'
      });
    });

    test('addToolResult produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addToolResult('cargo_test', 'ok: 5 passed', 'ok', {
        summary: '5 tests passed'
      });

      const block = ctx.toManifest().blocks[0]!;
      expect(block['type']).toBe('tool_result');
      expect(block['name']).toBe('cargo_test');
      expect(block['status']).toBe('ok');
      expect(block['summary']).toBe('5 tests passed');
    });

    test('addDocument produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addDocument('README', '# Hello', { format: 'markdown' });

      const block = ctx.toManifest().blocks[0]!;
      expect(block['type']).toBe('document');
      expect(block['title']).toBe('README');
      expect(block['format']).toBe('markdown');
    });

    test('addStructuredData produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addStructuredData('json', '{"key":"value"}');

      expect(ctx.toManifest().blocks[0]).toEqual({
        type: 'structured_data',
        format: 'json',
        content: '{"key":"value"}'
      });
    });

    test('addDiff produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addDiff('src/lib.rs', [
        { old_start: 1, new_start: 1, lines: '-old\n+new\n' }
      ]);

      const block = ctx.toManifest().blocks[0]!;
      expect(block['type']).toBe('diff');
      expect(block['path']).toBe('src/lib.rs');
      expect(block['hunks']).toHaveLength(1);
    });

    test('addAnnotation produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addCode('main.go', 'package main', 'go');
      ctx.addAnnotation(0, 'tag', 'entry-point');

      const block = ctx.toManifest().blocks[1]!;
      expect(block['type']).toBe('annotation');
      expect(block['target_block_id']).toBe(0);
    });

    test('addFileTree produces correct block', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addFileTree('src/', [
        { name: 'index.ts', kind: 'file', size: 100 },
        {
          name: 'utils',
          kind: 'dir',
          children: [{ name: 'helper.ts', kind: 'file', size: 50 }]
        }
      ]);

      const block = ctx.toManifest().blocks[0]!;
      expect(block['type']).toBe('file_tree');
      expect(block['root']).toBe('src/');
    });

    test('chaining works', () => {
      const ctx = createContextBuilder({ driver });
      ctx
        .addCode('a.ts', 'const a = 1;', 'typescript')
        .addCode('b.ts', 'const b = 2;', 'typescript')
        .addConversation('user', 'Review these files');

      expect(ctx.blockCount).toBe(3);
    });

    test('optional fields are omitted when not provided', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addCode('test.rs', 'fn test() {}', 'rust');

      const block = ctx.toManifest().blocks[0]!;
      expect(block).not.toHaveProperty('priority');
      expect(block).not.toHaveProperty('summary');
    });

    test('addBlock allows raw blocks', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addBlock({
        type: 'extension',
        namespace: 'com.example',
        type_name: 'custom',
        content: 'hello'
      });

      expect(ctx.toManifest().blocks[0]!['type']).toBe('extension');
    });
  });

  describe('toJSON()', () => {
    test('produces valid JSON string', () => {
      const ctx = createContextBuilder({ driver });
      ctx.addCode('test.py', 'print("hi")', 'python');

      const json = ctx.toJSON();
      const parsed = JSON.parse(json) as { blocks: unknown[] };
      expect(parsed.blocks).toHaveLength(1);
    });
  });

  describe('blockCount', () => {
    test('tracks accumulated blocks', () => {
      const ctx = createContextBuilder({ driver });
      expect(ctx.blockCount).toBe(0);
      ctx.addCode('a.ts', 'a', 'typescript');
      expect(ctx.blockCount).toBe(1);
      ctx.addConversation('user', 'hi');
      expect(ctx.blockCount).toBe(2);
    });
  });

  describe('build()', () => {
    test('encodes to .bcp file and validates', async () => {
      const ctx = createContextBuilder({
        driver,
        description: 'Integration test'
      });

      ctx
        .addCode('src/main.rs', 'fn main() {\n    println!("Hello!");\n}', 'rust', {
          priority: 'critical'
        })
        .addConversation('user', 'Add error handling')
        .addToolResult('cargo_check', 'no warnings');

      const result = await ctx.build();

      expect(result.blockCount).toBe(3);
      expect(existsSync(result.bcpPath)).toBe(true);
      expect(existsSync(result.manifestPath)).toBe(true);

      // Validate the produced .bcp file
      const validation = await driver.validate(result.bcpPath);
      expect(validation.valid).toBe(true);

      // Inspect it — priority creates an extra ANNOTATION block
      const inspection = await driver.inspect(result.bcpPath);
      expect(inspection.blockCount).toBeGreaterThanOrEqual(3);

      // Clean up
      unlinkSync(result.bcpPath);
      unlinkSync(result.manifestPath);
    });

    test('encodes to custom output path', async () => {
      const ctx = createContextBuilder({ driver });
      ctx.addCode('test.go', 'package main', 'go');

      const outputPath = `/tmp/bcp-test-custom-${Date.now()}.bcp`;
      const result = await ctx.build({ outputPath });

      expect(result.bcpPath).toBe(outputPath);
      expect(existsSync(outputPath)).toBe(true);

      unlinkSync(result.bcpPath);
      unlinkSync(result.manifestPath);
    });
  });

  describe('buildAndDecode()', () => {
    test('produces decoded text from built context', async () => {
      const ctx = createContextBuilder({ driver });
      ctx
        .addCode('main.rs', 'fn main() { println!("BCP"); }', 'rust')
        .addDocument('README', '# My Project', { format: 'markdown' });

      const result = await ctx.buildAndDecode(undefined, { mode: 'xml' });

      expect(result.blockCount).toBe(2);
      expect(result.text).toContain('BCP');
      expect(result.text).toContain('My Project');
      expect(result.bcpPath).toBeTruthy();

      unlinkSync(result.bcpPath);
    });

    test('respects token budget in decode', async () => {
      const ctx = createContextBuilder({ driver });

      for (let i = 0; i < 5; i++) {
        ctx.addCode(
          `file${i}.rs`,
          `fn func_${i}() { /* implementation ${i} */ }`,
          'rust',
          { priority: i === 0 ? 'critical' : 'background' }
        );
      }

      const result = await ctx.buildAndDecode(undefined, {
        mode: 'xml',
        tokenBudget: 50
      });

      expect(result.text.length).toBeGreaterThan(0);

      unlinkSync(result.bcpPath);
    });
  });

  describe('language inference', () => {
    const cases: Array<[string, string]> = [
      ['app.ts', 'typescript'],
      ['app.tsx', 'typescript'],
      ['app.js', 'javascript'],
      ['app.jsx', 'javascript'],
      ['lib.rs', 'rust'],
      ['script.py', 'python'],
      ['main.go', 'go'],
      ['App.java', 'java'],
      ['style.css', 'css'],
      ['page.html', 'html'],
      ['config.yml', 'yaml'],
      ['config.yaml', 'yaml'],
      ['data.json', 'json'],
      ['config.toml', 'toml'],
      ['README.md', 'markdown'],
      ['query.sql', 'sql'],
      ['Dockerfile', 'dockerfile'],
      ['unknown.xyz', 'xyz'],
      ['noext', 'text']
    ];

    for (const [path, expected] of cases) {
      test(`${path} -> ${expected}`, () => {
        const ctx = createContextBuilder({ driver });
        ctx.addFile(path, 'content');
        expect(ctx.toManifest().blocks[0]!['lang']).toBe(expected);
      });
    }
  });
});
