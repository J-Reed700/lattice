import { beforeEach, describe, expect, it } from 'vitest';

import {
  READER_DEFAULT_WIDTH,
  READER_MAX_WIDTH,
  READER_MIN_WIDTH,
  useChatReaderStore,
} from '../chatReaderStore';

import type { SourceWithMetadata } from '../../types/conversation';

const source = (number: number): SourceWithMetadata => ({
  documentId: `document-${number}`,
  chunkId: `chunk-${number}`,
  fileName: `Source ${number}.md`,
  filePath: `/vault/source-${number}.md`,
  mimeType: 'text/markdown',
  category: 'note',
  content: `Passage ${number}`,
  score: 0.5,
  fileSizeBytes: 1024,
  modifiedAt: '2026-09-19T00:00:00.000Z',
  citationId: number,
});

const citations = [source(1), source(2), source(3)];

beforeEach(() => {
  localStorage.clear();
  useChatReaderStore.setState({
    session: null,
    width: READER_DEFAULT_WIDTH,
    resolvedLocations: new Map(),
  });
});

describe('the chat reader session', () => {
  it('opens on the citation it was given and remembers who it belongs to', () => {
    useChatReaderStore.getState().open('message-1', citations, 1);

    const session = useChatReaderStore.getState().session;
    expect(session?.ownerKey).toBe('message-1');
    expect(session?.index).toBe(1);
    expect(session?.citations).toHaveLength(3);
  });

  it('lands on a real citation when asked for one that is not there', () => {
    useChatReaderStore.getState().open('message-1', citations, 9);
    expect(useChatReaderStore.getState().session?.index).toBe(2);

    useChatReaderStore.getState().setIndex(-4);
    expect(useChatReaderStore.getState().session?.index).toBe(0);
  });

  it('stays shut when there is nothing to show', () => {
    useChatReaderStore.getState().open('message-1', [], 0);
    expect(useChatReaderStore.getState().session).toBeNull();
  });

  it('forgets the session when it closes, so the next answer starts clean', () => {
    useChatReaderStore.getState().open('message-1', citations, 1);
    useChatReaderStore.getState().close();
    expect(useChatReaderStore.getState().session).toBeNull();
  });
});

describe('the chat reader width', () => {
  it('survives the session that set it', () => {
    useChatReaderStore.getState().setWidth(640);
    expect(useChatReaderStore.getState().width).toBe(640);
    expect(localStorage.getItem('chat.reader.width')).toBe('640');
  });

  it('refuses a width no document reads well at', () => {
    useChatReaderStore.getState().setWidth(40);
    expect(useChatReaderStore.getState().width).toBe(READER_MIN_WIDTH);

    useChatReaderStore.getState().setWidth(4000);
    expect(useChatReaderStore.getState().width).toBe(READER_MAX_WIDTH);
  });
});

describe('locations a viewer resolved', () => {
  it('are kept per chunk, so every answer citing it says the page', () => {
    useChatReaderStore.getState().rememberResolvedLocation('chunk-1', 'p. 12');
    expect(useChatReaderStore.getState().resolvedLocations.get('chunk-1')).toBe('p. 12');
  });

  it('ignore an empty label rather than blanking a page already found', () => {
    useChatReaderStore.getState().rememberResolvedLocation('chunk-1', 'p. 12');
    useChatReaderStore.getState().rememberResolvedLocation('chunk-1', '');
    expect(useChatReaderStore.getState().resolvedLocations.get('chunk-1')).toBe('p. 12');
  });
});
