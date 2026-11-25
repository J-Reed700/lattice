import { describe, expect, it } from 'vitest';

import { SCAN_BUDGET, inlineRuns, visibleText } from '../inlineMarkdown';

const runs = (text: string) => inlineRuns(text, 0, text.length);

/** The runs as [text, styling] pairs, for compact expectations. */
function shown(text: string) {
  return runs(text).map((run) => {
    const { start: _start, end: _end, text: shownText, atomic: _atomic, ...style } = run;
    return Object.keys(style).length > 0 ? [shownText, style] : [shownText];
  });
}

const joined = (text: string) => runs(text).map((run) => run.text).join('');

describe('inlineRuns', () => {
  it('gives plain text as one run', () => {
    expect(runs('hello world')).toEqual([{ start: 0, end: 11, text: 'hello world' }]);
    expect(runs('')).toEqual([]);
  });

  it('reads a sub-range of a larger text with offsets in the full text', () => {
    const text = 'xx **bold** yy';
    expect(inlineRuns(text, 3, 11)).toEqual([{ start: 5, end: 9, text: 'bold', strong: true }]);
  });

  describe('escapes and entities', () => {
    it('shows the punctuation of an escape, over both characters', () => {
      expect(runs('a\\*b')).toEqual([
        { start: 0, end: 1, text: 'a' },
        { start: 1, end: 3, text: '*', atomic: true },
        { start: 3, end: 4, text: 'b' },
      ]);
    });

    it('keeps escapes and entities as runs of their own, with literal text merging up to them', () => {
      expect(runs('a \\* b &amp; c')).toEqual([
        { start: 0, end: 2, text: 'a ' },
        { start: 2, end: 4, text: '*', atomic: true },
        { start: 4, end: 7, text: ' b ' },
        { start: 7, end: 12, text: '&', atomic: true },
        { start: 12, end: 14, text: ' c' },
      ]);
      expect(runs('\\*\\*&lt;&lt;').every((run) => run.atomic === true && run.end - run.start > 1)).toBe(true);
      expect(runs('\\*\\*&lt;&lt;')).toHaveLength(4);
    });

    it('leaves a backslash before a letter as it is', () => {
      expect(joined('C:\\dir')).toBe('C:\\dir');
    });

    it('does not let an escaped delimiter open emphasis', () => {
      expect(joined('\\*not em\\*')).toBe('*not em*');
      expect(runs('\\*not em\\*').some((run) => run.em)).toBe(false);
    });

    it.each([
      ['&amp;', '&'],
      ['&lt;', '<'],
      ['&gt;', '>'],
      ['&quot;', '"'],
      ['&#39;', "'"],
      ['&apos;', "'"],
      ['&nbsp;', '\u00a0'],
      ['&#123;', '{'],
      ['&#x1F;', '\u001f'],
      ['&#x41;', 'A'],
    ])('decodes %s over the whole entity', (entity, decoded) => {
      expect(runs(entity)).toEqual([{ start: 0, end: entity.length, text: decoded, atomic: true }]);
    });

    it('leaves an unknown or malformed entity as written', () => {
      expect(joined('&bogus; &amp &#; &#0;')).toBe('&bogus; &amp &#; &#0;');
    });

    it('does not decode inside a code span', () => {
      expect(shown('`&amp;`')).toEqual([['&amp;', { code: true }]]);
    });
  });

  describe('code spans', () => {
    it('shows the content without the backticks, literally', () => {
      expect(shown('use `a *b* \\n` here')).toEqual([['use '], ['a *b* \\n', { code: true }], [' here']]);
    });

    it('closes on a run of exactly the same length', () => {
      expect(shown('``a`b``')).toEqual([['a`b', { code: true }]]);
    });

    it('strips one space at each end when both are there, unless the content is only spaces', () => {
      expect(shown('` a `')).toEqual([['a', { code: true }]]);
      expect(shown('`  a  `')).toEqual([[' a ', { code: true }]]);
      expect(shown('` a`')).toEqual([[' a', { code: true }]]);
      expect(shown('`   `')).toEqual([['   ', { code: true }]]);
    });

    it('leaves unmatched backticks literal', () => {
      expect(shown('a ` b')).toEqual([['a ` b']]);
      expect(shown('``a`')).toEqual([['``a`']]);
    });

    it('lets a backslash end a code span', () => {
      expect(shown('`C:\\`')).toEqual([['C:\\', { code: true }]]);
    });
  });

  describe('links', () => {
    it('hides the brackets and destination and sets the href', () => {
      const text = 'see [the docs](https://example.com/a?b=1 "Title") now';
      expect(runs(text)).toEqual([
        { start: 0, end: 4, text: 'see ' },
        { start: 5, end: 13, text: 'the docs', href: 'https://example.com/a?b=1' },
        { start: 49, end: 53, text: ' now' },
      ]);
    });

    it('parses the label, so emphasis and code work inside it', () => {
      expect(shown('[**bold** and `code`](http://x.test)')).toEqual([
        ['bold', { strong: true, href: 'http://x.test' }],
        [' and ', { href: 'http://x.test' }],
        ['code', { code: true, href: 'http://x.test' }],
      ]);
    });

    it('allows <...> destinations and balanced parentheses', () => {
      expect(shown('[a](<https://x.test/a b>)')).toEqual([['a', { href: 'https://x.test/a b' }]]);
      expect(shown('[w](https://en.wikipedia.org/wiki/Foo_(bar))')).toEqual([
        ['w', { href: 'https://en.wikipedia.org/wiki/Foo_(bar)' }],
      ]);
    });

    it('takes http(s) in any case of the scheme, and not mailto', () => {
      expect(shown('[m](mailto:a@b.test) [u](HTTPS://X.TEST/)')).toEqual([
        ['m'],
        [' '],
        ['u', { href: 'HTTPS://X.TEST/' }],
      ]);
    });

    it.each(['javascript:alert(1)', 'file:///etc/passwd', '/relative', '#anchor'])(
      'shows the label of a link to %s with no href',
      (destination) => {
        expect(shown(`[label](${destination})`)).toEqual([['label']]);
      },
    );

    it('leaves a bracket with no destination, and reference links, literal', () => {
      expect(joined('[x] and [y] [z]: http://a.test and [x][ref] and [a] (b)')).toBe(
        '[x] and [y] [z]: http://a.test and [x][ref] and [a] (b)',
      );
      expect(runs('[x] (https://a.test)').some((run) => run.href)).toBe(false);
    });

    it('leaves a link with an unclosed destination literal', () => {
      expect(joined('[x](https://a.test and more')).toBe('[x](https://a.test and more');
    });
  });

  describe('images and autolinks', () => {
    it('shows an image as its alt text, without nested parsing', () => {
      expect(shown('a ![the *alt*](pic.png) b')).toEqual([['a '], ['the *alt*', { image: true }], [' b']]);
    });

    it('shows nothing for an image with no alt text', () => {
      expect(shown('a![](pic.png)b')).toEqual([['a'], ['b']]);
    });

    it('turns <https://...> into a link run of the address, and <mailto:...> into the address as plain text', () => {
      const text = 'go <https://example.com/x> or <mailto:a@b.test>';
      expect(runs(text)).toEqual([
        { start: 0, end: 3, text: 'go ' },
        { start: 4, end: 25, text: 'https://example.com/x', href: 'https://example.com/x' },
        { start: 26, end: 30, text: ' or ' },
        { start: 31, end: 46, text: 'mailto:a@b.test' },
      ]);
    });

    it('leaves other angle-bracket text literal', () => {
      expect(shown('a <div> and <ftp://x.test> b')).toEqual([['a <div> and <ftp://x.test> b']]);
    });
  });

  describe('emphasis', () => {
    it('reads * and _ as em, and doubled as strong', () => {
      expect(shown('*a* _b_ **c** __d__')).toEqual([
        ['a', { em: true }],
        [' '],
        ['b', { em: true }],
        [' '],
        ['c', { strong: true }],
        [' '],
        ['d', { strong: true }],
      ]);
    });

    it('reads *** as both', () => {
      expect(shown('***x***')).toEqual([['x', { strong: true, em: true }]]);
    });

    it('nests', () => {
      expect(shown('**a *b* c**')).toEqual([
        ['a ', { strong: true }],
        ['b', { strong: true, em: true }],
        [' c', { strong: true }],
      ]);
    });

    it('leaves unmatched delimiters literal', () => {
      expect(shown('**a')).toEqual([['**a']]);
      expect(shown('a* b')).toEqual([['a* b']]);
      expect(shown('*a **b* c')).toEqual([['*a *'], ['b', { em: true }], [' c']]);
    });

    it('does not open before whitespace or close after it', () => {
      expect(shown('2 * 3 * 4')).toEqual([['2 * 3 * 4']]);
      expect(shown('* a *')).toEqual([['* a *']]);
    });

    it('keeps intraword underscores literal', () => {
      expect(shown('snake_case_name and _leading')).toEqual([['snake_case_name and _leading']]);
    });

    it('may read a*b*c as em, as CommonMark does', () => {
      expect(shown('a*b*c')).toEqual([['a'], ['b', { em: true }], ['c']]);
    });

    it('does not match across different characters', () => {
      expect(shown('*a_')).toEqual([['*a_']]);
    });
  });

  describe('strikethrough', () => {
    it('reads ~~x~~', () => {
      expect(shown('a ~~gone~~ b')).toEqual([['a '], ['gone', { strike: true }], [' b']]);
    });

    it('leaves single and triple tildes literal', () => {
      expect(shown('~x~ ~~~y~~~')).toEqual([['~x~ ~~~y~~~']]);
    });
  });

  describe('invariants', () => {
    it('keeps every non-escape, non-entity run equal to its source', () => {
      const text =
        'A **bold [link *em*](https://x.test/a_(b)) and `code`** ~~gone~~ ![alt](i.png) <https://a.test> \\* &amp; end *x* _y_ snake_case';
      const all = runs(text);
      expect(all.length).toBeGreaterThan(5);
      for (const run of all) {
        if (run.text !== text.slice(run.start, run.end)) {
          expect(['*', '&']).toContain(run.text);
        }
      }
    });

    it('merges adjacent characters of the same styling, but not across other styling', () => {
      expect(shown('ab **c** de')).toEqual([['ab '], ['c', { strong: true }], [' de']]);
    });

    it('never throws, and stays in bounds, in order and without overlap, on random input', () => {
      const alphabet = '*_`[]()!<>~\\& a1\n|';
      let seed = 12345;
      const random = () => {
        seed = (seed * 1664525 + 1013904223) % 4294967296;
        return seed / 4294967296;
      };
      for (let round = 0; round < 500; round += 1) {
        const length = Math.floor(random() * 60);
        let text = '';
        for (let index = 0; index < length; index += 1) text += alphabet[Math.floor(random() * alphabet.length)];
        const all = inlineRuns(text, 0, text.length);
        let cursor = 0;
        for (const run of all) {
          expect(run.start).toBeGreaterThanOrEqual(cursor);
          expect(run.end).toBeGreaterThan(run.start);
          expect(run.end).toBeLessThanOrEqual(text.length);
          cursor = run.end;
        }
        expect(() => visibleText(text)).not.toThrow();
      }
    });

    const repeated = (unit: string) => (length: number) => unit.repeat(Math.ceil(length / unit.length)).slice(0, length);
    const halves = (open: string, close: string) => (length: number) => {
      const count = Math.ceil(length / (open.length + close.length));
      return (open.repeat(count) + close.repeat(count)).slice(0, length);
    };
    const pathological: Array<[string, (_length: number) => string]> = [
      ['unmatched brackets', repeated('[')],
      ['links that never close', repeated('[a](x')],
      ['openers with no closers', repeated('*a ')],
      ['closers with no openers', repeated('a* ')],
      ['mixed delimiters', repeated('*_~ a ')],
      ['backticks', repeated('`a``b```c')],
      ['nested emphasis', halves('*a ', 'a* ')],
      ['nested links', halves('[a', ']')],
      ['entities', repeated('&amp;')],
    ];

    it.each(pathological)('grows linearly with the length of %s', (_name, build) => {
      // Timed in alternation, fastest of fifteen each: a contended machine only ever
      // slows a run down, and alternating makes a busy spell hit both sizes alike,
      // so a busy test run cannot fake a ratio.
      const smallText = build(8_000);
      const bigText = build(32_000);
      inlineRuns(smallText, 0, smallText.length);
      inlineRuns(bigText, 0, bigText.length);
      const timed = (text: string) => {
        const started = performance.now();
        inlineRuns(text, 0, text.length);
        return performance.now() - started;
      };
      let small = Infinity;
      let big = Infinity;
      for (let round = 0; round < 15; round += 1) {
        small = Math.min(small, timed(smallText));
        big = Math.min(big, timed(bigText));
      }
      // Linear is about 4x for 4x the text; quadratic is about 16x.
      expect(big / Math.max(small, 1)).toBeLessThan(8);
    });

    describe('the link scan budget', () => {
      // `[a](x` opens a link whose destination never closes, so reading it scans to the
      // next space: n of them in a row scan about 2.5·n² characters.
      const brokenLinks = (scanned: number) => '[a](x'.repeat(Math.ceil(Math.sqrt(scanned / 2.5)));
      const linksTo = (text: string) => inlineRuns(text, 0, text.length).some((run) => run.href === 'https://ok.test');

      it('stops reading link destinations once a paragraph has scanned its budget', () => {
        expect(linksTo(`${brokenLinks(2 * SCAN_BUDGET)} [ok](https://ok.test)`)).toBe(false);
      });

      it('reads every link of a paragraph that stays within its budget', () => {
        expect(linksTo(`${brokenLinks(SCAN_BUDGET / 2)} [ok](https://ok.test)`)).toBe(true);
      });
    });
  });
});

describe('visibleText', () => {
  it('leaves block markers and inline syntax out', () => {
    const text = '# Title\n\nSome **bold** and [a link](https://x.test) here.\n\n- one\n- *two*\n\n> quoted\n> text';
    expect(visibleText(text)).toBe('Title\n\nSome bold and a link here.\n\none\ntwo\n\nquoted\ntext');
  });

  it('joins table cells with a dot and rows with a line break', () => {
    expect(visibleText('| A | B |\n|---|---|\n| **1** | 2 |')).toBe('A · B\n1 · 2');
  });

  it('keeps code literal and drops rules', () => {
    expect(visibleText('```\n**not bold**\n```\n\n---\n\nafter')).toBe('**not bold**\n\nafter');
  });

  it('reads a sub-range, with formatting whose syntax lies outside it', () => {
    const text = 'Start **bold words** end.';
    const from = text.indexOf('bold');
    expect(visibleText(text, from, text.indexOf(' end'))).toBe('bold words');
    expect(visibleText(text, 0, text.length)).toBe('Start bold words end.');
    expect(visibleText(text, text.indexOf('**'), text.length)).toBe('bold words end.');
  });

  it('shows an escape or entity wholly when its start is inside the range', () => {
    const text = 'a\\*b &amp; c';
    expect(visibleText(text, 1, 2)).toBe('*');
    expect(visibleText(text, 2, 3)).toBe('');
    expect(visibleText(text, 0, text.length)).toBe('a*b & c');
  });

  it('is empty for empty text', () => {
    expect(visibleText('')).toBe('');
  });
});
