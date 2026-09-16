import { type TypeBucket } from './hooks/useCorpusIdentity';

interface TypeFacetsProps {
  buckets: TypeBucket[];
  /** The active `filterByType` from the store, or null. */
  activeType: string | null;
  onSelect: (_type: string | null) => void;
}

const MAX_FACETS = 6;

/**
 * What the vault is made of, as one muted line you can click.
 *
 * Counts come from the unfiltered document list, so they do not move while a
 * facet is active — the line stays a readout of the whole library.
 */
export function TypeFacets({ buckets, activeType, onSelect }: TypeFacetsProps) {
  const visible = buckets.slice(0, MAX_FACETS);
  if (visible.length === 0) return null;

  return (
    <div
      role="tablist"
      aria-label="Filter by type"
      className="-mt-1 flex flex-wrap items-center gap-x-3 pb-3"
    >
      {visible.map((bucket, index) => {
        const isActive = activeType === bucket.label;
        return (
          // `role="presentation"` keeps the tab a direct child of the tablist in
          // the accessibility tree; the wrapper only carries the `·` separator.
          <span key={bucket.label} role="presentation" className="flex items-center gap-x-3">
            {index > 0 ? (
              <span aria-hidden className="text-text-muted">
                ·
              </span>
            ) : null}
            <button
              type="button"
              role="tab"
              aria-selected={isActive}
              onClick={() => onSelect(isActive ? null : bucket.label)}
              className={`text-xs tabular-nums transition-colors duration-fast ${
                isActive ? 'text-text-primary' : 'text-text-muted hover:text-text-secondary'
              }`}
            >
              {bucket.count.toLocaleString()} {bucket.label}
            </button>
          </span>
        );
      })}
      {activeType ? (
        <button
          type="button"
          onClick={() => onSelect(null)}
          className="text-xs text-text-muted transition-colors duration-fast hover:text-text-secondary"
        >
          Clear
        </button>
      ) : null}
    </div>
  );
}
