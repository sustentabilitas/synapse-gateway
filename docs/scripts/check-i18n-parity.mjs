#!/usr/bin/env node
import {existsSync, readdirSync, realpathSync} from 'node:fs';
import {join, posix} from 'node:path';
import {fileURLToPath} from 'node:url';

export const PAIRS = [
  ['docs', 'i18n/es/docusaurus-plugin-content-docs/current'],
  ['blog', 'i18n/es/docusaurus-plugin-content-blog'],
];

export const REQUIRED = [
  'i18n/es/code.json',
  'i18n/es/docusaurus-theme-classic/navbar.json',
  'i18n/es/docusaurus-theme-classic/footer.json',
  'i18n/es/docusaurus-plugin-content-docs/current.json',
  'i18n/es/docusaurus-plugin-content-blog/options.json',
];

const CONTENT = /\.mdx?$/;

export function listContent(root, prefix = '') {
  if (!existsSync(root)) return [];
  return readdirSync(root, {withFileTypes: true})
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .flatMap((entry) =>
      entry.isDirectory()
        ? listContent(join(root, entry.name), posix.join(prefix, entry.name))
        : CONTENT.test(entry.name)
          ? [posix.join(prefix, entry.name)]
          : [],
    );
}

export function findParityGaps(siteDir, {pairs = PAIRS, required = REQUIRED} = {}) {
  const pairGaps = pairs.flatMap(([en, es]) => {
    const enFiles = listContent(join(siteDir, en));
    const esFiles = listContent(join(siteDir, es));
    const esSet = new Set(esFiles);
    const enSet = new Set(enFiles);
    return [
      ...enFiles
        .filter((file) => !esSet.has(file))
        .map((file) => `missing Spanish translation: ${es}/${file} (source ${en}/${file})`),
      ...esFiles
        .filter((file) => !enSet.has(file))
        .map((file) => `orphan Spanish file: ${es}/${file} (no ${en}/${file})`),
    ];
  });
  const requiredGaps = required
    .filter((file) => !existsSync(join(siteDir, ...file.split('/'))))
    .map((file) => `missing required translation file: ${file}`);
  return [...pairGaps, ...requiredGaps];
}

if (process.argv[1] && fileURLToPath(import.meta.url) === realpathSync(process.argv[1])) {
  const siteDir = fileURLToPath(new URL('..', import.meta.url));
  const gaps = findParityGaps(siteDir);
  if (gaps.length > 0) {
    console.error(`i18n parity check failed (${gaps.length}):\n${gaps.join('\n')}`);
    process.exit(1);
  }
  console.log('i18n parity OK');
}
