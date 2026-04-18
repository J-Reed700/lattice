import { useState, useEffect, useMemo } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { HardDrive, Trash2, Check, Calendar, TrendingUp, Info, AlertCircle, Sparkles, ChevronRight, PackageOpen, Search, X } from 'lucide-react';

import { useDownloadedModels } from '../../hooks/useDownloadedModels';
import { VaultAPI } from '../../lib/api';
import { toast } from '../../stores/toastStore';
import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { ConfirmDialog } from '../ConfirmDialog';
import { Button } from '../ui/button';
import Card from '../ui/Card/Card';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip';

import type { DownloadedModel } from '../../types/downloadedModels';

function formatFileSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `${mb.toFixed(2)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}

function formatDate(isoDate: string | null): string {
  if (!isoDate) return 'Never';
  try {
    return formatDistanceToNow(new Date(isoDate), { addSuffix: true });
  } catch {
    return 'Unknown';
  }
}

function isExternalModel(model: DownloadedModel): boolean {
  const metadata = model.metadata as Record<string, unknown> | null;
  if (!metadata) return false;
  return metadata.source === 'external_directory' || metadata.external === true;
}

interface ModelCardProps {
  model: DownloadedModel;
  routerModelId?: string | null;
  onSetActiveChatModel: (id: string) => Promise<void>;
  onWarmUpActiveChatModel: () => Promise<void>;
  onSetActiveEmbeddingModel: (id: string) => Promise<void>;
  onDelete: (id: string, deleteFile: boolean) => Promise<void>;
  onViewDetails: (model: DownloadedModel) => void;
  onRefresh: () => Promise<void>;
}

function ModelCard({
  model,
  routerModelId,
  onSetActiveChatModel,
  onWarmUpActiveChatModel,
  onSetActiveEmbeddingModel,
  onDelete,
  onViewDetails,
  onRefresh,
}: ModelCardProps) {
  const [isSettingActiveChatModel, setIsSettingActiveChatModel] = useState(false);
  const [isSettingActiveEmbedding, setIsSettingActiveEmbedding] = useState(false);
  const [isWarmingUp, setIsWarmingUp] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const externalModel = isExternalModel(model);

  const handleSetActiveChatModel = async () => {
    setIsSettingActiveChatModel(true);
    try {
      if (model.is_active_for_chat) {
        const result = await VaultAPI.clearActiveChatModel();
        if (!result.ok) {
          throw new Error(result.error);
        }
        toast.success('Chat model deactivated');
      } else {
        await onSetActiveChatModel(model.model_id);
        setIsWarmingUp(true);
        try {
          await onWarmUpActiveChatModel();
          toast.success(`${model.model_name} is active and warmed up`);
        } catch (warmError) {
          toast.warning('Model set, but warm-up failed', {
            message: warmError instanceof Error ? warmError.message : String(warmError),
          });
        } finally {
          setIsWarmingUp(false);
        }
      }
      await onRefresh();
    } catch (error) {
      toast.error('Failed to update chat model', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSettingActiveChatModel(false);
    }
  };

  const handleWarmUpActiveChatModel = async () => {
    setIsWarmingUp(true);
    try {
      await onWarmUpActiveChatModel();
      toast.success(`${model.model_name} is warmed up and ready`);
    } catch (error) {
      toast.error('Failed to warm up model', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsWarmingUp(false);
    }
  };

  const handleSetActiveEmbeddingModel = async () => {
    setIsSettingActiveEmbedding(true);
    try {
      if (model.is_active_for_embedding) {
        const result = await VaultAPI.clearActiveEmbeddingModel();
        if (!result.ok) {
          throw new Error(result.error);
        }
        toast.success('Embedding model deactivated');
      } else {
        await onSetActiveEmbeddingModel(model.model_id);
        toast.success(`${model.model_name} is now the active embedding model`);
      }
      await onRefresh();
    } catch (error) {
      toast.error('Failed to update embedding model', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSettingActiveEmbedding(false);
    }
  };

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await onDelete(model.id, !externalModel);
      toast.success(
        externalModel
          ? `${model.model_name} removed from Recall`
          : `${model.model_name} deleted successfully`
      );
    } catch (error) {
      toast.error('Failed to delete model', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <>
      <Card padding="md" className="h-full relative">
        <div className="flex flex-col h-full">
          <div className="flex items-start justify-between gap-2 mb-3">
            <div className="flex-1 min-w-0">
              <h3 className="font-semibold text-sm text-[hsl(var(--text-primary))] truncate">
                {model.model_name}
              </h3>
              <p className="text-xs text-[hsl(var(--text-tertiary))] mt-0.5 truncate">
                {model.model_id}
              </p>
            </div>
            <div className="flex flex-col gap-1.5 flex-shrink-0">
              {model.is_active_for_chat && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className="flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] cursor-help border border-[hsl(var(--accent-muted))]">
                      <Check className="w-3 h-3" />
                      <span>Chat</span>
                    </div>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Active chat model for conversations</p>
                  </TooltipContent>
                </Tooltip>
              )}
              {model.is_active_for_embedding && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className="flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] cursor-help border border-[hsl(var(--accent-muted))]">
                      <Check className="w-3 h-3" />
                      <span>Embedding</span>
                    </div>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Active embedding model for search</p>
                  </TooltipContent>
                </Tooltip>
              )}
              {routerModelId && model.model_id === routerModelId && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className="flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))] cursor-help border border-[hsl(var(--success-muted))]">
                      <Check className="w-3 h-3" />
                      <span>Router</span>
                    </div>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Active routing model for follow-up detection</p>
                  </TooltipContent>
                </Tooltip>
              )}
            </div>
          </div>

          <div className="flex-1 space-y-2 mb-3">
            <div className="flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
              <HardDrive className="w-3 h-3" />
              <span>{formatFileSize(model.file_size_bytes)}</span>
            </div>
            <div className="flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
              <Calendar className="w-3 h-3" />
              <span>Downloaded {formatDate(model.downloaded_at)}</span>
            </div>
            {model.last_used_at && (
              <div className="flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
                <TrendingUp className="w-3 h-3" />
                <span>Last used {formatDate(model.last_used_at)}</span>
              </div>
            )}
            <div className="text-xs text-[hsl(var(--text-tertiary))]">
              Used {model.use_count} {model.use_count === 1 ? 'time' : 'times'}
            </div>
          </div>

          <div className="flex flex-col gap-2 pt-3 border-t border-[hsl(var(--border-subtle))]">
            {/* Activation/Deactivation Buttons Row */}
            {(() => {
              const isLanguageModel = model.model_type === 'language_model';
              const isEmbeddingModel = model.model_type === 'text_embeddings';

              // Show deactivate buttons for active models
              const showDeactivateChat = model.is_active_for_chat;
              const showDeactivateEmbedding = model.is_active_for_embedding;

              // Show activate buttons for inactive models (filtered by type)
              const showActivateChat = isLanguageModel && !model.is_active_for_chat;
              const showActivateEmbedding = isEmbeddingModel && !model.is_active_for_embedding;

              // Only render if at least one button should show
              const hasButtons = showDeactivateChat || showDeactivateEmbedding || showActivateChat || showActivateEmbedding;
              if (!hasButtons) {
                return null;
              }

              return (
                <div className="flex gap-2">
                  {/* Deactivate Chat Button */}
                  {showDeactivateChat && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={handleAsyncEvent(handleSetActiveChatModel)}
                      disabled={isSettingActiveChatModel}
                      className="flex-1 text-[hsl(var(--warning-fg))] hover:bg-[hsl(var(--warning-muted))]"
                    >
                      {isSettingActiveChatModel ? 'Deactivating...' : 'Deactivate Chat'}
                    </Button>
                  )}

                  {/* Deactivate Embedding Button */}
                  {showDeactivateEmbedding && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={handleAsyncEvent(handleSetActiveEmbeddingModel)}
                      disabled={isSettingActiveEmbedding}
                      className="flex-1 text-[hsl(var(--warning-fg))] hover:bg-[hsl(var(--warning-muted))]"
                    >
                      {isSettingActiveEmbedding ? 'Deactivating...' : 'Deactivate Embedding'}
                    </Button>
                  )}

                  {/* Activate Chat Button */}
                  {showActivateChat && (
                    <Button
                      variant="default"
                      size="sm"
                      onClick={handleAsyncEvent(handleSetActiveChatModel)}
                      disabled={isSettingActiveChatModel}
                      className="flex-1"
                    >
                      {isSettingActiveChatModel ? 'Activating...' : 'Set as Chat'}
                    </Button>
                  )}

                  {/* Activate Embedding Button */}
                  {showActivateEmbedding && (
                    <Button
                      variant="default"
                      size="sm"
                      onClick={handleAsyncEvent(handleSetActiveEmbeddingModel)}
                      disabled={isSettingActiveEmbedding}
                      className="flex-1"
                    >
                      {isSettingActiveEmbedding ? 'Activating...' : 'Set as Embedding'}
                    </Button>
                  )}
                </div>
              );
            })()}

            {/* Details & Delete Row - Always show */}
            <div className="flex gap-2">
              {model.is_active_for_chat && (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={handleAsyncEvent(handleWarmUpActiveChatModel)}
                  disabled={isWarmingUp}
                  className="flex-1"
                >
                  {isWarmingUp ? 'Warming up...' : 'Warm Up'}
                </Button>
              )}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onViewDetails(model)}
                className="flex-1 hover:bg-[hsl(var(--surface-raised))]"
              >
                <Info className="w-3.5 h-3.5 mr-1.5" />
                <span>Details</span>
              </Button>
              {/* Delete button - only show if not active */}
              {!model.is_active_for_chat && !model.is_active_for_embedding && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setShowDeleteConfirm(true)}
                  className="flex-1 relative z-10 pointer-events-auto text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))] hover:text-[hsl(var(--danger-fg))]"
                >
                  <Trash2 className="w-3.5 h-3.5 mr-1.5" />
                  <span>Delete</span>
                </Button>
              )}
            </div>
          </div>
        </div>
      </Card>

      <ConfirmDialog
        isOpen={showDeleteConfirm}
        title={externalModel ? 'Remove model?' : 'Delete model?'}
        message={
          externalModel
            ? `Remove "${model.model_name}" from Recall? The source file will remain on disk.`
            : `Delete "${model.model_name}"? This will remove the model file (${formatFileSize(model.file_size_bytes)}) from your disk.`
        }
        confirmLabel={isDeleting ? 'Deleting...' : (externalModel ? 'Remove' : 'Delete')}
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={handleAsyncEvent(handleDelete)}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </>
  );
}

interface ModelDetailsModalProps {
  model: DownloadedModel;
  onClose: () => void;
}

function ModelDetailsModal({ model, onClose }: ModelDetailsModalProps) {
  return (
    <div className="fixed inset-0 bg-[hsl(var(--overlay))] flex items-center justify-center z-50 p-4">
      <div className="bg-[hsl(var(--surface-raised))] rounded-xl shadow-md max-w-2xl w-full max-h-[80vh] overflow-y-auto p-6 border border-[hsl(var(--border-subtle))]">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-2xl font-bold text-[hsl(var(--text-primary))]">
            {model.model_name}
          </h2>
          <button
            onClick={onClose}
            className="text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-primary))]"
          >
            ✕
          </button>
        </div>

        <div className="space-y-4">
          <div>
            <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">Model ID</div>
            <div className="text-sm text-[hsl(var(--text-primary))]">{model.model_id}</div>
          </div>

          <div>
            <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">File Path</div>
            <div className="text-sm text-[hsl(var(--text-primary))] font-mono break-all bg-[hsl(var(--surface-raised))] p-2 rounded">
              {model.file_path}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">File Size</div>
              <div className="text-sm text-[hsl(var(--text-primary))]">{formatFileSize(model.file_size_bytes)}</div>
            </div>
            <div>
              <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">Use Count</div>
              <div className="text-sm text-[hsl(var(--text-primary))]">{model.use_count}</div>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">Downloaded</div>
              <div className="text-sm text-[hsl(var(--text-primary))]">{formatDate(model.downloaded_at)}</div>
            </div>
            <div>
              <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-1">Last Used</div>
              <div className="text-sm text-[hsl(var(--text-primary))]">{formatDate(model.last_used_at)}</div>
            </div>
          </div>

          {model.metadata && Object.keys(model.metadata).length > 0 && (
            <div>
              <div className="text-xs font-medium text-[hsl(var(--text-secondary))] mb-2">Metadata</div>
              <div className="text-sm text-[hsl(var(--text-primary))] font-mono bg-[hsl(var(--surface-raised))] p-3 rounded max-h-60 overflow-y-auto">
                <pre>{JSON.stringify(model.metadata, null, 2)}</pre>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export function DownloadedModelsTab() {
  return (
    <TooltipProvider delayDuration={300}>
      <DownloadedModelsTabContent />
    </TooltipProvider>
  );
}

function DownloadedModelsTabContent() {
  const {
    fetchDownloadedModels,
    setActiveChatModel,
    warmUpActiveChatModel,
    setActiveEmbeddingModel,
    deleteDownloadedModel,
    getAllDownloadedModels,
  } = useDownloadedModels();

  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedModel, setSelectedModel] = useState<DownloadedModel | null>(null);
  const [routerModelId, setRouterModelId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const downloadedModels = getAllDownloadedModels();

  const filteredModels = useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    if (!q) return downloadedModels;
    return downloadedModels.filter(
      (m) =>
        m.model_name.toLowerCase().includes(q) ||
        m.model_id.toLowerCase().includes(q) ||
        m.model_type?.toLowerCase().includes(q)
    );
  }, [downloadedModels, searchQuery]);

  useEffect(() => {
    const loadModels = async () => {
      try {
        setIsLoading(true);
        setError(null);
        await fetchDownloadedModels();
        const settingsResult = await VaultAPI.getSettings();
        if (settingsResult.ok) {
          setRouterModelId(settingsResult.data.llm.router?.model ?? null);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load downloaded models');
      } finally {
        setIsLoading(false);
      }
    };

    void loadModels();
  }, [fetchDownloadedModels]);

  const handleRefresh = async () => {
    await fetchDownloadedModels();
    const settingsResult = await VaultAPI.getSettings();
    if (settingsResult.ok) {
      setRouterModelId(settingsResult.data.llm.router?.model ?? null);
    }
  };

  const handleSetActiveChatModel = async (modelId: string) => {
    await setActiveChatModel(modelId);
  };

  const handleSetActiveEmbeddingModel = async (modelId: string) => {
    await setActiveEmbeddingModel(modelId);
  };

  const handleDelete = async (id: string, deleteFile: boolean) => {
    await deleteDownloadedModel(id, deleteFile);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="flex items-center gap-3 text-[hsl(var(--text-secondary))]">
          <div className="animate-spin rounded-full h-5 w-5 border-b-2 border-[hsl(var(--accent))]" />
          <span>Loading downloaded models...</span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4">
        <div className="text-sm text-[hsl(var(--danger-fg))] bg-[hsl(var(--danger-muted))] p-4 rounded-lg flex items-center gap-3">
          <AlertCircle className="w-5 h-5 flex-shrink-0" />
          <div>
            <div className="font-medium mb-1">Failed to load models</div>
            <div className="text-xs">{error}</div>
          </div>
        </div>
      </div>
    );
  }

  if (downloadedModels.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-center">
        <div className="p-4 bg-[hsl(var(--surface-raised))] rounded-full mb-4">
          <HardDrive className="w-8 h-8 text-[hsl(var(--text-muted))]" />
        </div>
        <h3 className="text-lg font-semibold text-[hsl(var(--text-primary))] mb-2">
          No Downloaded Models
        </h3>
        <p className="text-sm text-[hsl(var(--text-secondary))] max-w-md mb-4">
          You haven't downloaded any models yet. You can browse the Model Catalog or use Ollama
          as your chat provider.
        </p>

        {/* Actionable Button */}
        <Button
          variant="default"
          size="default"
          onClick={() => {
            toast.info('Navigate to Settings > AI Models to browse the catalog or configure Ollama', { duration: 5000 });
          }}
          className="mb-6"
        >
          Browse Model Catalog
        </Button>

        {/* Quick Guide Steps */}
        <div className="flex items-center justify-center gap-2 text-sm text-[hsl(var(--text-tertiary))]">
          <span className="flex items-center gap-1.5">
            <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] text-xs font-bold">
              1
            </span>
            Browse
          </span>
          <ChevronRight className="w-4 h-4" />
          <span className="flex items-center gap-1.5">
            <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] text-xs font-bold">
              2
            </span>
            Download
          </span>
          <ChevronRight className="w-4 h-4" />
          <span className="flex items-center gap-1.5">
            <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] text-xs font-bold">
              3
            </span>
            Activate
          </span>
        </div>
      </div>
    );
  }

  const totalSize = downloadedModels.reduce((sum, model) => sum + model.file_size_bytes, 0);
  const activeModel = downloadedModels.find(m => m.is_active_for_chat);
  const activeEmbeddingModel = downloadedModels.find(m => m.is_active_for_embedding);

  return (
    <div className="space-y-6">
    <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
      <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
        <PackageOpen className="w-5 h-5 text-[hsl(var(--accent))]" />
      </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Downloaded Models</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Manage your locally downloaded AI models
          </p>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 md:grid-cols-4 mb-6">
        <Card padding="md">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
              <HardDrive className="w-5 h-5 text-[hsl(var(--accent))]" />
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--text-secondary))]">Total Models</div>
              <div className="text-xl font-bold text-[hsl(var(--text-primary))]">
                {downloadedModels.length}
              </div>
            </div>
          </div>
        </Card>

        <Card padding="md">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
              <HardDrive className="w-5 h-5 text-[hsl(var(--accent))]" />
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--text-secondary))]">Total Size</div>
              <div className="text-xl font-bold text-[hsl(var(--text-primary))]">
                {formatFileSize(totalSize)}
              </div>
            </div>
          </div>
        </Card>

        <Card padding="md">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-[hsl(var(--success-muted))] rounded-lg">
              <Check className="w-5 h-5 text-[hsl(var(--success-fg))]" />
            </div>
            <div>
              <div className="text-xs text-[hsl(var(--text-secondary))]">Active Model</div>
              <div className="text-sm font-semibold text-[hsl(var(--text-primary))] truncate">
                {activeModel ? activeModel.model_name : 'None'}
              </div>
            </div>
          </div>
        </Card>

        {/* Active Embedding Model Card */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Card padding="md" className="cursor-help">
              <div className="flex items-center gap-3">
                <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
                  <Sparkles className="w-5 h-5 text-[hsl(var(--accent))]" />
                </div>
                <div>
                  <div className="text-xs text-[hsl(var(--text-secondary))]">Active Embedding</div>
                  <div className="text-sm font-semibold text-[hsl(var(--text-primary))] truncate">
                    {activeEmbeddingModel?.model_name || 'None'}
                  </div>
                </div>
              </div>
            </Card>
          </TooltipTrigger>
          <TooltipContent>
            <p>The model currently used for semantic search operations</p>
          </TooltipContent>
        </Tooltip>
      </div>

      {/* Search bar */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[hsl(var(--text-tertiary))]" />
        <input
          type="text"
          placeholder="Filter models..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="w-full pl-9 pr-8 py-2 text-sm rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus:outline-none focus:ring-1 focus:ring-[hsl(var(--accent))] focus:border-[hsl(var(--accent))]"
        />
        {searchQuery && (
          <button
            onClick={() => setSearchQuery('')}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))]"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {filteredModels.length === 0 ? (
        <div className="text-center py-8 text-sm text-[hsl(var(--text-tertiary))]">
          No models matching "{searchQuery}"
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              routerModelId={routerModelId}
              onSetActiveChatModel={handleSetActiveChatModel}
              onWarmUpActiveChatModel={warmUpActiveChatModel}
              onSetActiveEmbeddingModel={handleSetActiveEmbeddingModel}
              onDelete={handleDelete}
              onViewDetails={setSelectedModel}
              onRefresh={handleRefresh}
            />
          ))}
        </div>
      )}

      {selectedModel && (
        <ModelDetailsModal
          model={selectedModel}
          onClose={() => setSelectedModel(null)}
        />
      )}
    </div>
  );
}
