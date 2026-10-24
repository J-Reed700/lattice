import type { FormEvent, KeyboardEvent } from 'react';

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useComposer } from '@/features/chat/hooks/useComposer';
import { useModelWarmupStore } from '@/features/model/stores/modelWarmupStore';
import type { ChatImage } from '@/types/conversation';

type SuggestOptions = {
  resolveItems: (trigger: { kind: string; query: string; start: number; end: number }) => Array<Record<string, unknown>>;
  onAccept: (item: { id: string }, trigger: { kind: string; query: string; start: number; end: number }) => void;
  isBusy: (trigger: { kind: string; query: string }) => boolean;
};

const h = vi.hoisted(() => ({
  store: {
    conversations: [{ id: 'c1', spaceId: 'space-hist' }],
    spaces: [{ id: 'space-hist', name: 'History', toolPreferencesJson: null }],
    composerDraft: null as string | null,
    setComposerDraft: vi.fn(),
    sendMessage: vi.fn(),
  },
  settings: undefined as unknown,
  mentionDocs: { documents: [] as Array<Record<string, unknown>>, isLoading: false },
  suggestOptions: null as null | SuggestOptions,
  suggest: { trigger: null as null | { kind: string; query: string; start: number; end: number }, handleKeyDown: vi.fn(() => false), syncCaret: vi.fn() },
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: unknown) => unknown) => select(h.store),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: h.settings }) }));
vi.mock('@/features/chat/hooks/useSpaceDocuments', () => ({ useSpaceDocuments: () => h.mentionDocs }));
vi.mock('@/features/chat/components/composer/useComposerSuggest', () => ({
  useComposerSuggest: (options: SuggestOptions) => { h.suggestOptions = options; return h.suggest; },
}));

const submit = { preventDefault: vi.fn() } as unknown as FormEvent;
const noAttachments = () => ({ staged: [] as Array<{ path: string; name: string }>, importStaged: vi.fn(), takePictures: vi.fn((): ChatImage[] => []) });
const robin: ChatImage = {
  id: 'a'.repeat(32), ref: 'img-1', fileName: `${'a'.repeat(32)}.jpg`, mime: 'image/jpeg', width: 640, height: 480,
  byteSize: 52_000, caption: 'A robin on a garden fence', sourceUrl: null, sourceTitle: null, origin: 'attachment',
  license: null, attribution: null, originalName: 'robin.jpg',
};

type Options = Partial<Parameters<typeof useComposer>[0]>;
const renderComposer = (options: Options = {}) => {
  const onCompact = vi.fn();
  const attachments = options.attachments ?? noAttachments();
  const hook = renderHook((props: Options) => useComposer({
    conversationId: 'c1',
    conversationSpaceId: 'space-hist',
    conversationSpaceName: 'History',
    isSending: false,
    isCompacting: false,
    isChatUnavailable: false,
    canCompact: true,
    onCompact,
    attachments,
    ...options,
    ...props,
  }), { initialProps: {} });
  return { ...hook, onCompact, attachments };
};

const typeAndSubmit = async (result: { current: ReturnType<typeof useComposer> }, text: string) => {
  act(() => result.current.setInput(text));
  await act(async () => { await result.current.handleSubmit(submit); });
};

const key = (init: Partial<{ key: string; shiftKey: boolean; metaKey: boolean; ctrlKey: boolean; isComposing: boolean }>) => ({
  key: init.key ?? 'Enter',
  shiftKey: Boolean(init.shiftKey),
  metaKey: Boolean(init.metaKey),
  ctrlKey: Boolean(init.ctrlKey),
  nativeEvent: { isComposing: Boolean(init.isComposing) },
  preventDefault: vi.fn(),
}) as unknown as KeyboardEvent<HTMLTextAreaElement>;

describe('useComposer commands and guards', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.store.composerDraft = null;
    h.store.sendMessage.mockResolvedValue(undefined);
    h.settings = undefined;
    h.mentionDocs = { documents: [], isLoading: false };
    h.suggest.trigger = null;
    h.suggest.handleKeyDown.mockReturnValue(false);
    useModelWarmupStore.setState({ chat: { ...useModelWarmupStore.getState().chat, phase: 'idle' } });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('offers enabled, named custom tools sorted by name', () => {
    h.settings = { llm: { customTools: [
      { name: 'tide_tables', enabled: true },
      { name: 'atlas', enabled: true },
      { name: '  ', enabled: true },
      { name: 'off_tool', enabled: false },
    ] } };
    const { result } = renderComposer();
    expect(result.current.customTools.map((tool) => tool.name)).toEqual(['atlas', 'tide_tables']);
  });

  describe('typed slash commands', () => {
    it.each([
      ['/deep', (r: ReturnType<typeof useComposer>) => r.composerMode.deepResearch, true],
      ['/docs', (r: ReturnType<typeof useComposer>) => r.composerMode.knowledgeBase, true],
      ['/wiki', (r: ReturnType<typeof useComposer>) => r.composerMode.wikipedia, true],
      ['/followup', (r: ReturnType<typeof useComposer>) => r.turnMode, 'followup'],
      ['/query', (r: ReturnType<typeof useComposer>) => r.turnMode, 'query'],
    ])('%s flips its switch instead of sending', async (command, read, expected) => {
      const { result } = renderComposer();
      const before = read(result.current);
      await typeAndSubmit(result, command);
      expect(read(result.current)).toBe(expected);
      expect(read(result.current)).not.toBe(before);
      expect(h.store.sendMessage).not.toHaveBeenCalled();
      expect(result.current.input).toBe('');
    });

    it('/auto returns the turn mode to automatic', async () => {
      const { result } = renderComposer();
      await typeAndSubmit(result, '/query');
      await typeAndSubmit(result, '/auto');
      expect(result.current.turnMode).toBe('auto');
    });

    it('/compact compacts this conversation', async () => {
      const { result, onCompact } = renderComposer();
      await typeAndSubmit(result, '/compact');
      expect(onCompact).toHaveBeenCalledWith('c1');
    });
  });

  describe('mode chips', () => {
    it('remove the mode they show', async () => {
      const { result } = renderComposer();
      await typeAndSubmit(result, '/deep');
      await typeAndSubmit(result, '/wiki');
      await typeAndSubmit(result, '/web');
      await typeAndSubmit(result, '/query');
      const webBefore = result.current.composerMode.webSearch;
      act(() => result.current.removeMode('deep'));
      act(() => result.current.removeMode('wiki'));
      act(() => result.current.removeMode('web'));
      act(() => result.current.removeMode('turn'));
      const kbBefore = result.current.composerMode.knowledgeBase;
      act(() => result.current.removeMode('docs'));
      expect(result.current.composerMode.deepResearch).toBe(false);
      expect(result.current.composerMode.wikipedia).toBe(false);
      expect(result.current.composerMode.webSearch).toBe(!webBefore);
      expect(result.current.turnMode).toBe('auto');
      expect(result.current.composerMode.knowledgeBase).toBe(!kbBefore);
    });
  });

  describe('submit guards', () => {
    it('sends nothing blank, mid-turn, or without a conversation', async () => {
      const blank = renderComposer();
      await typeAndSubmit(blank.result, '   ');
      blank.unmount();
      const sending = renderComposer({ isSending: true });
      await typeAndSubmit(sending.result, 'Why did Rome fall?');
      sending.unmount();
      const none = renderComposer({ conversationId: null });
      await typeAndSubmit(none.result, 'Why did Rome fall?');
      expect(h.store.sendMessage).not.toHaveBeenCalled();
    });

    it('waits while the chat model warms or is unavailable', async () => {
      useModelWarmupStore.setState({ chat: { ...useModelWarmupStore.getState().chat, phase: 'started' } });
      const warming = renderComposer();
      await typeAndSubmit(warming.result, 'Why did Rome fall?');
      warming.unmount();
      useModelWarmupStore.setState({ chat: { ...useModelWarmupStore.getState().chat, phase: 'idle' } });
      const unavailable = renderComposer({ isChatUnavailable: true });
      await typeAndSubmit(unavailable.result, 'Why did Rome fall?');
      expect(h.store.sendMessage).not.toHaveBeenCalled();
      expect(unavailable.result.current.input).toBe('Why did Rome fall?');
    });

    it('holds the draft while compacting, but still runs commands', async () => {
      const { result } = renderComposer({ isCompacting: true });
      await typeAndSubmit(result, 'Why did Rome fall?');
      expect(h.store.sendMessage).not.toHaveBeenCalled();
      expect(result.current.input).toBe('Why did Rome fall?');
      await typeAndSubmit(result, '/deep');
      expect(result.current.composerMode.deepResearch).toBe(true);
    });

    it('keeps the draft when staged files fail to import', async () => {
      const attachments = { staged: [{ path: '/a.pdf', name: 'a.pdf' }], importStaged: vi.fn().mockResolvedValue(null), takePictures: vi.fn((): ChatImage[] => []) };
      const { result } = renderComposer({ attachments });
      await typeAndSubmit(result, 'Summarise the attachment');
      expect(h.store.sendMessage).not.toHaveBeenCalled();
      expect(result.current.input).toBe('Summarise the attachment');
    });

    it('sends pictures, and omits empty name or id lists', async () => {
      const attachments = { staged: [{ path: '/a.pdf', name: 'a.pdf' }], importStaged: vi.fn().mockResolvedValue({ names: [], documentIds: [] }), takePictures: vi.fn(() => [robin]) };
      const { result } = renderComposer({ attachments });
      await typeAndSubmit(result, 'What bird is this?');
      expect(h.store.sendMessage).toHaveBeenCalledWith('What bird is this?', 'c1', expect.any(Object), undefined, undefined, [robin]);
    });
  });

  describe('suggestions', () => {
    const slash = { kind: 'slash', query: 'de', start: 0, end: 3 };

    it('lists matching slash commands with their state', () => {
      const { result } = renderComposer();
      act(() => result.current.setInput('/de'));
      const items = h.suggestOptions!.resolveItems(slash);
      expect(items[0]).toMatchObject({ id: 'deep', label: '/deep' });
      expect(h.suggestOptions!.isBusy(slash)).toBe(false);
    });

    it('runs a picked slash command and removes its token', () => {
      vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { callback(0); return 0; });
      const { result } = renderComposer();
      const textarea = document.createElement('textarea');
      document.body.appendChild(textarea);
      (result.current.textareaRef as { current: HTMLTextAreaElement | null }).current = textarea;
      act(() => result.current.setInput('/de'));
      act(() => h.suggestOptions!.onAccept({ id: 'deep' }, slash));
      expect(result.current.input).toBe('');
      expect(result.current.composerMode.deepResearch).toBe(true);
      expect(textarea).toHaveFocus();
      expect(h.suggest.syncCaret).toHaveBeenCalledWith(textarea);
      textarea.remove();
    });

    it('offers this conversation’s documents for @, skipping ones already focused, and pins a pick', () => {
      const docs = [
        { documentId: 'd1', fileName: 'Gibbon vol 1.pdf', category: 'Book' },
        { documentId: 'd2', fileName: 'Notes on Rome.md', category: null },
      ];
      h.mentionDocs = { documents: docs, isLoading: false };
      h.suggest.trigger = { kind: 'mention', query: 'ro', start: 0, end: 3 };
      const { result, rerender } = renderComposer();
      rerender({});
      const mention = { kind: 'mention', query: 'ro', start: 0, end: 3 };
      expect(h.suggestOptions!.resolveItems(mention).map((item) => item.label)).toEqual(['Gibbon vol 1.pdf', 'Notes on Rome.md']);
      expect(h.suggestOptions!.resolveItems(mention)[1].description).toBe('In this space');
      expect(h.suggestOptions!.resolveItems({ ...mention, query: 'other' })).toEqual([]);
      expect(h.suggestOptions!.isBusy({ ...mention, query: 'other' })).toBe(true);
      act(() => result.current.setInput('@ro'));
      act(() => h.suggestOptions!.onAccept({ id: 'd2' }, mention));
      expect(result.current.focusDocuments.map((doc) => doc.documentId)).toEqual(['d2']);
      expect(result.current.placeholder).toBe(result.current.focusLabel);
      expect(h.suggestOptions!.resolveItems(mention).map((item) => item.label)).toEqual(['Gibbon vol 1.pdf']);
      act(() => h.suggestOptions!.onAccept({ id: 'missing' }, mention));
      expect(result.current.focusDocuments).toHaveLength(1);
    });

    it('is busy while documents load', () => {
      h.mentionDocs = { documents: [], isLoading: true };
      renderComposer();
      expect(h.suggestOptions!.isBusy({ kind: 'mention', query: '' })).toBe(true);
    });
  });

  describe('keys', () => {
    it('sends on Enter or Mod+Enter, not Shift+Enter, IME commits, or suggestion keys', async () => {
      const { result } = renderComposer();
      act(() => result.current.setInput('Why did Rome fall?'));
      await act(async () => { result.current.handleKeyDown(key({ shiftKey: true })); });
      await act(async () => { result.current.handleKeyDown(key({ isComposing: true })); });
      h.suggest.handleKeyDown.mockReturnValueOnce(true);
      await act(async () => { result.current.handleKeyDown(key({})); });
      await act(async () => { result.current.handleKeyDown(key({ key: 'a' })); });
      expect(h.store.sendMessage).not.toHaveBeenCalled();
      await act(async () => { result.current.handleKeyDown(key({})); });
      expect(h.store.sendMessage).toHaveBeenCalledTimes(1);
      act(() => result.current.setInput('And Byzantium?'));
      await act(async () => { result.current.handleKeyDown(key({ metaKey: true })); });
      act(() => result.current.setInput('And Persia?'));
      await act(async () => { result.current.handleKeyDown(key({ ctrlKey: true })); });
      expect(h.store.sendMessage).toHaveBeenCalledTimes(3);
    });
  });

  it('names the turn mode in the placeholder', async () => {
    const { result } = renderComposer();
    expect(result.current.placeholder).toBe('Ask anything');
    await typeAndSubmit(result, '/followup');
    expect(result.current.placeholder).toBe('Follow up');
    await typeAndSubmit(result, '/query');
    expect(result.current.placeholder).toBe('Search sources and answer');
  });

  it('takes a handed-over draft and clears the handover', () => {
    h.store.composerDraft = '> quoted passage\n\n';
    const { result } = renderComposer();
    expect(result.current.input).toBe('> quoted passage\n\n');
    expect(h.store.setComposerDraft).toHaveBeenCalledWith(null);
  });

  it('fills a question ready to edit', () => {
    const { result } = renderComposer();
    act(() => result.current.fill('What caused the Bronze Age collapse?'));
    expect(result.current.input).toBe('What caused the Bronze Age collapse?');
  });
});
