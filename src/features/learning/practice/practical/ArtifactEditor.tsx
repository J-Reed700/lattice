import { AlertTriangle, LoaderCircle, Play } from 'lucide-react';

import { LearningCodeEditor } from '@/features/learning/practice/practical/LearningCodeEditor';
import type { useLearningLabDraft } from '@/features/learning/practice/practical/useLearningLabDraft';
import { FileTabs } from '@/features/learning/practice/shared/FileTabs';
import type {
  LearningBuiltinRuntimeCapability,
  LearningLabFile,
  LearningLabRuntimeCapability,
  LearningPracticalActivityDto,
  LearningPracticalRunDto,
} from '@/lib/bindings';

export function ArtifactEditor({
  activeActivity,
  draft,
  editorFiles,
  selectedFile,
  setSelectedFilePath,
  canRunActivity,
  startingRun,
  runPending,
  activeRun,
  beginRun,
  builtinRuntime,
  capability,
}: {
  activeActivity: LearningPracticalActivityDto;
  draft: ReturnType<typeof useLearningLabDraft>;
  editorFiles: LearningLabFile[];
  selectedFile: LearningLabFile | undefined;
  setSelectedFilePath: (path: string | null) => void;
  canRunActivity: boolean;
  startingRun: boolean;
  runPending: boolean;
  activeRun: LearningPracticalRunDto | undefined;
  beginRun: () => Promise<void>;
  builtinRuntime: LearningBuiltinRuntimeCapability | undefined;
  capability: LearningLabRuntimeCapability | undefined;
}) {
  const runDisabledReason = !canRunActivity
    ? activeActivity.runtimeKind === 'none'
      ? 'This activity has no execution environment'
      : 'Execution environment unavailable'
    : draft.loadState !== 'ready'
      ? 'Restoring your work…'
      : !editorFiles.length
        ? 'No runnable files'
        : startingRun || runPending || Boolean(activeRun)
          ? 'Run already in progress'
          : null;

  return (
    <section className="rounded-2xl border border-border-default bg-surface p-4 sm:p-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <div className="text-xs font-semibold uppercase tracking-[.15em] text-accent">
            Artifact editor
          </div>
          <h3 className="mt-1 font-serif text-xl text-text-primary">
            Change the starter work
          </h3>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            disabled={Boolean(runDisabledReason)}
            onClick={() => void beginRun()}
            className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-40"
          >
            {startingRun || runPending || activeRun ? (
              <LoaderCircle size={13} className="animate-spin" />
            ) : (
              <Play size={13} />
            )}
            {startingRun || runPending
              ? 'Starting run…'
              : activeRun
                ? 'Run in progress…'
                : 'Run locally'}
          </button>
          {runDisabledReason && (
            <span className="text-xs text-text-muted">
              {runDisabledReason}
            </span>
          )}
        </div>

        <span role="status" aria-live="polite" className="text-xs text-text-muted">
          {draft.loadState === 'loading'
            ? 'Restoring your work…'
            : draft.saveState === 'saving'
              ? 'Saving…'
              : draft.saveState === 'dirty'
                ? 'Unsaved changes'
                : draft.error
                  ? 'Your changes need attention'
                  : 'Saved on this device'}
        </span>
      </div>

      {draft.error && (
        <div
          role="alert"
          className="mt-3 rounded-xl border border-danger-muted bg-danger-muted p-3 text-xs text-danger-fg"
        >
          <p>{draft.error}</p>
          <div className="mt-2 flex flex-wrap gap-3">
            {draft.saveState === 'conflict' ? (
              <>
                <button
                  type="button"
                  onClick={() => void draft.keepLocalEdits()}
                  className="font-medium text-accent underline underline-offset-4"
                >
                  Keep my edits
                </button>
                <button
                  type="button"
                  onClick={() => void draft.useSavedVersion()}
                  className="text-text-secondary underline underline-offset-4"
                >
                  Use saved version
                </button>
                <p className="w-full text-xs text-text-muted">
                  Keep my edits saves your changed files over the latest draft. Use saved version replaces your unsaved changes.
                </p>
              </>
            ) : (
              <button
                type="button"
                onClick={() => void (draft.loadState === 'error' ? draft.retryLoad() : draft.flush())}
                className="font-medium text-accent underline underline-offset-4"
              >
                {draft.loadState === 'error' ? 'Retry loading draft' : 'Retry saving draft'}
              </button>
            )}
          </div>
        </div>
      )}

      <div className="mt-4 overflow-hidden rounded-xl border border-border-default">
        {editorFiles.length > 0 && (
          <FileTabs
            files={editorFiles.map((file) => ({ path: file.path }))}
            activePath={selectedFile?.path ?? editorFiles[0]?.path ?? ''}
            onSelect={(path) => setSelectedFilePath(path)}
          />
        )}

        {editorFiles.map((file, index) => (
          <div
            key={`${activeActivity.id}:${activeActivity.revision}:${file.path}`}
            id={`lab-file-panel-${activeActivity.id}-${index}`}
            role="tabpanel"
            aria-labelledby={`lab-file-tab-${activeActivity.id}-${index}`}
            hidden={selectedFile?.path !== file.path}
          >
            <LearningCodeEditor
              path={file.path}
              showPath={false}
              value={file.content}
              ariaLabel={`Edit ${file.path}`}
              readOnly={draft.loadState !== 'ready'}
              onChange={(content) => draft.setFileContent(file.path, content)}
              onRunShortcut={() => void beginRun()}
            />
          </div>
        ))}

        {editorFiles.length === 0 && (
          <p className="bg-bg p-4 text-xs text-text-muted">
            This activity has no editable starter files.
          </p>
        )}
      </div>

      {activeActivity.runtimeKind !== 'none' && !canRunActivity && (
        <div className="mt-4 flex items-start gap-3 rounded-xl border border-warning-muted bg-warning-muted p-4">
          <AlertTriangle
            size={16}
            className="mt-0.5 shrink-0 text-warning-fg"
          />
          <div>
            <h4 className="text-xs font-semibold text-text-primary">
              This device cannot run this activity right now
            </h4>
            <p className="mt-1 text-xs leading-5 text-text-secondary">
              {activeActivity.runtimeUnavailableReason ||
                builtinRuntime?.reason ||
                capability?.reason ||
                'A matching execution environment is not available.'}{' '}
              Your edits are saved here while you set up the required language environment.
            </p>
          </div>
        </div>
      )}

      {activeActivity.runtimeKind !== 'none' && (
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
          <p className="text-xs leading-4 text-text-muted">
            {activeActivity.runtimeKind === 'builtin'
              ? 'Runs on: Built-in Python'
              : 'Runs on: A container'}
          </p>
        </div>
      )}
    </section>
  );
}
