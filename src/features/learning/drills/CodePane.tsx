import { CodeViewer } from '@/features/explorer/components/CodeViewer';
import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';
import { LearningCodeEditor } from '@/features/learning/practice/practical/LearningCodeEditor';
import { FileTabs } from '@/features/learning/practice/shared/FileTabs';
import type { CodeReviewDrillDto } from '@/lib/bindings';

import '@/features/explorer/components/explorer.css';

/**
 * The drill's files as tabs over one code view: read-only with line numbers to pick lines from, or, while the
 * learner writes a fix, an editable copy of every file.
 */
export function CodePane({
  drill,
  activePath,
  onActivePath,
  writingFix,
  selection,
  onSelectLines,
  highlight,
  highlightNonce,
  fixEdits,
  onFixEdit,
}: {
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

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-2xl border border-border-default bg-surface">
      <FileTabs
        ariaLabel="Files of the drill"
        files={files.map((file) => ({
          path: file.path,
          badge:
            fixEdits[file.path] !== undefined ? (
              <>
                <span className="sr-only"> (changed in your fix)</span>
                <span aria-hidden="true"> •</span>
              </>
            ) : null,
        }))}
        activePath={active.path}
        onSelect={onActivePath}
        idBase="drill-file-tab"
        panelId="drill-file-panel"
        actions={
          writingFix ? (
            <span className="shrink-0 text-xs font-medium text-text-secondary">
              Your fix: an editable copy
            </span>
          ) : undefined
        }
      />

      <div
        id="drill-file-panel"
        role="tabpanel"
        aria-labelledby={`drill-file-tab-${files.indexOf(active)}`}
        className="min-h-0 flex-1 overflow-auto"
      >
        {writingFix ? (
          files.map((file) => (
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
        ) : (
          <CodeViewer
            key={active.path}
            path={active.path}
            text={active.text}
            language={drill.material.kind === 'code' ? drill.material.language : ''}
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
