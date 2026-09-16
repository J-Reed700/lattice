import { useMemo } from 'react';

import { Download } from 'lucide-react';

import { useDownloadState } from '../../hooks/useDownloadState';

/**
 * Sidebar-rail affordance for the downloads drawer.
 *
 * Renders nothing when the store is empty so the rail stays clean for users who
 * have never downloaded a model. Sized to match `NavButton` (44×44 minimum tap
 * target) so it sits flush in the nav column.
 */
export function HeaderDownloadsIndicator() {
  const {
    downloadsMap: downloads,
    activeDownloadsSet: activeDownloads,
    isDrawerOpen,
    toggleDrawer,
  } = useDownloadState();

  const hasFailure = useMemo(
    () => Array.from(downloads.values()).some((d) => d.state === 'Failed'),
    [downloads]
  );

  const totalCount = downloads.size;
  const activeCount = activeDownloads.size;

  if (totalCount === 0) return null;

  const badgeCount = activeCount > 0 ? activeCount : null;
  const label =
    activeCount > 0
      ? `Downloads (${activeCount} active)`
      : hasFailure
        ? 'Downloads (has failures)'
        : 'Downloads';

  return (
    <button
      type="button"
      onClick={toggleDrawer}
      title={label}
      aria-label={label}
      aria-expanded={isDrawerOpen}
      className={`
        relative flex min-h-[44px] min-w-[44px] items-center justify-center rounded-md p-3
        transition-colors duration-fast
        ${
          isDrawerOpen
            ? 'bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))]'
            : 'text-[hsl(var(--text-muted))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
        }
      `}
    >
      <Download
        className={`h-[18px] w-[18px] ${activeCount > 0 ? 'animate-pulse' : ''}`}
        strokeWidth={1.75}
      />

      {badgeCount !== null && (
        <span className="absolute right-1 top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-[hsl(var(--accent))] px-1 text-[10px] font-medium leading-none text-[hsl(var(--bg))]">
          {badgeCount > 9 ? '9+' : badgeCount}
        </span>
      )}

      {badgeCount === null && hasFailure && (
        <span
          className="absolute right-1.5 top-1.5 h-2 w-2 rounded-full bg-[hsl(var(--danger,0_70%_50%))]"
          aria-hidden="true"
        />
      )}
    </button>
  );
}
