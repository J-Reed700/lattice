import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { cites, filePiece, said } from '@/tests/replyFixtures';
import { drawReply, findReplyMarks } from '@/utils/replyPieces';

import { createExtensions } from '../extensions';
import { CitationMarks, citationMarksKey } from '../extensions/citationMarks';
import { ClaimMarks, claimMarksKey } from '../extensions/claimMarks';
import { CodeRefMarks, codeRefMarksKey } from '../extensions/codeRefMarks';

import type { PluginKey } from '@tiptap/pm/state';
import type { DecorationSet } from '@tiptap/pm/view';

// A diagram's labels look like citations (`A[1]`), file references and
// sentences; none of them may be drawn over.
const ANSWER = drawReply([
  said('Requests go through the gateway '), cites(1), said('. See '), filePiece(1, 'src/main.rs', 12, 12),
  said(' for the entry point.\n\n'),
  said('```mermaid\nflowchart LR\n  A'), cites(7), said(' --> B[2]\n'),
  said('  B --> C["see src/main.rs:12"]\n'),
  said('  C --> D[Requests go through the gateway.]\n'),
  said('```\n\n'),
  said('Then the worker answers '), cites(2), said('.'),
], true);

describe('marks skip code blocks', () => {
  it('draws no citation, claim or file-reference mark inside a fence', () => {
    const editor = new Editor({
      extensions: [
        ...createExtensions(),
        CitationMarks.configure({ isCitation: () => true, findMarks: (text) => findReplyMarks(text, ANSWER.numbers), isVisible: () => true }),
        ClaimMarks.configure({
          getClaims: () => [{ sentence: 'Requests go through the gateway.', verdict: 'supported' }],
        }),
        CodeRefMarks.configure({ isEnabled: () => true, getFiles: () => ANSWER.files }),
        Markdown.configure({ html: false }),
      ],
      content: ANSWER.markdown,
      editable: false,
    });

    const code: Array<[number, number]> = [];
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === 'codeBlock') code.push([pos, pos + node.nodeSize]);
    });
    expect(code).toHaveLength(1);

    const drawn = (key: PluginKey) => {
      const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === key);
      return (plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet).find();
    };
    for (const key of [citationMarksKey, claimMarksKey, codeRefMarksKey]) {
      const decorations = drawn(key);
      // Each still draws in the prose around the fence.
      expect(decorations.length).toBeGreaterThan(0);
      for (const decoration of decorations) {
        const inside = code.some(([from, to]) => decoration.from < to && decoration.to > from);
        expect(inside, `${String(key)} at ${decoration.from}`).toBe(false);
      }
    }
    editor.destroy();
  });
});
