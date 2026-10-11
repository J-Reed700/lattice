/**
 * ModelRow
 *
 * One model in the catalog list: name, repo id, one muted meta line, and the
 * download control. A hairline row, not a card. Clicking the text opens the
 * detail view; the control on the right is a separate target.
 */

import { Check, CircleCheck, Download, Loader2, TriangleAlert } from 'lucide-react';

import { cn } from '@/lib/utils';
import type { SystemCapabilities } from '@/types/api/models';
import type { DownloadStatus } from '@/types/downloads';
import type { ModelRecommendation } from '@/types/modelCatalog';

import { computeModelFit, embeddingBlockReason, FIT_LABEL, formatCompact, formatSize, modelMetaLine } from './catalogUtils';
import { hasModelVersions, modelQuantization } from './quantization';
import { SECONDARY_BUTTON_CLASS } from '../settingsStyles';


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
  const FitIcon = fit?.verdict === 'fits' ? CircleCheck : TriangleAlert;
  const downloads = formatCompact(model.popularity_downloads);
  const compactMeta = [formatSize(metadata.size_gb), downloads ? `${downloads} downloads` : null].filter(Boolean).join(' · ');

  const renderAction = () => {
    if (isDownloaded) {
      return <span className="inline-flex items-center gap-1.5 rounded-md bg-success-muted px-2 py-1 text-xs font-medium text-success-fg"><Check className="h-3.5 w-3.5" aria-hidden="true" />Downloaded</span>;
    }

    if (activeDownload) {
      const percentage = activeDownload.percentage;
      return (
        <span role="status" className="inline-flex items-center gap-1.5 text-xs tabular-nums text-accent">
          <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
          {percentage != null ? `Downloading ${Math.round(percentage)}%` : 'Downloading…'}
        </span>
      );
    }

    if (isStarting) {
      return <span className="text-xs text-text-muted">Starting…</span>;
    }

    if (blockReason) {
      return (
        <button
          type="button"
          onClick={onDownload}
          className="text-xs text-text-muted hover:text-text-primary hover:underline"
          title={blockReason}
        >
          Unsupported
        </button>
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
      <button type="button" onClick={onDownload} className={`${SECONDARY_BUTTON_CLASS} gap-1.5`}>
        <Download className="h-3.5 w-3.5" aria-hidden="true" />
        Download
      </button>
    );
  };

  const action = renderAction();

  return (
    <div className="model-catalog-row flex items-start justify-between gap-3 border-b border-border-subtle py-4 transition-colors duration-fast">
      <button
        type="button"
        onClick={onSelect}
        className={cn(
          'min-w-0 flex-1 rounded-sm text-left outline-hidden',
          'focus-visible:ring-2 focus-visible:ring-ring',
        )}
      >
        <div title={metadata.name} className="line-clamp-2 text-sm font-semibold leading-snug text-text-primary">{metadata.name}</div>
        {!compact ? <div className="break-all font-mono text-xs text-text-muted">{repoId}</div> : null}
        {!compact && metadata.description ? <p className="mt-1 line-clamp-2 text-sm text-text-secondary">{metadata.description}</p> : null}
        <div className="mt-2 flex min-w-0 flex-wrap items-center gap-1.5 text-xs text-text-muted">
          {fit ? (
              <span
                className={cn('inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium',
                  fit.verdict === 'fits' ? 'bg-success-muted text-success-fg'
                    : fit.verdict === 'too-large' ? 'bg-danger-muted text-danger-fg'
                      : fit.verdict === 'tight' ? 'bg-warning-muted text-warning-fg' : 'bg-surface-raised text-text-secondary')}
                title={fit.reason}
              >
                <FitIcon className="h-3 w-3" aria-hidden="true" />
                {FIT_LABEL[fit.verdict]}
              </span>
          ) : null}
          <span>{compact ? compactMeta : modelMetaLine(metadata, model.popularity_downloads)}</span>
        </div>
        {!compact ? <p className="mt-1 text-xs text-text-muted">
          {metadata.minimum_ram_gb > 0 ? `Est. memory ${metadata.minimum_ram_gb.toFixed(1)} GB` : 'Memory estimate unavailable'}
          {metadata.license && metadata.license !== 'unknown' ? ` · ${metadata.license}` : ''}
          {hasModelVersions(metadata) ? ` · Listed version: ${modelQuantization(metadata) ?? 'see files'}` : ''}
        </p> : null}
      </button>
      <div className="flex shrink-0 flex-col items-end gap-2 pt-0.5">
        {hasModelVersions(metadata) ? <button type="button" onClick={onSelect} className="text-sm text-accent hover:underline" aria-label={`Versions of ${metadata.name}`}>Versions</button> : null}
        {action}
      </div>
    </div>
  );
}
