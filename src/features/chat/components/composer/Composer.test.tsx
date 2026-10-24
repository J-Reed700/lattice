import type { ComponentProps, ReactNode } from 'react';

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { Composer } from './Composer';

type Props = Record<string, any>;
const h = vi.hoisted(() => ({ props: {} as Record<string, Props> }));
const { stub } = vi.hoisted(() => ({
  stub: (name: string, render?: (props: Props) => ReactNode) => (props: Props) => {
    h.props[name] = props;
    return render ? <>{render(props)}</> : null;
  },
}));

vi.mock('@/features/chat/components/ChatDropStaging', () => ({ ChatDropStaging: stub('ChatDropStaging') }));
vi.mock('@/features/chat/components/ChatModelNotice', () => ({ ChatModelNotice: stub('ChatModelNotice') }));
vi.mock('@/features/chat/components/composer/ChatModelButton', () => ({ ChatModelButton: stub('ChatModelButton') }));
vi.mock('@/features/chat/components/composer/ComposerSuggest', () => ({ ComposerSuggest: stub('ComposerSuggest', (p) => <div role="listbox" aria-label={p.heading} />) }));
vi.mock('@/features/chat/components/composer/FocusChips', () => ({ FocusChips: stub('FocusChips') }));
vi.mock('@/features/chat/components/composer/ModeChips', () => ({ ModeChips: stub('ModeChips') }));
vi.mock('@/features/chat/components/ComposerControls', () => ({ ComposerControls: stub('ComposerControls', () => <p>Turn mode and tools panel</p>) }));
vi.mock('@/features/spaces/components/SpacePickerPopover', () => ({ SpacePickerPopover: stub('SpacePickerPopover', (p) => p.children) }));

const makeComposer = (overrides: Record<string, unknown> = {}) => ({
  input: '',
  setInput: vi.fn(),
  textareaRef: { current: null },
  placeholder: 'Ask anything',
  focusLabel: null,
  suggest: {
    isOpen: false, point: null, items: [], activeIndex: 0, trigger: null,
    syncCaret: vi.fn(), setFocused: vi.fn(), accept: vi.fn(), setActiveIndex: vi.fn(),
  },
  focusDocuments: [{ documentId: 'd1' }, { documentId: 'd2' }],
  applyFocusDocuments: vi.fn(),
  handleSubmit: vi.fn((event: Event) => event.preventDefault()),
  handleKeyDown: vi.fn(),
  isControlsOpen: false,
  setIsControlsOpen: vi.fn(),
  turnMode: 'auto',
  handleTurnModeChange: vi.fn(),
  toolPreferences: {},
  toggleKnowledgeBase: vi.fn(),
  toggleWebTools: vi.fn(),
  toggleWikiTools: vi.fn(),
  toggleDeepResearch: vi.fn(),
  customTools: [],
  enabledToolSet: new Set(),
  toggleCustomTool: vi.fn(),
  composerMode: { knowledgeBase: true, webSearch: false, wikipedia: false, deepResearch: false },
  enabledCustomToolNames: [],
  removeMode: vi.fn(),
  ...overrides,
});

const attachments = () => ({
  staged: [], isImporting: false, remove: vi.fn(), clear: vi.fn(), pictures: [], isAttachingPictures: false, removePicture: vi.fn(), chooseFiles: vi.fn(),
});

const renderComposer = (overrides: Record<string, any> = {}) => {
  const props = {
    composer: makeComposer(overrides.composer),
    attachments: attachments(),
    modelNotice: { hasChatModel: true },
    space: { id: 's-hist', name: 'History', onChange: vi.fn() },
    model: { effective: { label: 'Qwen 9B', seesImages: false } },
    isSending: false,
    isCompacting: false,
    isChatUnavailable: false,
    onCancel: vi.fn(),
    ...overrides,
  };
  if (overrides.composer) props.composer = makeComposer(overrides.composer);
  const view = render(<Composer {...(props as unknown as ComponentProps<typeof Composer>)} />);
  return { ...props, unmount: view.unmount };
};

const textarea = () => screen.getByRole('textbox', { name: 'Message composer' });

describe('Composer', () => {
  beforeEach(() => { h.props = {}; });

  it('wires typing, caret and focus to the composer state', () => {
    const { composer } = renderComposer();
    fireEvent.change(textarea(), { target: { value: 'Who built Hadrian’s Wall?' } });
    expect(composer.setInput).toHaveBeenCalledWith('Who built Hadrian’s Wall?');
    fireEvent.keyUp(textarea(), { key: 'a' });
    fireEvent.click(textarea());
    expect(composer.suggest.syncCaret).toHaveBeenCalledTimes(3);
    fireEvent.focus(textarea());
    fireEvent.blur(textarea());
    expect(composer.suggest.setFocused.mock.calls).toEqual([[true], [false]]);
    fireEvent.keyDown(textarea(), { key: 'Enter' });
    expect(composer.handleKeyDown).toHaveBeenCalled();
    expect(textarea()).toHaveAttribute('aria-expanded', 'false');
    expect(textarea()).not.toHaveAttribute('aria-controls');
  });

  it('lists commands under the caret', () => {
    renderComposer({ composer: { suggest: { ...makeComposer().suggest, isOpen: true, point: { x: 1, y: 2 }, activeIndex: 2, trigger: { kind: 'slash' } } } });
    expect(screen.getByRole('listbox', { name: 'Commands' })).toBeInTheDocument();
    expect(h.props.ComposerSuggest.emptyLabel).toBe('No command matches.');
    expect(textarea()).toHaveAttribute('aria-activedescendant', 'composer-suggest-list-2');
    expect(textarea()).toHaveAttribute('aria-controls', 'composer-suggest-list');
  });

  it('names the space when mentioning documents', () => {
    const suggest = { ...makeComposer().suggest, isOpen: true, point: { x: 1, y: 2 }, trigger: { kind: 'mention' } };
    renderComposer({ composer: { suggest } });
    expect(screen.getByRole('listbox', { name: 'Documents in History' })).toBeInTheDocument();
    expect(h.props.ComposerSuggest.emptyLabel).toBe('Looking…');
  });

  it('says which documents a chat without a space can read', () => {
    const suggest = { ...makeComposer().suggest, isOpen: true, point: { x: 1, y: 2 }, trigger: { kind: 'mention' } };
    renderComposer({ composer: { suggest }, space: null });
    expect(screen.getByRole('listbox', { name: 'Documents this chat can read' })).toBeInTheDocument();
    expect(h.props.SpacePickerPopover).toBeUndefined();
  });

  it('removes focus documents one at a time or all at once', () => {
    const { composer } = renderComposer();
    h.props.FocusChips.onRemove('d1');
    expect(composer.applyFocusDocuments).toHaveBeenCalledWith([{ documentId: 'd2' }]);
    h.props.FocusChips.onClear();
    expect(composer.applyFocusDocuments).toHaveBeenLastCalledWith([]);
  });

  it('opens the controls and attaches files', () => {
    const props = renderComposer({ composer: { isControlsOpen: true } });
    expect(screen.getByText('Turn mode and tools panel')).toBeInTheDocument();
    expect(h.props.ComposerControls.onToggleWebTools).toBe(props.composer.toggleWebTools);
    fireEvent.click(screen.getByRole('button', { name: 'Composer controls' }));
    fireEvent.click(screen.getByRole('button', { name: 'Add files to this conversation' }));
    expect(props.attachments.chooseFiles).toHaveBeenCalled();
  });

  it('names the space it searches, or the documents it is pinned to', () => {
    const first = renderComposer();
    expect(screen.getByRole('button', { name: 'Searching History. Change space' })).toBeInTheDocument();
    first.unmount();
    renderComposer({ composer: { focusLabel: '2 documents in History' } });
    expect(screen.getByRole('button', { name: '2 documents in History. Change space' })).toBeInTheDocument();
  });

  it('warns about pictures only for a model that cannot see them', () => {
    const first = renderComposer();
    expect(h.props.ChatDropStaging.blindModelName).toBe('Qwen 9B');
    first.unmount();
    const second = renderComposer({ model: { effective: { label: 'Qwen VL', seesImages: true } } });
    expect(h.props.ChatDropStaging.blindModelName).toBeNull();
    second.unmount();
    renderComposer({ model: { effective: null } });
    expect(h.props.ChatDropStaging.blindModelName).toBeNull();
  });

  it('shows a hint, sends, and waits for the model', () => {
    const first = renderComposer({ composer: { input: 'Why?' } });
    expect(screen.getByText('↵ send · ⇧↵ new line')).toBeInTheDocument();
    fireEvent.submit(textarea().closest('form')!);
    expect(h.props.ChatModelButton.effective.label).toBe('Qwen 9B');
    first.unmount();
    renderComposer({ composer: { input: 'Why?' }, isChatUnavailable: true });
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled();
  });

  it('waits while compacting, and stops a running answer', () => {
    const first = renderComposer({ composer: { input: 'Why?' }, isCompacting: true });
    expect(screen.getByText('Compacting context…')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send message' })).toHaveAttribute('title', 'Waiting for the context to finish compacting');
    first.unmount();
    const props = renderComposer({ isSending: true });
    expect(screen.getByText('Generating…')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Stop generating response' }));
    expect(props.onCancel).toHaveBeenCalled();
  });
});
