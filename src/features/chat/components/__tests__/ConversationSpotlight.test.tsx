import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ConversationSpotlight } from '@/features/chat/components/ConversationSpotlight';

const h = vi.hoisted(() => ({
  search: { isLoading: false, conversations: [] as unknown[], bookmarks: [] as unknown[] },
  searchArgs: [] as unknown[],
  selectConversation: vi.fn(),
  scrollToMessage: vi.fn(),
  reducedMotion: false,
}));

vi.mock('@/features/chat/hooks/useSpotlightSearch', () => ({
  useSpotlightSearch: (...args: unknown[]) => { h.searchArgs = args; return h.search; },
}));
vi.mock('@/hooks/useDebounce', () => ({ useDebounce: (value: unknown) => value }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select({
    selectConversation: h.selectConversation,
    spaces: [{ id: 's-hist', name: 'History', accentColor: '#884422' }, { id: 's-plain', name: 'Plain', accentColor: null }],
  }),
}));
vi.mock('@/utils/chatMessageNavigation', () => ({ scrollToMessage: h.scrollToMessage }));
vi.mock('framer-motion', async (importOriginal) => ({
  ...(await importOriginal<typeof import('framer-motion')>()),
  useReducedMotion: () => h.reducedMotion,
}));

const conversation = (id: string, title: string, spaceId: string | null) => ({ id, title, spaceId, updatedAt: '2026-05-01T10:00:00Z' });
const bookmark = (id: string, overrides: Record<string, unknown> = {}) => ({
  id, conversationId: 'c9', messageId: `m-${id}`, spaceId: 's-hist', title: null,
  conversationTitle: 'Byzantium', messagePreview: 'Constantinople fell in 1453', ...overrides,
});

const options = () => screen.getAllByRole('option');

describe('ConversationSpotlight', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.reducedMotion = false;
    h.selectConversation.mockResolvedValue(undefined);
    h.search = {
      isLoading: false,
      conversations: [conversation('c1', 'Roman roads', 's-hist'), conversation('c2', 'Bread', null), conversation('c3', 'Lost', 's-gone')],
      bookmarks: [bookmark('b1', { title: 'Fall of Constantinople' }), bookmark('b2', { spaceId: 's-unknown' })],
    };
  });

  it('searches as you type, only while open', () => {
    const { rerender } = render(<ConversationSpotlight isOpen={false} onClose={vi.fn()} />);
    expect(h.searchArgs).toEqual(['', false]);
    rerender(<ConversationSpotlight isOpen onClose={vi.fn()} />);
    fireEvent.change(screen.getByPlaceholderText('Search conversations and references'), { target: { value: 'rome' } });
    expect(h.searchArgs).toEqual(['rome', true]);
  });

  it('lists conversations then references, first one selected', () => {
    render(<ConversationSpotlight isOpen onClose={vi.fn()} />);
    expect(options()).toHaveLength(5);
    expect(options()[0]).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText(/^History · Conversation · /)).toBeInTheDocument();
    expect(screen.getByText(/^Conversation · /)).toBeInTheDocument();
    expect(screen.getByText(/^s-gone · Conversation · /)).toBeInTheDocument();
    expect(screen.getByText('Fall of Constantinople')).toBeInTheDocument();
    expect(screen.getAllByText('Byzantium')).toHaveLength(1);
    expect(screen.getByText('History · Byzantium · Constantinople fell in 1453')).toBeInTheDocument();
    expect(screen.getByText('s-unknown · Byzantium · Constantinople fell in 1453')).toBeInTheDocument();
  });

  it('shows a spinner while searching and says when nothing matches', () => {
    h.search = { isLoading: true, conversations: [], bookmarks: [] };
    const { rerender } = render(<ConversationSpotlight isOpen onClose={vi.fn()} />);
    expect(screen.queryByText('No matches.')).not.toBeInTheDocument();
    h.search = { isLoading: false, conversations: [], bookmarks: [] };
    rerender(<ConversationSpotlight isOpen onClose={vi.fn()} />);
    expect(screen.getByText('No matches.')).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(h.selectConversation).not.toHaveBeenCalled();
  });

  it('opens a conversation on click and closes', async () => {
    const onClose = vi.fn();
    render(<ConversationSpotlight isOpen onClose={onClose} />);
    fireEvent.click(screen.getByText('Bread'));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(h.selectConversation).toHaveBeenCalledWith('c2');
    expect(h.scrollToMessage).not.toHaveBeenCalled();
  });

  it('opens a reference at its message', async () => {
    const onClose = vi.fn();
    render(<ConversationSpotlight isOpen onClose={onClose} />);
    fireEvent.click(screen.getByText('Fall of Constantinople'));
    await waitFor(() => expect(h.scrollToMessage).toHaveBeenCalledWith('m-b1'));
    expect(h.selectConversation).toHaveBeenCalledWith('c9');
    expect(onClose).toHaveBeenCalled();
  });

  it('moves the selection with the arrow keys and the mouse, within bounds', async () => {
    const onClose = vi.fn();
    render(<ConversationSpotlight isOpen onClose={onClose} />);
    fireEvent.keyDown(window, { key: 'ArrowUp' });
    expect(options()[0]).toHaveAttribute('aria-selected', 'true');
    for (let i = 0; i < 6; i += 1) fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(options()[4]).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(window, { key: 'ArrowUp' });
    expect(options()[3]).toHaveAttribute('aria-selected', 'true');
    fireEvent.mouseEnter(screen.getByText('Bread').closest('button')!);
    expect(options()[1]).toHaveAttribute('aria-selected', 'true');
    fireEvent.mouseEnter(screen.getByText('Fall of Constantinople').closest('button')!);
    fireEvent.keyDown(window, { key: 'x' });
    await act(async () => { fireEvent.keyDown(window, { key: 'Enter' }); });
    expect(h.selectConversation).toHaveBeenCalledWith('c9');
  });

  it('draws a still selection bar when motion is reduced', () => {
    h.reducedMotion = true;
    render(<ConversationSpotlight isOpen onClose={vi.fn()} />);
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(options()[3]).toHaveAttribute('aria-selected', 'true');
  });

  it('closes on Escape and ignores keys once closed', () => {
    const onClose = vi.fn();
    const { rerender } = render(<ConversationSpotlight isOpen onClose={onClose} />);
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
    rerender(<ConversationSpotlight isOpen={false} onClose={onClose} />);
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(h.selectConversation).not.toHaveBeenCalled();
  });
});
