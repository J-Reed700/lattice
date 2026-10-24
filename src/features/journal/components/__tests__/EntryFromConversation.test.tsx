import type { ComponentProps } from 'react';

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { fetchConversationSpaceId } from '@/features/journal/api/queries';
import { EntryFromConversation } from '@/features/journal/components/EntryFromConversation';
import type { JournalSourceSummary } from '@/features/journal/hooks/useJournalSources';
import { ConversationSnapshotProvider } from '@/shared/conversations/conversationsStore';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';
import type { SnapshotMessage } from '@/types/api/dailyNotes';

const navigate = vi.hoisted(() => vi.fn());
vi.mock('react-router', async (importOriginal) => ({ ...(await importOriginal<typeof import('react-router')>()), useNavigate: () => navigate }));
vi.mock('@/features/journal/api/queries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/journal/api/queries')>()),
  fetchConversationSpaceId: vi.fn(),
}));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({
  FilePreviewModal: (props: { isOpen: boolean; source: { fileName: string } | null; citationIndex?: number; onClose: () => void; onCitationIndexChange: (i: number) => void }) =>
    props.isOpen ? (
      <div role="dialog" aria-label="Source preview">
        <span>Previewing {props.source?.fileName}</span>
        <span>Citation {props.citationIndex ?? 'none'}</span>
        <button type="button" onClick={() => props.onCitationIndexChange((props.citationIndex ?? 0) + 1)}>Next citation</button>
        <button type="button" onClick={props.onClose}>Close preview</button>
      </div>
    ) : null,
}));

const entry = { id: 'chat 1', title: 'Roman aqueducts', updatedAt: '2026-10-20T10:00:00Z', messageCount: 3 };
const sources = [
  { documentId: 'doc-1', filePath: '/notes/arches.md', fileName: 'Arch notes', score: 0.9, excerpt: 'Arches carry load.' },
  { filePath: 'https://example.org/concrete', fileName: 'Roman concrete', score: 0.5, excerpt: 'Volcanic ash.' },
];
const messages: SnapshotMessage[] = [
  { id: 'm1', role: 'user', content: 'How did aqueducts work?', createdAt: '2026-10-20T10:00:00Z' },
  { id: 'm2', role: 'assistant', content: 'Gravity and gentle gradients.', createdAt: '2026-10-20T10:01:00Z', metadata: JSON.stringify({ sources }) },
  { id: 'm3', role: 'assistant', content: 'Siphons crossed valleys.', createdAt: '2026-10-20T10:02:00Z' },
] as SnapshotMessage[];
const crossSource = (overrides: Partial<JournalSourceSummary> = {}): JournalSourceSummary => ({
  key: 'k1', documentId: 'doc-9', fileName: 'Vitruvius', filePath: '/library/vitruvius.pdf', category: 'Document', mimeType: '',
  excerpt: '', score: 1, referenceCount: 2, conversationIds: ['chat 7', 'chat 8'], conversationTitles: [], ...overrides,
});

let store: { createConversation: ReturnType<typeof vi.fn>; addConversationWebSource: ReturnType<typeof vi.fn> };

function renderEntry(overrides: Partial<ComponentProps<typeof EntryFromConversation>> = {}) {
  const props: ComponentProps<typeof EntryFromConversation> = {
    entry, messages, isLoading: false, crossEntrySources: [], crossEntryLoading: false, scannedConversationCount: 3,
    onJumpToEntry: vi.fn(), onNotify: vi.fn(), ...overrides,
  };
  render(
    <MemoryRouter>
      <ConversationSnapshotProvider value={store as unknown as ConversationsState}>
        <EntryFromConversation {...props} />
      </ConversationSnapshotProvider>
    </MemoryRouter>,
  );
  return props;
}

async function expand(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: /^Conversation\s+2 replies\s+2 sources$/ }));
}

beforeEach(() => {
  navigate.mockReset();
  vi.mocked(fetchConversationSpaceId).mockReset().mockResolvedValue('history-space');
  store = { createConversation: vi.fn().mockResolvedValue('new chat'), addConversationWebSource: vi.fn().mockResolvedValue(true) };
});

describe('EntryFromConversation', () => {
  it('renders nothing without an entry, or a hint when embedded in the rail', () => {
    const { unmount } = render(
      <MemoryRouter>
        <ConversationSnapshotProvider value={store as unknown as ConversationsState}>
          <EntryFromConversation entry={null} messages={[]} isLoading={false} crossEntrySources={[]} crossEntryLoading={false} scannedConversationCount={0} onJumpToEntry={vi.fn()} onNotify={vi.fn()} />
        </ConversationSnapshotProvider>
      </MemoryRouter>,
    );
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    unmount();
    renderEntry({ entry: null, embedded: true });
    expect(screen.getByText('No entry selected')).toBeInTheDocument();
  });

  it('cannot be opened while the first reply is still loading', () => {
    renderEntry({ messages: [], isLoading: true });
    expect(screen.getByRole('button', { name: 'Conversation · loading…' })).toBeDisabled();
  });

  it('says there are no replies yet and offers to continue the chat when embedded', async () => {
    const user = userEvent.setup();
    renderEntry({ messages: [messages[0]], embedded: true });
    expect(screen.getByRole('heading', { name: 'Roman aqueducts' })).toBeInTheDocument();
    expect(screen.getByText('No replies yet')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Continue in Chat' }));
    expect(navigate).toHaveBeenCalledWith('/chat?conversationId=chat%201');
  });

  it('shows a loading summary in the rail while replies load', () => {
    renderEntry({ messages: [], isLoading: true, embedded: true });
    expect(screen.getByText('Loading…')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continue in Chat' })).not.toBeInTheDocument();
  });

  it('counts replies and sources in singular form', () => {
    renderEntry({ messages: [{ ...messages[1], metadata: JSON.stringify({ sources: [sources[0]] }) }], embedded: true });
    expect(screen.getByText('1 reply · 1 source')).toBeInTheDocument();
    expect(screen.getByText('Gravity and gentle gradients.')).toBeInTheDocument();
  });

  it('expands to the replies and the sources they cited', async () => {
    const user = userEvent.setup();
    renderEntry();
    expect(screen.queryByText('Replies (2)')).not.toBeInTheDocument();
    await expand(user);
    expect(screen.getByRole('button', { name: /Conversation/ })).toHaveAttribute('aria-expanded', 'true');
    expect(screen.queryByText('Siphons crossed valleys.')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Replies (2)' }));
    expect(screen.getByText('Gravity and gentle gradients.')).toBeInTheDocument();
    expect(screen.getByText('Siphons crossed valleys.')).toBeInTheDocument();
    expect(screen.getByText('Sources in this entry')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Open in Chat' }));
    expect(navigate).toHaveBeenCalledWith('/chat?conversationId=chat%201');
  });

  it('opens a cited source in the reader and steps through the entry citations', async () => {
    const user = userEvent.setup();
    renderEntry();
    await expand(user);
    await user.click(screen.getByRole('button', { name: /Arch notes/ }));
    const preview = screen.getByRole('dialog', { name: 'Source preview' });
    expect(preview).toHaveTextContent('Previewing Arch notes');
    expect(preview).toHaveTextContent('Citation 0');
    await user.click(screen.getByRole('button', { name: 'Next citation' }));
    expect(preview).toHaveTextContent('Previewing Roman concrete');
    // Past the last citation there is nothing to step to.
    await user.click(screen.getByRole('button', { name: 'Next citation' }));
    expect(preview).toHaveTextContent('Citation 1');
    await user.click(screen.getByRole('button', { name: 'Close preview' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('starts a new chat in the entry space with its web sources attached', async () => {
    const user = userEvent.setup();
    const props = renderEntry();
    await expand(user);
    await user.click(screen.getByRole('button', { name: 'More actions' }));
    await user.click(screen.getByRole('button', { name: 'New chat from sources' }));
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/chat?conversationId=new%20chat'));
    expect(fetchConversationSpaceId).toHaveBeenCalledWith('chat 1');
    expect(store.createConversation).toHaveBeenCalledWith('Sources · Roman aqueducts', 'history-space');
    expect(store.addConversationWebSource).toHaveBeenCalledTimes(1);
    expect(store.addConversationWebSource).toHaveBeenCalledWith('new chat', 'https://example.org/concrete', {
      title: 'Roman concrete', excerpt: 'Volcanic ash.', relevanceScore: 0.5,
    });
    expect(props.onNotify).not.toHaveBeenCalled();
  });

  it('says so when no web link could be attached to the new chat', async () => {
    const user = userEvent.setup();
    store.addConversationWebSource.mockResolvedValue(false);
    const props = renderEntry({ entry: { ...entry, title: '   ' } });
    await expand(user);
    await user.click(screen.getByRole('button', { name: 'More actions' }));
    await user.click(screen.getByRole('button', { name: 'New chat from sources' }));
    await waitFor(() => expect(props.onNotify).toHaveBeenCalledWith('info', 'Opened new chat. No web links were attached from this journal entry.'));
    expect(store.createConversation).toHaveBeenCalledWith('Sources · Journal Entry', 'history-space');
    expect(navigate).toHaveBeenCalled();
  });

  it('reports an entry with no cited sources instead of opening an empty chat', async () => {
    const user = userEvent.setup();
    const props = renderEntry({ messages: [{ ...messages[2] }] });
    await user.click(screen.getByRole('button', { name: /^Conversation\s+1 reply\s+0 sources$/ }));
    await user.click(screen.getByRole('button', { name: 'More actions' }));
    await user.click(screen.getByRole('button', { name: 'New chat from sources' }));
    expect(props.onNotify).toHaveBeenCalledWith('error', 'No citation sources found for this journal entry yet.');
    expect(store.createConversation).not.toHaveBeenCalled();
  });

  it.each([
    [new Error('Space is archived'), 'Space is archived'],
    ['boom', 'Failed to start new chat from sources.'],
  ])('reports a failure to create the chat (%s)', async (failure, message) => {
    const user = userEvent.setup();
    store.createConversation.mockRejectedValue(failure);
    const props = renderEntry();
    await expand(user);
    await user.click(screen.getByRole('button', { name: 'More actions' }));
    await user.click(screen.getByRole('button', { name: 'New chat from sources' }));
    await waitFor(() => expect(props.onNotify).toHaveBeenCalledWith('error', message));
    expect(navigate).not.toHaveBeenCalled();
  });

  it('lists sources across entries and jumps to the first entry citing one', async () => {
    const user = userEvent.setup();
    const props = renderEntry({ crossEntrySources: [crossSource(), crossSource({ key: 'k2', fileName: 'Frontinus', conversationIds: [] , filePath: '/library/frontinus.pdf', documentId: 'doc-10' })] });
    await expand(user);
    await user.click(screen.getByRole('button', { name: 'View sources across all entries' }));
    expect(screen.getByText(/Scanning 3 recent entr/)).toHaveTextContent('Scanning 3 recent entries');
    expect(screen.getByText(/Referenced in 2 entr/)).toHaveTextContent('Referenced in 2 entries');
    expect(screen.getByText(/Referenced in 0 entr/)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Open entry' }));
    expect(props.onJumpToEntry).toHaveBeenCalledWith('chat 7');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await user.click(screen.getByText('Vitruvius'));
    expect(screen.getByRole('dialog')).toHaveTextContent('Citation none');
  });

  it.each([
    [{ crossEntryLoading: true }, 'Loading sources…'],
    [{ crossEntrySources: [] }, 'No sources found across recent entries.'],
  ])('shows the cross-entry state %#', async (overrides, text) => {
    const user = userEvent.setup();
    renderEntry({ ...overrides, scannedConversationCount: 1 });
    await expand(user);
    await user.click(screen.getByRole('button', { name: 'View sources across all entries' }));
    expect(screen.getByText(text)).toBeInTheDocument();
    expect(screen.getByText(/Scanning 1 recent entr/)).toHaveTextContent('Scanning 1 recent entry');
  });
});
