import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useExplorerStore } from '@/stores/explorerStore';

import { ExplorerTree } from '../ExplorerTree';
import { ScopeBar } from '../ScopeBar';

const mocks = vi.hoisted(() => ({ listDir: vi.fn() }));
vi.mock('@/lib/api', () => ({ VaultAPI: { explorerListDir: mocks.listDir } }));

const ROOT = '/Users/me/project';
const entry = (path: string, kind: 'directory' | 'file', ignored = false) => ({
  name: path.split('/').pop()!, path, kind, size: kind === 'file' ? 10 : null, ignored,
});
const LISTINGS: Record<string, ReturnType<typeof entry>[]> = {
  '': [entry('src', 'directory'), entry('target', 'directory', true), entry('README.md', 'file')],
  src: [entry('src/main.rs', 'file')],
};

function showTree() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><ExplorerTree root={ROOT} /></QueryClientProvider>);
}

describe('ScopeBar', () => {
  it('names the folder and offers only a way out of it', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<ScopeBar root={{ root: ROOT, name: 'project' }} onClose={onClose} />);
    expect(screen.getByText('project')).toBeInTheDocument();
    expect(screen.getByText(ROOT)).toBeInTheDocument();
    // No breadcrumb to widen the scope, no picker to swap it.
    expect(screen.getAllByRole('button')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: 'Close folder' }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('hides and shows the folder tree', async () => {
    const user = userEvent.setup();
    const onToggleTree = vi.fn();
    const { rerender } = render(<ScopeBar root={{ root: ROOT, name: 'project' }} onClose={vi.fn()} onToggleTree={onToggleTree} />);
    await user.click(screen.getByRole('button', { name: 'Hide folder tree' }));
    expect(onToggleTree).toHaveBeenCalledOnce();

    rerender(<ScopeBar root={{ root: ROOT, name: 'project' }} onClose={vi.fn()} treeHidden onToggleTree={onToggleTree} />);
    expect(screen.getByRole('button', { name: 'Show folder tree' })).toHaveAttribute('aria-pressed', 'true');
  });
});

describe('explorerStore.reveal', () => {
  beforeEach(() => useExplorerStore.setState({ expanded: new Set(), openPath: null, selection: null, highlight: null }));

  it('opens the file, expands its folders and lights the lines', () => {
    useExplorerStore.getState().reveal('src/features/a.rs', { startLine: 3, endLine: 6 });
    const state = useExplorerStore.getState();
    expect(state.openPath).toBe('src/features/a.rs');
    expect([...state.expanded].sort()).toEqual(['src', 'src/features']);
    expect(state.highlight).toMatchObject({ path: 'src/features/a.rs', range: { startLine: 3, endLine: 6 } });
  });

  it('keeps the selection when the reference is in the open file', () => {
    useExplorerStore.setState({ openPath: 'a.rs', selection: { startLine: 1, endLine: 2 } });
    useExplorerStore.getState().reveal('a.rs', { startLine: 9, endLine: 9 });
    expect(useExplorerStore.getState().selection).toEqual({ startLine: 1, endLine: 2 });
  });
});

describe('ExplorerTree', () => {
  beforeEach(() => {
    // jsdom does not lay out, so it has no scrolling to do.
    Element.prototype.scrollIntoView = vi.fn();
    useExplorerStore.setState({ expanded: new Set(), openPath: null, selection: null, highlight: null });
    mocks.listDir.mockImplementation(async (_root: string, path: string) => ({ ok: true, data: { root: ROOT, path, entries: LISTINGS[path] ?? [] } }));
  });

  it('dims ignored entries and moves through the folder with the keyboard', async () => {
    const user = userEvent.setup();
    showTree();
    const target = await screen.findByRole('treeitem', { name: /target/ });
    expect(target).toHaveClass('opacity-50');

    screen.getByRole('tree').focus();
    await user.keyboard('{ArrowDown}{ArrowRight}');
    expect(await screen.findByRole('treeitem', { name: /main\.rs/ })).toBeInTheDocument();
    expect(mocks.listDir).toHaveBeenCalledWith(ROOT, 'src');

    await user.keyboard('{ArrowRight}{Enter}');
    expect(useExplorerStore.getState().openPath).toBe('src/main.rs');

    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    await waitFor(() => expect(screen.queryByRole('treeitem', { name: /main\.rs/ })).not.toBeInTheDocument());
  });

  it('browses inside the scope without offering to move it', async () => {
    showTree();
    const src = await screen.findByRole('treeitem', { name: /src/ });
    expect(screen.queryByRole('button', { name: /scope/i })).not.toBeInTheDocument();
    src.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});

describe('explorerStore back and forward', () => {
  beforeEach(() => useExplorerStore.setState({ expanded: new Set(), openPath: null, back: [], forward: [], selection: null, highlight: null }));

  it('steps back and forward through the files opened', () => {
    const store = useExplorerStore.getState();
    store.openFile('a.rs');
    store.openFile('src/b.rs');
    store.openFile('c.rs');

    useExplorerStore.getState().goBack();
    expect(useExplorerStore.getState().openPath).toBe('src/b.rs');
    expect(useExplorerStore.getState().expanded.has('src')).toBe(true);
    useExplorerStore.getState().goBack();
    expect(useExplorerStore.getState().openPath).toBe('a.rs');
    useExplorerStore.getState().goBack();
    expect(useExplorerStore.getState().openPath).toBe('a.rs');

    useExplorerStore.getState().goForward();
    useExplorerStore.getState().goForward();
    expect(useExplorerStore.getState().openPath).toBe('c.rs');
    expect(useExplorerStore.getState().forward).toEqual([]);
  });

  it('drops the forward files when another file is opened', () => {
    const store = useExplorerStore.getState();
    store.openFile('a.rs');
    store.openFile('b.rs');
    useExplorerStore.getState().goBack();
    useExplorerStore.getState().openFile('c.rs');
    expect(useExplorerStore.getState().forward).toEqual([]);
    expect(useExplorerStore.getState().back).toEqual(['a.rs']);
  });

  it('remembers the file a line link left', () => {
    useExplorerStore.getState().openFile('a.rs');
    useExplorerStore.getState().reveal('b.rs', { startLine: 1, endLine: 2 });
    useExplorerStore.getState().goBack();
    expect(useExplorerStore.getState().openPath).toBe('a.rs');
  });

  it('starts over in a new folder', () => {
    useExplorerStore.getState().openFile('a.rs');
    useExplorerStore.getState().openFile('b.rs');
    useExplorerStore.getState().setRoot({ root: '/elsewhere', name: 'elsewhere' });
    expect(useExplorerStore.getState().back).toEqual([]);
  });
});
