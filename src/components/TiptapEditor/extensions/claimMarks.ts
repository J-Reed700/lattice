import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';

import type { Node as ProseMirrorNode } from '@tiptap/pm/model';

export const claimMarksKey = new PluginKey('claimMarks');

export interface ClaimMark {
  /** The sentence as the verifier saw it: raw markdown, `[n]` markers and all. */
  sentence: string;
  verdict: 'supported' | 'contradicted' | 'unsupported';
}

export interface ClaimMarksOptions {
  /** Read at draw time, so verdicts that arrive after the text still mark it. */
  getClaims: () => readonly ClaimMark[];
}

// Spelled out, not assembled: Tailwind keeps only the class names it can read.
const VERDICT_CLASS: Record<ClaimMark['verdict'], string> = {
  supported: 'claim claim-supported',
  contradicted: 'claim claim-contradicted',
  unsupported: 'claim claim-unsupported',
};

const CITATION_MARKER = /\[\d{1,5}\]/g;
const WORD_CHARACTER = /[\p{L}\p{N}]/u;
/** Shorter than this and a sentence could match somewhere it was never said. */
const MIN_COMPARABLE_LENGTH = 16;

/** One UTF-16 unit as it is compared, or '' when it is not a letter or digit. */
function comparableUnit(unit: string): string {
  if (!WORD_CHARACTER.test(unit)) return '';
  const lower = unit.toLowerCase();
  return lower.length === 1 ? lower : unit;
}

/** Letters and digits only, so markdown syntax and spacing cannot break a match. */
function comparable(sentence: string): string {
  const text = sentence.replace(CITATION_MARKER, '');
  let out = '';
  // By unit, not by code point: document positions are counted in units too.
  for (let i = 0; i < text.length; i += 1) out += comparableUnit(text[i] ?? '');
  return out;
}

/** The document's letters and digits, each with the position it came from. */
function comparableDocument(doc: ProseMirrorNode): { text: string; positions: number[] } {
  let text = '';
  const positions: number[] = [];
  doc.descendants((node, pos, parent) => {
    if (!node.isText || !node.text) return;
    if (parent?.type.spec.code) return;
    if (node.marks.some((mark) => mark.type.spec.code)) return;
    const skipped = new Set<number>();
    for (const match of node.text.matchAll(CITATION_MARKER)) {
      const start = match.index ?? 0;
      for (let i = start; i < start + match[0].length; i += 1) skipped.add(i);
    }
    for (let i = 0; i < node.text.length; i += 1) {
      const unit = skipped.has(i) ? '' : comparableUnit(node.text[i] ?? '');
      if (!unit) continue;
      text += unit;
      positions.push(pos + i);
    }
  });
  return { text, positions };
}

/**
 * Draws each checked sentence of a read-only answer with its verdict.
 *
 * The verifier reports sentences cut from the raw markdown; the viewer holds
 * the rendered document. They are matched on letters and digits alone, in
 * reading order, and a sentence that cannot be found is left unmarked rather
 * than guessed at. Decorations, like the citation chips: the text is untouched.
 * Each mark carries `data-claim="<index into getClaims()>"` for the host.
 */
export const ClaimMarks = Extension.create<ClaimMarksOptions>({
  name: 'claimMarks',

  addOptions() {
    return { getClaims: () => [] };
  },

  addProseMirrorPlugins() {
    const { getClaims } = this.options;
    let cache: { doc: ProseMirrorNode; claims: readonly ClaimMark[]; set: DecorationSet } | null = null;
    return [
      new Plugin({
        key: claimMarksKey,
        props: {
          decorations(state) {
            const claims = getClaims();
            if (claims.length === 0) return DecorationSet.empty;
            if (cache?.doc === state.doc && cache.claims === claims) return cache.set;

            const { text, positions } = comparableDocument(state.doc);
            const decorations: Decoration[] = [];
            let cursor = 0;
            claims.forEach((claim, index) => {
              const needle = comparable(claim.sentence);
              if (needle.length < MIN_COMPARABLE_LENGTH) return;
              let at = text.indexOf(needle, cursor);
              if (at === -1) at = text.indexOf(needle);
              if (at === -1) return;
              const from = positions[at];
              const last = positions[at + needle.length - 1];
              if (from === undefined || last === undefined) return;
              cursor = at + needle.length;
              decorations.push(
                Decoration.inline(from, last + 1, {
                  class: VERDICT_CLASS[claim.verdict],
                  'data-claim': String(index),
                }),
              );
            });

            const set = DecorationSet.create(state.doc, decorations);
            cache = { doc: state.doc, claims, set };
            return set;
          },
        },
      }),
    ];
  },
});
