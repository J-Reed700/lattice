import { useMemo } from 'react';

import { AnimatePresence, motion } from 'framer-motion';
import { Download } from 'lucide-react';

import { normalizePercentage } from './downloadFormat';
import { useDownloadState } from '../../hooks/useDownloadState';

/**
 * Floating pill showing aggregate progress while downloads are in flight.
 *
 * Appears only when there is active work and the drawer is closed — once the
 * drawer is open it would be redundant, and with nothing downloading it would be
 * clutter. Clicking it opens the drawer.
 */
export function DrawerTrigger() {
  const {
    downloadsMap: downloads,
    activeDownloadsSet: activeDownloads,
    isDrawerOpen,
    openDrawer,
  } = useDownloadState();

  // Aggregate across active downloads only. Entries with an unknown total are
  // excluded from the ratio rather than counted as zero, which would make the
  // bar appear to go backwards when a sizeless download joins the queue.
  const { activeCount, percentage } = useMemo(() => {
    let downloaded = 0;
    let total = 0;
    let count = 0;

    for (const id of activeDownloads) {
      const download = downloads.get(id);
      if (!download) continue;
      count += 1;
      if (download.total_bytes !== null && download.total_bytes > 0) {
        downloaded += download.bytes_downloaded;
        total += download.total_bytes;
      }
    }

    return {
      activeCount: count,
      percentage: total > 0 ? normalizePercentage(null, downloaded, total) : null,
    };
  }, [downloads, activeDownloads]);

  const visible = activeCount > 0 && !isDrawerOpen;

  return (
    <AnimatePresence>
      {visible && (
        <motion.button
          type="button"
          onClick={openDrawer}
          initial={{ opacity: 0, y: 12 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          transition={{ duration: 0.18, ease: 'easeOut' }}
          aria-label={`Show downloads (${activeCount} active)`}
          className="fixed bottom-5 right-5 z-30 flex items-center gap-2.5 rounded-full border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] py-2 pl-3 pr-4 shadow-md transition-colors duration-fast hover:bg-[hsl(var(--surface-raised))]"
        >
          <Download
            className="h-4 w-4 shrink-0 animate-pulse text-[hsl(var(--accent))]"
            strokeWidth={1.75}
          />
          <span className="flex flex-col items-start">
            <span className="text-xs font-medium leading-tight text-[hsl(var(--text-primary))]">
              {activeCount} download{activeCount === 1 ? '' : 's'}
            </span>
            <span className="mt-1 block h-1 w-24 overflow-hidden rounded-full bg-[hsl(var(--surface-raised))]">
              {percentage === null ? (
                <span className="block h-full w-1/3 animate-pulse rounded-full bg-[hsl(var(--accent))] opacity-60" />
              ) : (
                <span
                  className="block h-full rounded-full bg-[hsl(var(--accent))] transition-[width] duration-300 ease-out"
                  style={{ width: `${percentage}%` }}
                />
              )}
            </span>
          </span>
        </motion.button>
      )}
    </AnimatePresence>
  );
}
