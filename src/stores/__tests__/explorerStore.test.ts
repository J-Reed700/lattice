import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { currentExplorerFocus, useExplorerStore as store, type FolderIndexStatus } from '../explorerStore';

beforeEach(() => {
  localStorage.clear();
  store.setState({ root: null, expanded: new Set(), openPath: null, back: [], forward: [], selection: null, highlight: null, aliases: {}, threadByRoot: {}, indexStatus: null, indexEvents: 0 });
});
afterEach(() => { vi.restoreAllMocks(); localStorage.clear(); });

const root = { root: '/work/café', name: 'café' };
const range = { startLine: 3, endLine: 7 };

describe('Explorer session invariants', () => {
  it('limits history to the most recent 50 files', () => {
    for (let i = 0; i < 120; i++) store.getState().openFile(`src/${i}.ts`);
    expect(store.getState().back).toEqual(Array.from({ length: 50 }, (_, i) => `src/${i + 69}.ts`));
    for (let i = 0; i < 60; i++) store.getState().goBack();
    expect(store.getState().openPath).toBe('src/69.ts');
    expect(store.getState().back).toEqual([]);
    for (let i = 0; i < 60; i++) store.getState().goForward();
    expect(store.getState().openPath).toBe('src/119.ts');
    expect(store.getState().forward).toEqual([]);
  });

  it('matches a bounded navigation model over 1,000 repeatable mixed actions', () => {
    // Fixed seed makes a failing sequence reproducible on every platform.
    let seed = 0x71a77ce;
    const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed; };
    const model: { path: string | null; back: string[]; forward: string[] } = { path: null, back: [], forward: [] };
    for (let step = 0; step < 1000; step++) {
      const action = random() % 4;
      if (action < 2) {
        const path = `src/${random() % 17}.ts`;
        if (model.path !== path) {
          if (model.path) model.back = [...model.back, model.path].slice(-50);
          model.path = path; model.forward = [];
        }
        store.getState().openFile(path);
      } else if (action === 2) {
        const next = model.back.pop();
        if (next) { if (model.path) model.forward.push(model.path); model.path = next; }
        store.getState().goBack();
      } else {
        const next = model.forward.pop();
        if (next) { if (model.path) model.back.push(model.path); model.path = next; }
        store.getState().goForward();
      }
      expect({ path: store.getState().openPath, back: store.getState().back, forward: store.getState().forward }, `navigation step ${step}`).toEqual(model);
    }
  });

  it('repeated citations re-highlight without adding history or losing a selection', () => {
    store.getState().reveal('src/deep/file.ts', range);
    const firstNonce = store.getState().highlight!.nonce;
    store.getState().setSelection(range);
    store.getState().reveal('src/deep/file.ts', range);
    expect(store.getState().highlight!.nonce).toBeGreaterThan(firstNonce);
    expect(store.getState().back).toEqual([]);
    expect(currentExplorerFocus()).toEqual({ openPath: 'src/deep/file.ts', selection: range });
    expect([...store.getState().expanded]).toEqual(['src', 'src/deep']);
  });

  it('retargets chained aliases while preserving citation lines and history', () => {
    store.getState().openFile('README.md');
    store.getState().reveal('file.ts', range);
    store.getState().retarget('file.ts', 'src/file.ts');
    store.getState().retarget('src/file.ts', 'lib/file.ts');
    expect(store.getState().aliases).toEqual({ 'file.ts': 'lib/file.ts', 'src/file.ts': 'lib/file.ts' });
    expect(store.getState().highlight).toMatchObject({ path: 'lib/file.ts', range });
    expect(store.getState().back).toEqual(['README.md']);
    store.getState().openFile('README.md');
    store.getState().openFile('file.ts');
    expect(store.getState().openPath).toBe('lib/file.ts');
    const current = store.getState();
    store.getState().retarget('stale.ts', 'other.ts');
    store.getState().retarget('lib/file.ts', 'lib/file.ts');
    expect(store.getState()).toBe(current);
  });

  it('isolates file state between roots but preserves each root’s remembered thread', () => {
    store.getState().setRoot(root);
    store.getState().rememberThread(root.root, 'thread-a');
    store.getState().reveal('file.ts', range);
    store.getState().retarget('file.ts', 'src/file.ts');
    const unchanged = store.getState();
    store.getState().setRoot(root);
    expect(store.getState()).toBe(unchanged);
    store.getState().setRoot({ root: '/work/other', name: 'other' });
    expect(currentExplorerFocus()).toBeNull();
    expect(store.getState()).toMatchObject({ openPath: null, selection: null, highlight: null, back: [], forward: [], aliases: {}, threadByRoot: { [root.root]: 'thread-a' } });
    expect(store.getState().expanded.size).toBe(0);
    store.getState().setRoot(null);
    expect(localStorage.getItem('explorer.root')).toBeNull();
  });

  it('never changes a previously published expanded set', () => {
    const initial = store.getState().expanded;
    store.getState().toggleExpanded('src');
    const opened = store.getState().expanded;
    store.getState().setExpanded('src', true);
    expect(store.getState().expanded).toBe(opened);
    store.getState().setExpanded('src', false);
    store.getState().toggleExpanded('src');
    store.getState().toggleExpanded('src');
    expect(initial.size).toBe(0);
    expect([...opened]).toEqual(['src']);
    expect(store.getState().expanded.size).toBe(0);
  });

  it('ignores late index events from a closed root', () => {
    store.getState().setRoot(root);
    const status: FolderIndexStatus = { root: root.root, indexRoot: root.root, state: 'ready', filesTotal: 1, filesIndexed: 1, passagesTotal: 2, passagesEmbedded: 2, passagesPerSecond: null, etaSeconds: null, message: null };
    store.getState().setIndexStatus(status, true);
    expect(store.getState().indexEvents).toBe(1);
    store.getState().setIndexStatus({ ...status, root: '/closed' }, true);
    expect(store.getState().indexStatus).toBe(status);
    expect(store.getState().indexEvents).toBe(1);
    store.getState().setIndexStatus(null);
    expect(store.getState().indexStatus).toBeNull();
  });

  it('keeps preferences usable when storage writes fail', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new DOMException('Quota exceeded', 'QuotaExceededError'); });
    store.getState().setRoot(root);
    store.getState().rememberThread(root.root, 'thread');
    store.getState().rememberThread(root.root, 'thread');
    store.getState().forgetThread('/unknown');
    expect(store.getState().threadByRoot[root.root]).toBe('thread');
    store.getState().forgetThread(root.root);
    expect(store.getState().threadByRoot).toEqual({});
  });
});
