import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import {themes as prismThemes} from 'prism-react-renderer';

const REPO = 'https://github.com/sustentabilitas/synapse-gateway';

// Read the Docs serves each version under a path such as /en/latest/.
const site = new URL(process.env.READTHEDOCS_CANONICAL_URL || 'http://localhost:3000/');
const baseUrl = site.pathname.endsWith('/') ? site.pathname : `${site.pathname}/`;

const config: Config = {
  title: 'Synapse',
  tagline: 'The LLM gateway that keeps native power',
  favicon: 'img/favicon.svg',

  future: {
    v4: true,
  },

  url: site.origin,
  baseUrl,
  trailingSlash: true,

  organizationName: 'sustentabilitas',
  projectName: 'synapse-gateway',

  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',

  markdown: {
    format: 'detect',
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'es'],
    localeConfigs: {
      en: {label: 'English', htmlLang: 'en-GB'},
      es: {label: 'Español', htmlLang: 'es-ES'},
    },
  },

  presets: [
    [
      'classic',
      {
        docs: {
          path: 'docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars.ts',
          editUrl: `${REPO}/tree/main/docs/`,
          editLocalizedFiles: true,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themes: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        language: ['en', 'es'],
        docsRouteBasePath: '/docs',
        indexBlog: false,
        highlightSearchTermsOnTargetPage: true,
      },
    ],
  ],

  themeConfig: {
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    image: 'img/social-card.png',
    navbar: {
      title: 'Synapse',
      logo: {alt: 'Synapse', src: 'img/logo.svg', srcDark: 'img/logo-dark.svg'},
      items: [
        {type: 'docSidebar', sidebarId: 'docsSidebar', position: 'left', label: 'Docs'},
        {
          type: 'dropdown',
          label: 'Synapse family',
          position: 'left',
          items: [
            {to: '/docs/overview/introduction/', label: 'Gateway', activeBaseRegex: '/docs/(?!synapse-family/)'},
            {to: '/docs/synapse-family/proxy/overview/', label: 'Proxy', activeBasePath: 'docs/synapse-family/proxy'},
            {to: '/docs/synapse-family/a2a/overview/', label: 'A2A', activeBasePath: 'docs/synapse-family/a2a'},
            {to: '/docs/synapse-family/mcp/overview/', label: 'MCP', activeBasePath: 'docs/synapse-family/mcp'},
          ],
        },
        {type: 'localeDropdown', position: 'right'},
        {href: REPO, label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {label: 'Introduction', to: '/docs/overview/introduction/'},
            {label: 'Quickstart', to: '/docs/get-started/quickstart/'},
            {label: 'Configuration', to: '/docs/configuration/routes/'},
            {label: 'HTTP API', to: '/docs/reference/http-api/'},
          ],
        },
        {
          title: 'Community',
          items: [
            {label: 'GitHub', href: REPO},
            {label: 'Contributing', to: '/docs/contributing/'},
            {label: 'Security', to: '/docs/operating/security/'},
            {label: 'Code of Conduct', href: `${REPO}/blob/main/CODE_OF_CONDUCT.md`},
          ],
        },
        {
          title: 'More',
          items: [
            {label: 'crates.io', href: 'https://crates.io/crates/synapse-gateway'},
            {label: 'Docker Hub', href: 'https://hub.docker.com/r/sustentabilitas/synapse-gateway'},
            {label: 'docs.rs', href: 'https://docs.rs/synapse-gateway'},
          ],
        },
      ],
      copyright: `AGPL-3.0 licensed · © ${new Date().getFullYear()} Sustentabilitas`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['toml', 'rust'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
