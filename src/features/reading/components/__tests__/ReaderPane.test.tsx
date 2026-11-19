import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ReaderPane } from '@/features/reading/components/ReaderPane';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import {
  READER_DEFAULT_WIDTH,
  READER_MIN_WIDTH,
  useReaderStore,
} from '@/features/reading/stores/readerStore';
import type { SourceWithMetadata } from '@/types/conversation';


// The document itself is `SourceReaderBody`'s business and is tested through
// the reader it is shown in; this file is about the pane around it.
vi.mock('@/features/reading/components/SourceReaderBody', () => ({
  SourceReaderBody: () => <div data-testid="reader-body">Document</div>,
}));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({
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
const WIDE = { rowWidth: 1728, availableWidth: 1468, resetKey: 'conversation-1' };
/** Not wide enough for both. */
const NARROW = { rowWidth: 1100, availableWidth: 840, resetKey: 'conversation-1' };

const openReader = () =>
  act(() => {
    useReaderStore.getState().open('message-1', [source], 0);
  });

beforeEach(() => {
  localStorage.clear();
  useCitationDisplayStore.setState({ visible: true });
  useReaderStore.setState({
    session: null,
    width: READER_DEFAULT_WIDTH,
    resolvedLocations: new Map(),
  });
});

describe('where the reader appears', () => {
  it('takes no room at all until a citation is opened', () => {
    render(<ReaderPane {...WIDE} />);
    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
    expect(screen.queryByTestId('reader-overlay')).not.toBeInTheDocument();
  });

  it('docks beside the answer when the row can hold both', () => {
    render(<ReaderPane {...WIDE} />);
    openReader();

    expect(screen.getByTestId('reader-body')).toBeInTheDocument();
    expect(screen.getByLabelText('Source reader')).toHaveStyle({
      width: `${READER_DEFAULT_WIDTH}px`,
    });
  });

  it('falls back to the overlay rather than squeezing the answer', () => {
    render(<ReaderPane {...NARROW} />);
    openReader();

    expect(screen.getByTestId('reader-overlay')).toBeInTheDocument();
    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
  });
});

describe('resizing the docked reader', () => {
  it('moves with the arrow keys and returns to its default on a double click', () => {
    render(<ReaderPane {...WIDE} />);
    openReader();
    const handle = screen.getByRole('separator', { name: 'Resize source reader' });

    fireEvent.keyDown(handle, { key: 'ArrowLeft' });
    expect(useReaderStore.getState().width).toBe(READER_DEFAULT_WIDTH + 16);
    fireEvent.keyDown(handle, { key: 'ArrowRight' });
    expect(useReaderStore.getState().width).toBe(READER_DEFAULT_WIDTH);

    fireEvent.keyDown(handle, { key: 'ArrowRight' });
    fireEvent.doubleClick(handle);
    expect(useReaderStore.getState().width).toBe(READER_DEFAULT_WIDTH);
  });

  it('never gives the answer less room than it needs', () => {
    // 1000px to share: 560 for the answer leaves 440 for the reader, whatever
    // the remembered width says.
    useReaderStore.setState({ width: 900 });
    render(<ReaderPane rowWidth={1260} availableWidth={1000} resetKey="conversation-1" />);
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
  it('closes when clean reading is enabled outside the chat toggle', () => {
    render(<ReaderPane {...WIDE} />);
    openReader();
    expect(screen.getByTestId('reader-body')).toBeInTheDocument();
    act(() => useCitationDisplayStore.getState().setVisible(false));
    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
    expect(useReaderStore.getState().session).toBeNull();
    act(() => useCitationDisplayStore.getState().setVisible(true));
    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
  });

  it('closes a citing session when citations are hidden, and renders nothing', () => {
    useCitationDisplayStore.setState({ visible: false });
    render(<ReaderPane {...WIDE} />);
    openReader();

    expect(screen.queryByTestId('reader-body')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Source reader')).not.toBeInTheDocument();
    expect(useReaderStore.getState().session).toBeNull();
  });

  it('keeps a source opened for itself open while citations are hidden', () => {
    useCitationDisplayStore.setState({ visible: false });
    render(<ReaderPane {...WIDE} />);
    act(() => {
      useReaderStore.getState().open('saved-source', [source], 0, null, { citing: false });
    });

    expect(screen.getByTestId('reader-body')).toBeInTheDocument();
    expect(useReaderStore.getState().session?.citing).toBe(false);
  });

  it('closes on Escape and hands focus back to what opened it', () => {
    render(
      <>
        <button type="button">Citation 1</button>
        <ReaderPane {...WIDE} />
      </>
    );
    const chip = screen.getByRole('button', { name: 'Citation 1' });
    chip.focus();
    openReader();
    expect(screen.getByLabelText('Source reader')).toHaveFocus();

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(useReaderStore.getState().session).toBeNull();
    expect(chip).toHaveFocus();
  });

  it('closes when the text it was opened from changes', () => {
    const { rerender } = render(<ReaderPane {...WIDE} />);
    openReader();
    expect(screen.getByTestId('reader-body')).toBeInTheDocument();

    rerender(<ReaderPane {...WIDE} resetKey="conversation-2" />);

    expect(useReaderStore.getState().session).toBeNull();
  });
});
