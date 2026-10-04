import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';
import { CodeRefMarks, codeRefMarksKey, findCodeRefs, parseCodeRef } from '../extensions/codeRefMarks';

import type { DecorationSet } from '@tiptap/pm/view';

describe('parseCodeRef', () => {
  it.each([
    ['src/a.rs:10-24', { path: 'src/a.rs', startLine: 10, endLine: 24 }],
    ['README.md:3', { path: 'README.md', startLine: 3, endLine: 3 }],
    ['src/main.rs:3–6', { path: 'src/main.rs', startLine: 3, endLine: 6 }],
    ['./lib/util.ts:7', { path: 'lib/util.ts', startLine: 7, endLine: 7 }],
    ['@scope/pkg/index.js:12-4', { path: '@scope/pkg/index.js', startLine: 4, endLine: 12 }],
  ])('reads %s', (text, expected) => {
    expect(parseCodeRef(text)).toEqual(expected);
  });

  it.each([
    'foo:12', // no `/` or `.`: a label, not a path
    'http://x:80', // a URL
    'localhost:8080',
    'src/a.rs', // no line
    'src/a.rs:', // no line
    'src/a.rs:0',
    'src/a.rs:10-', // dangling range
    'src/a rs:10', // a space is not a path character
    'let x = a.b:3',
    'fn main() {}',
  ])('leaves %s as ordinary code', (text) => {
    expect(parseCodeRef(text)).toBeNull();
  });
});

describe('findCodeRefs', () => {
  it('waits for a reference still being written at the end of a streaming answer', () => {
    expect(findCodeRefs('Look at src/a.rs:1', { complete: false })).toEqual([]);
    expect(findCodeRefs('Look at src/a.rs:12 and', { complete: false })).toEqual([
      { path: 'src/a.rs', startLine: 12, endLine: 12 },
    ]);
    expect(findCodeRefs('Look at src/a.rs:12')).toEqual([{ path: 'src/a.rs', startLine: 12, endLine: 12 }]);
  });

  it('finds inline references in order and skips fenced code', () => {
    const markdown = [
      'Start at `src/main.rs:3-6`, then `README.md:1`.',
      '```',
      '`src/hidden.rs:1`',
      '```',
      'Not this: `foo:2`.',
    ].join('\n');
    expect(findCodeRefs(markdown).map((ref) => ref.path)).toEqual(['src/main.rs', 'README.md']);
  });
});

function drawnRefs(markdown: string, enabled: boolean): string[] {
  const editor = new Editor({
    extensions: [
      ...createExtensions(),
      CodeRefMarks.configure({ isEnabled: () => enabled }),
      Markdown.configure({ html: false }),
    ],
    content: markdown,
    editable: false,
  });
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === codeRefMarksKey);
  const set = plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet;
  const refs = set.find().map((decoration) => {
    const attrs = (decoration as unknown as { type: { attrs: Record<string, string> } }).type.attrs;
    const lines = attrs['data-code-ref-start'] ? `:${attrs['data-code-ref-start']}-${attrs['data-code-ref-end']}` : '';
    return `${attrs['data-code-ref']}${lines}`;
  });
  editor.destroy();
  return refs;
}

describe('CodeRefMarks', () => {
  const answer = 'The loop is in `src/main.rs:3-6`; `foo:12` is a label and `README.md:2` explains it.\n\n```\nsrc/main.rs:9\n```';

  it('decorates only inline code that is a line reference', () => {
    expect(drawnRefs(answer, true)).toEqual(['src/main.rs:3-6', 'README.md:2-2']);
  });

  it('draws nothing outside an explorer conversation', () => {
    expect(drawnRefs(answer, false)).toEqual([]);
  });

  /** Models do not keep to the backticks the prompt asks for. */
  it('draws references however the answer formats them', () => {
    const prose = [
      'The facade is src/lib/api.ts [package.json:51; src-tauri/Cargo.toml:35-38], registered at',
      '[src-tauri/src/main.rs:9, 64-67]. The loop is in **src/main.rs**:12 and `lib.rs` line 4;',
      'see lines 3–9 of `util.ts`, [the entry](src/main.rs#L20-L22) and [the docs](https://example.com/a.rs).',
    ].join(' ');
    expect(drawnRefs(prose, true)).toEqual([
      'src/lib/api.ts',
      'package.json:51-51',
      'src-tauri/Cargo.toml:35-38',
      'src-tauri/src/main.rs:9-9',
      'src-tauri/src/main.rs:64-67',
      'src/main.rs:12-12',
      'lib.rs:4-4',
      'util.ts:3-9',
      'src/main.rs:20-22',
    ]);
  });
});
