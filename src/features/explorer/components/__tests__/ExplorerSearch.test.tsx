import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useExplorerStore } from '@/features/explorer/stores/explorerStore';

import { ExplorerSearch } from '../ExplorerSearch';

const api = vi.hoisted(() => ({ explorerSearch: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const reveal = vi.fn();
const initialReveal = useExplorerStore.getState().reveal;

function renderSearch() {
  const onActiveChange = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  render(<ExplorerSearch root="/recipes" onActiveChange={onActiveChange} />, { wrapper });
  return { onActiveChange };
}

async function type(value: string) {
  fireEvent.change(screen.getByRole('searchbox', { name: 'Search in folder' }), { target: { value } });
  await settle();
}

/** Past the debounce, then let the search reply and its render land. */
async function settle() {
  for (let step = 0; step < 3; step += 1) {
    await act(async () => { await vi.advanceTimersByTimeAsync(300); });
  }
}

beforeEach(() => {
  vi.useFakeTimers();
  reveal.mockReset();
  useExplorerStore.setState({ reveal });
  api.explorerSearch.mockReset();
});
afterEach(() => {
  useExplorerStore.setState({ reveal: initialReveal });
  vi.useRealTimers();
});

describe('ExplorerSearch', () => {
  it('searches once typing settles and opens a match on its line', async () => {
    api.explorerSearch.mockResolvedValue({
      ok: true,
      data: { matches: [{ path: 'breads/rye.md', line: 12, column: 3, preview: '   Rye needs a longer proof. ' }], filesScanned: 40, truncated: true },
    });
    const { onActiveChange } = renderSearch();
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
    await type('  proof ');
    expect(api.explorerSearch).toHaveBeenCalledWith('/recipes', 'proof', { maxResults: 200 });
    expect(onActiveChange).toHaveBeenLastCalledWith(true);
    expect(screen.getByText('breads/rye.md:12')).toBeInTheDocument();
    expect(screen.getByText('Rye needs a longer proof.')).toBeInTheDocument();
    expect(screen.getByText('More matches than shown. Narrow the search.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /breads\/rye\.md:12/ }));
    expect(reveal).toHaveBeenCalledWith('breads/rye.md', { startLine: 12, endLine: 12 });
  });

  it('waits for two characters before searching', async () => {
    const { onActiveChange } = renderSearch();
    await type('a');
    expect(api.explorerSearch).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('Search results')).not.toBeInTheDocument();
    expect(onActiveChange).not.toHaveBeenCalledWith(true);
  });

  it('shows searching, no matches and errors', async () => {
    let finish!: (value: unknown) => void;
    api.explorerSearch.mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    renderSearch();
    await type('yeast');
    expect(screen.getByText('Searching…')).toBeInTheDocument();
    await act(async () => { finish({ ok: true, data: { matches: [], filesScanned: 1200, truncated: false } }); });
    await settle();
    expect(screen.getByText(`No matches in ${(1200).toLocaleString()} files.`)).toBeInTheDocument();

    api.explorerSearch.mockResolvedValue({ ok: false, error: 'Invalid pattern' });
    await type('yeast(');
    expect(screen.getByRole('alert')).toHaveTextContent('Invalid pattern');
  });

  it('clears the query with the button or Escape', async () => {
    api.explorerSearch.mockResolvedValue({ ok: true, data: { matches: [], filesScanned: 1, truncated: false } });
    const { onActiveChange } = renderSearch();
    await type('salt');
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    await act(async () => { await vi.advanceTimersByTimeAsync(300); });
    expect(screen.getByRole('searchbox')).toHaveValue('');
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole('button', { name: 'Clear search' })).not.toBeInTheDocument();

    await type('salt');
    fireEvent.keyDown(screen.getByRole('searchbox'), { key: 'Enter' });
    expect(screen.getByRole('searchbox')).toHaveValue('salt');
    fireEvent.keyDown(screen.getByRole('searchbox'), { key: 'Escape' });
    expect(screen.getByRole('searchbox')).toHaveValue('');
  });
});
