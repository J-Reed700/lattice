import { beforeEach, describe, expect, it } from 'vitest';

import type { SourceWithMetadata } from '@/types/conversation';

import {
  MAX_RESOLVED_LOCATIONS,
  READER_DEFAULT_WIDTH,
  READER_MAX_WIDTH,
  READER_MIN_WIDTH,
  useReaderStore,
} from '../readerStore';


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
  useReaderStore.setState({
    session: null,
    width: READER_DEFAULT_WIDTH,
    resolvedLocations: new Map(),
  });
});

describe('the chat reader session', () => {
  it('keeps the mark that was clicked, and lets go of it when stepping to another source', () => {
    useReaderStore.getState().open('message-1', citations, 1, 2);
    expect(useReaderStore.getState().session?.occurrence).toBe(2);

    // `]` moves to a source nobody clicked a mark of.
    useReaderStore.getState().setIndex(2);
    expect(useReaderStore.getState().session?.occurrence).toBeNull();

    // Opened from the source list: the source as a whole.
    useReaderStore.getState().open('message-1', citations, 0);
    expect(useReaderStore.getState().session?.occurrence).toBeNull();
  });

  it('opens on the citation it was given and remembers who it belongs to', () => {
    useReaderStore.getState().open('message-1', citations, 1);

    const session = useReaderStore.getState().session;
    expect(session?.ownerKey).toBe('message-1');
    expect(session?.index).toBe(1);
    expect(session?.citations).toHaveLength(3);
  });

  it('lands on a real citation when asked for one that is not there', () => {
    useReaderStore.getState().open('message-1', citations, 9);
    expect(useReaderStore.getState().session?.index).toBe(2);

    useReaderStore.getState().setIndex(-4);
    expect(useReaderStore.getState().session?.index).toBe(0);
  });

  it('stays shut when there is nothing to show', () => {
    useReaderStore.getState().open('message-1', [], 0);
    expect(useReaderStore.getState().session).toBeNull();
  });

  it('is a citing session by default, and not when opened to read a source for itself', () => {
    useReaderStore.getState().open('message-1', citations, 0);
    expect(useReaderStore.getState().session?.citing).toBe(true);

    useReaderStore.getState().open('saved-source', citations, 0, null, { citing: false });
    expect(useReaderStore.getState().session?.citing).toBe(false);
  });

  it('releases the session only for the owner that opened it', () => {
    useReaderStore.getState().open('message-1', citations, 0);

    useReaderStore.getState().release('message-2');
    expect(useReaderStore.getState().session?.ownerKey).toBe('message-1');

    useReaderStore.getState().release('message-1');
    expect(useReaderStore.getState().session).toBeNull();
  });

  it('forgets the session when it closes, so the next answer starts clean', () => {
    useReaderStore.getState().open('message-1', citations, 1);
    useReaderStore.getState().close();
    expect(useReaderStore.getState().session).toBeNull();
  });
});

describe('the chat reader width', () => {
  it('survives the session that set it', () => {
    useReaderStore.getState().setWidth(640);
    expect(useReaderStore.getState().width).toBe(640);
    expect(localStorage.getItem('chat.reader.width')).toBe('640');
  });

  it('refuses a width no document reads well at', () => {
    useReaderStore.getState().setWidth(40);
    expect(useReaderStore.getState().width).toBe(READER_MIN_WIDTH);

    useReaderStore.getState().setWidth(4000);
    expect(useReaderStore.getState().width).toBe(READER_MAX_WIDTH);
  });
});

describe('locations a viewer resolved', () => {
  it('bounds retained labels and can resolve an evicted citation again', () => {
    const remember = useReaderStore.getState().rememberResolvedLocation;
    for (let i = 0; i < MAX_RESOLVED_LOCATIONS; i += 1) {
      remember(`chunk-${i}`, `p. ${i}`);
    }
    remember('chunk-0', 'p. corrected');
    remember('new-chunk', 'p. new');
    const locations = useReaderStore.getState().resolvedLocations;
    expect(locations.size).toBe(MAX_RESOLVED_LOCATIONS);
    expect(locations.get('chunk-0')).toBe('p. corrected');
    expect(locations.has('chunk-1')).toBe(false);
    remember('chunk-1', 'p. 1');
    expect(useReaderStore.getState().resolvedLocations.get('chunk-1')).toBe('p. 1');
    expect(useReaderStore.getState().resolvedLocations.size).toBe(MAX_RESOLVED_LOCATIONS);
  });

  it('are kept per chunk, so every answer citing it says the page', () => {
    useReaderStore.getState().rememberResolvedLocation('chunk-1', 'p. 12');
    expect(useReaderStore.getState().resolvedLocations.get('chunk-1')).toBe('p. 12');
  });

  it('ignore an empty label rather than blanking a page already found', () => {
    useReaderStore.getState().rememberResolvedLocation('chunk-1', 'p. 12');
    useReaderStore.getState().rememberResolvedLocation('chunk-1', '');
    expect(useReaderStore.getState().resolvedLocations.get('chunk-1')).toBe('p. 12');
  });
});
