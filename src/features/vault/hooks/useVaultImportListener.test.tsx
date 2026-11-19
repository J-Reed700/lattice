import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { WORKSPACE_NOTES_QUERY_KEY } from '@/features/journal/api/queries';

import { useVaultImportListener } from './useVaultImportListener';
import { useVaultImportStore } from '../stores/vaultImportStore';

const events = vi.hoisted(() => ({
  listen: vi.fn(),
  unlisten: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: events.listen }));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={client}>{children}</QueryClientProvider>
);

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  events.handlers.clear();
  events.unlisten.mockReset();
  events.listen.mockReset().mockImplementation(async (name: string, handler: (event: { payload: unknown }) => void) => {
    events.handlers.set(name, handler);
    return events.unlisten;
  });
  useVaultImportStore.setState({ importTick: 0, lastImportedId: null });
});
afterEach(() => vi.restoreAllMocks());

describe('useVaultImportListener', () => {
  it('bumps the import counter and invalidates workspace notes for each imported note', async () => {
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries');
    const { unmount } = renderHook(() => useVaultImportListener(), { wrapper });
    await waitFor(() => expect(events.handlers.has('vault:note-imported')).toBe(true));
    act(() => events.handlers.get('vault:note-imported')!({ payload: { noteId: 'note-7' } }));
    expect(useVaultImportStore.getState()).toMatchObject({ importTick: 1, lastImportedId: 'note-7' });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: WORKSPACE_NOTES_QUERY_KEY });
    unmount();
    expect(events.unlisten).toHaveBeenCalledTimes(1);
  });

  it('ignores a malformed payload', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const invalidateSpy = vi.spyOn(client, 'invalidateQueries');
    renderHook(() => useVaultImportListener(), { wrapper });
    await waitFor(() => expect(events.handlers.has('vault:note-imported')).toBe(true));
    act(() => events.handlers.get('vault:note-imported')!({ payload: { id: 7 } }));
    expect(useVaultImportStore.getState().importTick).toBe(0);
    expect(invalidateSpy).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledWith('[useVaultImportListener] payload validation failed:', expect.anything());
  });

  it('drops a subscription that lands after unmount, and logs one that fails', async () => {
    let attach!: (unlisten: () => void) => void;
    events.listen.mockReturnValueOnce(new Promise((resolve) => { attach = resolve; }));
    const { unmount } = renderHook(() => useVaultImportListener(), { wrapper });
    unmount();
    const late = vi.fn();
    await act(async () => { attach(late); });
    expect(late).toHaveBeenCalledTimes(1);

    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    events.listen.mockRejectedValueOnce(new Error('event bus closed'));
    renderHook(() => useVaultImportListener(), { wrapper });
    await waitFor(() => expect(error).toHaveBeenCalledWith('[useVaultImportListener] failed to subscribe:', expect.any(Error)));
  });
});
