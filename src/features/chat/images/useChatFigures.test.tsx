import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { SourceWithMetadata } from '@/types/conversation';

import { useChatFigures } from './useChatFigures';

vi.mock('./chatImageUrl', () => ({ chatImageUrl: (image: { fileName: string }) => `chat-image://${image.fileName}` }));

const image = (ref: string, sourceUrl: string | null) => ({
  id: 'a'.repeat(32), ref, fileName: `${'b'.repeat(31)}${ref.slice(-1)}.png`, mime: 'image/png', width: 640, height: 480, byteSize: 1,
  caption: `Figure ${ref}`, sourceUrl, sourceTitle: sourceUrl ? 'Kew Gardens' : null,
  origin: sourceUrl ? 'page' : 'attachment', license: 'CC BY', attribution: 'RBG Kew',
});

const source = (url: string) => ({ filePath: url, documentId: url, fileName: url }) as unknown as SourceWithMetadata;

describe('useChatFigures', () => {
  it('has no figures for a message without pictures', () => {
    const { result } = renderHook(() => useChatFigures({ tempId: 't' } as never, new Map()));
    expect(result.current).toEqual([]);
  });

  it('serves each picture and numbers it by the citation of its source page', () => {
    const message = { tempId: 't', images: [
      image('img-1', 'https://www.kew.org/plants/oak#leaves'),
      image('img-2', 'https://example.org/not-cited'),
      image('att-3', null),
    ] };
    const citations = new Map([
      [1, source('https://WWW.KEW.ORG/plants/oak/')],
      [2, source('https://www.kew.org/plants/oak')],
      [3, source('/local/notes.md')],
    ]);
    const { result } = renderHook(() => useChatFigures(message as never, citations));
    expect(result.current.map((figure) => [figure.ref, figure.citationNumber, figure.url])).toEqual([
      ['img-1', 1, `chat-image://${'b'.repeat(31)}1.png`],
      ['img-2', null, `chat-image://${'b'.repeat(31)}2.png`],
      ['att-3', null, `chat-image://${'b'.repeat(31)}3.png`],
    ]);
    expect(result.current[0]).toMatchObject({ caption: 'Figure img-1', width: 640, height: 480, sourceTitle: 'Kew Gardens', license: 'CC BY', attribution: 'RBG Kew' });
  });

  it('matches a picture whose address is not a URL as written', () => {
    const message = { tempId: 't', images: [image('img-1', 'https://www.kew.org/x')] };
    const citations = new Map([[4, { filePath: 'https://www.kew.org/x', documentId: 'web:https%3A%2F%2Fwww.kew.org%2Fx', fileName: 'x' } as unknown as SourceWithMetadata]]);
    const { result } = renderHook(() => useChatFigures(message as never, citations));
    expect(result.current[0].citationNumber).toBe(4);
  });
});
