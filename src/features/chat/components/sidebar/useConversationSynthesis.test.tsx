import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { deferred } from '@/tests/deferred';

import { useConversationSynthesis } from './useConversationSynthesis';

const h = vi.hoisted(() => ({
  store: {} as Record<string, unknown>,
  activeSynthesis: null as null | { conversationIds: string[] },
  runSynthesis: vi.fn(),
  commands: [] as PaletteCommand[],
  toast: { info: vi.fn(() => 'toast-1'), success: vi.fn(), error: vi.fn(), dismiss: vi.fn() },
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('@/features/journal/synthesis/runSynthesis', () => ({ runSynthesis: h.runSynthesis }));
vi.mock('@/features/journal/synthesis/synthesisPanel', () => ({ useActiveSynthesis: () => h.activeSynthesis }));
vi.mock('@/features/palette/hooks/useRegisterPaletteCommands', () => ({
  useRegisterPaletteCommands: (commands: PaletteCommand[]) => { h.commands = commands; },
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const queryClient = new QueryClient();
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
);

const command = (id: string) => {
  const found = h.commands.find((entry) => entry.id === id);
  if (!found) throw new Error(`no command ${id}`);
  return found;
};

describe('useConversationSynthesis', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.activeSynthesis = null;
    h.store = {
      conversations: [{ id: 'c1', title: 'The Silk Road' }],
      activeConversationId: 'c1',
      continueInNewConversation: vi.fn().mockResolvedValue(undefined),
    };
  });

  it('starts a one-conversation synthesis into a capture', async () => {
    h.runSynthesis.mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useConversationSynthesis(), { wrapper });
    await act(() => result.current.synthesizeConversationToJournal('c1', 'The Silk Road'));
    expect(h.runSynthesis).toHaveBeenCalledWith({
      title: 'The Silk Road', heading: 'The Silk Road',
      request: { conversationIds: ['c1'], scope: 'conversation' },
      destination: { kind: 'capture' },
    }, queryClient);
  });

  it('reports a synthesis that cannot start', async () => {
    h.runSynthesis.mockRejectedValueOnce(new Error('no model'));
    const { result } = renderHook(() => useConversationSynthesis(), { wrapper });
    await act(() => result.current.synthesizeConversationToJournal('c1', 'x'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't start the synthesis", { message: 'no model' });
    h.runSynthesis.mockRejectedValueOnce('busy');
    await act(() => result.current.synthesizeConversationToJournal('c1', 'x'));
    expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't start the synthesis", { message: 'busy' });
  });

  it('names the conversation being synthesized only for a single-conversation run', () => {
    h.activeSynthesis = { conversationIds: ['c1'] };
    const { result, rerender } = renderHook(() => useConversationSynthesis(), { wrapper });
    expect(result.current.isSynthesizing).toBe(true);
    expect(result.current.synthesizingConversationId).toBe('c1');
    h.activeSynthesis = { conversationIds: ['c1', 'c2'] };
    rerender();
    expect(result.current.synthesizingConversationId).toBeNull();
    expect(command('chat.synthesizeToJournal').enabled).toBe(false);
  });

  it('continues in a new chat, showing progress until it finishes', async () => {
    const pending = deferred<void>();
    (h.store.continueInNewConversation as ReturnType<typeof vi.fn>).mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useConversationSynthesis(), { wrapper });
    let run!: Promise<void>;
    act(() => { run = result.current.continueConversationInNewChat('c1', 'The Silk Road'); });
    expect(result.current.continuingConversationId).toBe('c1');
    expect(h.toast.info).toHaveBeenCalledWith('Summarizing "The Silk Road"…', expect.objectContaining({ duration: 0 }));
    expect(command('chat.continueInNewChat').enabled).toBe(false);

    // A second request while one runs is ignored.
    await act(() => result.current.continueConversationInNewChat('c1', 'The Silk Road'));
    expect(h.store.continueInNewConversation).toHaveBeenCalledTimes(1);

    await act(async () => { pending.resolve(); await run; });
    expect(h.toast.success).toHaveBeenCalledWith('Continued in a new chat', { message: 'It starts from a summary of "The Silk Road".' });
    expect(h.toast.dismiss).toHaveBeenCalledWith('toast-1');
    expect(result.current.continuingConversationId).toBeNull();
  });

  it('reports a failed continuation and clears progress', async () => {
    (h.store.continueInNewConversation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('summary failed'));
    const { result } = renderHook(() => useConversationSynthesis(), { wrapper });
    await act(() => result.current.continueConversationInNewChat('c1', 'x'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't continue in a new chat", { message: 'summary failed' });
    expect(h.toast.dismiss).toHaveBeenCalledWith('toast-1');
    expect(result.current.continuingConversationId).toBeNull();
    (h.store.continueInNewConversation as ReturnType<typeof vi.fn>).mockRejectedValueOnce('gone');
    await act(() => result.current.continueConversationInNewChat('c1', 'x'));
    expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't continue in a new chat", { message: 'gone' });
  });

  it('registers palette commands that act on the open conversation', async () => {
    h.runSynthesis.mockResolvedValue(undefined);
    renderHook(() => useConversationSynthesis(), { wrapper });
    expect(command('chat.synthesizeToJournal').enabled).toBe(true);
    expect(command('chat.continueInNewChat').enabled).toBe(true);
    await act(async () => { command('chat.synthesizeToJournal').run(); });
    expect(h.runSynthesis).toHaveBeenCalledWith(expect.objectContaining({ title: 'The Silk Road' }), queryClient);
    await act(async () => { command('chat.continueInNewChat').run(); });
    expect(h.store.continueInNewConversation).toHaveBeenCalledWith('c1');
  });

  it('falls back to a generic title when the open conversation is not listed', async () => {
    h.runSynthesis.mockResolvedValue(undefined);
    h.store = { ...h.store, conversations: [] };
    renderHook(() => useConversationSynthesis(), { wrapper });
    await act(async () => { command('chat.synthesizeToJournal').run(); });
    expect(h.runSynthesis).toHaveBeenCalledWith(expect.objectContaining({ title: 'Conversation' }), queryClient);
    await act(async () => { command('chat.continueInNewChat').run(); });
    expect(h.toast.info).toHaveBeenCalledWith('Summarizing "Conversation"…', expect.anything());
  });

  it('disables and ignores palette commands with no open conversation', () => {
    h.store = { ...h.store, activeConversationId: null };
    renderHook(() => useConversationSynthesis(), { wrapper });
    expect(command('chat.synthesizeToJournal').enabled).toBe(false);
    expect(command('chat.continueInNewChat').enabled).toBe(false);
    command('chat.synthesizeToJournal').run();
    command('chat.continueInNewChat').run();
    expect(h.runSynthesis).not.toHaveBeenCalled();
    expect(h.store.continueInNewConversation).not.toHaveBeenCalled();
  });
});
