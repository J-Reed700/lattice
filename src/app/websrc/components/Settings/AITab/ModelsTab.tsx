/**
 * Models Tab - Model storage, external folders, and model catalog browser.
 */

import { useEffect, useRef, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { ChevronDown, ChevronRight, Download, FolderPlus, X } from 'lucide-react';

import { useLlmSettings } from './useLlmSettings';
import { ModelCatalogBrowser } from '../ModelCatalog';
import { VaultAPI } from '../../../lib/api';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import { toast } from '../../../stores/toastStore';

export function ModelsTab() {
  const { llmSettings, saveLlmUpdates } = useLlmSettings();

  const [modelDownloadPath, setModelDownloadPath] = useState('');
  const [isLoadingModelDownloadPath, setIsLoadingModelDownloadPath] = useState(true);
  const [isModelCatalogExpanded, setIsModelCatalogExpanded] = useState(false);
  const [isAddingExternalDirectory, setIsAddingExternalDirectory] = useState(false);
  const selectedCatalogModel = useModelCatalogStore((state) => state.selectedModel);
  const downloadSectionRef = useRef<HTMLElement | null>(null);

  const provider = llmSettings?.provider ?? 'auto';
  const externalModelDirectories = llmSettings?.externalModelDirectories ?? [];

  useEffect(() => {
    let isActive = true;

    const loadModelDownloadPath = async () => {
      setIsLoadingModelDownloadPath(true);
      const result = await VaultAPI.getModelDownloadPath();
      if (!isActive) return;
      setModelDownloadPath(result.ok ? result.data : '');
      setIsLoadingModelDownloadPath(false);
    };

    loadModelDownloadPath();

    return () => {
      isActive = false;
    };
  }, []);

  useEffect(() => {
    if (selectedCatalogModel && downloadSectionRef.current) {
      setIsModelCatalogExpanded(true);
      downloadSectionRef.current.scrollIntoView({
        block: 'start',
        behavior: 'smooth',
      });
    }
  }, [selectedCatalogModel]);

  const persistExternalDirectories = async (directories: string[]) => {
    if (!llmSettings) return;

    const normalized = Array.from(
      new Set(
        directories
          .map((dir) => dir.trim())
          .filter((dir) => dir.length > 0)
      )
    );

    await saveLlmUpdates({
      externalModelDirectories: normalized,
    });
  };

  const handleAddExternalDirectory = async () => {
    setIsAddingExternalDirectory(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Model Folder',
      });

      if (!selected || typeof selected !== 'string') {
        return;
      }

      if (externalModelDirectories.includes(selected)) {
        toast.info('Folder already added');
        return;
      }

      await persistExternalDirectories([...externalModelDirectories, selected]);
      toast.success('External model folder added');
    } catch (error) {
      toast.error('Failed to add external model folder', {
        message: String(error),
      });
    } finally {
      setIsAddingExternalDirectory(false);
    }
  };

  const handleRemoveExternalDirectory = async (path: string) => {
    await persistExternalDirectories(
      externalModelDirectories.filter((directory) => directory !== path)
    );
  };

  return (
    <div className="space-y-8">
      {/* Header */}
      <div className="flex items-center gap-3 pb-4 border-b border-[hsl(var(--border-subtle))]">
        <div className="p-2 bg-[hsl(var(--accent-muted))] rounded-lg">
          <Download className="w-5 h-5 text-[hsl(var(--accent))]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))]">Models</h2>
          <p className="text-sm text-[hsl(var(--text-secondary))]">
            Model storage, external folders, and catalog browser
          </p>
        </div>
      </div>

      <section className="space-y-4" ref={downloadSectionRef}>
        <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4 space-y-2">
          <div className="text-xs font-medium text-[hsl(var(--text-secondary))]">Model Storage</div>
          {isLoadingModelDownloadPath ? (
            <p className="text-xs text-[hsl(var(--text-tertiary))]">Loading model folder...</p>
          ) : modelDownloadPath ? (
            <code className="block rounded-md border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-2 text-xs text-[hsl(var(--text-primary))]">
              {modelDownloadPath}
            </code>
          ) : (
            <p className="text-xs text-[hsl(var(--text-tertiary))]">
              Unable to read local model folder path right now.
            </p>
          )}
          <p className="text-xs text-[hsl(var(--text-tertiary))]">
            This location is always available, even if your current chat provider is Ollama.
          </p>
        </div>

        <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4 space-y-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h4 className="text-sm font-semibold text-[hsl(var(--text-primary))]">
                External Model Folders
              </h4>
              <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
                Reuse models from LM Studio or other local directories.
              </p>
            </div>
            <button
              type="button"
              onClick={handleAddExternalDirectory}
              disabled={isAddingExternalDirectory}
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-lg bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] hover:opacity-90 disabled:opacity-60 disabled:cursor-not-allowed transition-opacity whitespace-nowrap"
            >
              <FolderPlus className="w-3.5 h-3.5" />
              {isAddingExternalDirectory ? 'Adding...' : 'Add Folder'}
            </button>
          </div>

          {externalModelDirectories.length === 0 ? (
            <p className="text-xs text-[hsl(var(--text-tertiary))]">
              No external model folders configured.
            </p>
          ) : (
            <div className="space-y-2">
              {externalModelDirectories.map((directory) => (
                <div
                  key={directory}
                  className="flex items-center justify-between gap-2 px-3 py-2 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))]"
                >
                  <code className="text-xs text-[hsl(var(--text-primary))] truncate">{directory}</code>
                  <button
                    type="button"
                    onClick={() => handleRemoveExternalDirectory(directory)}
                    className="p-1 text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--danger-fg))] transition-colors"
                    aria-label={`Remove external folder ${directory}`}
                    title="Remove folder"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}

          <p className="text-xs text-[hsl(var(--text-tertiary))]">
            Scans these folders for `.gguf` and `.onnx` files. Removing an external model entry
            from Recall does not delete the original file.
          </p>
        </div>

        {(provider === 'auto' || provider === 'local') ? (
          <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4 space-y-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <h4 className="text-sm font-semibold text-[hsl(var(--text-primary))]">
                  Browse Model Catalog
                </h4>
                <p className="text-xs text-[hsl(var(--text-secondary))] mt-1">
                  Browse public Hugging Face models. Expand only when needed.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setIsModelCatalogExpanded((previous) => !previous)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2.5 py-1.5 text-xs font-medium text-[hsl(var(--text-primary))] hover:border-[hsl(var(--border-default))]"
              >
                {isModelCatalogExpanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                {isModelCatalogExpanded ? 'Collapse' : 'Expand'}
              </button>
            </div>

            {isModelCatalogExpanded ? (
              <ModelCatalogBrowser
                routerModelId={llmSettings?.router?.model}
                onSetRouterModel={async (modelId) => {
                  if (!llmSettings) return;
                  await saveLlmUpdates({
                    router: {
                      ...llmSettings.router,
                      model: modelId,
                    },
                  });
                }}
              />
            ) : (
              <p className="text-xs text-[hsl(var(--text-tertiary))]">
                Catalog hidden to keep this page compact. Expand when you want to search or download models.
              </p>
            )}
          </div>
        ) : (
          <div className="rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4 space-y-2">
            <p className="text-xs text-[hsl(var(--text-secondary))]">
              You are currently using Ollama. Switch to <strong>Auto</strong> or <strong>Local Only</strong> to browse and download local models.
            </p>
          </div>
        )}
      </section>
    </div>
  );
}
