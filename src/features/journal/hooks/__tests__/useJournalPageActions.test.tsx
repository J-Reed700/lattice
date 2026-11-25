import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { NEW_ITEM_EVENT } from '@/shared/newItemEvent';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { useJournalPageActions } from '../useJournalPageActions';

const page = (id: string, title: string) => ({ id, title } as WorkspaceNote);

function setup(overrides: Partial<Parameters<typeof useJournalPageActions>[0]> = {}) {
  const options = {
    journalSpace: { id: 'j1', name: 'Garden' } as never, requestedJournalSpaceId: 'j1', isLoadingNote: false,
    selectPage: vi.fn().mockResolvedValue(undefined), createPage: vi.fn().mockResolvedValue(page('new-page', 'Untitled page')),
    renamePage: vi.fn().mockResolvedValue(true), deletePage: vi.fn().mockResolvedValue(true), notify: vi.fn(),
    searchParams: new URLSearchParams(), setSearchParams: vi.fn(), ...overrides,
  };
  const view = renderHook((props: typeof options) => useJournalPageActions(props), { initialProps: options });
  return { ...options, ...view };
}

describe('useJournalPageActions', () => {
  it('names the legacy first page as untitled and keeps other titles', () => {
    const { result } = setup();
    expect(result.current.displayPageTitle(page('p1', 'Journal · Garden'))).toBe('Untitled page');
    expect(result.current.displayPageTitle(page('p2', 'Seed order'))).toBe('Seed order');
    expect(result.current.displayPageTitle(page('p3', '  '))).toBe('Untitled page');
  });

  it('falls back to the default journal name when no journal is open', () => {
    const { result } = setup({ journalSpace: null });
    expect(result.current.displayPageTitle(page('p1', 'Journal · Journal'))).toBe('Untitled page');
  });

  it('selects a page', () => {
    const s = setup();
    act(() => s.result.current.handleSelectPage('p2'));
    expect(s.selectPage).toHaveBeenCalledWith('p2');
  });

  it('creates an untitled page and asks for its name until it is renamed', async () => {
    const s = setup();
    await act(() => s.result.current.handleNewPage());
    expect(s.createPage).toHaveBeenCalledWith('Untitled page');
    expect(s.result.current.pageToName).toBe('new-page');
    await act(() => s.result.current.handleRenamePage('new-page', 'Tomato log'));
    expect(s.renamePage).toHaveBeenCalledWith('new-page', 'Tomato log');
    expect(s.result.current.pageToName).toBeNull();
    expect(s.notify).not.toHaveBeenCalled();
  });

  it('does not ask for a name when the page was not created, and reports a failed rename', async () => {
    const s = setup({ createPage: vi.fn().mockResolvedValue(null), renamePage: vi.fn().mockResolvedValue(false) });
    await act(() => s.result.current.handleNewPage());
    expect(s.result.current.pageToName).toBeNull();
    await act(() => s.result.current.handleRenamePage('p1', 'x'));
    expect(s.notify).toHaveBeenCalledWith('error', 'Could not rename that page.');
  });

  it('deletes a page only after confirmation, and reports the outcome', async () => {
    const s = setup();
    act(() => s.result.current.handleDeletePage(page('p1', 'Journal · Garden')));
    expect(s.result.current.pagePendingDelete?.id).toBe('p1');
    expect(s.deletePage).not.toHaveBeenCalled();

    act(() => s.result.current.cancelDeletePage());
    expect(s.result.current.pagePendingDelete).toBeNull();
    expect(s.deletePage).not.toHaveBeenCalled();

    act(() => s.result.current.handleDeletePage(page('p2', 'Seed order')));
    await act(() => s.result.current.confirmDeletePage());
    expect(s.deletePage).toHaveBeenCalledWith('p2');
    expect(s.notify).toHaveBeenCalledWith('success', 'Deleted "Seed order".');
    expect(s.result.current.pagePendingDelete).toBeNull();

    vi.mocked(s.deletePage).mockResolvedValueOnce(false);
    act(() => s.result.current.handleDeletePage(page('p2', 'Seed order')));
    await act(() => s.result.current.confirmDeletePage());
    expect(s.notify).toHaveBeenCalledWith('error', 'Could not delete that page.');
  });

  it('starts a page on the app-wide new-item shortcut', async () => {
    const s = setup();
    await act(async () => { window.dispatchEvent(new Event(NEW_ITEM_EVENT)); });
    expect(s.createPage).toHaveBeenCalledTimes(1);
    s.unmount();
    window.dispatchEvent(new Event(NEW_ITEM_EVENT));
    expect(s.createPage).toHaveBeenCalledTimes(1);
  });

  it('answers a ?new=1 link once the page list has loaded, removing the flag', async () => {
    const searchParams = new URLSearchParams('new=1&journalSpaceId=j1');
    const s = setup({ searchParams, isLoadingNote: true });
    expect(s.createPage).not.toHaveBeenCalled();
    await act(async () => { s.rerender({ ...s, searchParams, isLoadingNote: false } as never); });
    expect(s.setSearchParams).toHaveBeenCalledWith(new URLSearchParams('journalSpaceId=j1'), { replace: true });
    expect(s.createPage).toHaveBeenCalledTimes(1);
  });

  it('ignores ?new=1 without a journal', () => {
    const s = setup({ searchParams: new URLSearchParams('new=1'), requestedJournalSpaceId: null });
    expect(s.setSearchParams).not.toHaveBeenCalled();
  });

  it('ignores NEW_ITEM_EVENT and handleNewPage when no journal space is active', async () => {
    const s = setup({ journalSpace: null, requestedJournalSpaceId: null });
    await act(async () => {
      window.dispatchEvent(new Event(NEW_ITEM_EVENT));
      await s.result.current.handleNewPage();
    });
    expect(s.createPage).not.toHaveBeenCalled();
  });
});
