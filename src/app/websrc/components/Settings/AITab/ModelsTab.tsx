/**
 * Models — where model files live, extra folders to scan, and the catalog.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { Boxes, X } from 'lucide-react';

import { ROW_ACTION_CLASS, SECONDARY_BUTTON_CLASS } from './shared';
import { useLlmSettings } from './useLlmSettings';
import { useRegisterPaletteCommands } from '../../../hooks/useRegisterPaletteCommands';
import { VaultAPI } from '../../../lib/api';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import { toast } from '../../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection } from '../../ui';
import { HuggingFaceSettings } from '../HuggingFaceSettings';
import { ModelCatalogBrowser } from '../ModelCatalog';

import type { PaletteCommand } from '../../../stores/paletteCommandsStore';

export function ModelsTab() {
  const { llmSettings, saveLlmUpdates } = useLlmSettings();

  const [modelDownloadPath, setModelDownloadPath] = useState('');
  const [isLoadingModelDownloadPath, setIsLoadingModelDownloadPath] = useState(true);
  const [isModelCatalogExpanded, setIsModelCatalogExpanded] = useState(false);
  const [isAddingExternalDirectory, setIsAddingExternalDirectory] = useState(false);
  const selectedCatalogModel = useModelCatalogStore((state) => state.selectedModel);
  const catalogSectionRef = useRef<HTMLDivElement | null>(null);

  const provider = llmSettings?.provider ?? 'auto';
  const externalModelDirectories = llmSettings?.externalModelDirectories ?? [];

  const loadModelDownloadPath = useCallback(async () => {
    setIsLoadingModelDownloadPath(true);
    try {
      const result = await VaultAPI.getModelDownloadPath();
      setModelDownloadPath(result.ok ? result.data : '');
    } catch {
      // The command can be missing on older builds. Fall through to the
      // error copy instead of leaving the row stuck on "Loading…".
      setModelDownloadPath('');
    } finally {
      setIsLoadingModelDownloadPath(false);
    }
  }, []);

  useEffect(() => {
    void loadModelDownloadPath();
  }, [loadModelDownloadPath]);

  useEffect(() => {
    if (selectedCatalogModel && catalogSectionRef.current) {
      setIsModelCatalogExpanded(true);
      catalogSectionRef.current.scrollIntoView({
        block: 'start',
        behavior: 'smooth',
      });
    }
  }, [selectedCatalogModel]);

  const persistExternalDirectories = async (directories: string[]) => {
    if (!llmSettings) return;

    const normalized = Array.from(
      new Set(directories.map((dir) => dir.trim()).filter((dir) => dir.length > 0))
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
      toast.error("Couldn't add that folder", {
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

  const canBrowseCatalog = provider === 'auto' || provider === 'local';

  const goToChatTab = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'models.browseCatalog',
        label: 'Browse model catalog',
        group: 'Models',
        icon: Boxes,
        run: () => {
          setIsModelCatalogExpanded(true);
          catalogSectionRef.current?.scrollIntoView({ block: 'start', behavior: 'smooth' });
        },
      },
    ],
    [],
  );
  useRegisterPaletteCommands(paletteCommands);

  return (
    <>
      <PageHeader title="Models" />

      <SettingsSection title="Model storage">
        <SettingsRow label="Downloads folder" stacked>
          {isLoadingModelDownloadPath ? (
            <p className="text-sm text-text-muted">Loading…</p>
          ) : modelDownloadPath ? (
            <p className="break-all font-mono text-xs text-text-primary">{modelDownloadPath}</p>
          ) : (
            <div className="flex items-center gap-2">
              <p className="text-sm text-text-muted">
                Couldn&apos;t read the models folder. Check that the app has disk access.
              </p>
              <button
                type="button"
                onClick={() => void loadModelDownloadPath()}
                className="shrink-0 text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Try again
              </button>
            </div>
          )}
        </SettingsRow>
      </SettingsSection>

      <SettingsSection
        title="External model folders"
        actions={
          <button
            type="button"
            onClick={handleAddExternalDirectory}
            disabled={isAddingExternalDirectory}
            className={SECONDARY_BUTTON_CLASS}
          >
            {isAddingExternalDirectory ? 'Adding…' : 'Add folder'}
          </button>
        }
      >
        {externalModelDirectories.length === 0 ? (
          <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
            No external folders.
          </div>
        ) : (
          externalModelDirectories.map((directory) => (
            <div
              key={directory}
              className="flex items-center justify-between gap-4 border-b border-border-subtle py-2.5"
            >
              <span className="truncate font-mono text-xs text-text-primary">{directory}</span>
              <button
                type="button"
                onClick={() => handleRemoveExternalDirectory(directory)}
                className={ROW_ACTION_CLASS}
                aria-label={`Remove external folder ${directory}`}
                title="Remove folder"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          ))
        )}
      </SettingsSection>

      <HuggingFaceSettings />

      <div ref={catalogSectionRef}>
        <SettingsSection
          title="Catalog"
          actions={
            canBrowseCatalog ? (
              <button
                type="button"
                onClick={() => setIsModelCatalogExpanded((previous) => !previous)}
                aria-expanded={isModelCatalogExpanded}
                className={SECONDARY_BUTTON_CLASS}
              >
                {isModelCatalogExpanded ? 'Hide catalog' : 'Browse catalog'}
              </button>
            ) : null
          }
        >
          {!canBrowseCatalog ? (
            <div className="flex items-center gap-2 border-b border-border-subtle py-3">
              <p className="text-sm text-text-muted">
                Local downloads are off while the provider is Ollama.
              </p>
              <button
                type="button"
                onClick={goToChatTab}
                className="text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Change provider
              </button>
            </div>
          ) : isModelCatalogExpanded ? (
            <div className="pt-4">
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
            </div>
          ) : null}
        </SettingsSection>
      </div>
    </>
  );
}
