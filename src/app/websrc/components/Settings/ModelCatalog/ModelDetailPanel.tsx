/**
 * ModelDetailPanel
 *
 * Detailed view of a selected model with full information and compatibility breakdown
 */

import { useState, useEffect, useCallback, useRef } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Download, HardDrive, Cpu, Zap, AlertCircle, CheckCircle2, Info, Loader2, Lock, Check } from 'lucide-react';

import { useDownloadedModels } from '../../../hooks/useDownloadedModels';
import { useDownloadState } from '../../../hooks/useDownloadState';
import { getErrorMessage } from '../../../lib/errorUtils';
import { useToastStore } from '../../../stores/toastStore';
import { handleAsyncEvent } from '../../../utils/promiseHandlers';
import { Button } from '../../ui/button';
import Card, { CardHeader, CardTitle, CardContent } from '../../ui/Card/Card';

import type { DownloadModelResponse } from '../../../types/download';
import type { CompatibilityLevel, ModelRecommendation } from '../../../types/modelCatalog';

interface ModelDetailPanelProps {
  model: ModelRecommendation;
  onBack: () => void;
  routerModelId?: string;
  onSetRouterModel?: (_modelId: string) => Promise<void>;
}

type DownloadModelCommandResponse =
  | DownloadModelResponse
  | { download_id: string; status: string };

export function ModelDetailPanel({
  model,
  onBack,
  routerModelId,
  onSetRouterModel,
}: ModelDetailPanelProps) {
  const { model: metadata, compatibility } = model;
  const popularityDownloads = model.popularity_downloads ?? null;
  const popularityLikes = model.popularity_likes ?? null;
  const formattedPopularityDownloads = popularityDownloads
    ? new Intl.NumberFormat(undefined).format(popularityDownloads)
    : null;
  const formattedPopularityLikes = popularityLikes
    ? new Intl.NumberFormat(undefined).format(popularityLikes)
    : null;
  const { isModelDownloaded, setActiveChatModel, setActiveEmbeddingModel, warmUpActiveChatModel } = useDownloadedModels();
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
  const embeddingCompat = metadata.embedding_compatibility ?? null;
  const isEmbeddingArchIncompatible =
    isEmbeddingCategory &&
    embeddingCompat !== null &&
    embeddingCompat.kind !== 'compatible';
  const incompatibilityReason =
    embeddingCompat?.kind === 'incompatible'
      ? embeddingCompat.reason
      : embeddingCompat?.kind === 'unknown'
        ? 'Architecture not recognized — only BERT-family embedders are supported today.'
        : null;
  const canSetRouter = Boolean(onSetRouterModel);

  // Check if model is already downloaded
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
        title: 'Download required',
        message: 'Download this model before setting it as the chat LLM.',
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
        title: 'Failed to update chat model',
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
        title: 'Download required',
        message: 'Download this model before setting it as the embedding model.',
      });
      return;
    }
    setIsSettingEmbeddingModel(true);
    try {
      await setActiveEmbeddingModel(metadata.id);
      addToast({
        type: 'success',
        title: 'Embedding model updated',
        message: `${metadata.name} is now the active embedding model.`,
      });
      queryClient.invalidateQueries({ queryKey: ['downloaded-models'] });
    } catch (error) {
      addToast({
        type: 'error',
        title: 'Failed to update embedding model',
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
        title: 'Download required',
        message: 'Download this model before setting it as the router.',
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
        title: 'Failed to update router model',
        message: getErrorMessage(error),
      });
    } finally {
      setIsSettingRouterModel(false);
    }
  };

  // Compatibility badge styling
  const getCompatibilityStyle = (level: CompatibilityLevel) => {
    switch (level) {
      case 'Excellent':
        return {
          bg: 'bg-[hsl(var(--success-muted))]',
          text: 'text-[hsl(var(--success-fg))]',
          icon: <CheckCircle2 className="w-4 h-4" />,
        };
      case 'Good':
        return {
          bg: 'bg-[hsl(var(--warning-muted))]',
          text: 'text-[hsl(var(--warning-fg))]',
          icon: <CheckCircle2 className="w-4 h-4" />,
        };
      case 'Poor':
        return {
          bg: 'bg-[hsl(var(--warning-muted))]',
          text: 'text-[hsl(var(--warning-fg))]',
          icon: <AlertCircle className="w-4 h-4" />,
        };
      case 'Incompatible':
        return {
          bg: 'bg-[hsl(var(--danger-muted))]',
          text: 'text-[hsl(var(--danger-fg))]',
          icon: <AlertCircle className="w-4 h-4" />,
        };
    }
  };

  const compatStyle = getCompatibilityStyle(compatibility.compatibility_level);

  // Handle model download
  const handleDownload = async () => {
    // Guard against multiple downloads, including in-flight invoke calls
    // that haven't yet registered an active download in the store.
    if (hasActiveDownload || isStartingDownload) {
      return;
    }

    setIsStartingDownload(true);
    try {
      // Use plugin pattern: model domain download command
      const response = await invoke<DownloadModelCommandResponse>('plugin:model|download_model', {
        modelId: metadata.id
      });

      // Handle both API shapes during migration: rich `state` payload and legacy `status` payload
      if ('state' in response) {
        if (response.state.type === 'DownloadStarted') {
          addToast({
            type: 'success',
            title: 'Download Started',
            message: `Downloading ${metadata.name} (${response.state.data.files_to_download} files)`,
          });
        } else if (response.state.type === 'AlreadyDownloaded') {
          addToast({
            type: 'info',
            title: 'Already Downloaded',
            message: `${metadata.name} is already downloaded (${response.state.data.verified_files} files verified)`,
          });
          // Update UI state immediately
          setIsDownloaded(true);
          // Invalidate query to refresh downloaded models list
          queryClient.invalidateQueries({ queryKey: ['downloaded-models'] });
        } else if (response.state.type === 'NetworkError') {
          addToast({
            type: 'error',
            title: 'Network Error',
            message: response.state.data.error_message,
          });
        } else if (response.state.type === 'OperationFailed') {
          addToast({
            type: 'error',
            title: 'Download Failed',
            message: response.state.data.error_message,
          });
        }
      } else if (response.status === 'started') {
        addToast({
          type: 'success',
          title: 'Download Started',
          message: `Downloading ${metadata.name}`,
        });
      } else if (response.status === 'already_downloaded') {
        addToast({
          type: 'info',
          title: 'Already Downloaded',
          message: `${metadata.name} is already downloaded`,
        });
        setIsDownloaded(true);
        queryClient.invalidateQueries({ queryKey: ['downloaded-models'] });
      } else if (response.status === 'network_error') {
        addToast({
          type: 'error',
          title: 'Network Error',
          message: 'A network error occurred while starting the download.',
        });
      } else if (response.status === 'failed') {
        addToast({
          type: 'error',
          title: 'Download Failed',
          message: 'Failed to start the download.',
        });
      }
    } catch (error) {
      console.error('Download failed:', error);

      const errorMessage = getErrorMessage(error);

      // Check for rate limit error
      if (errorMessage.includes('Rate limit')) {
        addToast({
          type: 'warning',
          title: 'Rate Limit Exceeded',
          message: 'Too many download requests. Please wait a moment and try again.',
        });
      } else {
        addToast({
          type: 'error',
          title: 'Download Failed',
          message: errorMessage,
        });
      }
    } finally {
      setIsStartingDownload(false);
    }
  };

  // Score bar component
  const ScoreBar = ({ score, label }: { score: number; label: string }) => {
    const getColor = (s: number) => {
      if (s >= 80) return 'bg-[hsl(var(--success-fg))]';
      if (s >= 50) return 'bg-[hsl(var(--warning-fg))]';
      return 'bg-[hsl(var(--danger-fg))]';
    };

    return (
      <div className="space-y-1">
        <div className="flex justify-between text-xs">
          <span className="text-[hsl(var(--text-secondary))]">{label}</span>
          <span className="font-medium text-[hsl(var(--text-primary))]">{score}/100</span>
        </div>
        <div className="h-2 bg-[hsl(var(--surface-raised))] rounded-full overflow-hidden">
          <div
            className={`h-full ${getColor(score)} transition-all duration-500`}
            style={{ width: `${score}%` }}
          />
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* Back button */}
      <Button variant="ghost" onClick={onBack} size="sm">
        <ArrowLeft className="w-4 h-4" />
        Back to Models
      </Button>

      {/* Header Card */}
      <Card padding="lg">
        <div className="space-y-4">
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1">
              <h2 className="text-2xl font-bold text-[hsl(var(--text-primary))] mb-2">
                {metadata.name}
              </h2>
              <div className="flex items-center gap-3 text-sm text-[hsl(var(--text-secondary))]">
                <span className="px-2 py-1 bg-[hsl(var(--surface-raised))] rounded">
                  {metadata.category}
                </span>
                <span className="px-2 py-1 bg-[hsl(var(--surface-raised))] rounded">
                  {metadata.performance_tier}
                </span>
                <span className="px-2 py-1 bg-[hsl(var(--surface-raised))] rounded">
                  {metadata.license}
                </span>
                {metadata.requires_auth && (
                  <span className="px-2 py-1 bg-[hsl(var(--warning-muted))] text-[hsl(var(--warning-fg))] rounded flex items-center gap-1">
                    <Lock className="w-3 h-3" />
                    Requires Token
                  </span>
                )}
                {metadata.format === 'safetensors' && (
                  <span
                    className="px-2 py-1 bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent-fg))] rounded"
                    title="HF safetensors format (unquantized). Larger on disk + RAM than a GGUF quant of the same model. Loaded via mistralrs auto-detect."
                  >
                    Safetensors
                  </span>
                )}
                {isEmbeddingArchIncompatible && (
                  <span
                    className="px-2 py-1 bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))] rounded flex items-center gap-1"
                    title={incompatibilityReason ?? undefined}
                  >
                    <AlertCircle className="w-3 h-3" />
                    Architecture not supported
                  </span>
                )}
              </div>
              {isEmbeddingArchIncompatible && incompatibilityReason && (
                <div className="mt-2 text-xs text-[hsl(var(--text-secondary))] max-w-xl">
                  {incompatibilityReason}
                </div>
              )}
            </div>
            <div className={`flex items-center gap-2 px-4 py-2 rounded-lg font-medium ${compatStyle.bg} ${compatStyle.text}`}>
              {compatStyle.icon}
              <span>{compatibility.compatibility_level}</span>
            </div>
          </div>

          <p className="text-sm text-[hsl(var(--text-secondary))] leading-relaxed">
            {metadata.description}
          </p>

          {((metadata.model_id && metadata.default_filename) || metadata.download_url) && (
            <>
              {isCheckingDownload ? (
                <Button variant="default" size="sm" disabled>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Checking...
                </Button>
              ) : isDownloaded ? (
                <Button
                  variant="default"
                  size="sm"
                  disabled
                  className="bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))] cursor-not-allowed"
                >
                  <Check className="w-4 h-4" />
                  Already Downloaded
                </Button>
              ) : isEmbeddingArchIncompatible ? (
                <Button
                  variant="default"
                  size="sm"
                  disabled
                  title={incompatibilityReason ?? undefined}
                  className="bg-[hsl(var(--surface-muted))] text-[hsl(var(--text-secondary))] cursor-not-allowed"
                >
                  <AlertCircle className="w-4 h-4" />
                  Not Compatible
                </Button>
              ) : (
                <Button
                  variant="default"
                  size="sm"
                  onClick={handleAsyncEvent(handleDownload)}
                  disabled={isDownloading || hasActiveDownload || isStartingDownload}
                >
                  {isStartingDownload ? (
                    <>
                      <Loader2 className="w-4 h-4 animate-spin" />
                      Starting…
                    </>
                  ) : isDownloading ? (
                    <>
                      <Loader2 className="w-4 h-4 animate-spin" />
                      Downloading...
                    </>
                  ) : (
                    <>
                      <Download className="w-4 h-4" />
                      Download Model
                    </>
                  )}
                </Button>
              )}
            </>
          )}

          {(isLlmCategory || isEmbeddingCategory) && (
            <div className="flex flex-wrap gap-2">
              {isLlmCategory && (
                <>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={handleAsyncEvent(handleSetActiveChat)}
                    disabled={!isDownloaded || isSettingChatModel}
                  >
                    {isSettingChatModel ? 'Setting…' : 'Set as Chat LLM'}
                  </Button>
                  {canSetRouter && (
                    <Button
                      variant={isRouterActive ? 'default' : 'secondary'}
                      size="sm"
                      onClick={handleAsyncEvent(handleSetRouter)}
                      disabled={!isDownloaded || isSettingRouterModel}
                    >
                      {isSettingRouterModel
                        ? 'Setting…'
                        : isRouterActive
                          ? 'Router (Active)'
                          : 'Set as Router'}
                    </Button>
                  )}
                </>
              )}
              {isEmbeddingCategory && (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={handleAsyncEvent(handleSetActiveEmbedding)}
                  disabled={!isDownloaded || isSettingEmbeddingModel}
                >
                  {isSettingEmbeddingModel ? 'Setting…' : 'Set as Embedding'}
                </Button>
              )}
            </div>
          )}
        </div>
      </Card>

      {/* Compatibility Breakdown */}
      <Card padding="md">
        <CardHeader>
          <CardTitle className="text-base">Compatibility Analysis</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4 mt-4">
            <ScoreBar score={compatibility.overall_score} label="Overall Compatibility" />
            <ScoreBar score={compatibility.ram_score} label="RAM Compatibility" />
            <ScoreBar score={compatibility.gpu_score} label="GPU Compatibility" />
            <ScoreBar score={compatibility.disk_score} label="Disk Space" />
          </div>

          {compatibility.estimated_tokens_per_second && (
            <div className="mt-4 pt-4 border-t border-[hsl(var(--border-subtle))]">
              <div className="flex items-center gap-2 text-sm">
                <Zap className="w-4 h-4 text-[hsl(var(--accent))]" />
                <span className="text-[hsl(var(--text-secondary))]">Estimated Speed:</span>
                <span className="font-medium text-[hsl(var(--text-primary))]">
                  {compatibility.estimated_tokens_per_second.toFixed(1)} tokens/sec
                </span>
              </div>
            </div>
          )}

          {compatibility.estimated_loading_time_seconds > 0 && (
            <div className="flex items-center gap-2 text-sm mt-2">
              <Cpu className="w-4 h-4 text-[hsl(var(--accent))]" />
              <span className="text-[hsl(var(--text-secondary))]">Load Time:</span>
              <span className="font-medium text-[hsl(var(--text-primary))]">
                ~{compatibility.estimated_loading_time_seconds.toFixed(1)} seconds
              </span>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Recommendations */}
      {compatibility.recommendations.length > 0 && (
        <Card padding="md">
          <CardHeader>
            <div className="flex items-center gap-2">
              <Info className="w-4 h-4 text-[hsl(var(--accent))]" />
              <CardTitle className="text-base">Recommendations</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <ul className="space-y-2 mt-4">
              {compatibility.recommendations.map((rec, idx) => (
                <li key={idx} className="flex items-start gap-2 text-sm text-[hsl(var(--text-secondary))]">
                  <span className="text-[hsl(var(--accent))] mt-1">•</span>
                  <span>{rec}</span>
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      {/* Blockers */}
      {compatibility.blockers.length > 0 && (
        <Card padding="md">
          <CardHeader>
            <div className="flex items-center gap-2">
              <AlertCircle className="w-4 h-4 text-[hsl(var(--danger-fg))]" />
              <CardTitle className="text-base">Compatibility Issues</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <ul className="space-y-2 mt-4">
              {compatibility.blockers.map((blocker, idx) => (
                <li key={idx} className="flex items-start gap-2 text-sm text-[hsl(var(--danger-fg))]">
                  <span className="mt-1">⚠️</span>
                  <span>{blocker}</span>
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      {/* Technical Specifications */}
      <Card padding="md">
        <CardHeader>
          <CardTitle className="text-base">Technical Specifications</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
            <div className="space-y-3">
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Model Size
                </div>
                <div className="flex items-center gap-2 text-sm text-[hsl(var(--text-primary))]">
                  <HardDrive className="w-4 h-4" />
                  {metadata.size_gb > 0 ? `${metadata.size_gb.toFixed(2)} GB` : 'Unknown'}
                </div>
              </div>
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Minimum RAM
                </div>
                <div className="text-sm text-[hsl(var(--text-primary))]">
                  {metadata.minimum_ram_gb.toFixed(1)} GB
                </div>
              </div>
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Recommended RAM
                </div>
                <div className="text-sm text-[hsl(var(--text-primary))]">
                  {metadata.recommended_ram_gb.toFixed(1)} GB
                </div>
              </div>
            </div>

            <div className="space-y-3">
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Context Length
                </div>
                <div className="text-sm text-[hsl(var(--text-primary))]">
                  {metadata.context_length.toLocaleString()} tokens
                </div>
              </div>
              {metadata.category === 'Embedding' && metadata.embedding_dimensions && (
                <div>
                  <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                    Embedding Dimensions
                  </div>
                  <div className="text-sm text-[hsl(var(--text-primary))]">
                    {metadata.embedding_dimensions}
                  </div>
                </div>
              )}
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Downloads
                </div>
                <div className="text-sm text-[hsl(var(--text-primary))]">
                  {formattedPopularityDownloads ?? 'Unavailable'}
                </div>
              </div>
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Likes
                </div>
                <div className="text-sm text-[hsl(var(--text-primary))]">
                  {formattedPopularityLikes ?? 'Unavailable'}
                </div>
              </div>
              <div>
                <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">
                  Quantizations
                </div>
                <div className="flex flex-wrap gap-1">
                  {metadata.supported_quantizations.map((quant) => (
                    <span
                      key={quant}
                      className="text-xs px-2 py-0.5 bg-[hsl(var(--surface-raised))] rounded"
                    >
                      {quant}
                    </span>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Capabilities */}
      {metadata.capabilities.length > 0 && (
        <Card padding="md">
          <CardHeader>
            <CardTitle className="text-base">Capabilities</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2 mt-4">
              {metadata.capabilities.map((capability) => (
                <span
                  key={capability}
                  className="px-3 py-1.5 text-sm bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] rounded-lg font-medium"
                >
                  {capability}
                </span>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
