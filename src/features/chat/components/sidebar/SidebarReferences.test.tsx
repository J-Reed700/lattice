import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { chatReferenceKey } from '@/utils/chatReferenceIndex';

import { SidebarReferences } from './SidebarReferences';

const h = vi.hoisted(() => ({
  navigate: vi.fn(),
  selectConversation: vi.fn(),
  scrollToMessage: vi.fn(),
  bookmarks: { data: undefined as unknown[] | undefined, isLoading: false },
  captures: { data: undefined as Map<string, unknown> | undefined, isFetching: false },
  bookmarkArgs: [] as unknown[],
  captureArgs: [] as unknown[],
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: () => ({ selectedSpaceId: 's1', selectConversation: h.selectConversation }),
}));
vi.mock('@/features/chat/hooks/workspaceQueries', () => ({
  useSidebarBookmarksQuery: (...args: unknown[]) => { h.bookmarkArgs = args; return h.bookmarks; },
  useCapturedReferencesQuery: (...args: unknown[]) => { h.captureArgs = args; return h.captures; },
}));
vi.mock('@/utils/chatMessageNavigation', () => ({ scrollToMessage: h.scrollToMessage }));

const bookmark = (id: string, role: string, overrides: Record<string, unknown> = {}) => ({
  id,
  conversationId: `c-${id}`,
  messageId: `m-${id}`,
  messageRole: role,
  title: null,
  conversationTitle: `Conversation ${id}`,
  messagePreview: `Preview ${id}`,
  createdAt: new Date().toISOString(),
  ...overrides,
});

const renderRefs = (props: { query?: string; active?: boolean } = {}) =>
  render(<SidebarReferences query={props.query ?? 'enzymes'} active={props.active ?? true} />, { wrapper: TooltipProvider });

describe('SidebarReferences', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.bookmarks = { data: undefined, isLoading: false };
    h.captures = { data: undefined, isFetching: false };
    h.selectConversation.mockResolvedValue(undefined);
  });

  afterEach(() => {
    localStorage.removeItem('feature.referenceInbox.v1');
  });

  it('renders nothing while inactive, and does not fetch captures', () => {
    const { container } = renderRefs({ active: false });
    expect(container).toBeEmptyDOMElement();
    expect(h.bookmarkArgs).toEqual(['enzymes', 's1', false]);
    expect(h.captureArgs).toEqual([false]);
  });

  it('shows a spinner while bookmarks load', () => {
    h.bookmarks = { data: undefined, isLoading: true };
    renderRefs();
    expect(screen.queryByText('No references yet.')).not.toBeInTheDocument();
    expect(screen.getByText('0 references')).toBeInTheDocument();
  });

  it('says when there are no references', () => {
    h.bookmarks = { data: [], isLoading: false };
    renderRefs();
    expect(screen.getByText('No references yet.')).toBeInTheDocument();
  });

  it('lists references with role, title fallback and capture state', () => {
    h.bookmarks = {
      data: [
        bookmark('1', 'assistant', { title: 'Krebs cycle summary' }),
        bookmark('2', 'user'),
      ],
      isLoading: false,
    };
    h.captures = { data: new Map([[chatReferenceKey('c-1', 'm-1'), { noteId: 'n1' }]]), isFetching: true };
    renderRefs();
    expect(screen.getByText('2 references · 1 captured')).toBeInTheDocument();
    expect(screen.getByText('Krebs cycle summary')).toBeInTheDocument();
    expect(screen.getByText('Conversation 2')).toBeInTheDocument();
    expect(screen.getByText(/^Assistant · Captured · /)).toBeInTheDocument();
    expect(screen.getByText(/^You · /)).toBeInTheDocument();
    expect(screen.getByText('Preview 2')).toBeInTheDocument();
  });

  it('counts a single reference in the singular', () => {
    h.bookmarks = { data: [bookmark('1', 'system')], isLoading: false };
    renderRefs();
    expect(screen.getByText('1 reference')).toBeInTheDocument();
  });

  it('filters by role', () => {
    h.bookmarks = { data: [bookmark('1', 'assistant'), bookmark('2', 'user'), bookmark('3', 'system')], isLoading: false };
    renderRefs();
    fireEvent.click(screen.getByRole('button', { name: 'You' }));
    expect(screen.getByText('Preview 2')).toBeInTheDocument();
    expect(screen.queryByText('Preview 1')).not.toBeInTheDocument();
    expect(screen.getByText('1 reference')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'System' }));
    expect(screen.getByText('Preview 3')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Assistant' }));
    expect(screen.getByText('Preview 1')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'All' }));
    expect(screen.getByText('3 references')).toBeInTheDocument();
  });

  it('shows no more than twenty references', () => {
    h.bookmarks = { data: Array.from({ length: 25 }, (_v, i) => bookmark(String(i), 'assistant')), isLoading: false };
    renderRefs();
    expect(screen.getAllByRole('button', { name: 'Open in chat' })).toHaveLength(20);
    expect(screen.getByText('25 references')).toBeInTheDocument();
  });

  it('opens a reference in its chat at its message', async () => {
    h.bookmarks = { data: [bookmark('1', 'assistant'), bookmark('2', 'user')], isLoading: false };
    renderRefs();
    fireEvent.click(screen.getByText('Preview 2'));
    fireEvent.click(screen.getAllByRole('button', { name: 'Open in chat' })[1]);
    await waitFor(() => expect(h.scrollToMessage).toHaveBeenCalledWith('m-2'));
    expect(h.selectConversation).toHaveBeenCalledWith('c-2');
  });

  it('opens the References page', () => {
    h.bookmarks = { data: [], isLoading: false };
    renderRefs();
    fireEvent.click(screen.getByRole('button', { name: 'Open References' }));
    expect(h.navigate).toHaveBeenCalledWith('/references');
  });

  it('hides capture state when the reference inbox is switched off', () => {
    localStorage.setItem('feature.referenceInbox.v1', 'false');
    h.bookmarks = { data: [bookmark('1', 'assistant')], isLoading: false };
    h.captures = { data: new Map([[chatReferenceKey('c-1', 'm-1'), { noteId: 'n1' }]]), isFetching: false };
    renderRefs();
    expect(h.captureArgs).toEqual([false]);
    expect(screen.queryByText(/reference$/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Captured/)).not.toBeInTheDocument();
  });
});
