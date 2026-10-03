import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useExplorerStore } from '@/stores/explorerStore';

import { ExplorerFileView } from '../ExplorerFileView';

const mocks = vi.hoisted(() => ({ read: vi.fn(), locate: vi.fn() }));
vi.mock('@/lib/api', () => ({ VaultAPI: { explorerReadFile: mocks.read, explorerLocateFile: mocks.locate } }));

const ROOT = '/Users/me/MusicVST';
const FILES: Record<string, string> = {
  'contracts/effect_api.hpp': '#pragma once\nstruct Effect {};\n',
  'src/util.hpp': '// one\n',
  'modules/engine/util.hpp': '// two\n',
};

function showView() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><ExplorerFileView root={ROOT} /></QueryClientProvider>);
}

describe('a cited path that is not a file', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useExplorerStore.setState({ expanded: new Set(), openPath: null, back: [], forward: [], selection: null, highlight: null, aliases: {} });
    mocks.read.mockImplementation(async (_root: string, path: string) => {
      const text = FILES[path];
      return text === undefined
        ? { ok: false, error: `Not found: No such file or folder: ${path}` }
        : { ok: true, data: { path, text, language: 'cpp', lineCount: text.split('\n').length - 1, sizeBytes: text.length, binary: false, tooLarge: false } };
    });
  });

  it('opens the one file it can mean, at the same lines', async () => {
    mocks.locate.mockResolvedValue({ ok: true, data: ['contracts/effect_api.hpp'] });
    useExplorerStore.getState().openFile('src/main.cpp');
    useExplorerStore.getState().reveal('effect_api.hpp', { startLine: 2, endLine: 2 });
    showView();

    await waitFor(() => expect(useExplorerStore.getState().openPath).toBe('contracts/effect_api.hpp'));
    const state = useExplorerStore.getState();
    expect(state.highlight).toMatchObject({ path: 'contracts/effect_api.hpp', range: { startLine: 2, endLine: 2 } });
    expect(state.expanded.has('contracts')).toBe(true);
    // Back goes to the file before the link, not to the path that was never there.
    expect(state.back).toEqual(['src/main.cpp']);
    expect(mocks.locate).toHaveBeenCalledWith(ROOT, 'effect_api.hpp');
  });

  it('opens the same link again without looking for it again', async () => {
    mocks.locate.mockResolvedValue({ ok: true, data: ['contracts/effect_api.hpp'] });
    useExplorerStore.getState().reveal('effect_api.hpp', { startLine: 2, endLine: 2 });
    showView();
    await waitFor(() => expect(useExplorerStore.getState().openPath).toBe('contracts/effect_api.hpp'));

    useExplorerStore.getState().openFile('src/util.hpp');
    useExplorerStore.getState().reveal('effect_api.hpp', { startLine: 1, endLine: 1 });

    const state = useExplorerStore.getState();
    expect(state.openPath).toBe('contracts/effect_api.hpp');
    expect(state.highlight).toMatchObject({ path: 'contracts/effect_api.hpp', range: { startLine: 1, endLine: 1 } });
    expect(state.back).toEqual(['contracts/effect_api.hpp', 'src/util.hpp']);
    expect(await screen.findByText('struct Effect {};')).toBeInTheDocument();
    expect(mocks.locate).toHaveBeenCalledTimes(1);
    expect(mocks.read.mock.calls.filter(([, path]) => path === 'effect_api.hpp')).toHaveLength(1);
    expect(mocks.read.mock.calls.filter(([, path]) => path === 'contracts/effect_api.hpp')).toHaveLength(1);
  });

  it('asks which file when the name fits several', async () => {
    const user = userEvent.setup();
    mocks.locate.mockResolvedValue({ ok: true, data: ['src/util.hpp', 'modules/engine/util.hpp'] });
    useExplorerStore.getState().reveal('util.hpp', { startLine: 1, endLine: 1 });
    showView();

    expect(await screen.findByText(/isn’t a path in this folder/)).toBeInTheDocument();
    expect(useExplorerStore.getState().openPath).toBe('util.hpp');
    await user.click(screen.getByRole('button', { name: 'modules/engine/util.hpp' }));
    expect(useExplorerStore.getState().openPath).toBe('modules/engine/util.hpp');
    expect(useExplorerStore.getState().highlight?.path).toBe('modules/engine/util.hpp');

    // The pick stands for the next link to the same name.
    useExplorerStore.getState().openFile('src/util.hpp');
    useExplorerStore.getState().reveal('util.hpp', { startLine: 1, endLine: 1 });
    expect(useExplorerStore.getState().openPath).toBe('modules/engine/util.hpp');
    expect(mocks.locate).toHaveBeenCalledTimes(1);
  });

  it('says so when nothing in the folder has that name', async () => {
    mocks.locate.mockResolvedValue({ ok: true, data: [] });
    useExplorerStore.getState().openFile('missing.cpp');
    showView();

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Not found: No such file or folder: missing.cpp Nothing else in this folder has that name.'
    );
  });
});
