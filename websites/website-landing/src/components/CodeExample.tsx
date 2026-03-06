const tsCode = `import { createDriver, createContextBuilder } from '@bit-context-protocol/driver';

const driver = createDriver(); // auto-detects WASM or CLI backend

const builder = createContextBuilder({ driver, description: 'Code review context' });

builder.addFile('src/auth.ts', authSource, { priority: 'critical' });
builder.addFile('src/types.ts', typesSource, { priority: 'high' });
builder.addDiff('src/auth.ts', hunks, { priority: 'critical' });
builder.addConversation('user', 'Review the auth refactor');
builder.addToolResult('bun test', testOutput, 'ok');

const result = await builder.buildAndDecode(
  { compressBlocks: true },
  { mode: 'xml', verbosity: 'adaptive', tokenBudget: 8000 }
);
// result.text — model-ready context, 8k tokens, 5 blocks`;

const cliCode = `# Encode a manifest into a .bcp file
$ bcp encode manifest.json -o context.bcp --compress

# Inspect the structure
$ bcp inspect context.bcp
  5 blocks, 12.4 KB (3.8 KB compressed)
  [0] code      — src/auth.ts (critical)
  [1] code      — src/types.ts (high)
  [2] diff      — src/auth.ts (critical)
  [3] conversation — user message (normal)
  [4] tool_result  — bun test (normal)

# Decode with a token budget
$ bcp decode context.bcp --mode xml --budget 8000
  Adaptive: 3 blocks full, 2 blocks summarized`;

export function CodeExample() {
  return (
    <section id="code" className="py-32 relative">
      <div className="max-w-7xl mx-auto px-6">
        <div className="text-center mb-16">
          <h2 className="text-3xl sm:text-4xl font-bold text-white mb-4">
            Two ways to work
          </h2>
          <p className="text-lg text-slate-400 max-w-2xl mx-auto">
            Use the TypeScript driver for programmatic context building, or the CLI for quick operations.
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* TypeScript */}
          <div className="rounded-2xl border border-slate-800/50 bg-slate-900/50 overflow-hidden">
            <div className="flex items-center gap-2 px-4 py-3 border-b border-slate-800/50">
              <div className="flex gap-1.5">
                <div className="w-2.5 h-2.5 rounded-full bg-red-500/50" />
                <div className="w-2.5 h-2.5 rounded-full bg-yellow-500/50" />
                <div className="w-2.5 h-2.5 rounded-full bg-green-500/50" />
              </div>
              <span className="text-xs text-slate-500 font-mono ml-2">TypeScript</span>
            </div>
            <pre className="p-5 overflow-x-auto text-sm leading-relaxed">
              <code className="text-slate-300 font-mono">
                {tsCode.split('\n').map((line, i) => (
                  <div key={i}>
                    {highlightTS(line)}
                  </div>
                ))}
              </code>
            </pre>
          </div>

          {/* CLI */}
          <div className="rounded-2xl border border-slate-800/50 bg-slate-900/50 overflow-hidden">
            <div className="flex items-center gap-2 px-4 py-3 border-b border-slate-800/50">
              <div className="flex gap-1.5">
                <div className="w-2.5 h-2.5 rounded-full bg-red-500/50" />
                <div className="w-2.5 h-2.5 rounded-full bg-yellow-500/50" />
                <div className="w-2.5 h-2.5 rounded-full bg-green-500/50" />
              </div>
              <span className="text-xs text-slate-500 font-mono ml-2">Terminal</span>
            </div>
            <pre className="p-5 overflow-x-auto text-sm leading-relaxed">
              <code className="text-slate-300 font-mono">
                {cliCode.split('\n').map((line, i) => (
                  <div key={i}>
                    {highlightCLI(line)}
                  </div>
                ))}
              </code>
            </pre>
          </div>
        </div>
      </div>
    </section>
  );
}

function highlightTS(line: string) {
  if (line.startsWith('import') || line.startsWith('const') || line.startsWith('  {') || line.startsWith('  const')) {
    return <span>{colorKeywords(line)}</span>;
  }
  if (line.startsWith('//')) {
    return <span className="text-slate-600">{line}</span>;
  }
  return <span>{colorKeywords(line)}</span>;
}

function colorKeywords(line: string) {
  const parts = line.split(/(import|from|const|await|async|true|false)/g);
  return parts.map((part, i) => {
    if (['import', 'from', 'const', 'await', 'async'].includes(part)) {
      return <span key={i} className="text-purple-400">{part}</span>;
    }
    if (['true', 'false'].includes(part)) {
      return <span key={i} className="text-amber-400">{part}</span>;
    }
    if (part.includes("'")) {
      const stringParts = part.split(/('[^']*')/g);
      return stringParts.map((sp, j) =>
        sp.startsWith("'") ? <span key={`${i}-${j}`} className="text-green-400">{sp}</span> : sp
      );
    }
    return part;
  });
}

function highlightCLI(line: string) {
  if (line.startsWith('#')) {
    return <span className="text-slate-600">{line}</span>;
  }
  if (line.startsWith('$')) {
    return (
      <>
        <span className="text-sky-400">$</span>
        <span className="text-white">{line.slice(1)}</span>
      </>
    );
  }
  if (line.includes('—')) {
    const [before, after] = line.split('—');
    return (
      <>
        <span className="text-sky-300">{before}</span>
        <span className="text-slate-500">— {after}</span>
      </>
    );
  }
  return <span className="text-slate-400">{line}</span>;
}
