import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatReaderPane } from '@/features/chat/components/reader/ChatReaderPane';
import { READER_DEFAULT_WIDTH, READER_MIN_WIDTH, useChatReaderStore } from '@/features/reading/stores/chatReaderStore';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { SourceWithMetadata } from '@/types/conversation';

const h = vi.hoisted(() => ({ rememberLocation: vi.fn() }));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (state: unknown) => unknown) => selector({ activeConversationId: 'c1' }),
}));
vi.mock('@/shared/sources/passageLocator', () => ({
  locatorFromSource: () => null,
  rememberLocation: h.rememberLocation,
}));
vi.mock('@/features/reading/components/FilePreviewModal', () => ({ FilePreviewModal: () => null }));
vi.mock('@/features/reading/components/SourceReaderBody', () => ({
  SourceReaderBody: ({ isFocused, onToggleFocus, onLocationResolved }: { isFocused: boolean; onToggleFocus: () => void; onLocationResolved: (chunkId: string, label: string) => void }) => (
    <div>
      <button type="button" onClick={onToggleFocus}>{isFocused ? 'Unfocus' : 'Focus'}</button>
      <button type="button" onClick={() => onLocationResolved('chunk-1', 'p. 12')}>Resolve</button>
      <input aria-label="Find in source" />
    </div>
  ),
}));

const source = { documentId: 'd1', chunkId: 'chunk-1', fileName: 'Herodotus.pdf', filePath: '/lib/herodotus.pdf', citationId: 1 } as unknown as SourceWithMetadata;
const WIDE = { rowWidth: 1800, availableWidth: 1500 };

const renderPane = () => {
  render(<ChatReaderPane {...WIDE} />);
  act(() => useChatReaderStore.getState().open('answer-1', [source], 0));
};

const pane = () => screen.getByRole('complementary', { name: 'Source reader' });
const handle = () => screen.getByRole('separator', { name: 'Resize source reader' });

describe('ChatReaderPane resizing and focus', () => {
  beforeAll(() => {
    Element.prototype.setPointerCapture = function () {};
    Element.prototype.hasPointerCapture = () => true;
    Element.prototype.releasePointerCapture = function () {};
  });

  beforeEach(() => {
    h.rememberLocation.mockClear();
    useCitationDisplayStore.setState({ visible: true });
    useChatReaderStore.setState({ session: null, width: READER_DEFAULT_WIDTH, resolvedLocations: new Map() });
  });

  it('widens as the handle is dragged left, and stops at the minimum', () => {
    renderPane();
    fireEvent.pointerDown(handle(), { button: 0, pointerId: 1, clientX: 1000 });
    expect(document.body.style.userSelect).toBe('none');
    fireEvent.pointerMove(handle(), { pointerId: 1, clientX: 900 });
    expect(pane()).toHaveStyle({ width: `${READER_DEFAULT_WIDTH + 100}px` });
    fireEvent.pointerMove(handle(), { pointerId: 2, clientX: 0 });
    expect(pane()).toHaveStyle({ width: `${READER_DEFAULT_WIDTH + 100}px` });
    fireEvent.pointerMove(handle(), { pointerId: 1, clientX: 2000 });
    expect(pane()).toHaveStyle({ width: `${READER_MIN_WIDTH}px` });
    fireEvent.pointerUp(handle(), { pointerId: 2 });
    expect(document.body.style.userSelect).toBe('none');
    fireEvent.pointerUp(handle(), { pointerId: 1 });
    expect(document.body.style.userSelect).toBe('');
  });

  it('ignores a drag with another button and stops on cancel', () => {
    renderPane();
    fireEvent.pointerDown(handle(), { button: 2, pointerId: 1, clientX: 1000 });
    fireEvent.pointerMove(handle(), { pointerId: 1, clientX: 900 });
    expect(pane()).toHaveStyle({ width: `${READER_DEFAULT_WIDTH}px` });
    fireEvent.pointerDown(handle(), { button: 0, pointerId: 3, clientX: 1000 });
    fireEvent.pointerCancel(handle(), { pointerId: 3 });
    fireEvent.pointerMove(handle(), { pointerId: 3, clientX: 900 });
    expect(pane()).toHaveStyle({ width: `${READER_DEFAULT_WIDTH}px` });
  });

  it('ignores other keys on the handle', () => {
    renderPane();
    fireEvent.keyDown(handle(), { key: 'ArrowUp' });
    expect(pane()).toHaveStyle({ width: `${READER_DEFAULT_WIDTH}px` });
  });

  it('takes the room the chat can spare in focus mode', () => {
    renderPane();
    fireEvent.click(screen.getByRole('button', { name: 'Focus' }));
    expect(pane()).toHaveStyle({ width: `${WIDE.availableWidth - 320}px` });
    fireEvent.click(screen.getByRole('button', { name: 'Unfocus' }));
    expect(pane()).toHaveStyle({ width: `${READER_DEFAULT_WIDTH}px` });
  });

  it('remembers where a passage was found', () => {
    renderPane();
    fireEvent.click(screen.getByRole('button', { name: 'Resolve' }));
    expect(h.rememberLocation).toHaveBeenCalledWith('chunk-1', 'p. 12');
    expect(useChatReaderStore.getState().resolvedLocations.get('chunk-1')).toBe('p. 12');
  });

  it('leaves Escape to a field inside the reader', () => {
    renderPane();
    fireEvent.keyDown(screen.getByRole('textbox', { name: 'Find in source' }), { key: 'Escape' });
    expect(useChatReaderStore.getState().session).not.toBeNull();
    fireEvent.keyDown(document, { key: 'a' });
    expect(useChatReaderStore.getState().session).not.toBeNull();
  });
});
