import type { ReactNode } from 'react';

import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { deferred } from '@/tests/deferred';

import { ChatPanel } from './ChatPanel';

type Props = Record<string, any>;

const h = vi.hoisted(() => ({
  store: {} as Record<string, any>,
  settings: { llm: { provider: 'local' } } as Record<string, unknown> | undefined,
  downloadedModels: [] as unknown[],
  loadedModels: [] as unknown[],
  warmup: { warming: false, chat: { phase: 'idle' } },
  effective: null as null | Record<string, unknown>,
  defaultOption: null as null | Record<string, unknown>,
  options: [] as unknown[],
  degradedFor: vi.fn(() => null as string | null),
  thread: { messages: [] as Array<Record<string, unknown>> },
  scroll: {} as Record<string, any>,
  compaction: {} as Record<string, any>,
  attachments: {} as Record<string, any>,
  composer: {} as Record<string, any>,
  composerArgs: {} as Props,
  switchActions: {} as Record<string, any>,
  openSpaces: [] as unknown[],
  paletteArgs: {} as Props,
  registered: [] as unknown[],
  props: {} as Record<string, Props>,
  toast: { error: vi.fn() },
}));

const { stub } = vi.hoisted(() => ({
  stub: (name: string, render?: (props: Props) => ReactNode) => (props: Props) => {
    h.props[name] = props;
    return render ? <>{render(props)}</> : <div data-testid={name} />;
  },
}));


vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (state: Record<string, unknown>) => unknown) => selector(h.store),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: h.settings }) }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: h.downloadedModels }) }));
vi.mock('@/features/model/hooks/useLoadedModels', () => ({
  useLoadedModels: () => h.loadedModels,
  degradedReasonFor: h.degradedFor,
}));
vi.mock('@/features/model/stores/modelWarmupStore', () => ({
  useModelWarmupStore: (selector: (state: unknown) => unknown) => selector(h.warmup),
  selectIsChatWarming: (state: { warming: boolean }) => state.warming,
}));
vi.mock('@/utils/chatModelSelection', () => ({
  resolveEffectiveChatOption: () => h.effective,
  resolveDefaultChatOption: () => h.defaultOption,
  buildChatModelOptions: () => h.options,
  isUnavailableChatModel: (option: { unavailable?: boolean } | null) => option?.unavailable === true,
  sameModelRef: (a: { id: string } | null, b: { id: string } | null) => Boolean(a && a.id === b?.id),
}));
vi.mock('@/utils/conversationTitles', () => ({ createDefaultConversationTitle: () => 'New chat' }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/features/chat/hooks/useThreadMessages', () => ({
  useThreadMessages: () => ({
    messages: h.thread.messages,
    messageContentById: new Map([['m1', 'Mitochondria make ATP']]),
    getMessageKey: (message: { id?: string; key?: string }) => message.id ?? message.key ?? 'k',
    getPersistedMessageId: (message: { id?: string }) => message.id ?? null,
  }),
}));
vi.mock('@/features/chat/hooks/useThreadScroll', () => ({ useThreadScroll: () => h.scroll }));
vi.mock('@/features/chat/hooks/useChatCompaction', () => ({ useChatCompaction: () => h.compaction }));
vi.mock('@/features/chat/hooks/useAttachmentImport', () => ({ useAttachmentImport: () => h.attachments }));
vi.mock('@/features/chat/hooks/useComposer', () => ({
  useComposer: (args: Props) => { h.composerArgs = args; return h.composer; },
}));
vi.mock('@/features/chat/hooks/useChatSwitchActions', () => ({ useChatSwitchActions: () => h.switchActions }));
vi.mock('@/features/spaces/components/SpacePickerPopover', () => ({ useOpenSpaces: () => h.openSpaces }));
vi.mock('@/features/palette/hooks/useRegisterPaletteCommands', () => ({
  useRegisterPaletteCommands: (commands: unknown[]) => { h.registered = commands; },
}));
vi.mock('@/features/chat/components/chatPaletteCommands', () => ({
  chatPaletteCommands: (args: Props) => { h.paletteArgs = args; return [{ id: 'palette' }]; },
}));
vi.mock('@/features/chat/components/NoConversationOpen', () => ({
  NoConversationOpen: stub('NoConversationOpen', (p) => (
    <button type="button" disabled={p.isCreatingConversation} onClick={() => void p.onCreate()}>Start a conversation</button>
  )),
}));
vi.mock('@/features/chat/components/tangents/ConversationTangents', () => ({
  ConversationTangents: stub('ConversationTangents', (p) => p.children),
}));
vi.mock('@/features/chat/components/ImportFailuresNotice', () => ({ ImportFailuresNotice: stub('ImportFailuresNotice') }));
vi.mock('@/features/chat/components/UtilityModelNotice', () => ({ UtilityModelNotice: stub('UtilityModelNotice') }));
vi.mock('@/features/chat/components/ConversationMemoryPanel', () => ({
  ConversationMemoryPanel: stub('ConversationMemoryPanel', (p) => (p.isOpen ? (
    <div role="dialog" aria-label="Memory">
      <button type="button" onClick={p.onClose}>Close memory</button>
      <button type="button" onClick={() => p.onOpenConversation('c-source')}>Open source chat</button>
    </div>
  ) : null)),
}));
vi.mock('@/features/chat/components/ChatStarters', () => ({ ChatStarters: stub('ChatStarters') }));
vi.mock('@/features/chat/components/ChatEmptyStateIngestDelta', () => ({ ChatEmptyStateIngestDelta: stub('ChatEmptyStateIngestDelta') }));
vi.mock('@/features/chat/components/VirtualizedMessageList', () => ({
  VirtualizedMessageList: stub('VirtualizedMessageList', (p) => (
    <ol>{(p.items as unknown[]).map((item, index) => <li key={index}>{p.renderItem(item, index)}</li>)}</ol>
  )),
}));
vi.mock('@/features/chat/components/Message', () => ({
  Message: (p: Props) => {
    (h.props.messages ??= {})[p.message.id ?? p.message.key] = p;
    return <p>{p.message.content}</p>;
  },
}));
vi.mock('@/features/chat/components/CompactionStatus', () => ({
  CompactionStatus: stub('CompactionStatus', (p) => <button type="button" onClick={p.onDismiss}>Dismiss compaction</button>),
}));
vi.mock('@/features/chat/components/ConversationLinkedDocumentsPanel', () => ({
  ConversationLinkedDocumentsPanel: stub('ConversationLinkedDocumentsPanel', (p) => (
    <button type="button" onClick={p.onMoveToGeneral}>Move to General</button>
  )),
}));
vi.mock('@/features/chat/components/ConversationNavigator', () => ({ ConversationNavigator: stub('ConversationNavigator') }));
vi.mock('@/features/chat/components/composer/Composer', () => ({
  Composer: stub('Composer', (p) => (
    <>
      <button type="button" onClick={p.onCancel}>Stop</button>
      {p.modelNotice.unavailableModel && <button type="button" onClick={p.modelNotice.unavailableModel.onUseDefault}>Use default model</button>}
    </>
  )),
}));

const makeStore = (overrides: Record<string, unknown> = {}) => ({
  activeConversationId: 'c1',
  conversations: [{ id: 'c1', spaceId: null, messages: [] }],
  spaces: [{ id: GENERAL_SPACE_ID, name: 'General' }, { id: 's-hist', name: 'History' }],
  selectedSpaceId: null,
  inFlightGenerations: new Map(),
  optimisticMessages: new Map(),
  liveModel: new Map(),
  messageRetrieval: new Map(),
  cancelGeneration: vi.fn().mockResolvedValue(undefined),
  createConversation: vi.fn().mockResolvedValue(undefined),
  regenerateResponse: vi.fn(),
  forkConversation: vi.fn(),
  selectConversation: vi.fn().mockResolvedValue(undefined),
  ...overrides,
});

const composerProps = () => h.props.Composer;

describe('ChatPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.props = {};
    h.store = makeStore();
    h.settings = { llm: { provider: 'local' } };
    h.warmup = { warming: false, chat: { phase: 'idle' } };
    h.effective = { id: 'qwen', label: 'Qwen 9B', ref: { id: 'qwen' }, seesImages: false };
    h.defaultOption = { id: 'qwen' };
    h.options = [{ id: 'qwen' }];
    h.degradedFor.mockReturnValue(null);
    h.thread = { messages: [] };
    h.scroll = {
      scrollContainerRef: { current: null },
      messageListRef: { current: null },
      shouldAutoScrollRef: { current: true },
      visibleMessage: { conversationId: null, index: 0 },
      handleVisibleMessage: vi.fn(),
      navigateToMessage: vi.fn(),
      navigateToLatest: vi.fn(),
      freshMessageKeys: new Set<string>(),
    };
    h.compaction = { compactionRun: null, dismissCompaction: vi.fn(), isCompacting: false, compactionRecord: null, handleCompact: vi.fn() };
    h.attachments = { isDragging: false, chooseFiles: vi.fn() };
    h.composer = { applyFocusDocuments: vi.fn(), resolveEffectiveToolPreferences: vi.fn(), toolPreferences: { web: true }, fill: vi.fn() };
    h.switchActions = { handleChangeSpace: vi.fn(), handleSearchGeneral: vi.fn(), handleChooseChatModel: vi.fn(), handleUseDefault: vi.fn() };
    h.openSpaces = [];
  });

  describe('with no conversation open', () => {
    beforeEach(() => {
      h.store = makeStore({ activeConversationId: null });
    });

    it('offers to start one', async () => {
      const creating = deferred<void>();
      h.store.createConversation.mockReturnValueOnce(creating.promise);
      render(<ChatPanel />);
      fireEvent.click(screen.getByRole('button', { name: 'Start a conversation' }));
      expect(h.store.createConversation).toHaveBeenCalledWith('New chat');
      expect(screen.getByRole('button', { name: 'Start a conversation' })).toBeDisabled();
      // A second press while the first is pending is ignored.
      await act(async () => { await h.props.NoConversationOpen.onCreate(); });
      expect(h.store.createConversation).toHaveBeenCalledTimes(1);
      await act(async () => { creating.resolve(); await creating.promise; });
      expect(screen.getByRole('button', { name: 'Start a conversation' })).toBeEnabled();
      expect(screen.queryByTestId('Composer')).not.toBeInTheDocument();
    });

    it('reports a conversation that could not start', async () => {
      h.store.createConversation.mockRejectedValueOnce(new Error('database locked'));
      render(<ChatPanel />);
      fireEvent.click(screen.getByRole('button', { name: 'Start a conversation' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't start a conversation", { message: 'database locked' }));
      h.store.createConversation.mockRejectedValueOnce('offline');
      fireEvent.click(screen.getByRole('button', { name: 'Start a conversation' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't start a conversation", { message: 'offline' }));
    });
  });

  it('shows starters for an empty thread, from the conversation’s space first', () => {
    h.store = makeStore({ conversations: [{ id: 'c1', spaceId: 's-hist', messages: [] }], selectedSpaceId: 's-other' });
    render(<ChatPanel />);
    expect(h.props.ChatStarters.spaceId).toBe('s-hist');
    expect(h.props.ChatStarters.onPick).toBe(h.composer.fill);
    expect(screen.getByTestId('ChatEmptyStateIngestDelta')).toBeInTheDocument();
    expect(screen.queryByTestId('VirtualizedMessageList')).not.toBeInTheDocument();
  });

  it('takes starters from the sidebar’s space before the chat has one', () => {
    h.store = makeStore({ selectedSpaceId: 's-hist' });
    render(<ChatPanel />);
    expect(h.props.ChatStarters.spaceId).toBe('s-hist');
  });

  it('renders each message with its neighbours and marks the compaction point', () => {
    h.thread = {
      messages: [
        { id: 'm1', role: 'user', content: 'What do mitochondria do?' },
        { id: 'm2', role: 'assistant', content: 'They make ATP.' },
        { key: 'pending', role: 'user', content: 'And chloroplasts?' },
      ],
    };
    h.scroll.freshMessageKeys = new Set(['m2']);
    h.compaction.compactionRecord = { upToMessageId: 'm2' };
    render(<ChatPanel />);
    expect(screen.getByText('They make ATP.')).toBeInTheDocument();
    expect(screen.getByRole('separator', { name: 'Context compacted' })).toBeInTheDocument();
    const second = h.props.messages.m2;
    expect(second).toMatchObject({ previousMessageId: 'm1', nextMessageId: undefined, isFresh: true, isLastTurn: false });
    expect(h.props.messages.m1).toMatchObject({ previousMessageId: undefined, nextMessageId: 'm2', isFresh: false });
    expect(h.props.messages.pending).toMatchObject({ isLastTurn: true, previousMessageId: 'm2' });
    expect(h.props.ConversationNavigator.activeIndex).toBe(2);
  });

  it('points the navigator at the visible message of this conversation', () => {
    h.thread = { messages: [{ id: 'm1', role: 'user', content: 'a' }, { id: 'm2', role: 'assistant', content: 'b' }] };
    h.scroll.visibleMessage = { conversationId: 'c1', index: 0 };
    render(<ChatPanel />);
    expect(h.props.ConversationNavigator.activeIndex).toBe(0);
  });

  it('stops following new output once the reader scrolls up', () => {
    h.thread = { messages: [{ id: 'm1', role: 'user', content: 'a' }] };
    render(<ChatPanel />);
    const scroller = h.scroll.scrollContainerRef.current as HTMLElement;
    Object.defineProperties(scroller, {
      scrollHeight: { configurable: true, value: 2000 },
      clientHeight: { configurable: true, value: 500 },
      scrollTop: { configurable: true, value: 1000 },
    });
    fireEvent.scroll(scroller);
    expect(h.scroll.shouldAutoScrollRef.current).toBe(false);
    Object.defineProperty(scroller, 'scrollTop', { configurable: true, value: 1450 });
    fireEvent.scroll(scroller);
    expect(h.scroll.shouldAutoScrollRef.current).toBe(true);
  });

  it('shows the drop overlay while files are dragged over', () => {
    h.attachments.isDragging = true;
    render(<ChatPanel />);
    expect(screen.getByText('Drop files to add them to this conversation.')).toBeInTheDocument();
  });

  it('stops a running generation, and only then', async () => {
    const { unmount } = render(<ChatPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(h.store.cancelGeneration).not.toHaveBeenCalled();
    unmount();
    h.store = makeStore({ inFlightGenerations: new Map([['c1', 'r1']]) });
    render(<ChatPanel />);
    expect(composerProps().isSending).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    await waitFor(() => expect(h.store.cancelGeneration).toHaveBeenCalledWith('c1'));
  });

  it('dismisses a compaction status', () => {
    h.compaction.compactionRun = { startedAt: 1 };
    render(<ChatPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss compaction' }));
    expect(h.compaction.dismissCompaction).toHaveBeenCalledWith('c1');
  });

  it('opens and closes conversation memory from the palette, and follows a source chat', async () => {
    render(<ChatPanel />);
    act(() => h.paletteArgs.setIsMemoryOpen(true));
    fireEvent.click(screen.getByRole('button', { name: 'Close memory' }));
    expect(screen.queryByRole('dialog', { name: 'Memory' })).not.toBeInTheDocument();
    act(() => h.paletteArgs.setIsMemoryOpen(true));
    fireEvent.click(screen.getByRole('button', { name: 'Open source chat' }));
    expect(screen.queryByRole('dialog', { name: 'Memory' })).not.toBeInTheDocument();
    await waitFor(() => expect(h.store.selectConversation).toHaveBeenCalledWith('c-source'));
  });

  it('moves the chat to General from the linked documents footer', () => {
    h.store = makeStore({ conversations: [{ id: 'c1', spaceId: 's-hist', messages: [] }] });
    render(<ChatPanel />);
    expect(h.props.ConversationLinkedDocumentsPanel.scopedSpaceName).toBe('History');
    fireEvent.click(screen.getByRole('button', { name: 'Move to General' }));
    expect(h.switchActions.handleSearchGeneral).toHaveBeenCalled();
  });

  it('does not call a General chat scoped', () => {
    h.store = makeStore({ conversations: [{ id: 'c1', spaceId: GENERAL_SPACE_ID, messages: [] }] });
    h.openSpaces = [{ id: 'a' }];
    render(<ChatPanel />);
    expect(h.props.ConversationLinkedDocumentsPanel.scopedSpaceName).toBeNull();
    expect(composerProps().space).toBeNull();
  });

  it('offers the space picker when there is more than one open space', () => {
    h.store = makeStore({ conversations: [{ id: 'c1', spaceId: GENERAL_SPACE_ID, messages: [] }] });
    h.openSpaces = [{ id: 'a' }, { id: 'b' }];
    render(<ChatPanel />);
    expect(composerProps().space).toEqual({ id: GENERAL_SPACE_ID, name: 'General', onChange: expect.any(Function) });
    composerProps().space.onChange('space-other');
    expect(h.switchActions.handleChangeSpace).toHaveBeenCalledWith('space-other');
  });

  describe('model state handed to the composer', () => {
    it('passes the effective model and choices', () => {
      render(<ChatPanel />);
      expect(composerProps().model).toMatchObject({
        choice: { kind: 'default' }, effective: h.effective, defaultOption: h.defaultOption, degraded: null, canChoose: true,
      });
      expect(composerProps().isChatUnavailable).toBe(false);
      expect(composerProps().modelNotice).toMatchObject({ hasChatModel: true, warmupPhase: 'idle', unavailableModel: null });
      expect(h.props.messages).toBeUndefined();
    });

    it('is unavailable with no chat model', () => {
      h.effective = null;
      h.options = [];
      render(<ChatPanel />);
      expect(composerProps().isChatUnavailable).toBe(true);
      expect(composerProps().model.canChoose).toBe(false);
      expect(h.paletteArgs.activeModelLabel).toBeNull();
    });

    it('is unavailable while the default model warms', () => {
      h.warmup = { warming: true, chat: { phase: 'loading' } };
      render(<ChatPanel />);
      expect(composerProps().isChatUnavailable).toBe(true);
      expect(composerProps().modelNotice.warmupPhase).toBe('loading');
    });

    it('ignores default-model warm-up for a chat with its own model', () => {
      h.warmup = { warming: true, chat: { phase: 'loading' } };
      h.store = makeStore({ conversations: [{ id: 'c1', spaceId: null, messages: [], modelChoice: { kind: 'model', model: { id: 'gemma' } } }] });
      render(<ChatPanel />);
      expect(composerProps().isChatUnavailable).toBe(false);
      expect(composerProps().modelNotice.warmupPhase).toBe('idle');
    });

    it('offers the default when the chosen model is unavailable', () => {
      h.effective = { label: 'Gone model', unavailable: true, reason: 'deleted' };
      render(<ChatPanel />);
      expect(composerProps().modelNotice.unavailableModel.reason).toBe('deleted');
      fireEvent.click(screen.getByRole('button', { name: 'Use default model' }));
      expect(h.switchActions.handleUseDefault).toHaveBeenCalled();
      expect(h.degradedFor).toHaveBeenCalledWith(null, []);
    });

    it('takes the degraded reason from the live turn when it runs on the effective model', () => {
      h.store = makeStore({
        inFlightGenerations: new Map([['c1', 'r1']]),
        liveModel: new Map([['c1', { model: { id: 'qwen' }, degraded: 'context shrunk' }]]),
      });
      render(<ChatPanel />);
      expect(composerProps().model.degraded).toBe('context shrunk');
      expect(h.degradedFor).not.toHaveBeenCalled();
    });

    it('falls back to the loaded-model reason when the live turn ran elsewhere', () => {
      h.degradedFor.mockReturnValue('cpu only');
      h.store = makeStore({
        inFlightGenerations: new Map([['c1', 'r1']]),
        liveModel: new Map([['c1', { model: { id: 'other' } }]]),
      });
      render(<ChatPanel />);
      expect(composerProps().model.degraded).toBe('cpu only');
    });

    it('treats a live turn with no degraded note as not degraded', () => {
      h.store = makeStore({
        inFlightGenerations: new Map([['c1', 'r1']]),
        liveModel: new Map([['c1', { model: { id: 'qwen' } }]]),
      });
      render(<ChatPanel />);
      expect(composerProps().model.degraded).toBeNull();
    });

    it('tells messages whether the model sees images', () => {
      h.effective = { id: 'qwen-vl', label: 'Qwen VL', ref: { id: 'qwen-vl' }, seesImages: true };
      h.thread = { messages: [{ id: 'm1', role: 'user', content: 'Which bird is this?' }] };
      render(<ChatPanel />);
      expect(h.props.messages.m1.chatModelSeesImages).toBe(true);
    });
  });

  describe('retrieval note', () => {
    const conversation = (messages: Array<{ id: string; role: string }>) => [{ id: 'c1', spaceId: null, messages }];

    it('reports why the last answer had no sources', () => {
      h.store = makeStore({
        conversations: conversation([{ id: 'u1', role: 'user' }, { id: 'a1', role: 'assistant' }, { id: 'u2', role: 'user' }]),
        messageRetrieval: new Map([['a1', { files: 0, passages: 0, unavailableReason: 'index rebuilding' }]]),
      });
      render(<ChatPanel />);
      expect(composerProps().modelNotice.retrievalUnavailableReason).toBe('index rebuilding');
    });

    it('says nothing when the last answer found sources', () => {
      h.store = makeStore({
        conversations: conversation([{ id: 'a1', role: 'assistant' }]),
        messageRetrieval: new Map([['a1', { files: 2, passages: 0, unavailableReason: 'stale' }]]),
      });
      render(<ChatPanel />);
      expect(composerProps().modelNotice.retrievalUnavailableReason).toBeNull();
    });

    it('says nothing without a trace, without answers, or while sending', () => {
      h.store = makeStore({ conversations: conversation([{ id: 'a1', role: 'assistant' }]) });
      const first = render(<ChatPanel />);
      expect(composerProps().modelNotice.retrievalUnavailableReason).toBeNull();
      first.unmount();
      h.store = makeStore({ conversations: conversation([{ id: 'u1', role: 'user' }]) });
      const second = render(<ChatPanel />);
      expect(composerProps().modelNotice.retrievalUnavailableReason).toBeNull();
      second.unmount();
      h.store = makeStore({
        inFlightGenerations: new Map([['c1', 'r']]),
        conversations: conversation([{ id: 'a1', role: 'assistant' }]),
        messageRetrieval: new Map([['a1', { files: 0, passages: 0, unavailableReason: 'x' }]]),
      });
      render(<ChatPanel />);
      expect(composerProps().modelNotice.retrievalUnavailableReason).toBeNull();
    });

    it('copes with a conversation missing from the list', () => {
      h.store = makeStore({ conversations: [] });
      render(<ChatPanel />);
      expect(composerProps().modelNotice.retrievalUnavailableReason).toBeNull();
    });
  });

  describe('composer and palette wiring', () => {
    it('allows compaction only with messages and none running', () => {
      const { unmount } = render(<ChatPanel />);
      expect(h.composerArgs.canCompact).toBe(false);
      unmount();
      h.thread = { messages: [{ id: 'm1', role: 'user', content: 'a' }] };
      render(<ChatPanel />);
      expect(h.composerArgs.canCompact).toBe(true);
      h.composerArgs.onCompact('c1');
      expect(h.compaction.handleCompact).toHaveBeenCalledWith('c1');
    });

    it('lets the palette regenerate only after an answer', () => {
      h.thread = { messages: [{ id: 'm1', role: 'user', content: 'a' }, { id: 'm2', role: 'assistant', content: 'b' }] };
      render(<ChatPanel />);
      expect(h.paletteArgs).toMatchObject({ canRegenerate: true, messageCount: 2, activeModelLabel: 'Qwen 9B', canSwitchModel: true });
      expect(h.registered).toEqual([{ id: 'palette' }]);
    });

    it('passes host notices and chips through to the composer', () => {
      render(<ChatPanel composerNotice={<span>Folder: /notes</span>} composerChips={<span>chip</span>} />);
      expect(composerProps().notice).toBeTruthy();
      expect(composerProps().chips).toBeTruthy();
      expect(h.props.ConversationTangents).toMatchObject({ conversationId: 'c1', toolPreferences: { web: true }, unavailable: false });
    });
  });
});
