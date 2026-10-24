import { useContext } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { deferred } from '@/tests/deferred';

import { ConversationTangents } from './ConversationTangents';
import { TangentSelectionContext } from './TangentSelection';

const h = vi.hoisted(() => ({
  api: { listConversationTangents: vi.fn(), createConversationTangent: vi.fn(), promoteConversationTangent: vi.fn() },
  panelProps: null as null | Record<string, any>,
  toast: { error: vi.fn(), success: vi.fn() },
}));

vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/features/reading/components/CitationVisibilityToggle', () => ({ CitationVisibilityToggle: () => null }));
vi.mock('./TangentPanel', () => ({
  TangentPanel: (props: Record<string, any>) => {
    h.panelProps = props;
    return <button type="button" onClick={props.onPromoted}>Promote {props.tangent.title}</button>;
  },
}));

const tangent = (id: string, title: string) => ({
  conversationId: id, parentConversationId: 'p1', sourceConversationId: 'p1', sourceMessageId: 'm1',
  selectedText: `${title} passage`, title, createdAt: '', updatedAt: '', contextMessageCount: 2,
});

function AskInTangent() {
  const tangents = useContext(TangentSelectionContext);
  return (
    <button type="button" disabled={tangents?.creating} onClick={() => tangents?.create({ conversationId: 'p1', messageId: 'm1', selectedText: 'Spice routes' })}>
      Ask in a tangent
    </button>
  );
}

let queryClient: QueryClient;
const renderTangents = () => render(
  <QueryClientProvider client={queryClient}>
    <TooltipProvider>
      <ConversationTangents conversationId="p1" toolPreferences={{ web: true } as never} unavailable={false}>
        <AskInTangent />
      </ConversationTangents>
    </TooltipProvider>
  </QueryClientProvider>,
);

const toggle = () => screen.getByRole('button', { name: /^Tangents/ });

describe('ConversationTangents', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    h.panelProps = null;
    h.api.listConversationTangents.mockResolvedValue({ ok: true, data: [tangent('t1', 'Silk Road'), tangent('t2', 'Amber Road')] });
    conversationUiStore.setState({ requestedTangent: null } as never);
  });

  it('counts tangents on the toggle and lists them when opened', async () => {
    renderTangents();
    await waitFor(() => expect(toggle()).toHaveTextContent('Tangents · 2'));
    expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(toggle());
    const panel = screen.getByRole('complementary', { name: 'Tangents' });
    expect(panel).toBeInTheDocument();
    expect(screen.getByText('Silk Road passage')).toBeInTheDocument();
  });

  it('opens a tangent and goes back to the list', async () => {
    renderTangents();
    fireEvent.click(toggle());
    fireEvent.click(await screen.findByRole('button', { name: /Amber Road/ }));
    expect(screen.getByRole('heading', { name: 'Tangent' })).toBeInTheDocument();
    expect(h.panelProps).toMatchObject({ toolPreferences: { web: true }, unavailable: false });
    fireEvent.click(screen.getByRole('button', { name: 'All tangents' }));
    expect(screen.getByRole('heading', { name: 'Tangents' })).toBeInTheDocument();
  });

  it('closes the panel after a promotion, and on Escape or the close button', async () => {
    renderTangents();
    fireEvent.click(toggle());
    fireEvent.click(await screen.findByRole('button', { name: /Silk Road/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Promote Silk Road' }));
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    expect(toggle()).toHaveFocus();
    fireEvent.click(toggle());
    fireEvent.keyDown(screen.getByRole('complementary'), { key: 'Escape' });
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    fireEvent.click(toggle());
    fireEvent.keyDown(screen.getByRole('complementary'), { key: 'a' });
    fireEvent.click(screen.getByRole('button', { name: 'Close tangents' }));
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    fireEvent.click(toggle());
    fireEvent.click(toggle());
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
  });

  it('explains tangents when there are none', async () => {
    h.api.listConversationTangents.mockResolvedValueOnce({ ok: true, data: [] });
    renderTangents();
    fireEvent.click(toggle());
    expect(screen.getByRole('status')).toHaveTextContent('Loading tangents…');
    expect(await screen.findByText(/Explore an idea without losing your place/)).toBeInTheDocument();
    expect(toggle()).toHaveTextContent(/^Tangents$/);
  });

  it('offers a retry when tangents cannot load', async () => {
    h.api.listConversationTangents.mockResolvedValueOnce({ ok: false, error: 'db locked' });
    renderTangents();
    fireEvent.click(toggle());
    expect(await screen.findByRole('alert')).toHaveTextContent('Couldn’t load tangents.');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByText('Silk Road')).toBeInTheDocument();
  });

  it('opens a tangent another surface asked for', async () => {
    renderTangents();
    await waitFor(() => expect(toggle()).toHaveTextContent('Tangents · 2'));
    act(() => conversationUiStore.setState({ requestedTangent: { parentId: 'p1', tangentId: 't2' } } as never));
    expect(await screen.findByRole('button', { name: 'Promote Amber Road' })).toBeInTheDocument();
    expect(conversationUiStore.getState().requestedTangent).toBeNull();
  });

  it('ignores a request meant for another conversation', () => {
    renderTangents();
    act(() => conversationUiStore.setState({ requestedTangent: { parentId: 'other', tangentId: 't2' } } as never));
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
  });

  describe('starting a tangent', () => {
    it('creates one from a selection, lists it first and opens it', async () => {
      const created = deferred<unknown>();
      h.api.createConversationTangent.mockReturnValueOnce(created.promise);
      renderTangents();
      await waitFor(() => expect(toggle()).toHaveTextContent('Tangents · 2'));
      const ask = screen.getByRole('button', { name: 'Ask in a tangent' }) as HTMLButtonElement;
      // Two requests in the same tick: the second is ignored.
      act(() => { ask.click(); ask.click(); });
      expect(ask).toBeDisabled();
      await waitFor(() => expect(h.api.createConversationTangent).toHaveBeenCalledTimes(1));
      expect(h.api.createConversationTangent).toHaveBeenCalledWith({ conversationId: 'p1', messageId: 'm1', selectedText: 'Spice routes' });
      await act(async () => { created.resolve({ ok: true, data: tangent('t3', 'Spice routes') }); await created.promise; });
      expect(await screen.findByRole('button', { name: 'Promote Spice routes' })).toBeInTheDocument();
      expect((queryClient.getQueryData(conversationKeys.tangents('p1')) as Array<{ conversationId: string }>).map((item) => item.conversationId)).toEqual(['t3', 't1', 't2']);
    });

    it('reports a tangent that could not start', async () => {
      h.api.createConversationTangent.mockResolvedValueOnce({ ok: false, error: 'message deleted' });
      renderTangents();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Ask in a tangent' })); });
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't start the tangent", { message: 'message deleted' }));
      expect(screen.getByRole('button', { name: 'Ask in a tangent' })).toBeEnabled();
    });
  });
});
