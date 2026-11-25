import { act, fireEvent, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MessageActions } from '@/features/chat/components/MessageActions';
import { render } from '@/tests/renderWithQueryClient';

vi.mock('@/features/model/hooks/useActiveModels', () => ({ useActiveModels: () => ({ activeModels: null }) }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: [] }) }));
vi.mock('@/features/model/hooks/useLoadedModels', () => ({ useLoadedModels: () => [], degradedReasonFor: () => null }));

/** Every verb is 100px wide; the row is as wide as `rowWidth`. */
let rowWidth = 0;
let resize: (() => void) | null = null;

const props = () => ({
  canBookmark: true,
  canDelete: true,
  canBranch: true,
  isBookmarked: false,
  isBusy: false,
  onCopy: vi.fn(),
  onBookmarkToggle: vi.fn(),
  onDelete: vi.fn(),
  onRegenerate: vi.fn(),
  onTryWithModel: vi.fn(),
  onEdit: vi.fn(),
  onBranch: vi.fn(),
});

const menu = () => screen.getByRole('menu', { name: 'More actions' });
const openMore = () => fireEvent.click(screen.getByRole('button', { name: 'More actions for this message' }));

describe('MessageActions folding', () => {
  beforeEach(() => {
    rowWidth = 0;
    resize = null;
    vi.stubGlobal('ResizeObserver', class {
      constructor(callback: () => void) { resize = callback; }
      observe() {}
      disconnect() {}
    });
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) {
      return this.querySelector('[data-verb]') ? rowWidth : 0;
    });
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      return { width: this.dataset.verb ? 100 : 0 } as DOMRect;
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('keeps primary verbs inline when the row has room, with secondary verbs in More', () => {
    rowWidth = 2000;
    const { unmount } = render(<MessageActions {...props()} role="assistant" isLastTurn />);
    expect(screen.getByText('Copy')).toBeInTheDocument();
    expect(screen.getByText('Reference')).toBeInTheDocument();
    expect(screen.getByText('Regenerate')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'More actions for this message' })).toBeInTheDocument();
    unmount();
    rowWidth = 0;
    render(<MessageActions {...props()} role="assistant" isLastTurn />);
    expect(screen.getByText('Regenerate')).toBeInTheDocument();
  });

  it('folds extra primary verbs into More when the row is narrow', () => {
    rowWidth = 350;
    const handlers = props();
    render(<MessageActions {...handlers} role="assistant" isLastTurn />);
    // copy + reference + More fit (300px <= 350px); regenerate folds into More.
    expect(screen.getByText('Copy')).toBeInTheDocument();
    expect(screen.getByText('Reference')).toBeInTheDocument();
    expect(screen.queryByText('Regenerate')).not.toBeInTheDocument();
    openMore();
    const items = within(menu()).getAllByRole('menuitem').map((item) => item.textContent);
    expect(items).toEqual(['Regenerate', 'Try with another model…', 'Branch here', 'Delete message']);
    expect(within(menu()).getByRole('separator')).toBeInTheDocument();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Regenerate' }));
    expect(handlers.onRegenerate).toHaveBeenCalled();
    expect(screen.queryByRole('menu', { name: 'More actions' })).not.toBeInTheDocument();
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Branch here' }));
    expect(handlers.onBranch).toHaveBeenCalled();
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Delete message' }));
    expect(handlers.onDelete).toHaveBeenCalled();
  });

  it('opens the model picker from Try with another model in More', () => {
    rowWidth = 350;
    render(<MessageActions {...props()} role="assistant" isLastTurn />);
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Try with another model…' }));
    expect(screen.getByRole('menu', { name: 'Answer again with' })).toBeInTheDocument();
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { callback(0); return 0; });
    fireEvent.keyDown(screen.getByRole('menu', { name: 'Answer again with' }), { key: 'Escape' });
    expect(screen.getByRole('button', { name: 'More actions for this message' })).toHaveFocus();
  });

  it('moves focus through the menu with the arrow keys, Home and End', () => {
    rowWidth = 350;
    render(<MessageActions {...props()} role="assistant" isLastTurn />);
    openMore();
    const items = within(menu()).getAllByRole('menuitem');
    items[0].focus();
    fireEvent.keyDown(menu(), { key: 'ArrowDown' });
    expect(items[1]).toHaveFocus();
    fireEvent.keyDown(menu(), { key: 'ArrowUp' });
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(menu(), { key: 'ArrowUp' });
    expect(items[3]).toHaveFocus();
    fireEvent.keyDown(menu(), { key: 'Home' });
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(menu(), { key: 'End' });
    expect(items[3]).toHaveFocus();
    fireEvent.keyDown(menu(), { key: 'a' });
    expect(items[3]).toHaveFocus();
  });

  it('skips disabled items, and does nothing when all are disabled', () => {
    rowWidth = 350;
    render(<MessageActions {...props()} role="assistant" isLastTurn isBusy />);
    openMore();
    for (const item of within(menu()).getAllByRole('menuitem')) expect(item).toBeDisabled();
    fireEvent.keyDown(menu(), { key: 'ArrowDown' });
  });

  it('folds Edit on a narrow question row', () => {
    rowWidth = 150;
    const handlers = props();
    render(<MessageActions {...handlers} role="user" isLastTurn />);
    openMore();
    expect(within(menu()).getAllByRole('menuitem').map((item) => item.textContent)).toEqual(['Edit', 'Branch here', 'Delete message']);
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Edit' }));
    expect(handlers.onEdit).toHaveBeenCalled();
  });

  it('folds the reference toggle, naming its state', () => {
    rowWidth = 120;
    const handlers = props();
    render(<MessageActions {...handlers} role="assistant" isLastTurn={false} isBookmarked />);
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Remove from references' }));
    expect(handlers.onBookmarkToggle).toHaveBeenCalled();
  });

  it('unfolds primary verbs when the row grows', () => {
    rowWidth = 350;
    render(<MessageActions {...props()} role="assistant" isLastTurn />);
    expect(screen.queryByText('Regenerate')).not.toBeInTheDocument();
    rowWidth = 2000;
    act(() => resize?.());
    expect(screen.getByText('Regenerate')).toBeInTheDocument();
  });

  it('measures a primary verb that folded before it was drawn', () => {
    rowWidth = 150;
    const { rerender } = render(<MessageActions {...props()} role="assistant" isLastTurn={false} />);
    // With foldCount=1 already active, adding Regenerate puts it inside the folded slice before measurement.
    rerender(<MessageActions {...props()} role="assistant" isLastTurn />);
    // Unfolded for one pass so it can be measured…
    expect(screen.getByText('Regenerate')).toBeInTheDocument();
    // …then the observer's next pass folds it by its real width.
    act(() => resize?.());
    openMore();
    expect(within(menu()).getByRole('menuitem', { name: 'Regenerate' })).toBeInTheDocument();
  });
});

describe('MessageActions inline verbs', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('copies and says so for two seconds', async () => {
    vi.useFakeTimers();
    const handlers = props();
    render(<MessageActions {...handlers} role="assistant" isLastTurn />);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Copy message to clipboard' })); });
    expect(handlers.onCopy).toHaveBeenCalled();
    expect(screen.getByText('Copied')).toBeInTheDocument();
    act(() => { vi.advanceTimersByTime(2000); });
    expect(screen.getByText('Copy')).toBeInTheDocument();
  });

  it('runs each primary and More verb', () => {
    const handlers = props();
    const { unmount } = render(<MessageActions {...handlers} role="assistant" isLastTurn />);
    fireEvent.click(screen.getByRole('button', { name: 'Add to references' }));
    fireEvent.click(screen.getByRole('button', { name: 'Regenerate this answer' }));
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Branch here' }));
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Delete message' }));
    expect(handlers.onBookmarkToggle).toHaveBeenCalled();
    expect(handlers.onRegenerate).toHaveBeenCalled();
    expect(handlers.onBranch).toHaveBeenCalled();
    expect(handlers.onDelete).toHaveBeenCalled();
    unmount();
    render(<MessageActions {...handlers} role="user" isLastTurn />);
    fireEvent.click(screen.getByRole('button', { name: 'Edit this message' }));
    expect(handlers.onEdit).toHaveBeenCalled();
  });

  it('picks another model for the answer from More', () => {
    const handlers = props();
    render(<MessageActions {...handlers} role="assistant" isLastTurn />);
    openMore();
    fireEvent.click(within(menu()).getByRole('menuitem', { name: 'Try with another model…' }));
    expect(screen.getByRole('menu', { name: 'Answer again with' })).toBeInTheDocument();
  });
});
