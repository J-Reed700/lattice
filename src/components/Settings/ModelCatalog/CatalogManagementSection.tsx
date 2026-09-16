/**
 * CatalogManagementSection
 *
 * The catalog's local cache: how many entries it holds, and the two actions
 * that change that.
 */

import { useState } from 'react';

import { cn } from '@/lib/utils';

import { useModelCatalog } from '../../../hooks/useModelCatalog';
import { ConfirmDialog } from '../../ConfirmDialog';
import { SettingsRow, SettingsSection } from '../../ui';
import { GHOST_BUTTON_CLASS } from '../settingsStyles';

export function CatalogManagementSection() {
  const { cacheStats, refreshCatalog, clearCache, loadCacheStats } = useModelCatalog({
    autoLoadCapabilities: false,
    autoLoadModels: false,
    loadCacheStats: true,
  });

  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isConfirmingClear, setIsConfirmingClear] = useState(false);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await refreshCatalog();
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleClearCache = async () => {
    await clearCache();
    await loadCacheStats();
    setIsConfirmingClear(false);
  };

  const cachedLabel = cacheStats
    ? cacheStats.expired_entries > 0
      ? `${cacheStats.total_entries} · ${cacheStats.expired_entries} expired`
      : `${cacheStats.total_entries}`
    : null;

  return (
    <>
      <SettingsSection
        title="Cache"
        actions={
          <>
            <button
              type="button"
              onClick={() => void handleRefresh()}
              disabled={isRefreshing}
              className={GHOST_BUTTON_CLASS}
            >
              {isRefreshing ? 'Refreshing…' : 'Refresh'}
            </button>
            <button
              type="button"
              onClick={() => setIsConfirmingClear(true)}
              className={cn(GHOST_BUTTON_CLASS, 'text-danger-fg hover:text-danger-fg')}
            >
              Clear
            </button>
          </>
        }
      >
        <SettingsRow label="Cached models">
          {cachedLabel ? (
            <span className="text-sm tabular-nums text-text-secondary">{cachedLabel}</span>
          ) : (
            <span className="text-sm text-text-muted">Loading…</span>
          )}
        </SettingsRow>
      </SettingsSection>

      <ConfirmDialog
        isOpen={isConfirmingClear}
        title="Clear the catalog cache?"
        message="Catalog results will be fetched again the next time you search."
        variant="warning"
        confirmLabel="Clear"
        onConfirm={handleClearCache}
        onCancel={() => setIsConfirmingClear(false)}
      />
    </>
  );
}
