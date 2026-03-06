const packages = [
  {
    name: 'bcp-wire',
    type: 'crate',
    description: 'Wire format primitives',
    href: 'https://crates.io/crates/bcp-wire'
  },
  {
    name: 'bcp-types',
    type: 'crate',
    description: 'Type definitions and block schemas',
    href: 'https://crates.io/crates/bcp-types'
  },
  {
    name: 'bcp-encoder',
    type: 'crate',
    description: 'BCP encoder with compression',
    href: 'https://crates.io/crates/bcp-encoder'
  },
  {
    name: 'bcp-decoder',
    type: 'crate',
    description: 'Streaming BCP decoder',
    href: 'https://crates.io/crates/bcp-decoder'
  },
  {
    name: 'bcp-driver',
    type: 'crate',
    description: 'High-level driver API',
    href: 'https://crates.io/crates/bcp-driver'
  },
  {
    name: 'bcp-cli',
    type: 'crate',
    description: 'CLI tool for .bcp files',
    href: 'https://crates.io/crates/bcp-cli'
  },
  {
    name: '@bit-context-protocol/driver',
    type: 'npm',
    description: 'TypeScript driver with CLI + WASM backends',
    href: 'https://www.npmjs.com/package/@bit-context-protocol/driver'
  },
  {
    name: '@bit-context-protocol/mcp',
    type: 'npm',
    description: 'MCP server for AI clients',
    href: 'https://www.npmjs.com/package/@bit-context-protocol/mcp'
  }
];

export function Ecosystem() {
  return (
    <section id="ecosystem" className="py-32 relative">
      <div className="absolute inset-0 bg-gradient-to-b from-transparent via-sky-950/10 to-transparent" />

      <div className="relative max-w-7xl mx-auto px-6">
        <div className="text-center mb-16">
          <h2 className="text-3xl sm:text-4xl font-bold text-white mb-4">
            The ecosystem
          </h2>
          <p className="text-lg text-slate-400 max-w-2xl mx-auto">
            6 Rust crates on crates.io and 2 TypeScript packages on npm.
            Everything is MIT licensed and ready to use.
          </p>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {packages.map((pkg) => (
            <a
              key={pkg.name}
              href={pkg.href}
              target="_blank"
              rel="noreferrer"
              className="group p-4 rounded-xl border border-slate-800/50 bg-slate-900/30 hover:bg-slate-900/50 hover:border-sky-500/20 transition-all"
            >
              <div className="flex items-center gap-2 mb-2">
                <span
                  className={`text-[10px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded ${
                    pkg.type === 'crate'
                      ? 'bg-orange-500/10 text-orange-400'
                      : 'bg-red-500/10 text-red-400'
                  }`}
                >
                  {pkg.type}
                </span>
              </div>
              <div className="font-mono text-sm text-white group-hover:text-sky-300 transition-colors mb-1 truncate">
                {pkg.name}
              </div>
              <p className="text-xs text-slate-500 leading-relaxed">
                {pkg.description}
              </p>
            </a>
          ))}
        </div>
      </div>
    </section>
  );
}
