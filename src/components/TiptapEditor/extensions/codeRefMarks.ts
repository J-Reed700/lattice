import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

/**
 * A line reference: `path:12` or `path:10-24` (an en dash works too), with a
 * path relative to the Explorer's scope. The backend prompt teaches the model
 * this exact grammar, so the two must change together.
 */
const CODE_REF_PATTERN = /^([A-Za-z0-9_@.+\-/]+):(\d+)(?:[-–](\d+))?$/;

export interface CodeRef {
  path: string;
  /** 1-based, inclusive. */
  startLine: number;
  endLine: number;
}

/**
 * Reads a line reference out of the whole content of one inline code span.
 *
 * The path must hold a `/` or a `.`: `foo:12` is as likely a label or a
 * `host:port` as a file. A URL never matches because `:` is not a path
 * character, so `http://x:80` fails at the scheme.
 */
export function parseCodeRef(text: string): CodeRef | null {
  const match = CODE_REF_PATTERN.exec(text.trim());
  if (!match) return null;
  const [, rawPath, start, end] = match;
  if (!rawPath.includes('/') && !rawPath.includes('.')) return null;
  const path = rawPath.replace(/^\.\//, '');
  const startLine = Number(start);
  const endLine = end === undefined ? startLine : Number(end);
  if (startLine < 1 || endLine < 1) return null;
  // A reversed range still names the same lines.
  return { path, startLine: Math.min(startLine, endLine), endLine: Math.max(startLine, endLine) };
}

/** Every line reference written as inline code in a markdown answer, in order. */
export function findCodeRefs(markdown: string): CodeRef[] {
  const refs: CodeRef[] = [];
  // Single-backtick spans only; fenced blocks are code, not references.
  for (const match of markdown.replace(/```[\s\S]*?(```|$)/g, '').matchAll(/`([^`\n]+)`/g)) {
    const ref = parseCodeRef(match[1]);
    if (ref) refs.push(ref);
  }
  return refs;
}

export const codeRefMarksKey = new PluginKey('codeRefMarks');

export interface CodeRefMarksOptions {
  /** Read at draw time, so a host can switch the chips on after mounting. */
  isEnabled: () => boolean;
}

/**
 * Draws line references in a read-only answer as chips.
 *
 * Decorations rather than nodes, like `citationMarks`: the document keeps the
 * literal text, so copy, export and the markdown round-trip are untouched. The
 * chip carries `data-code-ref` and its lines; whoever hosts the viewer handles
 * the click.
 */
export const CodeRefMarks = Extension.create<CodeRefMarksOptions>({
  name: 'codeRefMarks',

  addOptions() {
    return { isEnabled: () => false };
  },

  addProseMirrorPlugins() {
    const { isEnabled } = this.options;
    return [
      new Plugin({
        key: codeRefMarksKey,
        props: {
          decorations(state) {
            if (!isEnabled()) return DecorationSet.empty;
            const decorations: Decoration[] = [];
            state.doc.descendants((node, pos, parent) => {
              if (!node.isText || !node.text) return;
              if (parent?.type.spec.code) return;
              if (!node.marks.some((mark) => mark.type.spec.code)) return;
              const ref = parseCodeRef(node.text);
              if (!ref) return;
              const lines = ref.startLine === ref.endLine ? `line ${ref.startLine}` : `lines ${ref.startLine}–${ref.endLine}`;
              decorations.push(
                Decoration.inline(pos, pos + node.text.length, {
                  class: 'code-ref-chip',
                  'data-code-ref': ref.path,
                  'data-code-ref-start': String(ref.startLine),
                  'data-code-ref-end': String(ref.endLine),
                  role: 'button',
                  'aria-label': `Open ${ref.path}, ${lines}`,
                }),
              );
            });
            return DecorationSet.create(state.doc, decorations);
          },
        },
      }),
    ];
  },
});
