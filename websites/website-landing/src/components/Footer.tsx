export function Footer() {
  return (
    <footer className="border-t border-slate-800/50 py-12">
      <div className="max-w-7xl mx-auto px-6">
        <div className="flex flex-col md:flex-row items-center justify-between gap-6">
          <div className="flex items-center gap-2.5">
            <div className="w-7 h-7 rounded-md bg-gradient-to-br from-sky-400 to-blue-600 flex items-center justify-center font-mono font-bold text-xs text-white">
              B
            </div>
            <span className="text-sm text-slate-400">
              Bit Context Protocol
            </span>
          </div>

          <div className="flex items-center gap-6 text-sm text-slate-500">
            <a
              href="https://docs.bitcontextprotocol.com"
              className="hover:text-slate-300 transition-colors"
            >
              Docs
            </a>
            <a
              href="https://github.com/mega-blastoise/bit-context-protocol"
              target="_blank"
              rel="noreferrer"
              className="hover:text-slate-300 transition-colors"
            >
              GitHub
            </a>
            <a
              href="https://crates.io/crates/bcp-cli"
              target="_blank"
              rel="noreferrer"
              className="hover:text-slate-300 transition-colors"
            >
              crates.io
            </a>
            <a
              href="https://www.npmjs.com/org/bit-context-protocol"
              target="_blank"
              rel="noreferrer"
              className="hover:text-slate-300 transition-colors"
            >
              npm
            </a>
          </div>

          <p className="text-xs text-slate-600">
            MIT License &copy; {new Date().getFullYear()}
          </p>
        </div>
      </div>
    </footer>
  );
}
