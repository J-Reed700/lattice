/**
 * AIModelsTab — "Downloaded models".
 *
 * Every row in the `models` table (local downloads + the synthetic Ollama
 * server row) is eligible for one or more roles (Chat, Utility, Embedding).
 * Whichever model holds a role IS the model used for it — there is no
 * separate provider enum.
 *
 * Layout: hairline rows. {@link ROLES} drives the toggles;
 * `ModelRolesContext` holds the shared state and mutations.
 */

import { useMemo, useState } from 'react';

import { PageHeader, SidebarSearch, TooltipProvider } from '../ui';
import { LocalModelRow } from './modelRoles/LocalModelRow';
import { LocalModelRowSkeleton } from './modelRoles/LocalModelRowSkeleton';
import { ModelRolesProvider, useModelRoles } from './modelRoles/ModelRolesContext';
import { OllamaMetaRow } from './modelRoles/OllamaMetaRow';
import { EmptyState } from '../EmptyState/EmptyState';

function AIModelsTabContent() {
  const { localModels, isLoading } = useModelRoles();
  const [searchQuery, setSearchQuery] = useState('');

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

  return (
    <>
      <PageHeader title="Downloaded models" meta={meta} />

      {localModels.length > 0 ? (
        <div className="mb-6 max-w-sm">
          <SidebarSearch
            value={searchQuery}
            onChange={setSearchQuery}
            placeholder="Search models"
          />
        </div>
      ) : null}

      <div className="border-t border-border-subtle">
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
            <OllamaMetaRow />
          </>
        )}
      </div>

      {!isLoading && localModels.length === 0 && (
        <EmptyState
          title="No models downloaded."
          action={{
            label: 'Browse catalog',
            onClick: () =>
              window.dispatchEvent(
                new CustomEvent('settings:navigate-tab', { detail: { tab: 'models' } }),
              ),
          }}
        />
      )}
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
