import { describe, expect, it } from 'vitest';

import { ReplyReader, SpokenPieces } from './replySpeech';

async function collect(pieces: SpokenPieces): Promise<string[]> {
  const out: string[] = [];
  for await (const piece of pieces) out.push(piece);
  return out;
}

describe('ReplyReader', () => {
  it('hands out each sentence once the next has begun', () => {
    const reader = new ReplyReader();
    expect(reader.update('The first sent')).toEqual([]);
    expect(reader.update('The first sentence.')).toEqual([]);
    expect(reader.update('The first sentence. And the')).toEqual(['The first sentence.']);
    expect(reader.update('The first sentence. And the second one. Then')).toEqual(['And the second one.']);
    expect(reader.update('The first sentence. And the second one. Then')).toEqual([]);
  });

  it('finishes with what was held back', () => {
    const reader = new ReplyReader();
    reader.update('One here. Two here');
    expect(reader.finish('One here. Two here.')).toEqual(['Two here.']);
    expect(reader.finish('One here. Two here.')).toEqual([]);
  });

  it('says a reply that arrived whole', () => {
    expect(new ReplyReader().finish('One here. Two here.')).toEqual(['One here.', 'Two here.']);
  });

  it('skips code, even while its fence is still open', () => {
    const reader = new ReplyReader();
    expect(reader.update('Run this.\n\n```py\nprint("Hello. There.")\n')).toEqual([]);
    expect(reader.update('Run this.\n\n```py\nprint("Hello. There.")\n```\n\nThen look. At it')).toEqual(['Run this.', 'Then look.']);
  });

  it('does not stumble on an abbreviation mid-sentence', () => {
    const reader = new ReplyReader();
    expect(reader.update('Use a tool, e.g. a hammer')).toEqual([]);
    expect(reader.update('Use a tool, e.g. a hammer. Then rest')).toEqual(['Use a tool, e.g. a hammer.']);
  });

  it('breaks a sentence too long for one call into parts as it grows', () => {
    const reader = new ReplyReader();
    const long = Array.from({ length: 80 }, (_, i) => `word${i}`).join(' ');
    const parts = reader.update(`${long}, and more`);
    expect(parts.length).toBeGreaterThan(0);
    expect(parts.every((part) => part.length <= 300)).toBe(true);
    expect([...parts, ...reader.finish(`${long}, and more.`)].join(' ')).toBe(`${long}, and more.`);
  });
});

describe('SpokenPieces', () => {
  it('yields pieces as they are pushed and ends when closed', async () => {
    const pieces = new SpokenPieces();
    const iterator = pieces[Symbol.asyncIterator]();
    const first = iterator.next();
    pieces.push(['Hello.']);
    expect(await first).toEqual({ done: false, value: 'Hello.' });

    const end = iterator.next();
    pieces.close();
    expect((await end).done).toBe(true);
  });

  it('sends the first sentence alone and gathers what queued behind it', async () => {
    const pieces = new SpokenPieces();
    pieces.push(['One.', 'Two.', 'Three.']);
    pieces.close();
    expect(await collect(pieces)).toEqual(['One.', 'Two. Three.']);
  });

  it('keeps a gathered piece to a size the voice takes in one go', async () => {
    const pieces = new SpokenPieces();
    const sentence = `${'a'.repeat(150)}.`;
    pieces.push(['First.', sentence, sentence]);
    pieces.close();
    expect(await collect(pieces)).toEqual(['First.', sentence, sentence]);
  });

  it('says what was queued before closing', async () => {
    const pieces = new SpokenPieces();
    pieces.push(['One.']);
    pieces.close();
    pieces.push(['Too late.']);
    expect(await collect(pieces)).toEqual(['One.']);
  });

  it('drops what is queued when abandoned', async () => {
    const pieces = new SpokenPieces();
    pieces.push(['One.', 'Two.']);
    pieces.abandon();
    expect(await collect(pieces)).toEqual([]);
  });

  it('wakes a waiting reader when abandoned', async () => {
    const pieces = new SpokenPieces();
    const waiting = collect(pieces);
    await Promise.resolve();
    pieces.abandon();
    await expect(waiting).resolves.toEqual([]);
  });
});
