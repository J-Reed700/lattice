import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  READER_DEFAULT_WIDTH,
  READER_MIN_WIDTH,
  useChatReaderStore,
} from '@/stores/chatReaderStore';
import type { SourceWithMetadata } from '@/types/conversation';

import { ChatReaderPane } from '../ChatReaderPane';

let activeConversationId: string | null = 'conversation-1';

vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ activeConversationId }),
}));
// The document itself is `SourceReaderBody`'s business and is tested through
// the reader it is shown in; this file is about the pane around it.
vi.mock('../SourceReaderBody', () => ({
  SourceReaderBody: () => <div data-testid="reader-body">Document</div>,
}));
vi.mock('../../FilePreviewModal', () => ({
  FilePreviewModal: () => <div data-testid="reader-overlay">Document</div>,
}));

const source: SourceWithMetadata = {
  documentId: 'document-1',
  chunkId: 'chunk-1',
  fileName: 'Source 1.md',
  filePath: '/vault/source-1.md',
  mimeType: 'text/markdown',
  category: 'note',
  content: 'Passage one',
  score: 0.5,
  fileSizeBytes: 1024,
  modifiedAt: '2026-09-19T00:00:00.000Z',
  citationId: 1,
};

/** Wide enough for a 560px answer beside the reader, and then some. */
const WIDE = { rowWidth: 1728, availableWidth: 1468 };
/** Not wide enough for both. */
const NARROW = { rowWidth: 1100, availableWidth: 840 };

const openReader = () =>
  act(() => {
    useChatReaderStore.getState().open('message-1', [source], 0);
  });

beforeEach(() => {
  activeConversationId = 'conversation-1';
  localStorage.clear();
  useChatReaderStore.setState({
    session: null,
    width: READER_DEFAULT_WIDTH,
    resolvedLocations: new Map(),
  });
});

describe('where the reader appears', () => {
  it('takes no room at all until a citation is opened', () => {
    render(<ChatReaderPane {...WIDE} />);
    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
    expect(screen.queryByTestId('reader-overlay')).not.toBeInTheDocument();
  });

  it('docks beside the answer when the row can hold both', () => {
    render(<ChatReaderPane {...WIDE} />);
    openReader();

    expect(screen.getByTestId('reader-body')).toBeInTheDocument();
    expect(screen.getByLabelText('Source reader')).toHaveStyle({
      width: `${READER_DEFAULT_WIDTH}px`,
    });
  });

  it('falls back to the overlay rather than squeezing the answer', () => {
    render(<ChatReaderPane {...NARROW} />);
    openReader();

    expect(screen.getByTestId('reader-overlay')).toBeInTheDocument();
    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
  });
});

describe('resizing the docked reader', () => {
  it('moves with the arrow keys and returns to its default on a double click', () => {
    render(<ChatReaderPane {...WIDE} />);
    openReader();
    const handle = screen.getByRole('separator', { name: 'Resize source reader' });

    fireEvent.keyDown(handle, { key: 'ArrowLeft' });
    expect(useChatReaderStore.getState().width).toBe(READER_DEFAULT_WIDTH + 16);
    fireEvent.keyDown(handle, { key: 'ArrowRight' });
    expect(useChatReaderStore.getState().width).toBe(READER_DEFAULT_WIDTH);

    fireEvent.keyDown(handle, { key: 'ArrowRight' });
    fireEvent.doubleClick(handle);
    expect(useChatReaderStore.getState().width).toBe(READER_DEFAULT_WIDTH);
  });

  it('never gives the answer less room than it needs', () => {
    // 1000px to share: 560 for the answer leaves 440 for the reader, whatever
    // the remembered width says.
    useChatReaderStore.setState({ width: 900 });
    render(<ChatReaderPane rowWidth={1260} availableWidth={1000} />);
    openReader();

    expect(screen.getByLabelText('Source reader')).toHaveStyle({ width: '440px' });

    const handle = screen.getByRole('separator', { name: 'Resize source reader' });
    fireEvent.keyDown(handle, { key: 'ArrowLeft' });
    expect(screen.getByLabelText('Source reader')).toHaveStyle({ width: '440px' });
    expect(handle).toHaveAttribute('aria-valuemin', String(READER_MIN_WIDTH));
    expect(handle).toHaveAttribute('aria-valuemax', '440');
  });
});

describe('closing the reader', () => {
  it('closes on Escape and hands focus back to what opened it', () => {
    render(
      <>
        <button type="button">Citation 1</button>
        <ChatReaderPane {...WIDE} />
      </>
    );
    const chip = screen.getByRole('button', { name: 'Citation 1' });
    chip.focus();
    openReader();
    expect(screen.getByLabelText('Source reader')).toHaveFocus();

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(useChatReaderStore.getState().session).toBeNull();
    expect(chip).toHaveFocus();
  });

  it('closes when the chat moves to another conversation', () => {
    const { rerender } = render(<ChatReaderPane {...WIDE} />);
    openReader();
    expect(screen.getByTestId('reader-body')).toBeInTheDocument();

    activeConversationId = 'conversation-2';
    rerender(<ChatReaderPane {...WIDE} rowWidth={WIDE.rowWidth + 1} />);

    expect(useChatReaderStore.getState().session).toBeNull();
  });
});
