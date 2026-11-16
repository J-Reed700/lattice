import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { filePiece, said } from '@/tests/replyFixtures';
import { drawReply, type FileTarget } from '@/utils/replyPieces';

import { createExtensions } from '../extensions';
import { CodeRefMarks, codeRefMarksKey } from '../extensions/codeRefMarks';

import type { DecorationSet } from '@tiptap/pm/view';

function drawnRefs(markdown: string, files: readonly FileTarget[], links = true): string[] {
  const editor = new Editor({
    extensions: [
      ...createExtensions(),
      CodeRefMarks.configure({ isEnabled: () => links, getFiles: () => files }),
      Markdown.configure({ html: false }),
    ],
    content: markdown,
    editable: false,
  });
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === codeRefMarksKey);
  const set = plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet;
  const refs = set
    .find()
    .map((decoration) => (decoration as unknown as { type: { attrs: Record<string, string> } }).type.attrs)
    .filter((attrs) => attrs['data-code-ref'])
    .map((attrs) => {
      const lines = attrs['data-code-ref-start'] ? `:${attrs['data-code-ref-start']}-${attrs['data-code-ref-end']}` : '';
      return `${attrs['data-code-ref']}${lines}`;
    });
  editor.destroy();
  return refs;
}

describe('CodeRefMarks', () => {
  const reply = [
    said('The loop is in '),
    filePiece(1, 'src/main.rs', 3, 6),
    said(' and `foo:12` is a label; see '),
    { type: 'reference', reference: { written: '[f2]', kind: 'file', number: 2, name: 'README.md', lines: null } } as const,
    said('.\n\n```\nsrc/main.rs:9\n```'),
  ];

  it('draws a chip over each file piece of a folder reply and nothing else', () => {
    const drawn = drawReply(reply, true);
    expect(drawn.markdown).toContain('src/main.rs:3-6');
    expect(drawnRefs(drawn.markdown, drawn.files)).toEqual(['src/main.rs:3-6', 'README.md']);
  });

  it('draws no chip outside a folder reply, where a file piece is inline code', () => {
    const drawn = drawReply(reply);
    expect(drawn.files).toEqual([]);
    expect(drawn.markdown).toContain('`src/main.rs:3-6`');
    expect(drawnRefs(drawn.markdown, drawn.files)).toEqual([]);
  });

  it('does not read a chip from text that looks like a reference', () => {
    expect(drawnRefs('See `src/main.rs:3-6` and src/lib.rs:4.', [])).toEqual([]);
  });

  it('draws a chip over a link whose address points into the folder', () => {
    const markdown = 'See [the entry](src/main.rs#L20-L22) and [the docs](https://example.com/a.rs).';
    expect(drawnRefs(markdown, [])).toEqual(['src/main.rs:20-22']);
    expect(drawnRefs(markdown, [], false)).toEqual([]);
  });
});
