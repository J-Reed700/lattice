import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';
import { CitationMarks, citationMarksKey } from '../extensions/citationMarks';
import { ClaimMarks, claimMarksKey } from '../extensions/claimMarks';
import { CodeRefMarks, codeRefMarksKey } from '../extensions/codeRefMarks';

import type { PluginKey } from '@tiptap/pm/state';
import type { DecorationSet } from '@tiptap/pm/view';

// A diagram's labels look like citations (`A[1]`), file references and
// sentences; none of them may be drawn over.
const ANSWER = [
  'Requests go through the gateway [1]. See src/main.rs:12 for the entry point.',
  '',
  '```mermaid',
  'flowchart LR',
  '  A[1] --> B[2]',
  '  B --> C["see src/main.rs:12"]',
  '  C --> D[Requests go through the gateway.]',
  '```',
  '',
  'Then the worker answers [2].',
].join('\n');

describe('marks skip code blocks', () => {
  it('draws no citation, claim or file-reference mark inside a fence', () => {
    const editor = new Editor({
      extensions: [
        ...createExtensions(),
        CitationMarks.configure({ isCitation: () => true, isVisible: () => true }),
        ClaimMarks.configure({
          getClaims: () => [{ sentence: 'Requests go through the gateway.', verdict: 'supported' }],
        }),
        CodeRefMarks.configure({ isEnabled: () => true }),
        Markdown.configure({ html: false }),
      ],
      content: ANSWER,
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
