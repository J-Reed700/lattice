import { describe, expect, it } from 'vitest';


import type { ConversationMessageBookmarkDto } from '@/types';
import type { PassageReferenceDto } from '@/types/api/references';

import {
  bookmarkOrigin,
  bookmarkPreview,
  bookmarkTitle,
  mergeInboxItems,
  passagePreview,
  passageTitle,
  PASSAGE_ORIGIN,
} from '../inboxItems';

function bookmark(
  overrides: Partial<ConversationMessageBookmarkDto> = {},
): ConversationMessageBookmarkDto {
  return {
    id: 'bm_1',
    conversationId: 'conv_1',
    conversationTitle: 'Sleep study',
    spaceId: 'space_general',
    messageId: 'msg_1',
    messageRole: 'assistant',
    messagePreview: 'A preview line.',
    title: null,
    note: null,
    createdAt: '2026-09-05T10:00:00.000Z',
    ...overrides,
  } as ConversationMessageBookmarkDto;
}

function passage(overrides: Partial<PassageReferenceDto> = {}): PassageReferenceDto {
  return {
    id: 'pref_1',
    documentId: 'doc_1',
    chunkId: 'chunk_1',
    filePath: '/vault/paper.pdf',
    fileName: 'paper.pdf',
    locator: 'p. 12',
    text: '\n  First line of the passage.\nSecond line.',
    title: null,
    note: null,
    createdAt: '2026-09-05T09:00:00.000Z',
    ...overrides,
  };
}

describe('mergeInboxItems', () => {
  it('orders strictly newest-first across both kinds', () => {
    const items = mergeInboxItems(
      [
        bookmark({ id: 'bm_old', createdAt: '2026-09-01T10:00:00.000Z' }),
        bookmark({ id: 'bm_new', createdAt: '2026-09-06T10:00:00.000Z' }),
      ],
      [
        passage({ id: 'pref_mid', createdAt: '2026-09-03T10:00:00.000Z' }),
        passage({ id: 'pref_newest', createdAt: '2026-09-07T10:00:00.000Z' }),
      ],
    );

    expect(items.map((item) => item.id)).toEqual([
      'pref_newest',
      'bm_new',
      'pref_mid',
      'bm_old',
    ]);
  });

  it('breaks ties message-before-passage and is stable across calls', () => {
    const at = '2026-09-05T10:00:00.000Z';
    const bookmarks = [bookmark({ id: 'bm_a', createdAt: at })];
    const passages = [passage({ id: 'pref_a', createdAt: at })];

    const first = mergeInboxItems(bookmarks, passages);
    const second = mergeInboxItems(bookmarks, passages);

    expect(first.map((item) => item.id)).toEqual(['bm_a', 'pref_a']);
    expect(second.map((item) => item.id)).toEqual(first.map((item) => item.id));
  });

  it('sinks an unparseable createdAt to the end without throwing', () => {
    const items = mergeInboxItems(
      [bookmark({ id: 'bm_bad', createdAt: 'not a date' })],
      [passage({ id: 'pref_good', createdAt: '2026-09-05T10:00:00.000Z' })],
    );
    expect(items.map((item) => item.id)).toEqual(['pref_good', 'bm_bad']);
  });
});

describe('row labels', () => {
  // These are the functions `PassageListItem`, `ReferenceListItem` and
  // `PassageReader` call, so what is asserted here is what is rendered.
  it('labels each kind by where it came from', () => {
    expect(PASSAGE_ORIGIN).toBe('Document');
    expect(bookmarkOrigin(true)).toBe('Journal');
    expect(bookmarkOrigin(false)).toBe('Chat');
  });

  it('falls back to the file name for a passage with no title', () => {
    expect(passageTitle(passage({ title: null }))).toBe('paper.pdf');
    expect(passageTitle(passage({ title: '  Trial design  ' }))).toBe('Trial design');
  });

  it('falls back to the conversation, then to "Untitled reference"', () => {
    expect(bookmarkTitle(bookmark({ title: '  Kept for later  ' }))).toEqual({
      title: 'Kept for later',
      hasTitle: true,
    });
    expect(bookmarkTitle(bookmark({ title: null }))).toEqual({
      title: 'Sleep study',
      hasTitle: true,
    });
    expect(bookmarkTitle(bookmark({ title: '  ', conversationTitle: '  ' }))).toEqual({
      title: 'Untitled reference',
      hasTitle: false,
    });
  });

  it('previews the first non-empty line of a passage and the bookmark preview', () => {
    expect(passagePreview(passage())).toBe('First line of the passage.');
    expect(bookmarkPreview(bookmark())).toBe('A preview line.');
    expect(bookmarkPreview(bookmark({ messagePreview: undefined }))).toBe('');
  });
});
