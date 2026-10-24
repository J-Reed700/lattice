import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MAX_TANGENT_PASSAGE_LENGTH, TangentSelection, TangentSelectionContext } from './TangentSelection';

const toast = vi.hoisted(() => ({ error: vi.fn() }));
vi.mock('@/stores/toastStore', () => ({ toast }));

function select(node: Node) {
  const range = document.createRange();
  range.setStart(node, 0);
  range.setEnd(node, node.textContent?.length ?? 0);
  window.getSelection()?.removeAllRanges();
  window.getSelection()?.addRange(range);
}

const writeText = vi.fn();

function renderPassage(text = 'The Nile floods each summer.', { creating = false } = {}) {
  const create = vi.fn();
  render(
    <TangentSelectionContext.Provider value={{ create, creating }}>
      <article tabIndex={-1} aria-label="Answer">
        <TangentSelection conversationId="parent" messageId="reply" enabled><p>{text}</p></TangentSelection>
      </article>
      <p>Elsewhere on the page</p>
    </TangentSelectionContext.Provider>,
  );
  const passage = screen.getByText(text);
  return { create, passage };
}

function openMenu(passage: HTMLElement) {
  select(passage.firstChild!);
  fireEvent.contextMenu(passage, { clientX: 140, clientY: 90 });
  return screen.getByRole('menu', { name: 'Selected passage' });
}

function highlight(passage: HTMLElement) {
  select(passage.firstChild!);
  fireEvent(document, new Event('selectionchange'));
  return screen.getByRole('toolbar', { name: 'Selected passage' });
}

beforeEach(() => {
  vi.clearAllMocks();
  window.getSelection()?.removeAllRanges();
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
  writeText.mockResolvedValue(undefined);
});

afterEach(() => window.getSelection()?.removeAllRanges());

describe('the passage menu', () => {
  it('moves between its items with the arrow keys, wrapping at either end', () => {
    const { passage } = renderPassage();
    const menu = openMenu(passage);
    const ask = screen.getByRole('menuitem', { name: 'Ask in a tangent' });
    const copy = screen.getByRole('menuitem', { name: 'Copy' });
    expect(ask).toHaveFocus();

    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(copy).toHaveFocus();
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(ask).toHaveFocus();
    fireEvent.keyDown(menu, { key: 'ArrowUp' });
    expect(copy).toHaveFocus();
  });

  it('closes on Tab and hands focus back to the answer', () => {
    const { passage } = renderPassage();
    const menu = openMenu(passage);
    fireEvent.keyDown(menu, { key: 'Tab' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(screen.getByRole('article', { name: 'Answer' })).toHaveFocus();
  });

  it('copies the passage and closes', async () => {
    const { passage } = renderPassage();
    openMenu(passage);
    await act(async () => { fireEvent.click(screen.getByRole('menuitem', { name: 'Copy' })); });
    expect(writeText).toHaveBeenCalledWith('The Nile floods each summer.');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('says so when the passage cannot be copied', async () => {
    writeText.mockRejectedValueOnce(new Error('clipboard blocked'));
    const { passage } = renderPassage();
    openMenu(passage);
    await act(async () => { fireEvent.click(screen.getByRole('menuitem', { name: 'Copy' })); });
    expect(toast.error).toHaveBeenCalledWith("Couldn't copy the passage");
  });

  it('refuses a passage longer than a tangent can start from', () => {
    const { create, passage } = renderPassage('a'.repeat(MAX_TANGENT_PASSAGE_LENGTH + 1));
    openMenu(passage);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Ask in a tangent' }));
    expect(toast.error).toHaveBeenCalledWith('Choose a shorter passage', { message: 'A tangent can start from up to 8,000 characters.' });
    expect(create).not.toHaveBeenCalled();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('focuses Copy while a tangent is already being created', () => {
    const { passage } = renderPassage(undefined, { creating: true });
    openMenu(passage);
    expect(screen.getByRole('menuitem', { name: 'Ask in a tangent' })).toBeDisabled();
    expect(screen.getByRole('menuitem', { name: 'Copy' })).toHaveFocus();
  });

  it('stays open while the selection changes under it', () => {
    const { passage } = renderPassage();
    openMenu(passage);
    act(() => (document.activeElement as HTMLElement).blur());
    window.getSelection()?.removeAllRanges();
    fireEvent(document, new Event('selectionchange'));
    expect(screen.getByRole('menu')).toBeInTheDocument();
  });
});

describe('the highlight action', () => {
  it('ignores keys other than Escape, including the menu keys', () => {
    const { passage } = renderPassage();
    const toolbar = highlight(passage);
    fireEvent.keyDown(document, { key: 'Enter' });
    fireEvent.keyDown(toolbar, { key: 'Tab' });
    expect(screen.getByRole('toolbar')).toBe(toolbar);
  });

  it('closes on any press elsewhere, or a scroll', () => {
    const { passage } = renderPassage();
    highlight(passage);
    fireEvent.pointerDown(screen.getByText('Elsewhere on the page'), { button: 2 });
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();

    highlight(passage);
    fireEvent.scroll(document);
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
  });

  // Bug: TangentSelection.tsx:88 hands the resize event's target (window, not a Node) to Node.contains, which throws, so a resize never closes the action.
  it.fails('closes on a window resize', () => {
    const onError = (event: ErrorEvent) => event.preventDefault();
    window.addEventListener('error', onError);
    try {
      const { passage } = renderPassage();
      highlight(passage);
      fireEvent(window, new Event('resize'));
      expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
    } finally {
      window.removeEventListener('error', onError);
    }
  });

  it('stays put for a press inside it', () => {
    const { passage } = renderPassage();
    const toolbar = highlight(passage);
    fireEvent.pointerDown(screen.getByRole('button', { name: 'Ask in a tangent' }), { button: 0 });
    expect(screen.getByRole('toolbar')).toBe(toolbar);
  });

  it('drops the action when a drag is cancelled', () => {
    const { passage } = renderPassage();
    highlight(passage);
    fireEvent.pointerDown(passage, { button: 0 });
    select(passage.firstChild!);
    fireEvent(document, new Event('selectionchange'));
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();
    fireEvent(document, new Event('pointercancel'));
    expect(screen.queryByRole('toolbar')).not.toBeInTheDocument();

    fireEvent(document, new Event('selectionchange'));
    expect(screen.getByRole('toolbar')).toBeInTheDocument();
  });

  it('does not chase a selection made while the action has focus', () => {
    const { passage } = renderPassage();
    highlight(passage);
    const ask = screen.getByRole('button', { name: 'Ask in a tangent' });
    ask.focus();
    window.getSelection()?.removeAllRanges();
    fireEvent(document, new Event('selectionchange'));
    expect(screen.getByRole('toolbar')).toBeInTheDocument();
  });
});
