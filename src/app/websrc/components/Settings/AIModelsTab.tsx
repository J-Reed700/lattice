/**
 * AIModelsTab
 *
 * Unified model-role-assignment surface. Replaces the old "Downloaded"
 * tab. Each row in the `models` table (local downloads + Ollama tags
 * synced via /api/tags) is eligible for one or more roles (Chat,
 * Utility, Embedding).
 *
 * Visual layout: a CSS grid of cards.
 * - Local models: one card per model, action row generated from ROLES.
 * - Ollama server: one meta-card with three role dropdowns.
 *
 * Design patterns in use:
 * - **Config-driven rendering**: {@link ROLES} drives every button/row.
 * - **Context**: `ModelRolesContext` holds the shared state & mutations.
 * - **Compound-ish composition**: tab → provider → cards.
 *
 * The provider enum (`LLMProvider`) is gone — whichever model is assigned
 * to the chat role IS the chat model.
 */

import { useMemo, useState } from 'react';

import { AnimatePresence, motion } from 'framer-motion';
import { Search, X, PackageOpen } from 'lucide-react';

import { TooltipProvider } from '../ui/tooltip';
import { Icon } from '../ui/Icon';
import { EmptyState } from '../EmptyState/EmptyState';
import { LocalModelCard } from './modelRoles/LocalModelCard';
import { LocalModelCardSkeleton } from './modelRoles/LocalModelCardSkeleton';
import { OllamaMetaCard } from './modelRoles/OllamaMetaCard';
import { ModelRolesProvider, useModelRoles } from './modelRoles/ModelRolesContext';

// Shared transition for any motion in this tab — short, snappy, organic.
// Pulled inline (not from CSS tokens) because Framer Motion needs JS values.
const CARD_SPRING = { type: 'spring' as const, stiffness: 380, damping: 32, mass: 0.6 };

function AIModelsTabContent() {
  const { localModels, isLoading } = useModelRoles();
  const [searchQuery, setSearchQuery] = useState('');

  const filteredLocal = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return localModels;
    return localModels.filter(
      (m) =>
        m.model_name.toLowerCase().includes(q) ||
        m.model_id.toLowerCase().includes(q),
    );
  }, [localModels, searchQuery]);

  return (
    <div className="space-y-5">
      <div className="pb-4 border-b border-[hsl(var(--border-subtle))]">
        <h2 className="text-2xl font-semibold tracking-tight text-[hsl(var(--text-primary))]">
          AI Models
        </h2>
        <p className="text-sm text-[hsl(var(--text-secondary))] mt-1.5">
          Assign models to the Chat, Utility, and Embedding roles. Whichever model
          you pick here is the one the app uses for that role.
        </p>
      </div>

      {localModels.length > 0 && (
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[hsl(var(--text-tertiary))]" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search local models…"
            className="w-full pl-9 pr-9 py-2 text-sm rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] transition-shadow duration-fast"
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))]"
              aria-label="Clear search"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      )}

      {isLoading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <LocalModelCardSkeleton key={i} />
          ))}
        </div>
      ) : (
        <motion.div
          layout
          className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
        >
          <AnimatePresence mode="popLayout">
            {filteredLocal.map((model) => (
              <motion.div
                key={model.id}
                layout
                initial={{ opacity: 0, scale: 0.96 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.96 }}
                transition={CARD_SPRING}
              >
                <LocalModelCard model={model} />
              </motion.div>
            ))}
          </AnimatePresence>
          <motion.div layout transition={CARD_SPRING}>
            <OllamaMetaCard />
          </motion.div>
        </motion.div>
      )}

      {!isLoading && localModels.length === 0 && (
        <EmptyState
          icon={<Icon as={PackageOpen} size={32} />}
          title="No local models yet"
          description="Browse the Model Catalog to download a chat or embedding model — or use the Ollama card above if you've configured a remote server."
          action={{
            label: 'Open Model Catalog',
            onClick: () =>
              window.dispatchEvent(
                new CustomEvent('settings:navigate-tab', { detail: { tab: 'models' } }),
              ),
          }}
        />
      )}
    </div>
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
