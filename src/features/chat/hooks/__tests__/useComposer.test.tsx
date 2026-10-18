import type { FormEvent } from 'react';

import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { WEB_TOOL_NAMES } from '@/features/chat/components/composer/toolNames';
import { useComposer } from '@/features/chat/hooks/useComposer';

const store = vi.hoisted(() => ({
  conversations: [{ id: 'conversation-1', spaceId: 'space-thesis' }],
  spaces: [{ id: 'space-thesis', name: 'Thesis', toolPreferencesJson: JSON.stringify({ webSearch: true }) }],
  composerDraft: null as string | null,
  setComposerDraft: vi.fn(),
  sendMessage: vi.fn(),
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select(store),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: undefined }) }));
vi.mock('@/features/chat/hooks/useSpaceDocuments', () => ({
  useSpaceDocuments: () => ({ documents: [], isLoading: false }),
}));

const submit = { preventDefault: () => {} } as FormEvent;

function render(attachments: Parameters<typeof useComposer>[0]['attachments'] = { staged: [], importStaged: vi.fn(), takePictures: () => [] }) {
  return renderHook(() =>
    useComposer({
      conversationId: 'conversation-1',
      conversationSpaceId: 'space-thesis',
      conversationSpaceName: 'Thesis',
      isSending: false,
      isCompacting: false,
      isChatUnavailable: false,
      canCompact: true,
      onCompact: vi.fn(),
      attachments,
    }),
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  store.sendMessage.mockResolvedValue(undefined);
});

describe('the composer', () => {
  it('opens a conversation with the switches its space saved', () => {
    const { result } = render();

    expect(result.current.toolPreferences.webSearch).toBe(true);
    expect(result.current.toolPreferences.enabledTools).toEqual([...WEB_TOOL_NAMES]);
  });

  it('runs a typed slash command instead of sending it', async () => {
    const { result } = render();
    act(() => result.current.setInput('/web'));

    await act(async () => { await result.current.handleSubmit(submit); });

    expect(store.sendMessage).not.toHaveBeenCalled();
    expect(result.current.toolPreferences.webSearch).toBe(false);
    expect(result.current.input).toBe('');
  });

  it('attaches staged files first and sends the message with their ids', async () => {
    const importStaged = vi.fn().mockResolvedValue({ names: ['a.pdf'], documentIds: ['doc-a'] });
    const { result } = render({ staged: [{ path: '/tmp/a.pdf', name: 'a.pdf' }], importStaged, takePictures: () => [] });
    act(() => result.current.setInput('What does it say?'));

    await act(async () => { await result.current.handleSubmit(submit); });

    expect(importStaged).toHaveBeenCalled();
    expect(store.sendMessage).toHaveBeenCalledWith(
      'What does it say?',
      'conversation-1',
      expect.objectContaining({ webSearch: true, turnMode: 'auto' }),
      ['a.pdf'],
      ['doc-a'],
      undefined,
    );
    expect(result.current.input).toBe('');
  });

  it('sends the staged pictures with the message, already stored', async () => {
    const picture = {
      id: '0123456789abcdef0123456789abcdef', ref: 'att-1', fileName: '0123456789abcdef0123456789abcdef.jpg',
      mime: 'image/jpeg', width: 800, height: 600, byteSize: 1000, caption: 'pad.jpg', sourceUrl: null,
      sourceTitle: null, origin: 'attachment' as const, license: null, attribution: null, originalName: 'pad.jpg',
    };
    const importStaged = vi.fn();
    const { result } = render({ staged: [], importStaged, takePictures: () => [picture] });
    act(() => result.current.setInput('What is this?'));

    await act(async () => { await result.current.handleSubmit(submit); });

    expect(importStaged).not.toHaveBeenCalled();
    expect(store.sendMessage).toHaveBeenCalledWith(
      'What is this?', 'conversation-1', expect.anything(), undefined, undefined, [picture],
    );
  });

  it('keeps the draft when the staged files could not be attached', async () => {
    const importStaged = vi.fn().mockResolvedValue(null);
    const takePictures = vi.fn(() => []);
    const { result } = render({ staged: [{ path: '/tmp/a.pdf', name: 'a.pdf' }], importStaged, takePictures });
    act(() => result.current.setInput('What does it say?'));

    await act(async () => { await result.current.handleSubmit(submit); });

    expect(store.sendMessage).not.toHaveBeenCalled();
    expect(takePictures).not.toHaveBeenCalled();
    expect(result.current.input).toBe('What does it say?');
  });

  it('sends a query-mode turn with every source switched on', () => {
    const { result } = render();
    act(() => result.current.handleTurnModeChange('query'));

    expect(result.current.resolveEffectiveToolPreferences()).toMatchObject({
      turnMode: 'query',
      knowledgeBase: true,
      webSearch: true,
    });
  });
});
