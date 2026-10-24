import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { selectPaletteGroups, usePaletteCommandsStore } from '@/features/palette/stores/paletteCommandsStore';

import { useJournalPaletteCommands, useJournalShortcuts } from '../useJournalCommands';

const journal = { id: 'j1', name: 'Kitchen' } as never;
const visible = () => selectPaletteGroups(usePaletteCommandsStore.getState()).flatMap((g) => g.commands);
const command = (id: string) => usePaletteCommandsStore.getState().commands.get(id)!;

describe('useJournalPaletteCommands', () => {
  beforeEach(() => usePaletteCommandsStore.setState({ commands: new Map() }));

  it('offers each verb only when it can run, and runs it', () => {
    const handlers = { handleSynthesize: vi.fn().mockResolvedValue(true), handleNewEntry: vi.fn().mockResolvedValue(undefined), handleNewPage: vi.fn().mockResolvedValue(undefined) };
    const { rerender, unmount } = renderHook((props: { total?: number; busy: boolean; pinned: Set<string>; space: never | null }) => useJournalPaletteCommands({
      weekCandidateTotal: props.total, isSynthesizing: props.busy, pinnedIds: props.pinned, journalSpace: props.space, ...handlers,
    }), { initialProps: { total: undefined as number | undefined, busy: false, pinned: new Set<string>(), space: null as never | null } });
    expect(visible()).toEqual([]);

    rerender({ total: 3, busy: false, pinned: new Set(['e1']), space: journal });
    expect(visible().map((c) => c.label)).toEqual([
      'Synthesize the past week', 'Synthesize pinned entries', 'Ask in Chat, filed under this journal', 'New journal page',
    ]);
    command('journal.synthesize.week').run();
    command('journal.synthesize.pinned').run();
    command('journal.newEntry').run();
    command('journal.newPage').run();
    expect(handlers.handleSynthesize.mock.calls).toEqual([['week'], ['pinned']]);
    expect(handlers.handleNewEntry).toHaveBeenCalledWith();
    expect(handlers.handleNewPage).toHaveBeenCalled();

    rerender({ total: 3, busy: true, pinned: new Set(['e1']), space: journal });
    expect(visible().map((c) => c.id)).toEqual(['journal.newEntry', 'journal.newPage']);
    unmount();
    expect(usePaletteCommandsStore.getState().commands.size).toBe(0);
  });
});

describe('useJournalShortcuts', () => {
  const NOW = new Date(2026, 9, 24, 12);
  const entries = [
    { id: 'a', title: 'A', updatedAt: new Date(2026, 9, 20).toISOString(), messageCount: 1 },
    { id: 'b', title: 'B', updatedAt: 'garbled', messageCount: 1 },
    { id: 'c', title: 'C', updatedAt: new Date(2026, 9, 24, 8).toISOString(), messageCount: 1 },
  ];
  let input: HTMLInputElement;

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW);
    input = document.createElement('input');
    document.body.append(input);
  });
  afterEach(() => {
    vi.useRealTimers();
    input.remove();
  });

  function setup(selectedId: string | null, list = entries) {
    const options = {
      entries: list, selectedId, setSelectedId: vi.fn(), saveNow: vi.fn().mockResolvedValue(true),
      handleNewPage: vi.fn().mockResolvedValue(undefined), setSidebarCollapsed: vi.fn(),
    };
    const view = renderHook(() => useJournalShortcuts(options));
    return { ...options, ...view };
  }
  const press = (init: KeyboardEventInit, target: EventTarget = window) =>
    act(() => { target.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init })); });

  it('saves with Cmd+S and toggles the sidebar with Cmd+\\, even while typing', () => {
    const s = setup(null);
    press({ key: 's', metaKey: true }, input);
    expect(s.saveNow).toHaveBeenCalled();
    press({ key: '\\', ctrlKey: true }, input);
    const toggle = s.setSidebarCollapsed.mock.calls[0][0] as (v: boolean) => boolean;
    expect(toggle(false)).toBe(true);
    press({ key: 'S', metaKey: true, shiftKey: true });
    expect(s.saveNow).toHaveBeenCalledTimes(1);
  });

  it('starts a new page with Cmd+N, but not while typing', () => {
    const s = setup(null);
    press({ key: 'n', metaKey: true }, input);
    expect(s.handleNewPage).not.toHaveBeenCalled();
    press({ key: 'N', ctrlKey: true });
    expect(s.handleNewPage).toHaveBeenCalledTimes(1);
  });

  it('moves through entries with j/k and the arrows, clamped at both ends', () => {
    const s = setup('b');
    press({ key: 'j' });
    expect(s.setSelectedId).toHaveBeenLastCalledWith('c');
    press({ key: 'ArrowUp' });
    expect(s.setSelectedId).toHaveBeenLastCalledWith('a');
    s.unmount();

    const last = setup('c');
    press({ key: 'ArrowDown' });
    expect(last.setSelectedId).toHaveBeenLastCalledWith('c');
    last.unmount();

    const none = setup(null);
    press({ key: 'j' });
    expect(none.setSelectedId).toHaveBeenLastCalledWith('a');
    press({ key: 'k' });
    expect(none.setSelectedId).toHaveBeenLastCalledWith('a');
  });

  it('ignores navigation keys while typing or with no entries', () => {
    const s = setup('a');
    press({ key: 'j' }, input);
    expect(s.setSelectedId).not.toHaveBeenCalled();
    s.unmount();
    const empty = setup(null, []);
    press({ key: 'j' });
    press({ key: 'k' });
    press({ key: 't' });
    expect(empty.setSelectedId).not.toHaveBeenCalled();
  });

  it('jumps to today’s entry with t, if there is one', () => {
    const s = setup(null);
    press({ key: 'T' });
    expect(s.setSelectedId).toHaveBeenCalledWith('c');
    s.unmount();
    const old = setup(null, entries.slice(0, 2));
    press({ key: 't' });
    expect(old.setSelectedId).not.toHaveBeenCalled();
    press({ key: 'x' });
    old.unmount();
  });
});
