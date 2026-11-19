import { describe, expect, it } from 'vitest';

import { articleBlocks } from '../articleBlocks';

/** Each block as [kind, visible text, level], read back from the original text. */
function visible(text: string) {
  return articleBlocks(text).map((block) => ({
    kind: block.kind,
    text: text.slice(block.start, block.end),
    level: block.level,
  }));
}

describe('articleBlocks', () => {
  it('gives nothing for empty or blank text', () => {
    expect(articleBlocks('')).toEqual([]);
    expect(articleBlocks('  \n\n \t\n')).toEqual([]);
  });

  it('splits paragraphs on blank lines and trims each span', () => {
    const text = '  First paragraph.\nstill first.\n\n\n   Second paragraph.  \n\nThird.';
    expect(visible(text)).toEqual([
      { kind: 'paragraph', text: 'First paragraph.\nstill first.', level: undefined },
      { kind: 'paragraph', text: 'Second paragraph.', level: undefined },
      { kind: 'paragraph', text: 'Third.', level: undefined },
    ]);
  });

  it.each([
    ['```', '```'],
    ['~~~', '~~~'],
  ])('keeps a %s fenced block whole across its blank lines', (open, close) => {
    const text = `Before.\n\n${open}ts\nconst a = 1;\n\nconst b = 2;\n${close}\n\nAfter.`;
    expect(visible(text)).toEqual([
      { kind: 'paragraph', text: 'Before.', level: undefined },
      { kind: 'code', text: 'const a = 1;\n\nconst b = 2;', level: undefined },
      { kind: 'paragraph', text: 'After.', level: undefined },
    ]);
  });

  it('does not close a fence on a different marker character', () => {
    const text = '```\nline one\n~~~\nline two\n```';
    expect(visible(text)).toEqual([{ kind: 'code', text: 'line one\n~~~\nline two', level: undefined }]);
  });

  it('runs an unterminated fence to the end of the text', () => {
    const text = 'Intro.\n\n```\nopen code\n\nmore code';
    expect(visible(text)).toEqual([
      { kind: 'paragraph', text: 'Intro.', level: undefined },
      { kind: 'code', text: 'open code\n\nmore code', level: undefined },
    ]);
  });

  it('makes a heading its own block without its markers', () => {
    const text = '# One\n\n## Two ##\n\n###### Six';
    expect(visible(text)).toEqual([
      { kind: 'heading', text: 'One', level: 1 },
      { kind: 'heading', text: 'Two', level: 2 },
      { kind: 'heading', text: 'Six', level: 6 },
    ]);
  });

  it('splits the text on the line after a heading into its own block', () => {
    const text = '## Title\nBody right after.\nSecond line.';
    expect(visible(text)).toEqual([
      { kind: 'heading', text: 'Title', level: 2 },
      { kind: 'paragraph', text: 'Body right after.\nSecond line.', level: undefined },
    ]);
  });

  it('lets a heading interrupt a paragraph', () => {
    expect(visible('intro\n## Two\nmore')).toEqual([
      { kind: 'paragraph', text: 'intro', level: undefined },
      { kind: 'heading', text: 'Two', level: 2 },
      { kind: 'paragraph', text: 'more', level: undefined },
    ]);
  });

  it('reads consecutive headings with no blank lines between them', () => {
    expect(visible('## A\ntext\n## B\nmore')).toEqual([
      { kind: 'heading', text: 'A', level: 2 },
      { kind: 'paragraph', text: 'text', level: undefined },
      { kind: 'heading', text: 'B', level: 2 },
      { kind: 'paragraph', text: 'more', level: undefined },
    ]);
  });

  it('makes the lead-in line before a list a paragraph of its own, and starts the list at its first item', () => {
    expect(visible('Ingredients:\n- flour\n- sugar')).toEqual([
      { kind: 'paragraph', text: 'Ingredients:', level: undefined },
      { kind: 'list', text: '- flour\n- sugar', level: undefined },
    ]);
  });

  it('reads a heading directly before a list or code', () => {
    const text = '## Steps\n- one\n- two';
    expect(visible(text)).toEqual([
      { kind: 'heading', text: 'Steps', level: 2 },
      { kind: 'list', text: '- one\n- two', level: undefined },
    ]);
  });

  it.each(['- item', '* item', '+ item', '1. item', '12) item'])('reads "%s" as a list', (item) => {
    expect(visible(item)).toEqual([{ kind: 'list', text: item, level: undefined }]);
  });

  it('does not take #hashtags, seven hashes or a bare marker for headings', () => {
    for (const text of ['#hashtag is not a heading', '####### seven', '-not a list', '1.5 is a number']) {
      expect(visible(text)).toEqual([{ kind: 'paragraph', text, level: undefined }]);
    }
  });

  it('drops a heading that is only markers', () => {
    expect(articleBlocks('## ##')).toEqual([]);
  });

  it('keeps offsets in the original text after a heading, so marks line up', () => {
    const text = '# Title\n\nAlpha beta gamma.';
    const blocks = articleBlocks(text);
    const quote = 'beta gamma';
    const at = text.indexOf(quote);
    const block = blocks.find((candidate) => candidate.start <= at && at < candidate.end);
    expect(block?.kind).toBe('paragraph');
    expect(text.slice(block!.start, block!.end)).toContain(quote);
  });

  it('handles CRLF: paragraphs split, offsets stay original, and trailing CRs are trimmed', () => {
    const text = 'First.\r\n\r\nSecond line.\r\nMore.\r\n\r\n```\r\ncode\r\n\r\nmore\r\n```\r\n\r\n## Head\r\nAfter.';
    const blocks = visible(text);
    expect(blocks.map((block) => block.kind)).toEqual(['paragraph', 'paragraph', 'code', 'heading', 'paragraph']);
    expect(blocks[0]!.text).toBe('First.');
    expect(blocks[1]!.text).toBe('Second line.\r\nMore.');
    expect(blocks[2]!.text).toBe('code\r\n\r\nmore');
    expect(blocks[3]).toMatchObject({ text: 'Head', level: 2 });
    expect(blocks[4]!.text).toBe('After.');
  });

  describe('lists', () => {
    const slice = (text: string, range: { start: number; end: number }) => text.slice(range.start, range.end);

    it('gives each item its content range, depth, ordered flag and number', () => {
      const text = '- one\n  - nested\n    - deeper\n- two\n1. first\n12) twelfth';
      const [list] = articleBlocks(text);
      expect(list!.kind).toBe('list');
      expect(list!.items!.map((item) => [slice(text, item), item.depth, item.ordered, item.number])).toEqual([
        ['one', 0, false, undefined],
        ['nested', 1, false, undefined],
        ['deeper', 2, false, undefined],
        ['two', 0, false, undefined],
        ['first', 0, true, 1],
        ['twelfth', 0, true, 12],
      ]);
    });

    it('counts a tab as four spaces and returns to an outer depth', () => {
      const text = '- a\n\t- b\n    - c\n  - d\n- e';
      const depths = articleBlocks(text)[0]!.items!.map((item) => item.depth);
      expect(depths).toEqual([0, 1, 1, 1, 0]);
    });

    it('lets a line that is no item continue the item before it', () => {
      const text = '- one\n  carries on\n- two';
      const items = articleBlocks(text)[0]!.items!;
      expect(slice(text, items[0]!)).toBe('one\n  carries on');
      expect(slice(text, items[1]!)).toBe('two');
    });

    it('does not let a list item swallow the heading after it', () => {
      expect(visible('- a\n## Next')).toEqual([
        { kind: 'list', text: '- a', level: undefined },
        { kind: 'heading', text: 'Next', level: 2 },
      ]);
    });
  });

  describe('quotes', () => {
    it('reads each line without its marker, lazy continuation lines included', () => {
      const text = '> one\n>two\nlazy line\n>   spaced';
      const [quote] = articleBlocks(text);
      expect(quote!.kind).toBe('quote');
      expect(quote!.lines!.map((line) => text.slice(line.start, line.end))).toEqual(['one', 'two', 'lazy line', '  spaced']);
      expect(text.slice(quote!.start, quote!.end)).toBe(text);
    });

    it('allows up to three spaces before the marker', () => {
      expect(articleBlocks('   > ok')[0]!.kind).toBe('quote');
      expect(articleBlocks('    > code-ish')[0]!.kind).toBe('paragraph');
    });
  });

  describe('tables', () => {
    const cells = (text: string) =>
      articleBlocks(text)[0]!.rows!.map((row) => row.map((cell) => text.slice(cell.start, cell.end)));

    it('reads header and body rows with the delimiter row left out, and the alignment', () => {
      const text = '| A | B | C |\n| --- | :---: | ---: |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |';
      const [table] = articleBlocks(text);
      expect(table!.kind).toBe('table');
      expect(cells(text)).toEqual([['A', 'B', 'C'], ['1', '2', '3'], ['4', '5', '6']]);
      expect(table!.align).toEqual([null, 'center', 'right']);
      expect(text.slice(table!.start, table!.end)).toBe(text);
    });

    it('keeps an escaped pipe and a pipe inside a code span in their cell', () => {
      const text = '| a | b |\n|---|---|\n| x \\| y | `p|q` |';
      expect(cells(text)[1]).toEqual(['x \\| y', '`p|q`']);
    });

    it('reads an empty cell as a zero-length range at its position', () => {
      const text = '| a | b |\n|---|---|\n|  | z |';
      const row = articleBlocks(text)[0]!.rows![1]!;
      expect(row).toHaveLength(2);
      expect(row[0]!.end).toBe(row[0]!.start);
      expect(row[0]!.start).toBeGreaterThan(text.indexOf('|  |'));
      expect(row[0]!.start).toBeLessThan(text.indexOf('z'));
      expect(text.slice(row[1]!.start, row[1]!.end)).toBe('z');
    });

    it('needs no outer pipes', () => {
      const text = 'a | b\n--- | :--\n1 | 2';
      expect(cells(text)).toEqual([['a', 'b'], ['1', '2']]);
      expect(articleBlocks(text)[0]!.align).toEqual([null, 'left']);
    });

    it('makes the lines before the header a block of their own and ends at a line with no pipe', () => {
      const text = 'Lead in.\n| a | b |\n|---|---|\n| 1 | 2 |\nafter';
      expect(articleBlocks(text).map((block) => [block.kind, text.slice(block.start, block.end)])).toEqual([
        ['paragraph', 'Lead in.'],
        ['table', '| a | b |\n|---|---|\n| 1 | 2 |'],
        ['paragraph', 'after'],
      ]);
    });

    it('does not take a pipe line without a matching delimiter row for a table', () => {
      expect(visible('a | b\n--- | --- | ---')).toEqual([
        { kind: 'paragraph', text: 'a | b\n--- | --- | ---', level: undefined },
      ]);
    });
  });

  describe('rules, setext headings and code', () => {
    it.each(['---', '***', '___', '- - -', '* * *', '_  _  _'])('reads "%s" as a rule', (rule) => {
      expect(visible(rule)).toEqual([{ kind: 'rule', text: rule, level: undefined }]);
    });

    it('does not take two characters for a rule', () => {
      expect(articleBlocks('--')[0]!.kind).toBe('paragraph');
    });

    it('reads a paragraph over === or --- as a heading of the text above the underline', () => {
      expect(visible('Title\n=====\n\nSub head\nmore words\n---')).toEqual([
        { kind: 'heading', text: 'Title', level: 1 },
        { kind: 'heading', text: 'Sub head\nmore words', level: 2 },
      ]);
    });

    it('takes the language from the first word of the opening fence', () => {
      const text = '```ts title="x"\nconst a = 1;\n```\n\n~~~\nplain\n~~~';
      const code = articleBlocks(text).filter((block) => block.kind === 'code');
      expect(code.map((block) => block.language)).toEqual(['ts', undefined]);
    });
  });

  describe('lists that interrupt a paragraph', () => {
    const kinds = (text: string) => articleBlocks(text).map((block) => [block.kind, text.slice(block.start, block.end)]);

    it('keeps a hard-wrapped "2004." line in its paragraph', () => {
      const text = 'The survey ended in the year\n2004. The study found more.';
      expect(kinds(text)).toEqual([['paragraph', text]]);
    });

    it('lets an ordered list that starts at 1 interrupt a paragraph', () => {
      expect(kinds('Steps:\n1. First\n2. Second')).toEqual([
        ['paragraph', 'Steps:'],
        ['list', '1. First\n2. Second'],
      ]);
    });

    it('lets a bullet interrupt a paragraph', () => {
      expect(kinds('Lead-in\n- item')).toEqual([
        ['paragraph', 'Lead-in'],
        ['list', '- item'],
      ]);
    });

    it('still reads "2004. A year" at the start of a block as a list', () => {
      expect(kinds('2004. A year')).toEqual([['list', '2004. A year']]);
      expect(articleBlocks('2004. A year')[0]!.items![0]).toMatchObject({ ordered: true, number: 2004 });
    });

    it('reads no more than nine digits as an ordered marker', () => {
      expect(articleBlocks('1234567890. x')[0]!.kind).toBe('paragraph');
      expect(articleBlocks('123456789. x')[0]!.kind).toBe('list');
    });
  });
});
