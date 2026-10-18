/**
 * The Page view: a frozen copy of a cited web page in a fully sandboxed frame,
 * with browser chrome around it (design §9).
 *
 * The frame's `sandbox` attribute is present and empty, always: the page can
 * run nothing and reach nothing. Its links answer 204 and surface in the link
 * bar; if the frame ever loads a second document anyway, it is put back.
 */

import { type KeyboardEvent, type ReactNode, useCallback, useEffect, useRef, useState } from 'react';

import { TooltipProvider } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { formatBytes } from '@/utils/formatters';
import { openExternalUrl } from '@/utils/openExternalUrl';

import { LinkBar } from './LinkBar';
import { snapshotFrameUrl, splitUrl } from './snapshotFrameUrl';
import { SnapshotToolbar } from './SnapshotToolbar';
import { useWebPageSnapshot } from './useWebPageSnapshot';
import { useWebPageSnapshotStore } from './webPageSnapshotStore';

import type { WebPageSnapshotDto } from './api';

export interface WebPageSnapshotViewProps {
  conversationId: string;
  /** The cited address. */
  url: string;
  /** Overlay or docked width: icons only, no status line, host-only pill. */
  compact?: boolean;
  /** The Reader | Page tabs, drawn at the start of the toolbar row. */
  toggle?: ReactNode;
  onShowReader?: () => void;
  autoCapture?: boolean;
}

const primaryButton =
  'inline-flex h-8 items-center rounded-md bg-[hsl(var(--accent))] px-3 text-sm font-medium text-[hsl(var(--accent-fg))] transition-colors duration-fast hover:bg-[hsl(var(--accent-hover))] disabled:cursor-not-allowed disabled:opacity-60';
const secondaryButton =
  'inline-flex h-8 items-center rounded-md border border-border-default bg-surface-raised px-3 text-sm font-medium text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-hover';

function SkeletonBars() {
  return (
    <div aria-hidden className="flex flex-col gap-3 p-6">
      <div className="h-3 w-2/3 animate-pulse rounded bg-[hsl(var(--text-primary)/0.08)]" />
      <div className="h-3 w-5/6 animate-pulse rounded bg-[hsl(var(--text-primary)/0.08)]" />
      <div className="h-3 w-1/2 animate-pulse rounded bg-[hsl(var(--text-primary)/0.08)]" />
    </div>
  );
}

function CenteredState({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-8 text-center text-sm">
      {children}
    </div>
  );
}

interface SnapshotFrameProps {
  src: string;
  host: string;
  zoom: number;
}

/** Keyed by `src` by its parent, so a new address starts with fresh load counting. */
function SnapshotFrame({ src, host, zoom }: SnapshotFrameProps) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const loadsRef = useRef(0);
  const restoringRef = useRef(false);
  const [loaded, setLoaded] = useState(false);

  const onLoad = () => {
    loadsRef.current += 1;
    if (loadsRef.current === 1) {
      setLoaded(true);
      return;
    }
    // The load that putting it back causes is expected; anything else is the
    // page having gone somewhere, and a frozen page stays where it was.
    if (restoringRef.current) {
      restoringRef.current = false;
      return;
    }
    restoringRef.current = true;
    if (frameRef.current) frameRef.current.src = src;
  };

  return (
    <div className="relative h-full overflow-hidden rounded-md border border-subtle bg-white">
      <iframe
        ref={frameRef}
        sandbox=""
        referrerPolicy="no-referrer"
        title={`Frozen copy of ${host}`}
        src={src}
        onLoad={onLoad}
        className="block border-0"
        style={{
          transform: `scale(${zoom})`,
          transformOrigin: '0 0',
          width: `calc(100% / ${zoom})`,
          height: `calc(100% / ${zoom})`,
        }}
      />
      {loaded ? null : (
        <div className="absolute inset-0 bg-white">
          <SkeletonBars />
        </div>
      )}
    </div>
  );
}

function statusLine(snapshot: WebPageSnapshotDto): { text: string; partial: string | null; failed: boolean } {
  if (snapshot.verified === false) {
    return { text: '✗ Integrity check failed', partial: null, failed: true };
  }
  const parts: string[] = [];
  if (snapshot.verified === true) parts.push('✓ Verified');
  if (snapshot.byteSize !== null) parts.push(formatBytes(snapshot.byteSize));
  const resources = `${snapshot.resourceCount} ${snapshot.resourceCount === 1 ? 'resource' : 'resources'}`;
  parts.push(
    snapshot.missingResourceCount > 0 ? `${resources}, ${snapshot.missingResourceCount} missing` : resources,
  );
  if (snapshot.method === 'http') parts.push('captured over HTTP');
  else if (snapshot.method === 'browser') parts.push('captured in browser');
  const partial =
    snapshot.partial && snapshot.partialReasons.length > 0
      ? `incomplete: ${snapshot.partialReasons.join(', ')}`
      : null;
  return { text: parts.join(' · '), partial, failed: false };
}

export function WebPageSnapshotView({
  conversationId,
  url,
  compact = false,
  toggle,
  onShowReader,
  autoCapture = false,
}: WebPageSnapshotViewProps) {
  const { snapshot: own, isLoading, isCapturing, capture, retry } = useWebPageSnapshot(conversationId, url);
  // "Open frozen copy" from the link bar shows another snapshot of this
  // conversation in place, until the reader moves to another citation.
  const [switched, setSwitched] = useState<{ forUrl: string; snapshot: WebPageSnapshotDto } | null>(null);
  const snapshot = switched?.forUrl === url ? switched.snapshot : own;

  const autoCapturedUrlRef = useRef<string | null>(null);

  useEffect(() => {
    if (!autoCapture || isLoading || snapshot || isCapturing || !url) return;
    if (autoCapturedUrlRef.current === url) return;
    autoCapturedUrlRef.current = url;
    capture();
  }, [autoCapture, isLoading, snapshot, isCapturing, url, capture]);

  const zoom = useWebPageSnapshotStore((state) => state.zoom);
  const clean = useWebPageSnapshotStore((state) => state.clean);
  const setClean = useWebPageSnapshotStore((state) => state.setClean);
  const zoomIn = useWebPageSnapshotStore((state) => state.zoomIn);
  const zoomOut = useWebPageSnapshotStore((state) => state.zoomOut);
  const resetZoom = useWebPageSnapshotStore((state) => state.resetZoom);

  const shownUrl = snapshot?.finalUrl ?? snapshot?.requestedUrl ?? url;
  const { host } = splitUrl(shownUrl);

  const copyLink = useCallback(() => {
    void navigator.clipboard?.writeText(shownUrl).catch(() => undefined);
  }, [shownUrl]);
  const openLive = useCallback(() => {
    void openExternalUrl(shownUrl);
  }, [shownUrl]);

  // Zoom keys only while focus is somewhere in this pane.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!(event.metaKey || event.ctrlKey) || snapshot?.status !== 'ready') return;
    if (event.key === '=' || event.key === '+') zoomIn();
    else if (event.key === '-') zoomOut();
    else if (event.key === '0') resetZoom();
    else return;
    event.preventDefault();
  };

  const renderBody = () => {
    if (isLoading) return <SkeletonBars />;

    if (!snapshot) {
      return (
        <CenteredState>
          <p className="text-[hsl(var(--text-primary))]">No frozen copy of this page yet.</p>
          <button type="button" onClick={capture} disabled={isCapturing} className={primaryButton}>
            Capture now
          </button>
          <p className="text-xs text-[hsl(var(--text-muted))]">The text the answer used is still in Reader.</p>
        </CenteredState>
      );
    }

    if (snapshot.status === 'pending') {
      return (
        <CenteredState>
          <p className="text-[hsl(var(--text-primary))]">Capturing the page…</p>
          {snapshot.progressMessage ? (
            <p className="text-xs text-[hsl(var(--text-muted))]">{snapshot.progressMessage}</p>
          ) : null}
          <div
            role="progressbar"
            aria-label="Capturing the page…"
            className="relative h-0.5 w-40 overflow-hidden rounded-full bg-[hsl(var(--text-primary)/0.08)]"
          >
            <div className="absolute inset-y-0 left-0 w-1/3 animate-pulse rounded-full bg-[hsl(var(--accent))]" />
          </div>
        </CenteredState>
      );
    }

    if (snapshot.status === 'failed') {
      return (
        <CenteredState>
          <p className="text-[hsl(var(--text-primary))]">Couldn&apos;t capture this page.</p>
          {snapshot.errorMessage ? (
            <p className="max-w-sm text-xs text-[hsl(var(--text-secondary))]">{snapshot.errorMessage}</p>
          ) : null}
          <div className="flex items-center gap-2">
            <button type="button" onClick={retry} disabled={isCapturing} className={primaryButton}>
              Try again
            </button>
            <button type="button" onClick={openLive} className={secondaryButton}>
              Open live page
            </button>
          </div>
        </CenteredState>
      );
    }

    const src = snapshotFrameUrl(snapshot.id, clean);
    return (
      <div className="h-full p-2">
        <SnapshotFrame key={src} src={src} host={host} zoom={zoom} />
      </div>
    );
  };

  const status = snapshot?.status === 'ready' && !compact ? statusLine(snapshot) : null;

  return (
    <TooltipProvider delayDuration={300}>
      <div
        role="group"
        aria-label={`Frozen copy of ${host}`}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="flex h-full min-h-0 flex-col overflow-hidden rounded-lg border border-subtle bg-surface focus:outline-hidden"
      >
        <SnapshotToolbar
          toggle={toggle}
          url={shownUrl}
          snapshot={snapshot}
          compact={compact}
          clean={clean}
          zoom={zoom}
          onCleanChange={setClean}
          onZoomIn={zoomIn}
          onZoomOut={zoomOut}
          onZoomReset={resetZoom}
          onCopyLink={copyLink}
          onOpenLive={openLive}
          onCaptureAgain={retry}
          onShowReader={onShowReader}
        />
        {snapshot?.status === 'ready' ? (
          <LinkBar
            conversationId={conversationId}
            snapshotId={snapshot.id}
            onOpenFrozenCopy={(next) => setSwitched({ forUrl: url, snapshot: next })}
          />
        ) : null}
        <div className="min-h-0 flex-1">{renderBody()}</div>
        {status ? (
          <p
            className={cn(
              'shrink-0 truncate border-t border-subtle px-3 py-1.5 text-xs',
              status.failed ? 'text-[hsl(var(--danger-fg))]' : 'text-[hsl(var(--text-muted))]',
            )}
          >
            {status.text}
            {status.partial ? (
              <span className="text-[hsl(var(--warning-fg))]"> · {status.partial}</span>
            ) : null}
          </p>
        ) : null}
      </div>
    </TooltipProvider>
  );
}
