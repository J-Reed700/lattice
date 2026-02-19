/**
 * ModelFilterPanel
 *
 * Filter controls for category, size, and performance tier
 */

import { Filter, X } from 'lucide-react';

import { useModelCatalogStore } from '../../../stores/modelCatalogStore';

import type { ModelCategory, PerformanceTier } from '../../../types/modelCatalog';

export function ModelFilterPanel() {
  const filters = useModelCatalogStore((state) => state.filters);
  const sortBy = useModelCatalogStore((state) => state.sortBy);
  const setFilters = useModelCatalogStore((state) => state.setFilters);
  const setSortBy = useModelCatalogStore((state) => state.setSortBy);

  const categories: Array<{ value: ModelCategory; label: string }> = [
    { value: 'LLM', label: 'LLM' },
    { value: 'Embedding', label: 'Embedding' },
    { value: 'OCR', label: 'OCR' },
  ];

  const tiers: Array<{ value: PerformanceTier; label: string; desc: string }> = [
    { value: 'Fast', label: 'Fast', desc: 'Quick responses' },
    { value: 'Balanced', label: 'Balanced', desc: 'Good tradeoff' },
    { value: 'Accurate', label: 'Accurate', desc: 'Best quality' },
  ];

  const hasActiveFilters =
    filters.category !== null ||
    filters.max_size_gb !== null ||
    filters.required_capabilities.length > 0;

  const clearFilters = () => {
    setFilters({
      category: null,
      max_size_gb: null,
      required_capabilities: [],
      query_text: filters.query_text, // Keep search query
    });
  };

  return (
    <div className="space-y-5 p-5 bg-[linear-gradient(165deg,var(--surface-elevated),var(--bg-secondary))] border border-[var(--border-color)] rounded-xl shadow-[0_10px_30px_rgba(0,0,0,0.14)]">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Filter className="w-4 h-4 text-[var(--accent-primary)]" />
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">Filters</h3>
        </div>
        {hasActiveFilters && (
          <button
            onClick={clearFilters}
            className="text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center gap-1 transition-colors"
          >
            <X className="w-3 h-3" />
            Clear
          </button>
        )}
      </div>

      {/* Sort */}
      <div className="space-y-2">
        <label
          htmlFor="model-sort-order"
          className="text-xs font-semibold tracking-wide uppercase text-[var(--text-secondary)]"
        >
          Sort By
        </label>
        <select
          id="model-sort-order"
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value as typeof sortBy)}
          className="w-full px-3 py-2 text-xs font-medium bg-[var(--bg-tertiary)] text-[var(--text-primary)] border border-[var(--border-color)] rounded-lg focus:outline-none focus:ring-2 focus:ring-[var(--accent-primary)]"
        >
          <option value="recommended">Recommended</option>
          <option value="popularity">Popularity (Downloads)</option>
          <option value="likes">Likes</option>
        </select>
      </div>

      {/* Category Filter */}
      <div className="space-y-2">
        <label className="text-xs font-semibold tracking-wide uppercase text-[var(--text-secondary)]">Category</label>
        <div className="grid grid-cols-3 gap-2">
          {categories.map((cat) => (
            <button
              key={cat.value}
              onClick={() =>
                setFilters({
                  category: filters.category === cat.value ? null : cat.value,
                })
              }
              className={`
                px-2 py-2 text-[11px] font-semibold rounded-lg border transition-all truncate
                ${
                  filters.category === cat.value
                    ? 'bg-[var(--accent-primary)] text-white border-[var(--accent-primary)]'
                    : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] border-[var(--border-color)] hover:border-[var(--accent-primary)]'
                }
              `}
            >
              {cat.label}
            </button>
          ))}
        </div>
      </div>

      {/* Size Filter */}
      <div className="space-y-2">
        <label className="text-xs font-semibold tracking-wide uppercase text-[var(--text-secondary)]">
          Max Size: {filters.max_size_gb ? `${filters.max_size_gb} GB` : 'Any'}
        </label>
        <input
          type="range"
          min="1"
          max="20"
          step="1"
          value={filters.max_size_gb || 20}
          onChange={(e) =>
            setFilters({
              max_size_gb: parseInt(e.target.value) === 20 ? null : parseInt(e.target.value),
            })
          }
          className="w-full h-2 bg-[var(--bg-tertiary)] rounded-lg appearance-none cursor-pointer accent-[var(--accent-primary)]"
        />
        <div className="flex justify-between text-xs text-[var(--text-tertiary)]">
          <span>1 GB</span>
          <span>20 GB</span>
        </div>
      </div>

      {/* Performance Tier Filter */}
      <div className="space-y-2">
        <label className="text-xs font-semibold tracking-wide uppercase text-[var(--text-secondary)]">
          Performance Tier
        </label>
        <div className="space-y-2">
          {tiers.map((tier) => {
            const isActive = filters.required_capabilities.includes(tier.value.toLowerCase());
            return (
              <button
                key={tier.value}
                onClick={() => {
                  const capability = tier.value.toLowerCase();
                  setFilters({
                    required_capabilities: isActive
                      ? filters.required_capabilities.filter((c) => c !== capability)
                      : [...filters.required_capabilities, capability],
                  });
                }}
                className={`
                  w-full px-3 py-2 text-left rounded-lg border transition-all
                  ${
                    isActive
                      ? 'bg-[var(--accent-light)] border-[var(--accent-primary)]'
                      : 'bg-[var(--bg-tertiary)] border-[var(--border-color)] hover:border-[var(--accent-primary)]'
                  }
                `}
              >
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-xs font-medium text-[var(--text-primary)]">
                      {tier.label}
                    </div>
                    <div className="text-xs text-[var(--text-secondary)] mt-0.5">
                      {tier.desc}
                    </div>
                  </div>
                  {isActive && (
                    <div className="w-4 h-4 rounded-full bg-[var(--accent-primary)] flex items-center justify-center">
                      <svg className="w-3 h-3 text-white" fill="currentColor" viewBox="0 0 20 20">
                        <path
                          fillRule="evenodd"
                          d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                          clipRule="evenodd"
                        />
                      </svg>
                    </div>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
