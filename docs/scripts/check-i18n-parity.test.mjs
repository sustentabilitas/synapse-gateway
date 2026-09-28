import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync, mkdirSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join} from 'node:path';
import {findParityGaps} from './check-i18n-parity.mjs';

const PAIRS = [['docs', 'es/docs']];

function site(files) {
  const dir = mkdtempSync(join(tmpdir(), 'parity-'));
  files.forEach((file) => {
    mkdirSync(dirname(join(dir, file)), {recursive: true});
    writeFileSync(join(dir, file), '# page\n');
  });
  return dir;
}

test('matching trees have no gaps', () => {
  const dir = site(['docs/a.md', 'docs/sub/b.md', 'es/docs/a.md', 'es/docs/sub/b.md']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: []}), []);
});

test('reports an English page with no Spanish twin', () => {
  const dir = site(['docs/a.md', 'docs/sub/b.md', 'es/docs/a.md']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: []}), [
    'missing Spanish translation: es/docs/sub/b.md (source docs/sub/b.md)',
  ]);
});

test('reports a Spanish page with no English source', () => {
  const dir = site(['docs/a.md', 'es/docs/a.md', 'es/docs/old.md']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: []}), [
    'orphan Spanish file: es/docs/old.md (no docs/old.md)',
  ]);
});

test('ignores non-markdown files', () => {
  const dir = site(['docs/a.md', 'docs/_category_.yaml', 'es/docs/a.md']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: []}), []);
});

test('treats .md and .mdx as content', () => {
  const dir = site(['docs/a.mdx', 'es/docs/a.mdx']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: []}), []);
});

test('reports missing required files', () => {
  const dir = site(['docs/a.md', 'es/docs/a.md']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: ['es/code.json']}), [
    'missing required translation file: es/code.json',
  ]);
});

test('a missing Spanish directory reports every English page', () => {
  const dir = site(['docs/a.md']);
  assert.deepEqual(findParityGaps(dir, {pairs: PAIRS, required: []}), [
    'missing Spanish translation: es/docs/a.md (source docs/a.md)',
  ]);
});
