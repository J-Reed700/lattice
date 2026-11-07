import type { KeyboardEvent } from 'react';

import { CodeViewer } from '@/features/explorer/components/CodeViewer';
import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import { LearningCodeEditor } from '@/features/learning/practice/practical/LearningCodeEditor';
import type { CodeReviewDrillDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';

import '@/features/explorer/components/explorer.css';

/**
 * The drill's files as tabs over one code view: read-only with line numbers to pick lines from, or, while the
 * learner writes a fix, an editable copy of every file.
 */
export function CodePane({ drill, activePath, onActivePath, writingFix, selection, onSelectLines, highlight, highlightNonce, fixEdits, onFixEdit }: {
  drill: CodeReviewDrillDto;
  activePath: string;
  onActivePath: (path: string) => void;
  writingFix: boolean;
  /** The lines picked in the open file. */
  selection: ExplorerLineRange | null;
  onSelectLines: (range: ExplorerLineRange | null) => void;
  /** Lines of the open file to scroll to, whenever `highlightNonce` changes. */
  highlight: ExplorerLineRange | null;
  highlightNonce: number | null;
  /** The text of each file the learner changed in their fix, by path. */
  fixEdits: Readonly<Record<string, string>>;
  onFixEdit: (path: string, text: string) => void;
}) {
  const { files } = drill;
  const active = files.find((file) => file.path === activePath) ?? files[0];

  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!step && event.key !== 'Home' && event.key !== 'End') return;
    event.preventDefault();
    const target = event.key === 'Home' ? 0 : event.key === 'End' ? files.length - 1 : (index + step + files.length) % files.length;
    onActivePath(files[target].path);
    document.getElementById(`drill-file-tab-${target}`)?.focus();
  };

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-2xl border border-border bg-surface">
      <div className="flex items-center justify-between gap-2 border-b border-border bg-background p-2">
        <div role="tablist" aria-label="Files of the drill" className="flex min-w-0 flex-wrap gap-1">
          {files.map((file, index) => (
            <button
              key={file.path}
              type="button"
              role="tab"
              id={`drill-file-tab-${index}`}
              aria-selected={file.path === active.path}
              aria-controls="drill-file-panel"
              tabIndex={file.path === active.path ? 0 : -1}
              onClick={() => onActivePath(file.path)}
              onKeyDown={(event) => moveFocus(event, index)}
              className={cn(
                'max-w-full break-all rounded-lg px-3 py-1.5 font-mono text-xs transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent',
                file.path === active.path ? 'bg-surface text-accent shadow-sm' : 'text-text-secondary hover:bg-surface',
              )}
            >
              {file.path}
              {fixEdits[file.path] !== undefined && <span className="sr-only"> (changed in your fix)</span>}
              {fixEdits[file.path] !== undefined && <span aria-hidden="true"> •</span>}
            </button>
          ))}
        </div>
        {writingFix && <span className="shrink-0 text-xs font-medium text-text-secondary">Your fix: an editable copy</span>}
      </div>
      <div
        id="drill-file-panel"
        role="tabpanel"
        aria-labelledby={`drill-file-tab-${files.indexOf(active)}`}
        className="min-h-0 flex-1 overflow-auto"
      >
        {writingFix
          ? files.map((file) => (
            <div key={file.path} hidden={file.path !== active.path}>
              <LearningCodeEditor
                path={file.path}
                showPath={false}
                value={fixEdits[file.path] ?? file.text}
                ariaLabel={`Edit ${file.path}`}
                onChange={(text) => onFixEdit(file.path, text)}
              />
            </div>
          ))
          : (
            <CodeViewer
              key={active.path}
              path={active.path}
              text={active.text}
              language={drill.language}
              selection={selection}
              highlight={highlight}
              highlightNonce={highlightNonce}
              onSelectLines={onSelectLines}
            />
          )}
      </div>
    </div>
  );
}
