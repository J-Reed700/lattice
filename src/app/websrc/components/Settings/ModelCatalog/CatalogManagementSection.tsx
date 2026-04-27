/**
 * CatalogManagementSection
 *
 * Cache management controls with refresh and clear cache buttons
 */

import { useState } from 'react';

import { RefreshCw, Trash2, Database } from 'lucide-react';

import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import { Button } from '../../ui/button';
import Card, { CardHeader, CardTitle, CardContent } from '../../ui/Card/Card';

export function CatalogManagementSection() {
  const cacheStats = useModelCatalogStore((state) => state.cacheStats);
  const refreshCatalog = useModelCatalogStore((state) => state.refreshCatalog);
  const clearCache = useModelCatalogStore((state) => state.clearCache);
  const loadCacheStats = useModelCatalogStore((state) => state.loadCacheStats);

  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isClearing, setIsClearing] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await refreshCatalog();
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleClearCache = async () => {
    setIsClearing(true);
    try {
      await clearCache();
      await loadCacheStats();
      setShowConfirm(false);
    } finally {
      setIsClearing(false);
    }
  };

  return (
    <Card padding="md" className="rounded-xl">
      <CardHeader>
        <div className="flex items-center gap-2">
          <Database className="w-4 h-4 text-[hsl(var(--accent))]" />
          <CardTitle className="text-base">Catalog Management</CardTitle>
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-4 mt-4">
          {/* Cache Stats */}
          {cacheStats && (
            <div className="p-3 bg-[hsl(var(--surface))] rounded-lg">
              <div className="grid grid-cols-3 gap-4 text-center">
                <div>
                  <div className="text-xs text-[hsl(var(--text-secondary))] mb-1">Total</div>
                  <div className="text-lg font-semibold text-[hsl(var(--text-primary))]">
                    {cacheStats.total_entries}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--text-secondary))] mb-1">Valid</div>
                  <div className="text-lg font-semibold text-[hsl(var(--success-fg))]">
                    {cacheStats.valid_entries}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--text-secondary))] mb-1">Expired</div>
                  <div className="text-lg font-semibold text-[hsl(var(--warning-fg))]">
                    {cacheStats.expired_entries}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="space-y-2">
            <Button
              variant="outline"
              onClick={handleRefresh}
              disabled={isRefreshing}
              className="w-full justify-center"
            >
              <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
              {isRefreshing ? 'Refreshing...' : 'Refresh Catalog'}
            </Button>

            {!showConfirm ? (
              <Button
                variant="destructive"
                onClick={() => setShowConfirm(true)}
                disabled={isClearing}
                className="w-full justify-center"
              >
                <Trash2 className="w-4 h-4" />
                Clear Cache
              </Button>
            ) : (
              <div className="grid grid-cols-2 gap-2">
                <Button
                  variant="destructive"
                  onClick={handleClearCache}
                  disabled={isClearing}
                  className="w-full justify-center"
                >
                  {isClearing ? 'Clearing...' : 'Confirm'}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => setShowConfirm(false)}
                  disabled={isClearing}
                  className="w-full justify-center"
                >
                  Cancel
                </Button>
              </div>
            )}
          </div>

          {/* Info */}
          <p className="text-xs text-[hsl(var(--text-secondary))] leading-relaxed">
            Refreshing updates the catalog with the latest models. Clearing the cache removes all
            cached search results and forces fresh data retrieval.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}
