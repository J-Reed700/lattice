/**
 * The browser chrome above a frozen page: one 40px row with the Reader | Page
 * tabs at the start, the address pill and frozen-on chip in the middle, and
 * the view controls at the end. Copy is design §9, word for word.
 */

import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from 'react';

import { Lock, Minus, MoreHorizontal, Plus, Snowflake, Sparkles } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

import { middleEllipsis, splitUrl } from './snapshotFrameUrl';

import type { WebPageSnapshotDto } from './api';
import type { WebPageMode } from './webPageSnapshotStore';

const TABS: { id: WebPageMode; label: string; hint: string }[] = [
  { id: 'reader', label: 'Reader', hint: 'Text with the cited passages marked' },
  { id: 'page', label: 'Page', hint: 'The page as it was captured' },
];

export interface ReaderPageToggleProps {
  mode: WebPageMode;
  onChange: (_mode: WebPageMode) => void;
  /** The reader was opened from a clicked citation, whose marks only Reader shows. */
  marked?: boolean;
}

/** The segmented `Reader | Page` switch. A tablist, arrow keys included. */
export function ReaderPageToggle({ mode, onChange, marked = false }: ReaderPageToggleProps) {
  const tabRefs = useRef<Record<WebPageMode, HTMLButtonElement | null>>({ reader: null, page: null });

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const next: WebPageMode = mode === 'reader' ? 'page' : 'reader';
    onChange(next);
    tabRefs.current[next]?.focus();
  };

  return (
    <TooltipProvider delayDuration={300}>
      <div
        role="tablist"
        aria-label="Reading mode"
        onKeyDown={onKeyDown}
        className="inline-flex h-7 shrink-0 items-center rounded-md bg-[hsl(var(--text-primary)/0.06)] p-0.5"
      >
        {TABS.map((tab) => {
          const selected = tab.id === mode;
          return (
            <Tooltip key={tab.id}>
              <TooltipTrigger asChild>
                <button
                  ref={(element) => {
                    tabRefs.current[tab.id] = element;
                  }}
                  type="button"
                  role="tab"
                  aria-selected={selected}
                  tabIndex={selected ? 0 : -1}
                  onClick={() => onChange(tab.id)}
                  className={cn(
                    'h-6 rounded-[5px] px-2.5 text-xs font-medium transition-colors duration-fast',
                    'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                    selected
                      ? 'bg-surface-raised text-[hsl(var(--text-primary))] shadow-sm'
                      : 'text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-primary))]',
                  )}
                >
                  {tab.label}
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom">
                <span className="flex flex-col gap-0.5">
                  <span>{tab.hint}</span>
                  {tab.id === 'page' && marked ? <span>Marked passages are shown in Reader</span> : null}
                </span>
              </TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </TooltipProvider>
  );
}

function formatFrozenDate(iso: string): { short: string; exact: string } | null {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return {
    short: date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }),
    exact: date.toLocaleString(undefined, { dateStyle: 'full', timeStyle: 'long' }),
  };
}

export interface SnapshotToolbarProps {
  /** The Reader | Page tabs, drawn at the start of the row. */
  toggle?: ReactNode;
  /** The address being shown: the snapshot's final URL, else the cited one. */
  url: string;
  snapshot: WebPageSnapshotDto | null;
  compact: boolean;
  clean: boolean;
  zoom: number;
  onCleanChange: (_clean: boolean) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomReset: () => void;
  onCopyLink: () => void;
  onOpenLive: () => void;
  onCaptureAgain: () => void;
  onShowReader?: () => void;
}

const chromeText = 'text-[hsl(var(--text-muted))]';

export function SnapshotToolbar({
  toggle,
  url,
  snapshot,
  compact,
  clean,
  zoom,
  onCleanChange,
  onZoomIn,
  onZoomOut,
  onZoomReset,
  onCopyLink,
  onOpenLive,
  onCaptureAgain,
  onShowReader,
}: SnapshotToolbarProps) {
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const { host, path, secure } = splitUrl(url);
  const ready = snapshot?.status === 'ready';
  const redirectedFrom =
    snapshot?.finalUrl && snapshot.finalUrl !== snapshot.requestedUrl ? snapshot.requestedUrl : null;
  const frozen = ready && snapshot?.capturedAt ? formatFrozenDate(snapshot.capturedAt) : null;

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const menuItem = (label: string, action: () => void) => (
    <button
      key={label}
      type="button"
      role="menuitem"
      onClick={() => {
        setMenuOpen(false);
        action();
      }}
      className="flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-sm text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-hover focus-visible:bg-surface-hover focus-visible:outline-hidden"
    >
      {label}
    </button>
  );

  return (
    <div className="flex h-10 shrink-0 items-center gap-2 border-b border-subtle bg-surface-raised px-2">
      {toggle}

      <div className="flex min-w-0 flex-1 items-center justify-center gap-1.5">
        <button
          type="button"
          title={url}
          onClick={() => {
            onCopyLink();
            setCopied(true);
          }}
          className="inline-flex h-7 min-w-0 max-w-[28rem] items-center gap-1.5 rounded-full bg-[hsl(var(--text-primary)/0.05)] px-3 text-xs transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.09)] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          {secure ? <Lock aria-hidden size={11} className={cn('shrink-0', chromeText)} /> : null}
          {snapshot?.faviconDataUrl ? (
            <img src={snapshot.faviconDataUrl} alt="" className="h-3.5 w-3.5 shrink-0 rounded-sm" />
          ) : null}
          <span className="shrink-0 font-medium text-[hsl(var(--text-primary))]">{host}</span>
          {copied ? (
            <span className={cn('shrink-0', chromeText)}>Copied</span>
          ) : !compact && path ? (
            <span className={cn('min-w-0 truncate', chromeText)}>{middleEllipsis(path)}</span>
          ) : null}
        </button>

        {redirectedFrom ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <span tabIndex={0} className={cn('shrink-0 cursor-default text-xs', chromeText)}>
                ↪
              </span>
            </TooltipTrigger>
            <TooltipContent side="bottom">Redirected from {redirectedFrom}</TooltipContent>
          </Tooltip>
        ) : null}

        {frozen ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <span
                tabIndex={0}
                aria-label={compact ? `Frozen ${frozen.short}` : undefined}
                className={cn(
                  'inline-flex h-6 shrink-0 cursor-default items-center gap-1 rounded-full px-2 text-xs',
                  chromeText,
                )}
              >
                <Snowflake aria-hidden size={12} />
                {compact ? null : <span>Frozen {frozen.short}</span>}
              </span>
            </TooltipTrigger>
            <TooltipContent side="bottom">{frozen.exact}</TooltipContent>
          </Tooltip>
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-0.5">
        {ready ? (
          <>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  aria-pressed={clean}
                  aria-label={compact ? 'Clean' : undefined}
                  onClick={() => onCleanChange(!clean)}
                  className={cn(
                    'inline-flex h-7 items-center gap-1 rounded-md text-xs font-medium transition-colors duration-fast',
                    'focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                    compact ? 'w-7 justify-center' : 'px-2',
                    clean
                      ? 'bg-accent-muted text-accent'
                      : 'text-text-tertiary hover:bg-[hsl(var(--text-primary)/0.07)] hover:text-text-primary',
                  )}
                >
                  <Sparkles aria-hidden size={13} />
                  {compact ? null : <span>Clean</span>}
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom">Hide cookie walls, pop-ups and overlays</TooltipContent>
            </Tooltip>

            <div className="ml-1 flex items-center">
              <IconButton label="Zoom out" shortcut="⌘-" onClick={onZoomOut} disabled={zoom <= 0.5}>
                <Minus />
              </IconButton>
              {compact ? null : (
                <button
                  type="button"
                  aria-label="Reset zoom"
                  title="Reset zoom"
                  onClick={onZoomReset}
                  className={cn(
                    'h-7 min-w-[3rem] rounded-md px-1 text-xs tabular-nums transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.07)] focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                    chromeText,
                  )}
                >
                  {Math.round(zoom * 100)}%
                </button>
              )}
              <IconButton label="Zoom in" shortcut="⌘=" onClick={onZoomIn} disabled={zoom >= 2}>
                <Plus />
              </IconButton>
            </div>
          </>
        ) : null}

        <Popover open={menuOpen} onOpenChange={setMenuOpen}>
          <PopoverTrigger asChild>
            <IconButton label="More" aria-haspopup="menu">
              <MoreHorizontal />
            </IconButton>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-60 p-1">
            <div role="menu" aria-label="More" className="flex flex-col">
              {menuItem('Open live page in browser', onOpenLive)}
              {menuItem('Copy link', onCopyLink)}
              {snapshot?.status === 'failed' ? menuItem('Capture again', onCaptureAgain) : null}
              {onShowReader ? menuItem('Show in Reader', onShowReader) : null}
            </div>
          </PopoverContent>
        </Popover>
      </div>
    </div>
  );
}
