import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, useLocation } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { VaultAPI } from '@/lib/api';
import type { ConversationSpaceDto } from '@/types';

import { RailSpacesSection } from '../RailSpacesSection';
import { RAIL_SPACE_IDS_KEY, resolveVisibleRailSpaces } from '../railSpacesStorage';

const api = VaultAPI as unknown as Record<string, ReturnType<typeof vi.fn>>;

function makeSpace(index: number, overrides: Partial<ConversationSpaceDto> = {}): ConversationSpaceDto {
  return {
    id: `space-${index}`,
    name: `Space ${index}`,
    description: `Notes for space ${index}`,
    icon: null,
    accentColor: '#b45309',
    spacePrompt: null,
    toolPreferencesJson: null,
    isArchived: false,
    sortOrder: index,
    createdAt: '2026-10-01T00:00:00Z',
    updatedAt: '2026-10-20T00:00:00Z',
    ...overrides,
  };
}

function Location() {
  const loc = useLocation();
  return <output aria-label="location">{loc.pathname + loc.search}</output>;
}

function renderSpaces({
  compact = false,
  isOnChat = false,
  initialPath = '/home',
}: { compact?: boolean; isOnChat?: boolean; initialPath?: string } = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <MemoryRouter initialEntries={[initialPath]}>
          <RailSpacesSection compact={compact} isOnChat={isOnChat} />
          <Location />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

describe('RailSpacesSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    conversationUiStore.setState({ selectedSpaceId: null, activeConversationId: null });
    api.listConversationSpaces = vi.fn().mockResolvedValue({ ok: true, data: [] });
    api.createConversationSpace = vi.fn().mockResolvedValue({
      ok: true,
      data: makeSpace(99, { id: 'space-new', name: 'Biochemistry' }),
    });
  });

  it('handles 0 spaces gracefully with a quiet New space affordance and inline creation', async () => {
    const user = userEvent.setup();
    renderSpaces();

    const newBtn = await screen.findByRole('button', { name: 'New space' });
    await user.click(newBtn);

    const input = await screen.findByLabelText('New space name');
    await user.type(input, 'Biochemistry');
    await user.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(api.createConversationSpace).toHaveBeenCalledTimes(1));
    expect(api.createConversationSpace).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Biochemistry' }),
    );
    expect(conversationUiStore.getState().selectedSpaceId).toBe('space-new');
    expect(screen.getByRole('status', { name: 'location' })).toHaveTextContent('/chat');
  });

  it('shows non-archived spaces, syncs active state, and toggles back to All on Chat', async () => {
    const user = userEvent.setup();
    api.listConversationSpaces.mockResolvedValue({
      ok: true,
      data: [
        makeSpace(1, { id: 's-research', name: 'Research' }),
        makeSpace(2, { id: 's-design', name: 'Design' }),
        makeSpace(3, { id: 's-archived', name: 'Old Archive', isArchived: true }),
      ],
    });

    const { rerender } = renderSpaces({ isOnChat: false });

    expect(await screen.findByRole('button', { name: 'Space: Research' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Space: Design' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Space: Old Archive' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /More spaces/ })).not.toBeInTheDocument();

    // Clicking a space navigates to /chat and selects it in conversationUiStore
    await user.click(screen.getByRole('button', { name: 'Space: Research' }));
    expect(conversationUiStore.getState().selectedSpaceId).toBe('s-research');
    expect(screen.getByRole('status', { name: 'location' })).toHaveTextContent('/chat');

    // Re-render as if we are now on /chat; clicking the active space again clears back to All spaces
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <MemoryRouter initialEntries={['/chat']}>
            <RailSpacesSection compact={false} isOnChat />
            <Location />
          </MemoryRouter>
        </TooltipProvider>
      </QueryClientProvider>,
    );

    const activeBtn = await screen.findByRole('button', { name: 'Space: Research' });
    expect(activeBtn).toHaveAttribute('aria-pressed', 'true');
    await user.click(activeBtn);
    expect(conversationUiStore.getState().selectedSpaceId).toBeNull();
  });

  it('caps direct rail spaces at 5, shows +N More spaces, and persists pin configuration', async () => {
    const user = userEvent.setup();
    const eightSpaces = Array.from({ length: 8 }, (_, idx) =>
      makeSpace(idx + 1, { id: `sp-${idx + 1}`, name: `Topic ${idx + 1}` }),
    );
    api.listConversationSpaces.mockResolvedValue({ ok: true, data: eightSpaces });

    const { unmount } = renderSpaces();

    // Wait for spaces to load — only 5 rendered directly in the rail
    expect(await screen.findByRole('button', { name: 'Space: Topic 1' })).toBeInTheDocument();
    const section = screen.getByRole('region', { name: 'Spaces' });
    const railSpaceButtons = within(section).getAllByRole('button', { name: /^Space: / });
    expect(railSpaceButtons).toHaveLength(5);
    expect(screen.queryByRole('button', { name: 'Space: Topic 6' })).not.toBeInTheDocument();

    // More spaces row shows +3 (single unified responsive button)
    const moreButton = screen.getByRole('button', { name: 'More spaces (3 more)' });
    await user.click(moreButton);

    // Popover shows count and all 8 spaces
    const popover = await screen.findByRole('dialog');
    expect(within(popover).getByText('Showing 5 of 8 in sidebar')).toBeInTheDocument();
    expect(within(popover).getByText('Topic 8')).toBeInTheDocument();

    // Pinning a 6th space is disabled until one is unpinned
    expect(within(popover).getByRole('button', { name: 'Pin Topic 7 to sidebar' })).toBeDisabled();

    // Unpin Topic 1, then pin Topic 7
    await user.click(within(popover).getByRole('button', { name: 'Unpin Topic 1 from sidebar' }));
    const pin7 = within(popover).getByRole('button', { name: 'Pin Topic 7 to sidebar' });
    expect(pin7).not.toBeDisabled();
    await user.click(pin7);

    // Persisted in localStorage
    const saved = JSON.parse(localStorage.getItem(RAIL_SPACE_IDS_KEY) ?? '[]') as string[];
    expect(saved).toEqual(['sp-2', 'sp-3', 'sp-4', 'sp-5', 'sp-7']);

    // Filter inside popover works without altering the rail behind it
    await user.type(within(popover).getByLabelText('Filter spaces'), 'Topic 8');
    expect(within(popover).getByText('Topic 8')).toBeInTheDocument();
    expect(within(popover).queryByText('Topic 2')).not.toBeInTheDocument();

    // Selecting Topic 8 from popover switches space and navigates to /chat
    await user.click(within(popover).getByText('Topic 8'));
    expect(conversationUiStore.getState().selectedSpaceId).toBe('sp-8');
    expect(screen.getByRole('status', { name: 'location' })).toHaveTextContent('/chat');

    // Remounting preserves the pinned spaces from localStorage
    unmount();
    renderSpaces();
    expect(await screen.findByRole('button', { name: 'Space: Topic 7' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Space: Topic 1' })).not.toBeInTheDocument();
  });

  it('supports compact 60px collapsed rail mode cleanly', async () => {
    const user = userEvent.setup();
    const sixSpaces = Array.from({ length: 6 }, (_, idx) =>
      makeSpace(idx + 1, { id: `sp-${idx + 1}`, name: `Compact ${idx + 1}` }),
    );
    api.listConversationSpaces.mockResolvedValue({ ok: true, data: sixSpaces });

    renderSpaces({ compact: true });

    expect(await screen.findByRole('button', { name: 'Space: Compact 1' })).toBeInTheDocument();
    const compactMore = screen.getByRole('button', { name: 'More spaces (1 more)' });
    await user.click(compactMore);
    expect(await screen.findByText('Showing 5 of 6 in sidebar')).toBeInTheDocument();
  });

  it('honors an explicit empty pin list without snapping 5 defaults back in, and falls back only on stale deleted IDs', async () => {
    const user = userEvent.setup();
    const sixSpaces = Array.from({ length: 6 }, (_, idx) =>
      makeSpace(idx + 1, { id: `sp-${idx + 1}`, name: `Alpha ${idx + 1}` }),
    );

    // Storage-level checks for explicit [] vs stale deleted IDs
    const emptyExplicit = resolveVisibleRailSpaces(sixSpaces, []);
    expect(emptyExplicit.visible).toEqual([]);
    expect(emptyExplicit.hiddenCount).toBe(6);
    expect(emptyExplicit.isCustomized).toBe(true);

    const staleFallback = resolveVisibleRailSpaces(sixSpaces, ['deleted-1', 'deleted-2']);
    expect(staleFallback.visible.map((s) => s.id)).toEqual(['sp-1', 'sp-2', 'sp-3', 'sp-4', 'sp-5']);
    expect(staleFallback.hiddenCount).toBe(1);
    expect(staleFallback.isCustomized).toBe(false);

    // Interactive check: unpinning the last pinned space keeps 0 visible + More spaces (+6)
    localStorage.setItem(RAIL_SPACE_IDS_KEY, JSON.stringify(['sp-1']));
    api.listConversationSpaces.mockResolvedValue({ ok: true, data: sixSpaces });
    renderSpaces();

    expect(await screen.findByRole('button', { name: 'Space: Alpha 1' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'More spaces (5 more)' }));
    const popover = await screen.findByRole('dialog');
    await user.click(within(popover).getByRole('button', { name: 'Unpin Alpha 1 from sidebar' }));

    expect(within(popover).getByText('Showing 0 of 6 in sidebar')).toBeInTheDocument();
    expect(within(popover).getByRole('button', { name: 'Pin Alpha 2 to sidebar' })).not.toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Space: Alpha 1' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'More spaces (6 more)' })).toBeInTheDocument();
  });

  it('uses the full active pill only when on Chat to avoid dual competing active pills off Chat', async () => {
    conversationUiStore.setState({ selectedSpaceId: 'sp-1' });
    api.listConversationSpaces.mockResolvedValue({
      ok: true,
      data: [makeSpace(1, { id: 'sp-1', name: 'Research' })],
    });

    const { rerender } = renderSpaces({ isOnChat: false });
    const offChatBtn = await screen.findByRole('button', { name: 'Space: Research' });
    expect(offChatBtn).toHaveAttribute('aria-pressed', 'true');
    expect(offChatBtn).not.toHaveClass('bg-surface');

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    rerender(
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <MemoryRouter initialEntries={['/chat']}>
            <RailSpacesSection compact={false} isOnChat />
            <Location />
          </MemoryRouter>
        </TooltipProvider>
      </QueryClientProvider>,
    );
    const onChatBtn = await screen.findByRole('button', { name: 'Space: Research' });
    expect(onChatBtn).toHaveClass('bg-surface', 'shadow-sm', 'ring-1', 'ring-border-subtle');
  });
});
