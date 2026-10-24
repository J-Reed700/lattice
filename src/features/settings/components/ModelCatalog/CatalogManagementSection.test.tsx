import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { CatalogManagementSection } from './CatalogManagementSection';

const h = vi.hoisted(() => ({ catalog: {} as Record<string, any> }));
vi.mock('@/features/model/hooks/useModelCatalog', () => ({ useModelCatalog: () => h.catalog }));

describe('CatalogManagementSection', () => {
  beforeEach(() => {
    h.catalog = {
      cacheStats: { total_entries: 42, expired_entries: 0 },
      refreshCatalog: vi.fn().mockResolvedValue(undefined),
      clearCache: vi.fn().mockResolvedValue(undefined),
      loadCacheStats: vi.fn().mockResolvedValue(undefined),
    };
  });

  it('shows how many models are cached, and how many expired', () => {
    const first = render(<CatalogManagementSection />);
    expect(screen.getByText('42')).toBeInTheDocument();
    first.unmount();
    h.catalog.cacheStats = { total_entries: 42, expired_entries: 5 };
    const second = render(<CatalogManagementSection />);
    expect(screen.getByText('42 · 5 expired')).toBeInTheDocument();
    second.unmount();
    h.catalog.cacheStats = null;
    render(<CatalogManagementSection />);
    expect(screen.getByText('Loading…')).toBeInTheDocument();
  });

  it('refreshes the catalog, showing progress', async () => {
    const pending = deferred<void>();
    h.catalog.refreshCatalog.mockReturnValueOnce(pending.promise);
    render(<CatalogManagementSection />);
    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(screen.getByRole('button', { name: 'Refreshing…' })).toBeDisabled();
    await act(async () => { pending.resolve(); await pending.promise; });
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeEnabled();
  });

  it('clears the cache after confirmation and reloads the count', async () => {
    render(<CatalogManagementSection />);
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByText('Clear the catalog cache?')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: /^Clear/ }));
    await waitFor(() => expect(h.catalog.loadCacheStats).toHaveBeenCalled());
    expect(h.catalog.clearCache).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('keeps the cache when the confirmation is cancelled', async () => {
    render(<CatalogManagementSection />);
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(h.catalog.clearCache).not.toHaveBeenCalled();
  });
});
