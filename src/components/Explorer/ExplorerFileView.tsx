import { useQuery } from '@tanstack/react-query';
import { FileQuestion, X } from 'lucide-react';

import { VaultAPI } from '@/lib/api';
import { useExplorerStore } from '@/stores/explorerStore';

import { CodeViewer } from './CodeViewer';
import { describeLines } from './describeLines';

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The middle column: the open file, read-only. */
export function ExplorerFileView({ root }: { root: string }) {
  const openPath = useExplorerStore((state) => state.openPath);
  const selection = useExplorerStore((state) => state.selection);
  const highlight = useExplorerStore((state) => state.highlight);
  const setSelection = useExplorerStore((state) => state.setSelection);

  const file = useQuery({
    queryKey: ['explorer', 'file', root, openPath],
    queryFn: async () => {
      const result = await VaultAPI.explorerReadFile(root, openPath!);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: Boolean(openPath),
    staleTime: 5_000,
  });

  if (!openPath) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center">
        <p className="max-w-xs text-sm text-text-muted">Open a file from the tree. Click line numbers to pick lines for the chat; references in answers light up here.</p>
      </div>
    );
  }

  const data = file.data;
  const text = data && !data.binary && !data.tooLarge ? data.text : null;
  const activeHighlight = highlight?.path === openPath ? highlight : null;
  const segments = openPath.split('/');

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b border-border-subtle px-3">
        <p className="min-w-0 flex-1 truncate text-[12.5px] text-text-secondary" title={openPath}>
          {segments.length > 1 && <span className="text-text-muted">{segments.slice(0, -1).join(' / ')} / </span>}
          <span className="font-medium text-text-primary">{segments[segments.length - 1]}</span>
        </p>
        {selection && (
          <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-sm bg-[hsl(var(--text-primary)/0.06)] pl-1.5 pr-0.5 text-[11px] tabular-nums text-text-secondary">
            {describeLines(selection)}
            <button
              type="button"
              aria-label="Clear line selection"
              onClick={() => setSelection(null)}
              className="inline-flex h-4 w-4 items-center justify-center rounded-sm text-text-tertiary hover:bg-[hsl(var(--text-primary)/0.08)] hover:text-text-primary"
            >
              <X className="h-3 w-3" strokeWidth={2} />
            </button>
          </span>
        )}
        {data && !data.binary && !data.tooLarge && (
          <span className="shrink-0 text-[11px] tabular-nums text-text-muted">
            {data.lineCount.toLocaleString()} {data.lineCount === 1 ? 'line' : 'lines'}{data.language ? ` · ${data.language}` : ''}
          </span>
        )}
      </div>
      <div className="min-h-0 flex-1">
        {file.isPending ? (
          <p className="px-4 py-3 text-xs text-text-muted">Opening…</p>
        ) : file.isError ? (
          <p role="alert" className="px-4 py-3 text-xs text-danger-fg">{file.error.message}</p>
        ) : text !== null ? (
          <CodeViewer
            path={openPath}
            text={text}
            language={data?.language ?? null}
            selection={selection}
            highlight={activeHighlight?.range ?? null}
            highlightNonce={activeHighlight?.nonce ?? null}
            onSelectLines={setSelection}
          />
        ) : data ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
            <FileQuestion className="h-5 w-5 text-text-muted" strokeWidth={1.5} aria-hidden="true" />
            <p className="text-sm text-text-secondary">
              {data.binary ? 'This is a binary file.' : 'This file is too large to show.'}
            </p>
            <p className="text-xs text-text-muted">{formatBytes(data.sizeBytes)}</p>
          </div>
        ) : null}
      </div>
    </div>
  );
}
