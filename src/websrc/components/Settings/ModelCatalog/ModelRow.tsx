/**
 * ModelRow
 *
 * One model in the catalog list: name, repo id, one muted meta line, and the
 * download control. A hairline row, not a card. Clicking the text opens the
 * detail view; the control on the right is a separate target.
 */

import { cn } from '@/lib/utils';

import { computeModelFit, embeddingBlockReason, FIT_LABEL, modelMetaLine } from './catalogUtils';
import { SECONDARY_BUTTON_CLASS } from '../settingsStyles';

import type { SystemCapabilities } from '../../../types/api/models';
import type { DownloadStatus } from '../../../types/downloads';
import type { ModelRecommendation } from '../../../types/modelCatalog';

interface ModelRowProps {
  model: ModelRecommendation;
  onSelect: () => void;
  isDownloaded: boolean;
  activeDownload: DownloadStatus | null;
  isStarting: boolean;
  onDownload: () => void;
  /** This machine, for the one-word fit verdict. Null while unknown. */
  capabilities?: SystemCapabilities | null;
  /** True when a Hugging Face token is stored. Gated rows need one. */
  hasHfToken?: boolean;
  /** Sends the user to the token field on this page. */
  onAddToken?: () => void;
  /** Category previews keep repository details in the full model view. */
  compact?: boolean;
}

export function ModelRow({
  model,
  onSelect,
  isDownloaded,
  activeDownload,
  isStarting,
  onDownload,
  capabilities,
  hasHfToken = false,
  onAddToken,
  compact = false,
}: ModelRowProps) {
  const { model: metadata } = model;
  const fit = computeModelFit(metadata, capabilities);
  const repoId = metadata.model_id ?? metadata.id;
  const blockReason = embeddingBlockReason(metadata);
  const isDownloadable =
    Boolean(metadata.model_id && metadata.default_filename) || Boolean(metadata.download_url);

  const renderAction = () => {
    if (isDownloaded) {
      return <span className="text-xs text-text-muted">Downloaded</span>;
    }

    if (activeDownload) {
      const percentage = activeDownload.percentage;
      return (
        <span className="text-xs tabular-nums text-text-muted">
          {percentage != null ? `Downloading ${Math.round(percentage)}%` : 'Downloading…'}
        </span>
      );
    }

    if (isStarting) {
      return <span className="text-xs text-text-muted">Starting…</span>;
    }

    if (blockReason) {
      return (
        <span className="text-xs text-text-muted" title={blockReason}>
          Unsupported
        </span>
      );
    }

    // A gated model can't be fetched without a token, so the row offers the one
    // thing that unblocks it instead of a Download button that will 401. This
    // sits ahead of the downloadable check: a gated row with no file to fetch
    // yet still needs the token before it can ever be fetched, and rendering
    // nothing leaves the user with no way to act on it.
    if (metadata.requires_auth && !hasHfToken && onAddToken) {
      return (
        <button type="button" onClick={onAddToken} className="text-xs text-accent hover:underline">
          Token required · Add token
        </button>
      );
    }

    if (!isDownloadable) return null;

    return (
      <button type="button" onClick={onDownload} className={SECONDARY_BUTTON_CLASS}>
        Download
      </button>
    );
  };

  const action = renderAction();

  return (
    <div className="flex items-start justify-between gap-4 border-b border-border-subtle py-3">
      <button
        type="button"
        onClick={onSelect}
        className={cn(
          'min-w-0 flex-1 rounded-sm text-left outline-none',
          'focus-visible:ring-2 focus-visible:ring-ring',
        )}
      >
        <div title={metadata.name} className={cn('text-sm font-medium text-text-primary', compact ? 'line-clamp-2' : 'truncate')}>{metadata.name}</div>
        {!compact ? <div className="truncate font-mono text-xs text-text-muted">{repoId}</div> : null}
        <div className="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-text-muted">
          {fit ? (
            <>
              <span
                className={cn(fit.verdict === 'too-large' && 'text-danger-fg')}
                title={fit.reason}
              >
                {FIT_LABEL[fit.verdict]}
              </span>
              <span aria-hidden="true">·</span>
            </>
          ) : null}
          <span className="truncate">{modelMetaLine(metadata, model.popularity_downloads)}</span>
        </div>
      </button>
      {action ? <div className="flex shrink-0 items-center pt-0.5">{action}</div> : null}
    </div>
  );
}
