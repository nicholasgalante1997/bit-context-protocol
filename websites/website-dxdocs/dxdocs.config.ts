export default {
  title: 'Bit Context Protocol',
  description: 'A binary serialization format that maximizes semantic density within token-constrained LLM context windows.',

  favicon: '/public/favicon.svg',

  navigation: [
    {
      type: 'group',
      title: 'Getting Started',
      items: [
        { type: 'page', path: '/', title: 'Introduction' },
        { type: 'page', path: '/quickstart', title: 'Quick Start' }
      ]
    },
    {
      type: 'group',
      title: 'Concepts',
      items: [
        { type: 'page', path: '/concepts/architecture', title: 'Architecture' },
        { type: 'page', path: '/concepts/data-flow', title: 'Data Flow' },
        { type: 'page', path: '/concepts/wire-format', title: 'Wire Format' },
        { type: 'page', path: '/concepts/block-types', title: 'Block Types' },
        { type: 'page', path: '/concepts/compression', title: 'Compression' },
        { type: 'page', path: '/concepts/content-addressing', title: 'Content Addressing' },
        { type: 'page', path: '/concepts/token-budget', title: 'Token Budget' },
        { type: 'page', path: '/concepts/forward-compatibility', title: 'Forward Compatibility' }
      ]
    },
    {
      type: 'group',
      title: 'Guides',
      items: [
        { type: 'page', path: '/guides/encoding', title: 'Encoding' },
        { type: 'page', path: '/guides/decoding', title: 'Decoding' },
        { type: 'page', path: '/guides/rendering', title: 'Rendering' },
        { type: 'page', path: '/guides/cli', title: 'CLI Reference' },
        { type: 'page', path: '/guides/mcp-server', title: 'MCP Server' },
        { type: 'page', path: '/guides/manifest-format', title: 'Manifest Format' }
      ]
    },
    {
      type: 'group',
      title: 'Crate Reference',
      items: [
        { type: 'page', path: '/reference/crate-bcp-wire', title: 'bcp-wire' },
        { type: 'page', path: '/reference/crate-bcp-types', title: 'bcp-types' },
        { type: 'page', path: '/reference/crate-bcp-encoder', title: 'bcp-encoder' },
        { type: 'page', path: '/reference/crate-bcp-decoder', title: 'bcp-decoder' },
        { type: 'page', path: '/reference/crate-bcp-driver', title: 'bcp-driver' },
        { type: 'page', path: '/reference/crate-bcp-cli', title: 'bcp-cli' },
        { type: 'page', path: '/reference/crate-bcp-tests', title: 'bcp-tests' }
      ]
    },
    {
      type: 'group',
      title: 'Reference Tables',
      items: [
        { type: 'page', path: '/reference/errors', title: 'Error Catalog' },
        { type: 'page', path: '/reference/enums', title: 'Shared Enums' }
      ]
    },
    {
      type: 'group',
      title: 'Specification',
      items: [
        { type: 'page', path: '/rfc/specification', title: 'RFC Draft v0.1.0' }
      ]
    },
    {
      type: 'group',
      title: 'Resources',
      items: [
        { type: 'page', path: '/resources/comparison', title: 'Comparison' },
        { type: 'page', path: '/resources/token-efficiency', title: 'Token Efficiency' },
        { type: 'page', path: '/resources/roadmap', title: 'Roadmap' },
        { type: 'page', path: '/resources/faq', title: 'FAQ' }
      ]
    }
  ],

  headerLinks: [
    { label: 'GitHub', href: 'https://github.com/nicholasgalante1997/bit-context-protocol', icon: 'github' }
  ],

  theme: {
    accentColor: '#6366F1',
    darkMode: 'media'
  },

  output: {
    outDir: './dist'
  }
};
