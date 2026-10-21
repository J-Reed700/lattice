import { useRef } from 'react';

import { AnimatePresence, motion } from 'framer-motion';
import { Activity, X } from 'lucide-react';

import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import type { ActivityEntryDto } from '@/lib/bindings';

import { ActivityRow } from './ActivityRow';
import { useOpenActivity } from '../hooks/useOpenActivity';
import { activitySummary } from '../model/activityFormat';


interface ActivityDrawerProps {
  open: boolean;
  onClose: () => void;
  items: readonly ActivityEntryDto[];
  /** When the snapshot was read (Unix ms): retry times are told against it. */
  readAt: number;
  /** The snapshot could not be read. */
  failed?: boolean;
}

/**
 * Slide-over listing everything running, paused or waiting, in the
 * Downloads drawer's pattern: Escape and a click outside close it, focus
 * moves into it on open and back on close.
 */
export function ActivityDrawer({ open, onClose, items, readAt, failed = false }: ActivityDrawerProps) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const openItem = useOpenActivity(onClose);
  const summary = failed ? 'Could not read what is running.' : activitySummary(items);
  const empty = items.length === 0 && !failed;

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onClose(); }}>
      <AnimatePresence>
        {open && (
          <DialogContent
            forceMount
            unstyled
            hideClose
            asChild
            aria-describedby={undefined}
            overlayClassName="overlay-fade fixed inset-0 z-40 bg-overlay"
            onOpenAutoFocus={(event) => { event.preventDefault(); panelRef.current?.focus(); }}
          >
            <motion.div
              ref={panelRef}
              tabIndex={-1}
              className="fixed right-0 top-0 z-50 flex h-full w-full max-w-sm flex-col border-l border-border-subtle bg-bg shadow-md outline-hidden"
              initial={{ x: '100%' }}
              animate={{ x: 0 }}
              exit={{ x: '100%' }}
              transition={{ type: 'tween', duration: 0.2, ease: 'easeOut' }}
            >
              <header className="flex items-center gap-2 border-b border-border-subtle px-4 py-3">
                <Activity className="h-4 w-4 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
                <DialogTitle asChild>
                  <h2 className="flex-1 font-serif text-sm font-semibold text-text-primary">Activity</h2>
                </DialogTitle>
                <button
                  type="button"
                  onClick={onClose}
                  aria-label="Close activity"
                  className="flex h-8 w-8 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-surface-raised hover:text-text-primary"
                >
                  <X className="h-4 w-4" strokeWidth={1.75} />
                </button>
              </header>

              {/* Announced as it changes; while nothing runs the empty state shows it instead. */}
              <p
                role="status"
                aria-live="polite"
                className={empty ? 'sr-only' : 'border-b border-border-subtle px-4 py-2 text-xs text-text-secondary'}
              >
                {summary}
              </p>

              <div className="flex-1 overflow-y-auto p-3">
                {empty ? (
                  <div className="flex h-full flex-col items-center justify-center px-6 text-center">
                    <Activity className="h-6 w-6 text-text-muted" strokeWidth={1.5} aria-hidden="true" />
                    <p aria-hidden="true" className="mt-3 text-sm text-text-primary">Nothing is running.</p>
                    <p className="mt-1 text-xs text-text-muted">
                      Lessons, indexes, model requests and downloads appear here while they run.
                    </p>
                  </div>
                ) : (
                  <ul aria-label="Running, paused and waiting work" className="space-y-2">
                    {items.map((item) => (
                      <ActivityRow key={item.id} item={item} now={readAt} onOpen={openItem} />
                    ))}
                  </ul>
                )}
              </div>
            </motion.div>
          </DialogContent>
        )}
      </AnimatePresence>
    </Dialog>
  );
}
