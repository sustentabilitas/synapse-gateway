import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import {themes as prismThemes} from 'prism-react-renderer';

const REPO = 'https://github.com/sustentabilitas/synapse-gateway';

// Read the Docs serves each version under a path such as /en/latest/.
const site = new URL(process.env.READTHEDOCS_CANONICAL_URL ?? 'http://localhost:3000/');
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
    navbar: {
      title: 'Synapse',
      items: [
        {type: 'docSidebar', sidebarId: 'docsSidebar', position: 'left', label: 'Docs'},
        {type: 'localeDropdown', position: 'right'},
        {href: REPO, label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      style: 'dark',
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
