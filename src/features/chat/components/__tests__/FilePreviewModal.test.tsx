import { useState } from 'react';

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FilePreviewModal } from '@/features/chat/components/FilePreviewModal';
import type { SourceWithMetadata } from '@/types/conversation';


// The reader body is shared with the docked pane and is covered by its own
// tests; here only the overlay behavior matters.
vi.mock('@/features/chat/components/reader/SourceReaderBody', () => ({
  SourceReaderBody: ({
    source,
    onMinimize,
    onToggleFocus,
  }: {
    source: SourceWithMetadata;
    onMinimize?: () => void;
    onToggleFocus?: () => void;
  }) => (
    <div data-testid="reader-body">
      <span>{source.fileName}</span>
      {onMinimize && (
        <button type="button" aria-label="Minimize reader" onClick={onMinimize}>
          minimize
        </button>
      )}
      {onToggleFocus && (
        <button type="button" aria-label="Expand reader" onClick={onToggleFocus}>
          expand
        </button>
      )}
    </div>
  ),
}));

function source(overrides: Partial<SourceWithMetadata> = {}): SourceWithMetadata {
  return {
    documentId: 'doc-1',
    chunkId: 'chunk-1',
    fileName: 'notes.txt',
    filePath: '/vault/notes.txt',
    mimeType: 'text/plain',
    category: 'local file',
    content: 'Saved citation content',
    score: 1,
    fileSizeBytes: 100,
    modifiedAt: '2026-09-20T00:00:00.000Z',
    ...overrides,
  };
}

/**
 * Controlled, like the journal screen: closing the reader flips `isOpen`, so
 * the dialog actually unmounts when it is dismissed.
 */
function ControlledPane({ presentation }: { presentation: 'reading-pane' | 'dialog' }) {
  const [isOpen, setIsOpen] = useState(true);
  return (
    <FilePreviewModal
      isOpen={isOpen}
      presentation={presentation}
      source={source()}
      onClose={() => setIsOpen(false)}
    />
  );
}

/**
 * The page the reader floats over: a trigger (what the user clicked to open
 * the reader) and a scroller that must keep its position when the reader
 * closes.
 *
 * The trigger is focused *before* the reader opens, exactly like the real
 * click-to-open flow: the reader is non-modal, so if focus were moved outside
 * of it after it opened, Radix would treat that as a dismissal.
 */
function renderPane(presentation: 'reading-pane' | 'dialog' = 'reading-pane') {
  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.setAttribute('data-testid', 'open-trigger');
  trigger.textContent = 'Open reader';
  document.body.appendChild(trigger);
  const scroller = document.createElement('div');
  scroller.setAttribute('data-testid', 'page-scroller');
  scroller.style.height = '100px';
  scroller.style.overflowY = 'auto';
  scroller.style.position = 'relative';
  scroller.innerHTML = '<div style="height: 500px">Journal body</div>';
  scroller.scrollTop = 120;
  document.body.appendChild(scroller);
  const outside = document.createElement('div');
  outside.setAttribute('data-testid', 'page-content');
  outside.textContent = 'Journal page underneath';
  document.body.appendChild(outside);

  trigger.focus();
  render(<ControlledPane presentation={presentation} />);
  return { trigger, scroller, outside };
}

/**
 * Radix registers its document-level pointerdown listener on a setTimeout(0),
 * so an outside click must wait one macrotask to land on a live listener.
 */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 0);
    });
  });
}

beforeEach(() => {
  // jsdom's cleanup only tears down React trees; the manually appended page
  // elements have to go too, or they accumulate across tests.
  document.body.querySelectorAll('[data-testid]').forEach((node) => node.remove());
});

describe('FilePreviewModal reading pane', () => {
  it('dismisses when the page underneath is clicked and returns focus without scrolling', async () => {
    const { trigger, scroller, outside } = renderPane();
    await waitFor(() => expect(screen.getByTestId('reader-body')).toBeTruthy());
    const scrollTopBefore = scroller.scrollTop;

    await settle();
    fireEvent.pointerDown(outside);

    expect(await screen.findByText('Journal page underneath')).toBeTruthy();
    expect(screen.queryByTestId('reader-body')).toBeNull();
    expect(document.activeElement).toBe(trigger);
    // Closing the reader must not scroll the page (the old behavior yanked it
    // back to the top of the journal entry).
    expect(scroller.scrollTop).toBe(scrollTopBefore);
  });

  it('dismisses on Escape', async () => {
    const { trigger } = renderPane();
    await waitFor(() => expect(screen.getByTestId('reader-body')).toBeTruthy());

    fireEvent.keyDown(document.body, { key: 'Escape' });

    expect(await screen.findByText('Journal page underneath')).toBeTruthy();
    expect(screen.queryByTestId('reader-body')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('minimizes to a pill, stays up while the page underneath is clicked, and closes from the pill', async () => {
    const { outside } = renderPane();
    await waitFor(() => expect(screen.getByTestId('reader-body')).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: 'Minimize reader' }));
    expect(screen.queryByTestId('reader-body')).toBeNull();
    expect(screen.getByRole('button', { name: 'Restore reader' })).toBeTruthy();
    expect(screen.getByText('notes.txt')).toBeTruthy();

    // The pill is sticky: reading the page underneath must not dismiss it.
    await settle();
    fireEvent.pointerDown(outside);
    expect(screen.getByRole('button', { name: 'Restore reader' })).toBeTruthy();

    // Restore brings the reader back.
    fireEvent.click(screen.getByRole('button', { name: 'Restore reader' }));
    expect(screen.getByTestId('reader-body')).toBeTruthy();

    // Minimize again, then close from the pill.
    fireEvent.click(screen.getByRole('button', { name: 'Minimize reader' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close reader' }));
    expect(screen.queryByTestId('reader-body')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Restore reader' })).toBeNull();
  });

  it('still closes on Escape while minimized', async () => {
    const { trigger } = renderPane();
    await waitFor(() => expect(screen.getByTestId('reader-body')).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: 'Minimize reader' }));
    expect(screen.getByRole('button', { name: 'Restore reader' })).toBeTruthy();

    fireEvent.keyDown(document.body, { key: 'Escape' });

    expect(await screen.findByText('Journal page underneath')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Restore reader' })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('does not offer minimize or expand in the full-screen dialog', async () => {
    renderPane('dialog');
    await waitFor(() => expect(screen.getByTestId('reader-body')).toBeTruthy());

    expect(screen.queryByRole('button', { name: 'Minimize reader' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Expand reader' })).toBeNull();
    // The dialog keeps its modal overlay.
    expect(document.querySelector('.source-reader--dialog')).toBeTruthy();
  });
});
