import { useState, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import '@/tests/interactionDom';

import { CitationFootnote } from '@/features/reading/components/CitationFootnote';
import { ReaderPane } from '@/features/reading/components/ReaderPane';
import { useReaderStore } from '@/features/reading/stores/readerStore';
import type { SourceWithMetadata } from '@/types/conversation';

vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<typeof import('@/lib/api')>()), default: {} }));
// The reader's body asks the conversations store which chat it sits beside; none here.
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) =>
    selector({ activeConversationId: null, conversations: [], spaces: [], selectedSpaceId: null, messageVerification: new Map(), loadConversationLinkedDocuments: vi.fn() }),
}));

const source: SourceWithMetadata = {
  documentId: 'course:version-1',
  chunkId: 'version-1#0',
  fileName: 'Field notes',
  filePath: '',
  mimeType: 'text/html',
  category: 'Saved source',
  content: 'The full captured source.',
  score: 1,
  fileSizeBytes: 0,
  modifiedAt: '2026-09-19T00:00:00.000Z',
  citationId: 1,
};

// Narrow: the reader is an overlay, a non-modal dialog, rather than a docked pane.
const NARROW = { rowWidth: 1100, availableWidth: 840, resetKey: 'narrow' };

/** The test ends the recording, so nothing depends on how long a render takes. */
const recording: { finish: (() => void) | null } = { finish: null };

/** What the practice flow does: the opening is recorded first, and the popover says "busy" meanwhile. */
function RecordsFirst() {
  const [busy, setBusy] = useState(false);
  return (
    <CitationFootnote
      number={1}
      source={source}
      disabledReason={busy ? 'Another workbench action is in progress.' : undefined}
      onViewFile={() => {
        setBusy(true);
        recording.finish = () => {
          openReader();
          setBusy(false);
        };
      }}
    />
  );
}

function renderBeside(onViewFile: (() => void) | null) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}><MemoryRouter>{children}</MemoryRouter></QueryClientProvider>
  );
  return render(
    <>
      {onViewFile ? <CitationFootnote number={1} source={source} onViewFile={onViewFile} /> : <RecordsFirst />}
      <ReaderPane {...NARROW} />
    </>,
    { wrapper },
  );
}

const openReader = () => useReaderStore.getState().open('owner-1', [source], 0);
const chip = () => screen.getByRole('button', { name: /Citation 1/ });
const settle = (ms = 20) => act(() => new Promise<void>((resolve) => setTimeout(resolve, ms)));

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
  useReaderStore.setState({ session: null, resolvedLocations: new Map() });
  recording.finish = null;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('CitationFootnote beside a narrow reader', () => {
  it('leaves the reader open and focused when it was opened at once, and returns focus to the chip on close', async () => {
    renderBeside(openReader);
    fireEvent.click(chip());
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await settle();
    await settle();

    const dialog = screen.getByRole('dialog', { name: 'Field notes' });
    expect(dialog.contains(document.activeElement)).toBe(true);

    fireEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
    await settle();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(chip()).toHaveFocus();
  });

  it('keeps focus on the chip while the opening is recorded first, then leaves the reader open', async () => {
    renderBeside(null);
    fireEvent.click(chip());
    fireEvent.click(await screen.findByRole('button', { name: 'View source' }));
    await settle();
    expect(chip()).toHaveFocus();

    act(() => recording.finish?.());
    await settle();
    const dialog = screen.getByRole('dialog', { name: 'Field notes' });
    await settle();
    expect(screen.getByRole('dialog')).toBe(dialog);

    fireEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
    await settle();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(chip()).toHaveFocus();
  });
});
