import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import type { ConversationTangentDto } from '@/lib/bindings';

import { TangentSelection, TangentSelectionContext } from './TangentSelection';
import { findPassageRange } from './useTangentSourceHighlights';

function body(html: string) {
  const root = document.createElement('div');
  root.innerHTML = html;
  document.body.append(root);
  return root;
}

const tangent = (overrides: Partial<ConversationTangentDto> = {}): ConversationTangentDto => ({
  conversationId: 'tangent-1', parentConversationId: 'parent', sourceConversationId: 'parent', sourceMessageId: 'reply',
  selectedText: 'beta gamma', title: 'On beta', createdAt: '', updatedAt: '', contextMessageCount: 0, ...overrides,
});

describe('findPassageRange', () => {
  afterEach(() => { document.body.innerHTML = ''; });

  it('finds a selection that spans a <strong> boundary', () => {
    const root = body('<p>Alpha <strong>beta</strong> gamma delta</p>');
    expect(findPassageRange(root, 'beta gamma')?.toString()).toBe('beta gamma');
  });

  it('matches across blocks whatever the whitespace, and inside code', () => {
    const root = body('<p>First line ends here.</p><p>Second <code>x  =  1</code> starts</p>');
    expect(findPassageRange(root, 'ends here.\n\nSecond x = 1')?.toString()).toBe('ends here.Second x  =  1');
  });

  it('marks nothing for a missing passage or for the whole message', () => {
    const root = body('<p>Alpha <em>beta</em></p>');
    expect(findPassageRange(root, 'not here')).toBeNull();
    expect(findPassageRange(root, 'Alpha beta')).toBeNull();
    expect(findPassageRange(root, '   ')).toBeNull();
  });
});

describe('tangent source highlights', () => {
  const highlights = new Map<string, { ranges: Range[] }>();

  beforeEach(() => {
    highlights.clear();
    vi.stubGlobal('CSS', { highlights });
    vi.stubGlobal('Highlight', class { ranges: Range[]; constructor(...ranges: Range[]) { this.ranges = ranges; } });
    vi.spyOn(Range.prototype, 'getClientRects').mockReturnValue([{ left: 10, right: 90, top: 10, bottom: 30 }] as unknown as DOMRectList);
    conversationUiStore.setState({ requestedTangent: null });
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  const renderReply = (tangents: ConversationTangentDto[]) => render(
    <TangentSelectionContext.Provider value={{ create: vi.fn(), creating: false, tangents }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled><p>Alpha <strong>beta</strong> gamma delta</p></TangentSelection>
    </TangentSelectionContext.Provider>,
  );

  it('marks the passage, opens its tangent on click and clears on unmount', () => {
    const view = renderReply([tangent(), tangent({ conversationId: 'other', sourceMessageId: 'elsewhere', selectedText: 'Alpha' })]);
    expect(highlights.get('tangent-source')?.ranges.map(range => range.toString())).toEqual(['beta gamma']);

    const text = screen.getByText('beta');
    fireEvent.mouseMove(text, { clientX: 40, clientY: 20 });
    expect(text.closest('[data-tangent-hover]')).toHaveAttribute('title', 'Open tangent: On beta');

    fireEvent.click(text, { clientX: 200, clientY: 200 });
    expect(conversationUiStore.getState().requestedTangent).toBeNull();
    fireEvent.click(text, { clientX: 40, clientY: 20 });
    expect(conversationUiStore.getState().requestedTangent).toEqual({ parentId: 'parent', tangentId: 'tangent-1' });

    view.unmount();
    expect(highlights.has('tangent-source')).toBe(false);
  });

  it('does nothing without the Custom Highlight API', () => {
    vi.stubGlobal('Highlight', undefined);
    renderReply([tangent()]);
    fireEvent.click(screen.getByText('beta'), { clientX: 40, clientY: 20 });
    expect(highlights.size).toBe(0);
    expect(conversationUiStore.getState().requestedTangent).toBeNull();
  });
});
