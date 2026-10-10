import { describe, expect, it } from 'vitest';

import {
  buildPassageTextRenderer,
  buildItemTextRenderer,
  escapeHtml,
  findPassagePage,
} from '../pdfPassageSearch';

const fakePdf = (pages: string[]) => ({
  numPages: pages.length,
  getPage: async (pageNumber: number) => ({
    getTextContent: async () => ({
      items: pages[pageNumber - 1].split(' ').map((str) => ({ str })),
    }),
  }),
});

describe('escapeHtml', () => {
  it('neutralises a script tag', () => {
    expect(escapeHtml('<script>alert(1)</script>')).toBe(
      '&lt;script&gt;alert(1)&lt;/script&gt;'
    );
  });

  it('escapes ampersands and quotes', () => {
    expect(escapeHtml('a & b "c"')).toBe('a &amp; b &quot;c&quot;');
  });

  it('leaves ordinary text alone', () => {
    expect(escapeHtml('plain text')).toBe('plain text');
  });
});

describe('buildPassageTextRenderer', () => {
  it('wraps only the needle and escapes the rest', () => {
    const render = buildPassageTextRenderer('the sample');
    const output = render({ str: 'before the sample <b>after</b>' });
    expect(output).toContain('<mark class="lattice-pdf-mark">the sample</mark>');
    expect(output).toContain('&lt;b&gt;after&lt;/b&gt;');
  });

  it('emits no unescaped angle bracket outside the mark element', () => {
    const render = buildPassageTextRenderer('needle');
    const output = render({ str: '<img src=x onerror=alert(1)> needle <script>' });
    const withoutMarks = output.replace(/<\/?mark[^>]*>/g, '');
    expect(withoutMarks).not.toContain('<');
    expect(withoutMarks).not.toContain('>');
  });

  it('escapes a needle that itself contains markup', () => {
    const render = buildPassageTextRenderer('<b>');
    const output = render({ str: 'x <b> y' });
    expect(output).toBe('x <mark class="lattice-pdf-mark">&lt;b&gt;</mark> y');
  });

  it('matches case-insensitively', () => {
    const render = buildPassageTextRenderer('Sample');
    expect(render({ str: 'a sample here' })).toContain('<mark');
  });

  it('escapes everything when the needle is empty', () => {
    const render = buildPassageTextRenderer('   ');
    expect(render({ str: '<b>x</b>' })).toBe('&lt;b&gt;x&lt;/b&gt;');
  });

  it('marks every occurrence in one item', () => {
    const render = buildPassageTextRenderer('abc');
    const output = render({ str: 'abc and abc' });
    expect(output.match(/<mark/g)).toHaveLength(2);
  });
});

describe('findPassagePage', () => {
  const passage =
    'the specific passage we are looking for lives here on the third page of this document';

  it('finds the passage on page 3', async () => {
    const pdf = fakePdf([
      'first page filler about something else entirely and unrelated words',
      'second page filler about another topic that has nothing to do with it',
      passage,
    ]);
    const hit = await findPassagePage(pdf, passage);
    expect(hit?.page).toBe(3);
  });

  it('returns a literal run that occurs on the page, for marking', async () => {
    const pdf = fakePdf(['filler', passage]);
    const hit = await findPassagePage(pdf, passage);
    expect(hit?.needle.length).toBeGreaterThan(0);
    expect(passage.toLowerCase()).toContain(hit!.needle);
  });

  it('respects maxPages', async () => {
    const pdf = fakePdf(['filler one here', 'filler two here', passage]);
    expect(await findPassagePage(pdf, passage, { maxPages: 2 })).toBeNull();
  });

  it('honours an aborted signal', async () => {
    const pdf = fakePdf([passage]);
    expect(
      await findPassagePage(pdf, passage, { signal: { aborted: true } })
    ).toBeNull();
  });

  it('returns null when the passage is not in the document', async () => {
    const pdf = fakePdf(['completely different content on every single page here']);
    expect(await findPassagePage(pdf, passage)).toBeNull();
  });

  it('returns null for text too short to identify a passage', async () => {
    const pdf = fakePdf([passage]);
    expect(await findPassagePage(pdf, 'hi')).toBeNull();
  });

  it('skips a page that fails to render instead of giving up', async () => {
    const pdf = {
      numPages: 2,
      getPage: async (pageNumber: number) => {
        if (pageNumber === 1) throw new Error('broken page');
        return {
          getTextContent: async () => ({
            items: passage.split(' ').map((str) => ({ str })),
          }),
        };
      },
    };
    const hit = await findPassagePage(pdf, passage);
    expect(hit?.page).toBe(2);
  });
});

describe('PDF text fragment highlighting', () => {
  it('marks a passage across individual word items, including punctuation differences', async () => {
    const words = 'Before. The cited passage, crosses several PDF text fragments here. After.'.split(' ');
    const pdf = { numPages: 1, getPage: async () => ({ getTextContent: async () => ({ items: words.map(str => ({ str })) }) }) };
    const hit = await findPassagePage(pdf, 'The cited passage crosses several PDF text fragments here');
    expect(hit).not.toBeNull();
    const render = buildItemTextRenderer(hit!.marks);
    const rendered = words.map((str, itemIndex) => render({ str, itemIndex }));
    expect(rendered[0]).not.toContain('<mark');
    expect(rendered[rendered.length - 1]).not.toContain('<mark');
    expect(rendered.slice(1, -1).every(item => item.includes('<mark'))).toBe(true);
  });

  it('searches a supplied page first and still builds highlights', async () => {
    const text = 'This is the specific cited passage that must be highlighted';
    const hit = await findPassagePage(fakePdf([text, text]), text, { preferredPage: 2 });
    expect(hit?.page).toBe(2);
    expect(Object.keys(hit!.marks).length).toBeGreaterThan(1);
  });

  it('falls back when the supplied page is wrong', async () => {
    const text = 'This is the specific cited passage that must be highlighted';
    const hit = await findPassagePage(fakePdf([text, 'unrelated']), text, { preferredPage: 2 });
    expect(hit?.page).toBe(1);
  });
});

it('matches extractor spacing differences without highlighting repeated query words', async () => {
  const words = ['patent patent patent', 'The patent application', 'must include a written description', 'of the claimed invention.', 'patent patent'];
  const pdf = { numPages: 1, getPage: async () => ({ getTextContent: async () => ({ items: words.map(str => ({ str })) }) }) };
  const hit = await findPassagePage(pdf, 'The patent applicationmust include a written descriptionof the claimed invention.');
  expect(hit).not.toBeNull();
  const renderer = buildItemTextRenderer(hit!.marks);
  expect(renderer({ str: words[0], itemIndex: 0 })).not.toContain('<mark');
  expect(renderer({ str: words[4], itemIndex: 4 })).not.toContain('<mark');
  for (const itemIndex of [1, 2, 3]) expect(renderer({ str: words[itemIndex], itemIndex })).toContain('<mark');
});
