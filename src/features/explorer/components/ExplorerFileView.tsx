import { useEffect } from 'react';

import { useQuery } from '@tanstack/react-query';
import { ChevronLeft, ChevronRight, FileQuestion, X } from 'lucide-react';

import { useExplorerStore } from '@/features/explorer/stores/explorerStore';
import { VaultAPI } from '@/lib/api';
import { resolveLanguage } from '@/lib/code/languages';

import { CodeViewer } from './CodeViewer';
import { describeLines } from '../model/describeLines';

/**
 * Files read once stay in memory for half an hour, so a link back to one
 * opens it at once. A file older than a minute (or any, when the window
 * regains focus) is read again behind what is shown, so edits made in
 * another editor still come through.
 */
const FILE_CACHE = { staleTime: 60_000, gcTime: 30 * 60_000 } as const;

/**
 * The open path named no file. Answers often cite a file by its name alone
 * (`effect_api.hpp:12`) or by an abbreviated path. The resolver returns only
 * the strongest matches that respect its named folders: one opens at the same
 * lines, several are offered, and none says so.
 */
function MissingFile({ root, path, message }: { root: string; path: string; message: string }) {
  const retarget = useExplorerStore((state) => state.retarget);
  const located = useQuery({
    queryKey: ['explorer', 'locate', root, path],
    queryFn: async () => {
      const result = await VaultAPI.explorerLocateFile(root, path);
      if (!result.ok) throw new Error(result.error);
      // The path itself comes back when the file is there but would not read.
      return { exists: result.data.includes(path), matches: result.data.filter((match) => match !== path) };
    },
    ...FILE_CACHE,
  });
  const matches = located.data?.matches ?? [];
  const only = matches.length === 1 ? matches[0] : null;

  useEffect(() => {
    if (only) retarget(path, only);
  }, [only, path, retarget]);

  if (located.isPending || only) {
    return <p className="px-4 py-3 text-xs text-text-muted">Looking for {path} in this folder…</p>;
  }
  if (matches.length > 1) {
    return (
      <div className="h-full overflow-y-auto px-4 py-3">
        <p className="text-xs text-text-secondary">
          More than one file matches <code className="font-mono">{path}</code>. Choose one:
        </p>
        <ul className="mt-2 flex flex-col gap-0.5">
          {matches.map((match) => (
            <li key={match}>
              <button
                type="button"
                onClick={() => retarget(path, match)}
                className="w-full truncate rounded-sm px-2 py-1 text-left font-mono text-[12px] text-text-primary hover:bg-[hsl(var(--text-primary)/0.06)]"
                title={match}
              >
                {match}
              </button>
            </li>
          ))}
        </ul>
      </div>
    );
  }
  return (
    <p role="alert" className="px-4 py-3 text-xs text-danger-fg">
      {located.error?.message ?? message}
      {located.data && !located.data.exists && ' No file in this folder matches that path.'}
    </p>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().includes('MAC');
const MOD = isMac ? '⌘' : 'Ctrl+';

const NAV_BUTTON =
  'inline-flex h-6 w-6 items-center justify-center rounded-sm text-text-tertiary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.08)] hover:text-text-primary disabled:pointer-events-none disabled:opacity-35';

/** The middle column: the open file, read-only. */
export function ExplorerFileView({ root }: { root: string }) {
  const openPath = useExplorerStore((state) => state.openPath);
  const selection = useExplorerStore((state) => state.selection);
  const highlight = useExplorerStore((state) => state.highlight);
  const setSelection = useExplorerStore((state) => state.setSelection);
  const canGoBack = useExplorerStore((state) => state.back.length > 0);
  const canGoForward = useExplorerStore((state) => state.forward.length > 0);
  const goBack = useExplorerStore((state) => state.goBack);
  const goForward = useExplorerStore((state) => state.goForward);

  // ⌘[ and ⌘] step through the files opened, as in a browser or an editor.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mod = isMac ? event.metaKey : event.ctrlKey;
      if (!mod || event.altKey || event.shiftKey) return;
      if (event.key !== '[' && event.key !== ']') return;
      event.preventDefault();
      if (event.key === '[') goBack();
      else goForward();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [goBack, goForward]);

  const file = useQuery({
    queryKey: ['explorer', 'file', root, openPath],
    queryFn: async () => {
      const result = await VaultAPI.explorerReadFile(root, openPath!);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    enabled: Boolean(openPath),
    ...FILE_CACHE,
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
  const languageLabel = data ? resolveLanguage(openPath, data.language)?.label ?? data.language : null;
  const segments = openPath.split('/');

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b border-border-subtle px-3">
        <div className="-ml-1.5 flex shrink-0 items-center">
          <button type="button" aria-label="Back" title={`Back (${MOD}[)`} disabled={!canGoBack} onClick={goBack} className={NAV_BUTTON}>
            <ChevronLeft className="h-4 w-4" strokeWidth={1.75} />
          </button>
          <button type="button" aria-label="Forward" title={`Forward (${MOD}])`} disabled={!canGoForward} onClick={goForward} className={NAV_BUTTON}>
            <ChevronRight className="h-4 w-4" strokeWidth={1.75} />
          </button>
        </div>
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
            {data.lineCount.toLocaleString()} {data.lineCount === 1 ? 'line' : 'lines'}{languageLabel ? ` · ${languageLabel}` : ''}
          </span>
        )}
      </div>
      <div className="min-h-0 flex-1">
        {file.isPending ? (
          <p className="px-4 py-3 text-xs text-text-muted">Opening…</p>
        ) : file.isError ? (
          <MissingFile root={root} path={openPath} message={file.error.message} />
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
