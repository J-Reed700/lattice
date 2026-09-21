import { useState, useCallback } from 'react';

import { Pause, Play, X, RotateCw, Trash2 } from 'lucide-react';

import {
  basename,
  formatEta,
  formatProgressBytes,
  formatSpeed,
  normalizePercentage,
  STATE_LABELS,
  STATE_TEXT_CLASS,
} from './downloadFormat';
import { DownloadProgressBar } from './DownloadProgressBar';
import { fileNameWithinModel } from '../../hooks/useDownloads';
import { showErrorToast } from '../../utils/toast';

import type { DownloadStatus } from '../../types/downloads';

/**
 * The subset of `useDownloadActions` a row needs. Passing the actions in
 * (rather than calling the hook per row) keeps a drawer full of rows from
 * creating dozens of identical callback sets.
 */
export interface DownloadRowActions {
  pauseDownload: (id: string) => Promise<void>;
  resumeDownload: (id: string) => Promise<void>;
  cancelDownload: (id: string) => Promise<void>;
  retryDownload: (id: string) => Promise<void>;
  removeDownload: (id: string) => Promise<void>;
}

interface DownloadRowProps {
  /**
   * The **store key**, not `download.id`. `useDownloadActions` resolves the
   * backend id from this key; for grouped model files the two differ
   * (`<model_id>:<filename>` vs. the backend row id).
   */
  storeKey: string;
  download: DownloadStatus;
  actions: DownloadRowActions;
  /** Nested inside an aggregated model group — renders tighter, no title row. */
  nested?: boolean;
}

interface ActionButtonProps {
  onClick: () => void;
  disabled: boolean;
  label: string;
  children: React.ReactNode;
  danger?: boolean;
}

function ActionButton({ onClick, disabled, label, children, danger }: ActionButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={label}
      aria-label={label}
      className={`
        flex h-7 w-7 items-center justify-center rounded-md
        transition-colors duration-fast
        disabled:cursor-not-allowed disabled:opacity-40
        ${
          danger
            ? 'text-[hsl(var(--text-muted))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--danger,0_70%_50%))]'
            : 'text-[hsl(var(--text-muted))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
        }
      `}
    >
      {children}
    </button>
  );
}

export function DownloadRow({ storeKey, download, actions, nested = false }: DownloadRowProps) {
  const [busy, setBusy] = useState(false);

  const run = useCallback(
    async (verb: string, fn: (id: string) => Promise<void>) => {
      setBusy(true);
      try {
        await fn(storeKey);
      } catch (error) {
        showErrorToast(`Could not ${verb} download`, error);
      } finally {
        setBusy(false);
      }
    },
    [storeKey]
  );

  const { state } = download;
  const percentage = normalizePercentage(
    download.percentage,
    download.bytes_downloaded,
    download.total_bytes
  );
  // Inside a model's group the header already names the model, so a row says
  // which of its files it is.
  const name = nested
    ? fileNameWithinModel(download)
    : download.model_name || basename(download.destination);

  const canPause = state === 'Downloading' || state === 'Pending';
  const canResume = state === 'Paused';
  const canCancel = state === 'Downloading' || state === 'Pending' || state === 'Paused';
  const canRetry = state === 'Failed' || state === 'Cancelled';
  const canRemove = state === 'Completed' || state === 'Failed' || state === 'Cancelled';

  return (
    <div
      className={`
        ${nested ? 'py-2 pl-3' : 'rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-3'}
      `}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p
            className="truncate text-sm text-[hsl(var(--text-primary))]"
            title={download.destination}
          >
            {name}
          </p>
          <p className="mt-0.5 flex flex-wrap items-center gap-x-2 text-xs text-[hsl(var(--text-muted))]">
            <span className={STATE_TEXT_CLASS[state]}>{STATE_LABELS[state]}</span>
            <span aria-hidden="true">·</span>
            <span>{formatProgressBytes(download.bytes_downloaded, download.total_bytes)}</span>
            {state === 'Downloading' && (
              <>
                <span aria-hidden="true">·</span>
                <span>{formatSpeed(download.bytes_per_second)}</span>
                <span aria-hidden="true">·</span>
                <span>{formatEta(download.eta_seconds)} left</span>
              </>
            )}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-0.5">
          {canPause && (
            <ActionButton
              onClick={() => void run('pause', actions.pauseDownload)}
              disabled={busy}
              label={`Pause ${name}`}
            >
              <Pause className="h-3.5 w-3.5" strokeWidth={1.75} />
            </ActionButton>
          )}
          {canResume && (
            <ActionButton
              onClick={() => void run('resume', actions.resumeDownload)}
              disabled={busy}
              label={`Resume ${name}`}
            >
              <Play className="h-3.5 w-3.5" strokeWidth={1.75} />
            </ActionButton>
          )}
          {canRetry && (
            <ActionButton
              onClick={() => void run('retry', actions.retryDownload)}
              disabled={busy}
              label={`Retry ${name}`}
            >
              <RotateCw className="h-3.5 w-3.5" strokeWidth={1.75} />
            </ActionButton>
          )}
          {canCancel && (
            <ActionButton
              onClick={() => void run('cancel', actions.cancelDownload)}
              disabled={busy}
              label={`Cancel ${name}`}
              danger
            >
              <X className="h-3.5 w-3.5" strokeWidth={1.75} />
            </ActionButton>
          )}
          {canRemove && (
            <ActionButton
              onClick={() => void run('remove', actions.removeDownload)}
              disabled={busy}
              label={`Remove ${name} from list`}
              danger
            >
              <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
            </ActionButton>
          )}
        </div>
      </div>

      {state !== 'Completed' && state !== 'Cancelled' && (
        <div className="mt-2">
          <DownloadProgressBar
            percentage={percentage}
            state={state}
            label={`${name} download progress`}
          />
        </div>
      )}

      {download.error_message && (
        <p className="mt-1.5 break-words text-xs text-[hsl(var(--danger,0_70%_50%))]">
          {download.error_message}
        </p>
      )}
    </div>
  );
}
