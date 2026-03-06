// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
  site: 'https://bitcontextprotocol.com',
  base: '/',
  integrations: [
    starlight({
      title: 'Bit Context Protocol',
      logo: {
        light: './src/assets/logo/light.svg',
        dark: './src/assets/logo/dark.svg',
        replacesTitle: false
      },
      favicon: '/favicon.svg',
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/nicholasgalante1997/bit-context-protocol'
        }
      ],
      customCss: ['./src/styles/global.css'],
      defaultLocale: 'root',
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Introduction', slug: '' },
            { label: 'Quick Start', slug: 'quickstart' }
          ]
        },
        {
          label: 'Concepts',
          items: [
            { label: 'Architecture', slug: 'concepts/architecture' },
            { label: 'Data Flow', slug: 'concepts/data-flow' },
            { label: 'Wire Format', slug: 'concepts/wire-format' },
            { label: 'Block Types', slug: 'concepts/block-types' },
            { label: 'Compression', slug: 'concepts/compression' },
            { label: 'Content Addressing', slug: 'concepts/content-addressing' },
            { label: 'Token Budget', slug: 'concepts/token-budget' },
            { label: 'Forward Compatibility', slug: 'concepts/forward-compatibility' }
          ]
        },
        {
          label: 'Guides',
          items: [
            { label: 'Encoding', slug: 'guides/encoding' },
            { label: 'Decoding', slug: 'guides/decoding' },
            { label: 'Rendering', slug: 'guides/rendering' },
            { label: 'CLI Reference', slug: 'guides/cli' },
            { label: 'MCP Server', slug: 'guides/mcp-server' },
            { label: 'Manifest Format', slug: 'guides/manifest-format' }
          ]
        },
        {
          label: 'Crate Reference',
          items: [
            { label: 'bcp-wire', slug: 'reference/crate-bcp-wire' },
            { label: 'bcp-types', slug: 'reference/crate-bcp-types' },
            { label: 'bcp-encoder', slug: 'reference/crate-bcp-encoder' },
            { label: 'bcp-decoder', slug: 'reference/crate-bcp-decoder' },
            { label: 'bcp-driver', slug: 'reference/crate-bcp-driver' },
            { label: 'bcp-cli', slug: 'reference/crate-bcp-cli' },
            { label: 'bcp-tests', slug: 'reference/crate-bcp-tests' }
          ]
        },
        {
          label: 'Reference Tables',
          items: [
            { label: 'Error Catalog', slug: 'reference/errors' },
            { label: 'Shared Enums', slug: 'reference/enums' }
          ]
        },
        {
          label: 'Specification',
          items: [
            { label: 'RFC Draft v0.1.0', slug: 'rfc/specification' }
          ]
        },
        {
          label: 'Resources',
          items: [
            { label: 'Comparison', slug: 'resources/comparison' },
            { label: 'Token Efficiency', slug: 'resources/token-efficiency' },
            { label: 'Roadmap', slug: 'resources/roadmap' },
            { label: 'FAQ', slug: 'resources/faq' },
            { label: 'Migration', slug: 'resources/migration' }
          ]
        }
      ]
    })
  ]
});
