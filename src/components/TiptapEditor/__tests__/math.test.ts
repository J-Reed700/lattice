import { Editor } from '@tiptap/core';
import { Markdown } from 'tiptap-markdown';
import { afterEach, describe, expect, it } from 'vitest';

import { createExtensions } from '../extensions';
import { CitationMarks, citationMarksKey } from '../extensions/citationMarks';
import { ClaimMarks, claimMarksKey, type ClaimMark } from '../extensions/claimMarks';
import { selectedTextWithFormulas, textWithFormulas, ViewerMath } from '../extensions/math';
import { getMarkdownFromEditor } from '../markdownStorage';

import type { PluginKey } from '@tiptap/pm/state';
import type { DecorationSet } from '@tiptap/pm/view';

const editors: Editor[] = [];

afterEach(() => {
  editors.splice(0).forEach((editor) => editor.destroy());
});

function viewer(markdown: string, claims: readonly ClaimMark[] = []): Editor {
  const element = document.createElement('div');
  document.body.append(element);
  const editor = new Editor({
    element,
    extensions: [
      ...createExtensions(),
      ...ViewerMath,
      CitationMarks.configure({ isCitation: () => true, isVisible: () => true }),
      ClaimMarks.configure({ getClaims: () => claims }),
      Markdown.configure({ html: false }),
    ],
    content: markdown,
    editable: false,
  });
  editors.push(editor);
  return editor;
}

/** Each formula in reading order, as `kind:delimiter:latex`. */
function formulas(markdown: string): string[] {
  const found: string[] = [];
  viewer(markdown).state.doc.descendants((node) => {
    if (node.type.name === 'inlineMath' || node.type.name === 'blockMath') {
      found.push(`${node.type.name}:${String(node.attrs.delimiter)}:${String(node.attrs.latex)}`);
    }
  });
  return found;
}

function decorations(editor: Editor, key: PluginKey) {
  const plugin = editor.state.plugins.find((candidate) => candidate.spec.key === key);
  return (plugin?.props.decorations?.call(plugin, editor.state) as DecorationSet).find();
}

describe('math in the viewer', () => {
  it('reads inline and display formulas in each delimiter', () => {
    expect(formulas('Area is $\\pi r^2$ and \\(x+1\\) here.')).toEqual([
      'inlineMath:$:\\pi r^2',
      'inlineMath:\\(:x+1',
    ]);
    expect(formulas('Mass:\n\n$$\nE = mc^2\n$$\n\nDone.')).toEqual(['blockMath:$$:\nE = mc^2\n']);
    expect(formulas('So\n\\[ a^2 + b^2 = c^2 \\]\nholds.')).toEqual(['blockMath:\\[: a^2 + b^2 = c^2 ']);
    expect(formulas('- item\n\n  $$x_1$$')).toEqual(['blockMath:$$:x_1']);
    // Inside a paragraph, `$$…$$` is a formula in the line.
    expect(formulas('where $$k$$ counts runs')).toEqual(['inlineMath:$$:k']);
  });

  it('leaves prose dollars, escapes, code and unclosed openers as text', () => {
    for (const prose of [
      'It costs $5 and $10 today.',
      'Pay $ 5$ now.',
      'A $x $ gap.',
      'Then $x$5 follows.',
      'A literal \\$x$ sign.',
      'Use `$HOME` and `$x$` in code.',
      '```\n$$x$$\n```',
      'Still arriving: $\\frac{a}{b',
      '$$\n\\sum_i x_i\n',
      'Gap $$\n\nx$$ apart.',
      '\\[Note\\] to self.',
    ]) {
      expect(formulas(prose), prose).toEqual([]);
    }
    expect(viewer('A literal \\$x$ sign.').getText()).toBe('A literal $x$ sign.');
  });

  it('writes each formula back as it was written, and reads it as its source', () => {
    const markdown = 'Area is $\\pi r^2$, \\(x\\) and $$k$$.\n\n$$\nE = mc^2\n$$\n\n\\[a\\]';
    const editor = viewer(markdown);
    expect(getMarkdownFromEditor(editor)).toBe(markdown);
    expect(editor.getText()).toContain('Area is $\\pi r^2$, $x$ and $k$.');
    expect(editor.getText()).toContain('$$\nE = mc^2\n$$');
  });

  it('draws a formula with KaTeX and shows LaTeX it cannot read as its source', () => {
    const dom = viewer('Good $\\frac{a}{b}$, bad $\\frac{a$.\n\n$$\n\\begin{x}\n$$').view.dom;
    expect(dom.querySelector('[data-type="inline-math"] .katex')).not.toBeNull();
    expect(dom.querySelector('.inline-math-error')?.textContent).toBe('\\frac{a');
    expect(dom.querySelector('.block-math-error')?.textContent).toBe('\n\\begin{x}\n');
  });

  it('draws no citation chip for brackets inside a formula', () => {
    const editor = viewer('The sequence $a[1] + a[2]$ grows [3].');
    const chips = decorations(editor, citationMarksKey);
    expect(chips.map((chip) => editor.state.doc.textBetween(chip.from, chip.to))).toEqual(['[3]']);
  });

  it('finds a checked sentence that holds a formula', () => {
    const sentence = 'Precision is $\\frac{TP}{TP + FP}$ over every retrieved passage [1].';
    const editor = viewer(`Intro line here.\n\n${sentence} Next one.`, [{ sentence, verdict: 'supported' }]);
    const [mark] = decorations(editor, claimMarksKey);
    expect(mark).toBeDefined();
    const covered: string[] = [];
    editor.state.doc.nodesBetween(mark!.from, mark!.to, (node) => {
      if (node.type.name === 'inlineMath') covered.push(String(node.attrs.latex));
    });
    expect(covered).toEqual(['\\frac{TP}{TP + FP}']);
    expect(editor.state.doc.textBetween(mark!.from, mark!.to)).toBe('Precision is  over every retrieved passage');
  });

  it('reads rendered formulas back as their source for text taken from the page', () => {
    const editor = viewer('Area is $\\pi r^2$.\n\n$$\nx\n$$');
    const dom = editor.view.dom;
    expect(textWithFormulas(dom)).toBe('Area is $\\pi r^2$.\n\n$$\nx\n$$\n');
    expect(textWithFormulas(viewer('No math here.').view.dom)).toBeNull();

    const katex = dom.querySelector('[data-type="inline-math"] .katex-html');
    const range = document.createRange();
    range.selectNodeContents(katex!);
    expect(selectedTextWithFormulas(range)).toBe('$\\pi r^2$');
  });
});
