/**
 * AIModelsTab — "Downloaded models".
 *
 * Every row in the `models` table (local downloads + the synthetic Ollama
 * server row) is eligible for one or more roles (Chat, Utility, Embedding).
 * Chat selection also updates the LLM provider, which takes precedence over
 * the stored local model assignment. llama.cpp uses its saved connection.
 *
 * Layout: hairline rows. {@link ROLES} drives the toggles;
 * `ModelRolesContext` holds the shared state and mutations.
 */

import { useMemo, useState } from 'react';

import { ArrowUpRight, HardDrive, Network } from 'lucide-react';

import { useSettingsQuery } from '../../hooks/queries/useSettingsQuery';
import { PageHeader, SettingsSection, SidebarSearch, TooltipProvider } from '../ui';
import { configuredConnections } from './modelRoles/configuredConnections';
import { LlamaCppMetaRow } from './modelRoles/LlamaCppMetaRow';
import { LocalModelRow } from './modelRoles/LocalModelRow';
import { LocalModelRowSkeleton } from './modelRoles/LocalModelRowSkeleton';
import { ModelRolesProvider, useModelRoles } from './modelRoles/ModelRolesContext';
import { OllamaMetaRow } from './modelRoles/OllamaMetaRow';
import { SECONDARY_BUTTON_CLASS } from './settingsStyles';
import { EmptyState } from '../EmptyState/EmptyState';

function AIModelsTabContent() {
  const { localModels, isLoading, error, refresh } = useModelRoles();
  const { data: settings } = useSettingsQuery();
  const [searchQuery, setSearchQuery] = useState('');
  const connections = configuredConnections(settings?.llm);

  const filteredLocal = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return localModels;
    return localModels.filter(
      (m) => m.model_name.toLowerCase().includes(q) || m.model_id.toLowerCase().includes(q),
    );
  }, [localModels, searchQuery]);

  const meta = isLoading
    ? undefined
    : `${localModels.length} ${localModels.length === 1 ? 'model' : 'models'}`;
  const browseCatalog = () => window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'models' } }));

  return (
    <>
      <PageHeader title="Downloaded models" meta={meta} description="Manage local files and choose which models power each task." actions={
        <button type="button" onClick={browseCatalog} className={SECONDARY_BUTTON_CLASS}>Browse catalog <ArrowUpRight className="ml-2 h-4 w-4" aria-hidden="true" /></button>
      } />

      <div className="mb-6 flex flex-wrap gap-x-5 gap-y-2 rounded-lg border border-accent/20 bg-accent-muted px-4 py-3 text-xs leading-relaxed text-text-secondary">
        <span><strong className="font-semibold text-accent">Chat</strong> · Conversations</span>
        <span><strong className="font-semibold text-accent">Utility</strong> · Background tasks</span>
        <span><strong className="font-semibold text-accent">Embedding</strong> · Library search</span>
      </div>
      <SettingsSection title="On this computer" actions={<HardDrive className="h-4 w-4 text-text-muted" aria-hidden="true" />}>
      {localModels.length > 0 ? (
        <div className="border-b border-border-subtle py-3">
          <SidebarSearch
            value={searchQuery}
            onChange={setSearchQuery}
            placeholder="Search models"
          />
        </div>
      ) : null}

      <div>
        {isLoading ? (
          Array.from({ length: 4 }).map((_, i) => <LocalModelRowSkeleton key={i} />)
        ) : (
          <>
            {filteredLocal.map((model) => (
              <LocalModelRow key={model.id} model={model} />
            ))}
            {searchQuery && filteredLocal.length === 0 ? (
              <div className="border-b border-border-subtle py-3 text-sm text-text-muted">
                No models match &ldquo;{searchQuery}&rdquo;.
              </div>
            ) : null}
          </>
        )}
      </div>

      {!isLoading && error ? (
        <EmptyState
          title="Couldn't load your models."
          description={error}
          action={{ label: 'Try again', onClick: () => void refresh() }}
        />
      ) : null}

      {!isLoading && !error && localModels.length === 0 && (
        <EmptyState
          title="No models downloaded."
          description="Download a model from the catalog to run it on this computer."
        />
      )}
      </SettingsSection>
      {!isLoading && (connections.llamaCpp || connections.ollama) ? (
        <SettingsSection title="Connected servers" description="Use models served by your configured connections." actions={<Network className="h-4 w-4 text-text-muted" aria-hidden="true" />}>
          {connections.llamaCpp ? <LlamaCppMetaRow /> : null}
          {connections.ollama ? <OllamaMetaRow /> : null}
        </SettingsSection>
      ) : null}
    </>
  );
}

export function AIModelsTab() {
  return (
    <TooltipProvider delayDuration={300}>
      <ModelRolesProvider>
        <AIModelsTabContent />
      </ModelRolesProvider>
    </TooltipProvider>
  );
}
