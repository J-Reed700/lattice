import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { usePaletteCommandsStore } from '@/stores/paletteCommandsStore';

import { useConversationExport } from '../useConversationExport';

const navigate = vi.hoisted(() => vi.fn());
const getConversationMessages = vi.hoisted(() => vi.fn());
const quickCapture = vi.hoisted(() => vi.fn());
const writeText = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const storeState = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));

vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router');
  return { ...actual, useNavigate: () => navigate };
});

vi.mock('@/lib/api', () => ({
  VaultAPI: { getConversationMessages, quickCapture },
  default: { getConversationMessages, quickCapture },
}));

vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (selector: (_state: unknown) => unknown) => selector(storeState.current),
}));

vi.mock('../workspaceQueries', () => ({
  useJournalsQuery: () => ({ journals: [{ id: 'journal-field', name: 'Field Log' }] }),
}));

const message = (role: string, content: string, metadata?: unknown) => ({
  id: `${role}-1`,
  conversationId: 'conv-1',
  role,
  content,
  tokens: 10,
  createdAt: '2026-09-19T14:00:00.000Z',
  metadata: metadata ? JSON.stringify(metadata) : null,
  status: 'completed',
});

describe('useConversationExport', () => {
  beforeEach(() => {
    navigate.mockClear();
    quickCapture.mockReset().mockResolvedValue({ ok: true, data: { noteId: 'note-1', noteTitle: 'Week of Sep 14' } });
    writeText.mockClear();
    Object.assign(navigator, { clipboard: { writeText } });
    usePaletteCommandsStore.setState({ commands: new Map() });
    storeState.current = {
      conversations: [
        { id: 'conv-1', title: 'How much cooling does canopy buy?', spaceId: 'space-thesis', updatedAt: '2026-09-19T14:00:00.000Z' },
      ],
      spaces: [{ id: 'space-thesis', name: 'Heat Island Thesis' }],
      activeConversationId: 'conv-1',
    };
    getConversationMessages.mockReset().mockResolvedValue({
      ok: true,
      data: {
        messages: [
          message('user', 'What do my sources say?'),
          message('assistant', 'About 1.2 °C per 10 points [1].', {
            verification: { enabled: true, claimsEvaluated: 2, unsupportedClaims: [] },
          }),
        ],
        total: 2,
      },
    });
  });

  it('registers both ways out with the command palette', () => {
    renderHook(() => useConversationExport());

    const labels = [...usePaletteCommandsStore.getState().commands.values()].map((c) => c.label);
    expect(labels).toContain('Copy conversation as Markdown');
    expect(labels).toContain('Save conversation to Journal');
  });

  it('offers neither when no conversation is open', () => {
    storeState.current = { ...storeState.current, activeConversationId: null };
    renderHook(() => useConversationExport());

    const commands = [...usePaletteCommandsStore.getState().commands.values()];
    expect(commands.every((command) => command.enabled === false)).toBe(true);
  });

  it('copies the whole thread, with its space, sources and verification line', async () => {
    const { result } = renderHook(() => useConversationExport());

    await act(async () => {
      await result.current.copyConversationAsMarkdown('conv-1', 'How much cooling does canopy buy?');
    });

    expect(writeText).toHaveBeenCalledTimes(1);
    const markdown = writeText.mock.calls[0]![0] as string;
    expect(markdown).toContain('# How much cooling does canopy buy?');
    expect(markdown).toContain('Heat Island Thesis');
    expect(markdown).toContain('## You');
    expect(markdown).toContain('2 of 2 checked sentences backed.');
  });

  it('names a journal space as one, so a filed thread does not look like a library', async () => {
    storeState.current = {
      ...storeState.current,
      conversations: [{ id: 'conv-1', title: 'Field notes', spaceId: 'journal-field', updatedAt: '' }],
    };
    const { result } = renderHook(() => useConversationExport());

    await act(async () => {
      await result.current.copyConversationAsMarkdown('conv-1', 'Field notes');
    });

    expect(writeText.mock.calls[0]![0]).toContain('Field Log · Journal');
  });

  it('writes the same markdown to the journal and opens the page it landed on', async () => {
    const { result } = renderHook(() => useConversationExport());

    await act(async () => {
      await result.current.saveConversationToJournal('conv-1', 'How much cooling does canopy buy?');
    });

    await waitFor(() => expect(quickCapture).toHaveBeenCalledTimes(1));
    expect(quickCapture.mock.calls[0]![0]).toContain('# How much cooling does canopy buy?');
    expect(navigate).toHaveBeenCalledWith('/journals?noteId=note-1');
  });

  it('writes nothing for a conversation with no messages', async () => {
    getConversationMessages.mockResolvedValue({ ok: true, data: { messages: [], total: 0 } });
    const { result } = renderHook(() => useConversationExport());

    await act(async () => {
      await result.current.saveConversationToJournal('conv-1', 'Empty');
    });

    expect(quickCapture).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  });

  it('does not claim a save when the messages could not be read', async () => {
    getConversationMessages.mockResolvedValue({ ok: false, error: 'database is locked' });
    const { result } = renderHook(() => useConversationExport());

    await act(async () => {
      await result.current.saveConversationToJournal('conv-1', 'Canopy');
    });

    expect(quickCapture).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  });

  it('does not open a journal page when the capture failed', async () => {
    quickCapture.mockResolvedValue({ ok: false, error: 'disk full' });
    const { result } = renderHook(() => useConversationExport());

    await act(async () => {
      await result.current.saveConversationToJournal('conv-1', 'Canopy');
    });

    expect(navigate).not.toHaveBeenCalled();
  });
});
