import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TangentSelection, TangentSelectionContext } from './TangentSelection';

function select(start: Node, end: Node = start) {
  const range = document.createRange();
  range.setStart(start, 0);
  range.setEnd(end, end.textContent?.length ?? 0);
  window.getSelection()?.removeAllRanges();
  window.getSelection()?.addRange(range);
}

describe('TangentSelection', () => {
  beforeEach(() => window.getSelection()?.removeAllRanges());

  it('captures the selection before focusing the menu and supports keyboard activation', () => {
    const create = vi.fn();
    render(<TangentSelectionContext.Provider value={{ create, creating: false }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled><p>Selected answer</p></TangentSelection>
    </TangentSelectionContext.Provider>);
    const answer = screen.getByText('Selected answer');
    select(answer.firstChild!);
    fireEvent.contextMenu(answer, { clientX: 120, clientY: 80 });
    const ask = screen.getByRole('menuitem', { name: 'Ask in a tangent' });
    expect(ask).toHaveFocus();
    window.getSelection()?.removeAllRanges();
    fireEvent.click(ask);
    expect(create).toHaveBeenCalledWith({ conversationId: 'parent', messageId: 'reply', selectedText: 'Selected answer' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('captures a passage without the citation marks a selection crosses', () => {
    const create = vi.fn();
    render(<TangentSelectionContext.Provider value={{ create, creating: false }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled>
        <p data-testid="cited">Uses a hash map.<span className="cite-chip" data-cite="2">{'\uE000\uE001\uE00B'}</span> Lookups are fast.</p>
      </TangentSelection>
    </TangentSelectionContext.Provider>);
    const answer = screen.getByTestId('cited');
    const range = document.createRange();
    range.selectNodeContents(answer);
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
    fireEvent.contextMenu(answer, { clientX: 120, clientY: 80 });
    fireEvent.click(screen.getByRole('menuitem', { name: 'Ask in a tangent' }));
    const selectedText = create.mock.calls[0]![0].selectedText as string;
    expect(selectedText).toBe('Uses a hash map. Lookups are fast.');
    expect(selectedText).not.toMatch(/[\uE000-\uE00D]/);
  });

  it('leaves the native menu available for a collapsed or cross-message selection', () => {
    render(<TangentSelectionContext.Provider value={{ create: vi.fn(), creating: false }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled><p>First reply</p></TangentSelection>
      <p>Another message</p>
    </TangentSelectionContext.Provider>);
    const answer = screen.getByText('First reply');
    expect(fireEvent.contextMenu(answer)).toBe(true);
    select(answer.firstChild!, screen.getByText('Another message').firstChild!);
    fireEvent(document, new Event('selectionchange'));
    expect(fireEvent.contextMenu(answer)).toBe(true);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
  });

  it('reveals an action after highlighting, keeps focus, and captures the exact passage', () => {
    const create = vi.fn();
    render(<TangentSelectionContext.Provider value={{ create, creating: false }}>
      <input aria-label="Main draft" defaultValue="Keep writing here" />
      <TangentSelection conversationId="parent" messageId="reply" enabled><p>Selected answer</p></TangentSelection>
    </TangentSelectionContext.Provider>);
    const draft = screen.getByRole('textbox', { name: 'Main draft' });
    draft.focus();
    const answer = screen.getByText('Selected answer');
    fireEvent.pointerDown(answer, { button: 0 });
    select(answer.firstChild!);
    fireEvent(document, new Event('selectionchange'));
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
    fireEvent.pointerUp(answer);
    expect(screen.getByRole('toolbar', { name: 'Selected passage' })).toBeVisible();
    expect(draft).toHaveFocus();
    const ask = screen.getByRole('button', { name: 'Ask in a tangent' });
    window.getSelection()?.removeAllRanges();
    fireEvent.click(ask);
    expect(create).toHaveBeenCalledWith({ conversationId: 'parent', messageId: 'reply', selectedText: 'Selected answer' });
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
    expect(draft).toHaveValue('Keep writing here');
  });

  it('responds to keyboard selection, updates the passage, and dismisses on Escape or collapse', () => {
    render(<TangentSelectionContext.Provider value={{ create: vi.fn(), creating: false }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled><p>Selected answer</p></TangentSelection>
    </TangentSelectionContext.Provider>);
    const answer = screen.getByText('Selected answer');
    select(answer.firstChild!);
    fireEvent(document, new Event('selectionchange'));
    expect(screen.getByRole('toolbar')).toBeVisible();
    fireEvent.keyDown(document, { key: 'Escape' });
    fireEvent.keyUp(document, { key: 'Escape' });
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
    select(answer.firstChild!);
    fireEvent(document, new Event('selectionchange'));
    expect(screen.getByRole('toolbar')).toBeVisible();
    window.getSelection()?.removeAllRanges();
    fireEvent(document, new Event('selectionchange'));
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
  });

  it('offers no tangent for a selection of only whitespace', () => {
    const create = vi.fn();
    render(<TangentSelectionContext.Provider value={{ create, creating: false }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled><p>Nile   flood</p></TangentSelection>
    </TangentSelectionContext.Provider>);
    const answer = screen.getByText('Nile flood');
    const range = document.createRange();
    range.setStart(answer.firstChild!, 4);
    range.setEnd(answer.firstChild!, 7);
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
    fireEvent(document, new Event('selectionchange'));
    expect(fireEvent.contextMenu(answer)).toBe(true);
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  it('offers no tangent for an unfinished reply and dismisses on Escape', () => {
    const create = vi.fn();
    const content = (enabled: boolean) => <TangentSelectionContext.Provider value={{ create, creating: false }}>
      <TangentSelection conversationId="parent" messageId="reply" enabled={enabled}><p>Answer</p></TangentSelection>
    </TangentSelectionContext.Provider>;
    const view = render(content(false));
    select(screen.getByText('Answer').firstChild!);
    fireEvent.contextMenu(screen.getByText('Answer'));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    view.rerender(content(true));
    select(screen.getByText('Answer').firstChild!);
    fireEvent.contextMenu(screen.getByText('Answer'));
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });
});
