/**
 * ModelDetailPanel
 *
 * One model in full: what it is, whether it fits this machine, and the
 * actions available for it. Hairline sections of label/value rows; the
 * Download button is the only accent on the page.
 */

import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeft } from 'lucide-react';

import { cn } from '@/lib/utils';

import {
  CATEGORY_LABELS,
  computeModelFit,
  embeddingBlockReason,
  FIT_LABEL,
  formatSize,
} from './catalogUtils';
import { startModelDownload } from './startModelDownload';
import { useHuggingFaceTokenStatusQuery } from '../../../hooks/queries/useHuggingFaceTokenQuery';
import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { useDownloadState } from '../../../hooks/useDownloadState';
import { useModelCatalog } from '../../../hooks/useModelCatalog';
import { getErrorMessage } from '../../../lib/errorUtils';
import { useToastStore } from '../../../stores/toastStore';
import { handleAsyncEvent } from '../../../utils/promiseHandlers';
import {
  GHOST_BUTTON_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from '../settingsStyles';

import type { ModelRecommendation } from '../../../types/modelCatalog';

interface ModelDetailPanelProps {
  model: ModelRecommendation;
  onBack: () => void;
  routerModelId?: string;
  onSetRouterModel?: (_modelId: string) => Promise<void>;
  /** Sends the user to the Hugging Face token field on this page. */
  onAddToken?: () => void;
}

function DetailSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="pb-2 text-sm font-medium text-text-secondary">{title}</h3>
      <div className="border-t border-border-subtle">{children}</div>
    </section>
  );
}

function DetailRow({
  label,
  value,
  tone = 'default',
}: {
  label: string;
  value: ReactNode;
  tone?: 'default' | 'danger';
}) {
  return (
    <div className="flex items-start justify-between gap-6 border-b border-border-subtle py-2.5">
      <span className="shrink-0 text-sm text-text-secondary">{label}</span>
      <span
        className={cn(
          'text-right text-sm tabular-nums',
          tone === 'danger' ? 'text-danger-fg' : 'text-text-primary',
        )}
      >
        {value}
      </span>
    </div>
  );
}

function NoteRow({ children, tone = 'default' }: { children: ReactNode; tone?: 'default' | 'danger' }) {
  return (
    <p
      className={cn(
        'border-b border-border-subtle py-2.5 text-sm',
        tone === 'danger' ? 'text-danger-fg' : 'text-text-secondary',
      )}
    >
      {children}
    </p>
  );
}

export function ModelDetailPanel({
  model,
  onBack,
  routerModelId,
  onSetRouterModel,
  onAddToken,
}: ModelDetailPanelProps) {
  const { model: metadata, compatibility } = model;
  const { systemCapabilities } = useModelCatalog({
    autoLoadCapabilities: true,
    autoLoadModels: false,
  });
  const { data: hfTokenStatus } = useHuggingFaceTokenStatusQuery();
  const fit = computeModelFit(metadata, systemCapabilities);
  const hasHfToken = hfTokenStatus?.isSet ?? false;
  const popularityDownloads = model.popularity_downloads ?? null;
  const popularityLikes = model.popularity_likes ?? null;
  const formattedPopularityDownloads = popularityDownloads
    ? new Intl.NumberFormat(undefined).format(popularityDownloads)
    : null;
  const formattedPopularityLikes = popularityLikes
    ? new Intl.NumberFormat(undefined).format(popularityLikes)
    : null;
  const {
    isModelDownloaded,
    setActiveChatModel,
    setActiveEmbeddingModel,
    warmUpActiveChatModel,
  } = useDownloadedModels();
  const addToast = useToastStore((state) => state.addToast);
  const queryClient = useQueryClient();
  const [isDownloaded, setIsDownloaded] = useState(false);
  const [isCheckingDownload, setIsCheckingDownload] = useState(true);
  // Tracks the moment between click and the backend registering the download.
  // Without this, the button stays enabled for ~3s while the Tauri command
  // does its initial network/DB work, letting impatient users spam the button.
  const [isStartingDownload, setIsStartingDownload] = useState(false);
  const [isSettingChatModel, setIsSettingChatModel] = useState(false);
  const [isSettingEmbeddingModel, setIsSettingEmbeddingModel] = useState(false);
  const [isSettingRouterModel, setIsSettingRouterModel] = useState(false);
  const checkRequestRef = useRef(0);

  const { getActiveDownloadForModel, hasActiveDownloadForModel } = useDownloadState();
  const activeDownload = getActiveDownloadForModel(metadata.id);
  const hasActiveDownload = hasActiveDownloadForModel(metadata.id);
  const isDownloading = hasActiveDownload && activeDownload?.state === 'Downloading';
  const isRouterActive = routerModelId === metadata.id;
  const isLlmCategory = metadata.category === 'LLM';
  const isEmbeddingCategory = metadata.category === 'Embedding';
  // Architecture compatibility for embedding models. Server-side gate also
  // exists in the download use case; this is purely UX so users see the
  // "won't work" reason before clicking download instead of after a
  // multi-GB transfer.
  const incompatibilityReason = embeddingBlockReason(metadata);
  const isEmbeddingArchIncompatible = incompatibilityReason !== null;
  const canSetRouter = Boolean(onSetRouterModel);
  const hasDownloadSource =
    Boolean(metadata.model_id && metadata.default_filename) || Boolean(metadata.download_url);

  const checkDownloadStatus = useCallback(async () => {
    const requestId = ++checkRequestRef.current;
    const currentModelId = metadata.id;
    setIsCheckingDownload(true);
    try {
      const downloaded = await isModelDownloaded(currentModelId);
      if (checkRequestRef.current === requestId) {
        setIsDownloaded(downloaded);
      }
    } catch (error) {
      console.error('Failed to check download status:', error);
    } finally {
      if (checkRequestRef.current === requestId) {
        setIsCheckingDownload(false);
      }
    }
  }, [isModelDownloaded, metadata.id]);

  // Re-check download status when download completes
  useEffect(() => {
    if (activeDownload?.state === 'Completed') {
      void checkDownloadStatus();
    }
  }, [activeDownload?.state, checkDownloadStatus]);

  useEffect(() => {
    void checkDownloadStatus();
  }, [checkDownloadStatus]);

  const handleSetActiveChat = async () => {
    if (!isDownloaded) {
      addToast({
        type: 'warning',
        title: 'Download it first',
        message: 'This model has to be on disk before it can be the chat model.',
      });
      return;
    }
    setIsSettingChatModel(true);
    try {
      await setActiveChatModel(metadata.id);
      try {
        await warmUpActiveChatModel();
        addToast({
          type: 'success',
          title: 'Chat model ready',
          message: `${metadata.name} is active and warmed up.`,
        });
      } catch (warmError) {
        addToast({
          type: 'warning',
          title: 'Chat model updated',
          message: `${metadata.name} is active, but warm-up failed: ${getErrorMessage(warmError)}`,
        });
      }
      queryClient.invalidateQueries({ queryKey: ['downloaded-models'] });
    } catch (error) {
      addToast({
        type: 'error',
        title: "Couldn't set the chat model",
        message: getErrorMessage(error),
      });
    } finally {
      setIsSettingChatModel(false);
    }
  };

  const handleSetActiveEmbedding = async () => {
    if (!isDownloaded) {
      addToast({
        type: 'warning',
        title: 'Download it first',
        message: 'This model has to be on disk before it can be the embedding model.',
      });
      return;
    }
    setIsSettingEmbeddingModel(true);
    try {
      await setActiveEmbeddingModel(metadata.id);
      addToast({
        type: 'success',
        title: 'Embedding model updated',
        message: `${metadata.name} is prepared. Restart Lattice to use its search index.`,
      });
      queryClient.invalidateQueries({ queryKey: ['downloaded-models'] });
    } catch (error) {
      addToast({
        type: 'error',
        title: "Couldn't set the embedding model",
        message: getErrorMessage(error),
      });
    } finally {
      setIsSettingEmbeddingModel(false);
    }
  };

  const handleSetRouter = async () => {
    if (!onSetRouterModel) return;
    if (!isDownloaded) {
      addToast({
        type: 'warning',
        title: 'Download it first',
        message: 'This model has to be on disk before it can route.',
      });
      return;
    }
    setIsSettingRouterModel(true);
    try {
      await onSetRouterModel(metadata.id);
      addToast({
        type: 'success',
        title: 'Router model updated',
        message: `${metadata.name} is now the routing model.`,
      });
    } catch (error) {
      addToast({
        type: 'error',
        title: "Couldn't set the router model",
        message: getErrorMessage(error),
      });
    } finally {
      setIsSettingRouterModel(false);
    }
  };

  const handleDownload = async () => {
    // Guard against multiple downloads, including in-flight invoke calls
    // that haven't yet registered an active download in the store.
    if (hasActiveDownload || isStartingDownload) {
      return;
    }

    setIsStartingDownload(true);
    try {
      const { alreadyDownloaded } = await startModelDownload({
        metadata,
        addToast,
        queryClient,
      });
      if (alreadyDownloaded) setIsDownloaded(true);
    } finally {
      setIsStartingDownload(false);
    }
  };

  const size = formatSize(metadata.size_gb);
  const headerMeta = [
    CATEGORY_LABELS[metadata.category],
    metadata.performance_tier,
    size,
    isRouterActive ? 'Router' : null,
  ].filter(Boolean) as string[];

  const downloadControl = () => {
    if (isEmbeddingArchIncompatible) {
      return (
        <button
          type="button"
          onClick={handleAsyncEvent(handleDownload)}
          className={SECONDARY_BUTTON_CLASS}
          title={incompatibilityReason ?? undefined}
        >
          Not supported on this build
        </button>
      );
    }
    if (!hasDownloadSource) return null;
    if (isCheckingDownload) return <span className="text-sm text-text-muted">Checking…</span>;
    if (isDownloaded) return <span className="text-sm text-text-muted">Downloaded</span>;
    if (isStartingDownload) return <span className="text-sm text-text-muted">Starting…</span>;
    if (isDownloading || hasActiveDownload) {
      const percentage = activeDownload?.percentage;
      return (
        <span className="text-sm tabular-nums text-text-muted">
          {percentage != null ? `Downloading ${Math.round(percentage)}%` : 'Downloading…'}
        </span>
      );
    }
    return (
      <button
        type="button"
        onClick={handleAsyncEvent(handleDownload)}
        className={PRIMARY_BUTTON_CLASS}
      >
        Download
      </button>
    );
  };

  return (
    <div className="space-y-8">
      <div>
        <button type="button" onClick={onBack} className={cn(GHOST_BUTTON_CLASS, 'gap-1.5 pl-1.5')}>
          <ArrowLeft className="h-4 w-4" />
          Back
        </button>
      </div>

      <div className="space-y-3">
        <div>
          <h2 className="text-lg font-medium text-text-primary">{metadata.name}</h2>
          <p className="mt-0.5 break-all font-mono text-xs text-text-muted">
            {metadata.model_id ?? metadata.id}
          </p>
          <p className="mt-1 text-xs text-text-muted">{headerMeta.join(' · ')}</p>
        </div>

        {metadata.description ? (
          <p className="max-w-[70ch] text-sm leading-relaxed text-text-secondary">
            {metadata.description}
          </p>
        ) : null}

        {incompatibilityReason ? (
          <p className="max-w-[70ch] text-sm text-danger-fg">{incompatibilityReason}</p>
        ) : null}

        <div className="flex flex-wrap items-center gap-2 pt-1">
          {downloadControl()}

          {isLlmCategory ? (
            <>
              <button
                type="button"
                onClick={handleAsyncEvent(handleSetActiveChat)}
                disabled={!isDownloaded || isSettingChatModel}
                className={SECONDARY_BUTTON_CLASS}
              >
                {isSettingChatModel ? 'Setting…' : 'Use for chat'}
              </button>
              {canSetRouter ? (
                <button
                  type="button"
                  onClick={handleAsyncEvent(handleSetRouter)}
                  disabled={!isDownloaded || isSettingRouterModel || isRouterActive}
                  className={SECONDARY_BUTTON_CLASS}
                >
                  {isSettingRouterModel
                    ? 'Setting…'
                    : isRouterActive
                      ? 'Routing'
                      : 'Use for routing'}
                </button>
              ) : null}
            </>
          ) : null}

          {isEmbeddingCategory ? (
            <button
              type="button"
              onClick={handleAsyncEvent(handleSetActiveEmbedding)}
              disabled={
                !isDownloaded || isSettingEmbeddingModel || isEmbeddingArchIncompatible
              }
              className={SECONDARY_BUTTON_CLASS}
            >
              {isSettingEmbeddingModel ? 'Setting…' : 'Use for embeddings'}
            </button>
          ) : null}
        </div>
      </div>

      <DetailSection title="Fit on this machine">
        <DetailRow
          label="Overall"
          value={`${compatibility.compatibility_level} · ${compatibility.overall_score}/100`}
          tone={
            compatibility.compatibility_level === 'Incompatible' ||
            compatibility.compatibility_level === 'Poor'
              ? 'danger'
              : 'default'
          }
        />
        <DetailRow label="Memory" value={`${compatibility.ram_score}/100`} />
        <DetailRow label="GPU" value={`${compatibility.gpu_score}/100`} />
        <DetailRow label="Disk" value={`${compatibility.disk_score}/100`} />
        {compatibility.estimated_tokens_per_second ? (
          <DetailRow
            label="Estimated speed"
            value={`${compatibility.estimated_tokens_per_second.toFixed(1)} tokens/sec`}
          />
        ) : null}
        {compatibility.estimated_loading_time_seconds > 0 ? (
          <DetailRow
            label="Load time"
            value={`~${compatibility.estimated_loading_time_seconds.toFixed(1)} s`}
          />
        ) : null}
      </DetailSection>

      {compatibility.blockers.length > 0 ? (
        <DetailSection title="Blockers">
          {compatibility.blockers.map((blocker) => (
            <NoteRow key={blocker} tone="danger">
              {blocker}
            </NoteRow>
          ))}
        </DetailSection>
      ) : null}

      {compatibility.recommendations.length > 0 ? (
        <DetailSection title="Notes">
          {compatibility.recommendations.map((recommendation) => (
            <NoteRow key={recommendation}>{recommendation}</NoteRow>
          ))}
        </DetailSection>
      ) : null}

      <DetailSection title="Specifications">
        <DetailRow label="Size" value={size ?? 'Unknown'} />
        <DetailRow label="Minimum memory" value={`${metadata.minimum_ram_gb.toFixed(1)} GB`} />
        <DetailRow label="Recommended memory" value={`${metadata.recommended_ram_gb.toFixed(1)} GB`} />
        <DetailRow label="Context" value={`${metadata.context_length.toLocaleString()} tokens`} />
        {isEmbeddingCategory && metadata.embedding_dimensions ? (
          <DetailRow label="Dimensions" value={metadata.embedding_dimensions} />
        ) : null}
        {metadata.supported_quantizations.length > 0 ? (
          <DetailRow label="Quantizations" value={metadata.supported_quantizations.join(', ')} />
        ) : null}
        <DetailRow label="Format" value={metadata.format === 'safetensors' ? 'Safetensors' : 'GGUF'} />
        <DetailRow label="License" value={metadata.license} />
        {fit ? <DetailRow label="Fit" value={`${FIT_LABEL[fit.verdict]} · ${fit.reason}`} /> : null}
        {metadata.requires_auth ? (
          <DetailRow
            label="Access"
            value={
              hasHfToken ? (
                'Gated · token set'
              ) : onAddToken ? (
                <button
                  type="button"
                  onClick={onAddToken}
                  className="text-sm text-accent hover:underline"
                >
                  Token required · Add token
                </button>
              ) : (
                'Hugging Face token required'
              )
            }
          />
        ) : null}
        {formattedPopularityDownloads ? (
          <DetailRow label="Downloads" value={formattedPopularityDownloads} />
        ) : null}
        {formattedPopularityLikes ? (
          <DetailRow label="Likes" value={formattedPopularityLikes} />
        ) : null}
        {metadata.capabilities.length > 0 ? (
          <DetailRow label="Capabilities" value={metadata.capabilities.join(', ')} />
        ) : null}
      </DetailSection>
    </div>
  );
}
