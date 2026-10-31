import { describe, expect, it } from 'vitest';

import { sentences, speakableText, speechChunks } from './speechText';

describe('speakableText', () => {
  it('drops code blocks and keeps the prose around them', () => {
    const markdown = 'Run this first:\n\n```ts\nconst x = await load();\n```\n\nThen it is ready.';
    expect(speakableText(markdown)).toBe('Run this first:\n\nThen it is ready.');
  });

  it('drops citation markers, footnotes and URLs but keeps link text', () => {
    const markdown =
      'Spacing helps recall [1][2], as [the review](https://example.org/review) shows [3, 4]. See https://example.org for more.[^1]\n\n[^1]: A footnote.';
    expect(speakableText(markdown)).toBe('Spacing helps recall, as the review shows. See for more.');
  });

  it('removes markdown symbols and ends headings and list items with a full stop', () => {
    const markdown = '## Key ideas\n\n- **Retrieval** beats *rereading*\n- Space your reviews\n\n> Quoted ~~not~~ text';
    expect(speakableText(markdown)).toBe(
      'Key ideas.\n\nRetrieval beats rereading.\n\nSpace your reviews.\n\nQuoted not text',
    );
  });

  it('keeps short inline code as a word and drops code-like spans', () => {
    expect(speakableText('Call `fetch` once, not `items.map((x) => x)`.')).toBe('Call fetch once, not.');
  });

  it('keeps prices that look like the ends of inline math', () => {
    expect(speakableText('Lunch costs $5 and dinner $10.')).toBe('Lunch costs $5 and dinner $10.');
  });

  it('reads tables row by row and drops math and images', () => {
    const markdown = '| Term | Meaning |\n|---|---|\n| FSRS | A scheduler |\n\n![chart](a.png) The rate is $e^{-t}$ here.\n\n$$\\int f$$';
    expect(speakableText(markdown)).toBe('Term, Meaning. FSRS, A scheduler.\n\nThe rate is here.');
  });
});

describe('sentences', () => {
  it('splits at sentence ends but not abbreviations, initials or decimals', () => {
    expect(sentences('It costs 3.5 dollars, e.g. at lunch. Dr. Smith agreed! Did J. R. R. Tolkien? Yes.')).toEqual([
      'It costs 3.5 dollars, e.g. at lunch.',
      'Dr. Smith agreed!',
      'Did J. R. R. Tolkien?',
      'Yes.',
    ]);
  });
});

describe('speechChunks', () => {
  it('sends the first sentence alone so speech starts quickly, then gathers the rest', () => {
    const chunks = speechChunks('One. Two. Three. Four.', { target: 12 });
    expect(chunks).toEqual(['One.', 'Two. Three.', 'Four.']);
  });

  it('breaks a sentence longer than the limit at a clause', () => {
    const long = `${'word '.repeat(30).trim()}, and then ${'more '.repeat(30).trim()}.`;
    const chunks = speechChunks(long, { max: 200 });
    expect(chunks.length).toBeGreaterThan(1);
    expect(chunks.every((chunk) => chunk.length <= 200)).toBe(true);
    expect(chunks[0].endsWith(',')).toBe(true);
  });

  it('has nothing to say for an answer that is only code', () => {
    expect(speechChunks('```\nprint(1)\n```')).toEqual([]);
  });
});
