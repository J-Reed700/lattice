import { Mark, mergeAttributes } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

export interface WikilinkOptions {
  HTMLAttributes: Record<string, unknown>;
  onWikilinkClick?: (title: string) => void;
}

/**
 * Inline wikilink mark that renders [[text]] as a clickable link.
 * In markdown, wikilinks are stored as [[title]] and displayed
 * as styled inline elements.
 */
export const Wikilink = Mark.create<WikilinkOptions>({
  name: 'wikilink',
  inclusive: false,
  excludes: '_',

  addOptions() {
    return {
      HTMLAttributes: {},
      onWikilinkClick: undefined,
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-wikilink]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        'data-wikilink': '',
        class:
          'text-[hsl(var(--accent))] cursor-pointer hover:underline font-medium',
      }),
      0,
    ];
  },

  addProseMirrorPlugins() {
    const onClick = this.options.onWikilinkClick;
    return [
      new Plugin({
        key: new PluginKey('wikilinkClick'),
        props: {
          handleClick(_view, _pos, event) {
            const target = event.target;
            if (!(target instanceof HTMLElement) || !onClick) return false;
            // The mark renders `data-wikilink`; the decoration path (plain
            // `[[text]]` that has not been converted yet) renders
            // `data-wikilink-decoration` and carries the name as its value.
            if (target.hasAttribute('data-wikilink')) {
              onClick(target.textContent ?? '');
              return true;
            }
            const decorated = target.getAttribute('data-wikilink-decoration');
            if (decorated !== null) {
              onClick(decorated);
              return true;
            }
            return false;
          },
        },
      }),
      // Decorations plugin: highlights [[...]] patterns in plain text
      // so they appear styled even before markdown conversion
      new Plugin({
        key: new PluginKey('wikilinkDecorations'),
        props: {
          decorations(state) {
            const decorations: Decoration[] = [];
            state.doc.descendants((node, pos) => {
              if (!node.isText || !node.text) return;
              const regex = /\[\[([^\]]+)\]\]/g;
              let match: RegExpExecArray | null;
              while ((match = regex.exec(node.text)) !== null) {
                const from = pos + match.index;
                const to = from + match[0].length;
                decorations.push(
                  Decoration.inline(from, to, {
                    class:
                      'text-[hsl(var(--accent))] cursor-pointer hover:underline font-medium',
                    'data-wikilink-decoration': match[1],
                  }),
                );
              }
            });
            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
