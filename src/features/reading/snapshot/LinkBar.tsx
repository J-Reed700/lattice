/**
 * The bar that slides in when a link inside a frozen page is clicked.
 *
 * The frozen page never navigates (the scheme answers a link with 204); the
 * backend reports the click and this bar offers the two things a click could
 * have meant. Nothing opens without a second, deliberate click here.
 */

import { useEffect, useMemo, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { ArrowUpRight, X } from 'lucide-react';

import { EventSchemas, listenValidated, TauriEventNames } from '@/types/events';
import { openExternalUrl } from '@/utils/openExternalUrl';

import { listWebPageSnapshots, type WebPageSnapshotDto } from './api';
import { normalizeSnapshotUrl, splitUrl } from './snapshotFrameUrl';
import { webPageSnapshotListQueryKey } from './useWebPageSnapshot';

/** How long the bar waits for a decision before it goes away by itself. */
export const LINK_BAR_TIMEOUT_MS = 12_000;

export interface LinkBarProps {
  conversationId: string;
  /** The snapshot on screen; clicks reported for any other one are not ours. */
  snapshotId: string;
  onOpenFrozenCopy: (_snapshot: WebPageSnapshotDto) => void;
}

interface ClickedLink {
  url: string;
  /** Bumped per click, so the same link clicked twice restarts the timer. */
  seq: number;
}

function sameUrl(snapshot: WebPageSnapshotDto, target: string): boolean {
  return (
    snapshot.normalizedUrl === target ||
    normalizeSnapshotUrl(snapshot.requestedUrl) === target ||
    (snapshot.finalUrl !== null && normalizeSnapshotUrl(snapshot.finalUrl) === target)
  );
}

const barButton =
  'inline-flex h-6 shrink-0 items-center rounded-md px-2 text-xs font-medium transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring';

export function LinkBar({ conversationId, snapshotId, onOpenFrozenCopy }: LinkBarProps) {
  const [link, setLink] = useState<ClickedLink | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenValidated(TauriEventNames.WebPageSnapshot.Link, EventSchemas.WebPageSnapshot.Link, ({ payload }) => {
      if (payload.snapshotId !== snapshotId) return;
      setLink((previous) => ({ url: payload.url, seq: (previous?.seq ?? 0) + 1 }));
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [snapshotId]);

  // Is there a frozen copy of the clicked page in this conversation? Without
  // the list the bar still offers the browser.
  const target = link ? normalizeSnapshotUrl(link.url) : null;
  const snapshotsQuery = useQuery({
    queryKey: webPageSnapshotListQueryKey(conversationId),
    queryFn: () => listWebPageSnapshots(conversationId),
    enabled: target !== null,
    retry: false,
    staleTime: 0,
  });
  const match = useMemo(
    () =>
      target
        ? snapshotsQuery.data?.find(
            (snapshot) => snapshot.status === 'ready' && snapshot.id !== snapshotId && sameUrl(snapshot, target),
          ) ?? null
        : null,
    [snapshotId, snapshotsQuery.data, target],
  );

  useEffect(() => {
    if (!link) return;
    const timer = window.setTimeout(() => setLink(null), LINK_BAR_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [link]);

  if (!link) return null;
  const { host, path } = splitUrl(link.url);

  return (
    <div
      role="status"
      className="flex shrink-0 items-center gap-2 border-b border-subtle bg-surface-raised px-3 py-1.5 text-xs animate-in fade-in-0 slide-in-from-top-1 duration-fast"
    >
      <ArrowUpRight aria-hidden size={13} className="shrink-0 text-[hsl(var(--text-muted))]" />
      <span className="min-w-0 flex-1 truncate text-[hsl(var(--text-secondary))]">
        This page links to{' '}
        <span className="text-[hsl(var(--text-primary))]">
          {host}
          {path}
        </span>
      </span>
      <button
        type="button"
        onClick={() => {
          void openExternalUrl(link.url);
          setLink(null);
        }}
        className={`${barButton} text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--text-primary)/0.07)]`}
      >
        Open in browser
      </button>
      {match ? (
        <button
          type="button"
          onClick={() => {
            onOpenFrozenCopy(match);
            setLink(null);
          }}
          className={`${barButton} bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))]`}
        >
          Open frozen copy
        </button>
      ) : null}
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => setLink(null)}
        className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[hsl(var(--text-muted))] transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.07)] hover:text-[hsl(var(--text-primary))]"
      >
        <X aria-hidden size={13} />
      </button>
    </div>
  );
}
