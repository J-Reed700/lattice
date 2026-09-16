import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui';
import type { ConversationMessageBookmarkDto } from '@/types';

import { ReferenceInbox } from '../ReferenceInbox';

const listMessageBookmarks = vi.fn();
const listWorkspaceNotes = vi.fn();
const listConversationSpaces = vi.fn();
const listJournals = vi.fn();
const listPassageReferences = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    listMessageBookmarks: (...args: unknown[]) => listMessageBookmarks(...args),
    listWorkspaceNotes: () => listWorkspaceNotes(),
    listConversationSpaces: () => listConversationSpaces(),
    listJournals: () => listJournals(),
    listPassageReferences: (...args: unknown[]) => listPassageReferences(...args),
    getConversationMessages: async () => ({ ok: false, error: 'not loaded in tests' }),
  };
  return { VaultAPI: api, default: api };
});

// The preview modal pulls in the PDF renderer; the inbox's selection behaviour
// does not need it.
vi.mock('@/components/Chat/FilePreviewModal', () => ({
  FilePreviewModal: () => null,
}));

function bookmark(
  overrides: Partial<ConversationMessageBookmarkDto> = {},
): ConversationMessageBookmarkDto {
  return {
    id: 'bm_1',
    conversationId: 'conv_1',
    conversationTitle: 'First reference',
    spaceId: 'space_general',
    messageId: 'msg_1',
    messageRole: 'assistant',
    messagePreview: 'A preview line.',
    title: null,
    note: null,
    createdAt: '2026-09-06T10:00:00.000Z',
    ...overrides,
  } as ConversationMessageBookmarkDto;
}

function LocationProbe() {
  const location = useLocation();
  return <span data-testid="search">{location.search}</span>;
}

function renderInbox(entry = '/references') {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <MemoryRouter initialEntries={[entry]}>
          <LocationProbe />
          <ReferenceInbox />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

/**
 * Regression cover for the selection loop: the hook used to force `?referenceId=`
 * back onto the selection while the surface forced the selection into the URL,
 * so a second click never settled and pinned the CPU. The explicit timeouts
 * below make a return of that behaviour fail the run instead of hanging it.
 */
describe('ReferenceInbox selection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listMessageBookmarks.mockResolvedValue({
      ok: true,
      data: {
        bookmarks: [
          bookmark({ id: 'bm_1', conversationTitle: 'First reference' }),
          bookmark({
            id: 'bm_2',
            messageId: 'msg_2',
            conversationTitle: 'Second reference',
            createdAt: '2026-09-05T10:00:00.000Z',
          }),
        ],
      },
    });
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [] } });
    listConversationSpaces.mockResolvedValue({ ok: true, data: [] });
    listJournals.mockResolvedValue({ ok: true, data: [] });
    listPassageReferences.mockResolvedValue({ ok: true, data: [] });
  });

  it(
    'settles on the row that was clicked, with the URL agreeing',
    async () => {
      const user = userEvent.setup();
      renderInbox();

      await waitFor(() => expect(screen.getByTitle('First reference')).toBeInTheDocument());
      await waitFor(() =>
        expect(screen.getByTestId('search')).toHaveTextContent('referenceId=bm_1'),
      );

      await user.click(screen.getByTitle('Second reference'));

      await waitFor(() =>
        expect(screen.getByTestId('search')).toHaveTextContent('referenceId=bm_2'),
      );
      const rows = screen.getAllByRole('button', { name: /reference/i });
      const active = rows.filter((row) => row.getAttribute('aria-current') === 'page');
      expect(active).toHaveLength(1);
      expect(active[0]).toHaveTextContent('Second reference');

      // Stable: the pair does not trade places on subsequent renders.
      await new Promise((resolve) => setTimeout(resolve, 60));
      expect(screen.getByTestId('search')).toHaveTextContent('referenceId=bm_2');
      expect(
        screen.getAllByRole('button', { name: /reference/i }).filter(
          (row) => row.getAttribute('aria-current') === 'page',
        )[0],
      ).toHaveTextContent('Second reference');
    },
    8_000,
  );

  it(
    'opens the requested reference from a deep link, then follows the user',
    async () => {
      const user = userEvent.setup();
      renderInbox('/references?referenceId=bm_2');

      await waitFor(() =>
        expect(screen.getByTestId('search')).toHaveTextContent('referenceId=bm_2'),
      );

      await user.click(screen.getByTitle('First reference'));
      await waitFor(() =>
        expect(screen.getByTestId('search')).toHaveTextContent('referenceId=bm_1'),
      );
    },
    8_000,
  );
});
