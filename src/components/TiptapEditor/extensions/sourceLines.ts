import { Extension, type AnyExtension } from '@tiptap/core';

import type MarkdownIt from 'markdown-it';

const SOURCE_LINE_TYPES = [
  'paragraph',
  'heading',
  'listItem',
  'taskItem',
  'blockquote',
  'codeBlock',
  'tableRow',
] as const;

export const SourceLines: AnyExtension = Extension.create({
  name: 'sourceLines',

  addGlobalAttributes() {
    return [
      {
        types: [...SOURCE_LINE_TYPES],
        attributes: {
          sourceLines: {
            default: null,
            parseHTML: (element: HTMLElement) => {
              const direct = element.getAttribute('data-source-lines');
              if (direct !== null) return direct;
              if (element.tagName === 'PRE') {
                return element.querySelector('code')?.getAttribute('data-source-lines') ?? null;
              }
              return null;
            },
            renderHTML: (attributes: Record<string, unknown>) =>
              typeof attributes.sourceLines === 'string' && attributes.sourceLines
                ? { 'data-source-lines': attributes.sourceLines }
                : {},
          },
        },
      },
    ];
  },

  addStorage() {
    return {
      markdown: {
        parse: {
          setup(md: MarkdownIt) {
            md.core.ruler.after('block', 'sourceLines', (state) => {
              for (const token of state.tokens) {
                if (!token.map) continue;
                if (token.nesting === 1 || token.type === 'fence' || token.type === 'code_block') {
                  token.attrSet('data-source-lines', `${token.map[0]}-${token.map[1]}`);
                }
              }
            });
          },
        },
      },
    };
  },
});
