import { useEffect, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { Search, X } from 'lucide-react';

import { VaultAPI } from '@/lib/api';
import { useExplorerStore } from '@/stores/explorerStore';

const MIN_QUERY = 2;
const DEBOUNCE_MS = 250;

/**
 * Search the folder's text. While there is a query its matches replace the
 * tree; picking one opens the file on that line. Ignored files are skipped by
 * the backend, as they are for the model.
 */
export function ExplorerSearch({ root, onActiveChange }: { root: string; onActiveChange: (_active: boolean) => void }) {
  const reveal = useExplorerStore((state) => state.reveal);
  const [input, setInput] = useState('');
  const [query, setQuery] = useState('');

  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(input.trim()), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [input]);

  const active = query.length >= MIN_QUERY;
  useEffect(() => onActiveChange(active), [active, onActiveChange]);

  const results = useQuery({
    queryKey: ['explorer', 'search', root, query],
    queryFn: async () => {
      const result = await VaultAPI.explorerSearch(root, query, { maxResults: 200 });
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: active,
    staleTime: 5_000,
  });

  return (
    <>
      <div className="relative shrink-0 border-b border-border-subtle px-2 py-1.5">
        <Search className="pointer-events-none absolute left-4 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted" strokeWidth={1.75} aria-hidden="true" />
        <input
          type="search"
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => { if (event.key === 'Escape') setInput(''); }}
          placeholder="Search in folder"
          aria-label="Search in folder"
          className="h-7 w-full rounded-md bg-[hsl(var(--text-primary)/0.04)] pl-7 pr-7 text-[12.5px] text-text-primary placeholder:text-text-muted focus:outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-search-cancel-button]:hidden"
        />
        {input && (
          <button
            type="button"
            aria-label="Clear search"
            onClick={() => setInput('')}
            className="absolute right-3.5 top-1/2 inline-flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-sm text-text-tertiary hover:text-text-primary"
          >
            <X className="h-3 w-3" strokeWidth={2} />
          </button>
        )}
      </div>
      {active && (
        <div className="min-h-0 flex-1 overflow-y-auto py-1" aria-label="Search results">
          {results.isPending ? (
            <p className="px-4 py-2 text-xs text-text-muted">Searching…</p>
          ) : results.isError ? (
            <p role="alert" className="px-4 py-2 text-xs text-danger-fg">{results.error.message}</p>
          ) : results.data.matches.length === 0 ? (
            <p className="px-4 py-2 text-xs text-text-muted">No matches in {results.data.filesScanned.toLocaleString()} files.</p>
          ) : (
            <>
              {results.data.matches.map((match) => (
                <button
                  key={`${match.path}:${match.line}:${match.column}`}
                  type="button"
                  onClick={() => reveal(match.path, { startLine: match.line, endLine: match.line })}
                  className="row-hover flex w-full flex-col px-3 py-1 text-left"
                >
                  <span className="truncate text-[11.5px] text-text-muted">{match.path}:{match.line}</span>
                  <span className="truncate font-mono text-[12px] text-text-primary">{match.preview.trim()}</span>
                </button>
              ))}
              {results.data.truncated && <p className="px-3 py-2 text-[11px] text-text-muted">More matches than shown. Narrow the search.</p>}
            </>
          )}
        </div>
      )}
    </>
  );
}
