import { newId } from '@/lib/api';
import { useEffect, useMemo, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { AlertTriangle, BookmarkPlus, BookOpenText, LoaderCircle, RotateCcw, Search, Trash2, Undo2 } from 'lucide-react';

import {
  useCreateLearningSourceSelector,
  useDeleteLearningSourceV2,
  useReimportLearningSource,
  useSearchLearningSourcesSemantically,
} from '@/features/learning/portability/useLearningPortability';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import { useRetryAction } from '@/features/learning/practice/shared/useRetryAction';
import { courseIndexKeys, useCourseIndexStatus } from '@/features/learning/sources/useCourseIndexStatus';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import type { LearningSourceLibraryItemDto } from '@/lib/bindings';


function sourceName(source: LearningSourceLibraryItemDto) {
  return source.activeVersion?.title ?? source.pendingVersion?.title ?? source.requestedUrl ?? source.versions[0]?.title ?? 'Saved source';
}

function isCasConflict(message: string) {
  return /(revision|compare.and.swap|conflict|changed elsewhere|source changed; reload and retry)/i.test(message);
}

export function SourceMaintenancePanel({ programId }: { programId: string }) {
  const workspace = useLearningSourceWorkspace(programId);
  const [selectedId, setSelectedId] = useState('');
  const [versionId, setVersionId] = useState('');
  const [searchDraft, setSearchDraft] = useState('');
  const [searchText, setSearchText] = useState('');
  const [deleteReason, setDeleteReason] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [statusMessage, setStatusMessage] = useState('');
  const [error, setError] = useState('');
  const [reloadRequired, setReloadRequired] = useState(false);
  const [selection, setSelection] = useState<{ start: number; end: number } | null>(null);

  const { retryAction, setRetryAction, clearRetry } = useRetryAction();
  const retryPayload = useRef<{ kind: 'delete' | 'reimport' | 'selector'; request: Record<string, unknown> } | null>(null);
  const retry = useRef<Record<string, { key: string; request: Record<string, unknown> }>>({});

  const selected = workspace.data?.sources.find((source) => source.id === selectedId) ?? null;
  const sourceRequest = selected && versionId ? { programId, sourceId: selected.id, versionId } : null;
  const version = useLearningSourceVersion(sourceRequest);
  const semantic = useSearchLearningSourcesSemantically(searchText.trim() ? { programId, query: searchText.trim(), limit: 20 } : null);

  const indexStatus = useCourseIndexStatus(programId);
  const newToIndex = indexStatus?.state === 'indexing' ? indexStatus.referencesNew : 0;
  const client = useQueryClient();

  useEffect(() => {
    if (semantic.dataUpdatedAt) void client.invalidateQueries({ queryKey: courseIndexKeys.status(programId), exact: true });
  }, [client, programId, semantic.dataUpdatedAt]);

  const selectors = useCreateLearningSourceSelector(programId);
  const remove = useDeleteLearningSourceV2(programId);
  const restore = useReimportLearningSource(programId);
  const versions = useMemo(() => selected?.versions ?? [], [selected?.versions]);
  const sources = useMemo(() => workspace.data?.sources ?? [], [workspace.data?.sources]);

  useEffect(() => {
    if (!sources.length || sources.some((source) => source.id === selectedId)) return;
    const first = sources[0];
    setSelectedId(first.id);
    setVersionId(first.activeVersionId ?? first.pendingVersionId ?? first.versions[0]?.id ?? '');
  }, [sources, selectedId]);

  const chooseSource = (source: LearningSourceLibraryItemDto) => {
    setSelectedId(source.id);
    setVersionId(source.activeVersionId ?? source.pendingVersionId ?? source.versions[0]?.id ?? '');
    setSelection(null);
    setConfirmDelete(false);
    setDeleteReason('');
    setStatusMessage('');
    setError('');
    setReloadRequired(false);
    clearRetry();
    retryPayload.current = null;
  };

  const setRetry = (kind: string, payload: Record<string, unknown>, identity: string) => {
    const key = JSON.stringify(payload);
    const current = retry.current[kind];
    if (current?.key === key) return current.request;
    const request = { ...payload, operationId: identity };
    retry.current[kind] = { key, request };
    return request;
  };

  const retryMutation = async () => {
    if (!retryPayload.current) return;
    setError('');
    setReloadRequired(false);
    const action = retryPayload.current;
    try {
      if (action.kind === 'delete') {
        await remove.mutateAsync(action.request as never);
        setStatusMessage('The source was removed from active program materials. Its historical source record remains available.');
      } else if (action.kind === 'reimport') {
        await restore.mutateAsync(action.request as never);
        setStatusMessage('The saved version was restored as the active source version.');
      } else {
        const saved = await selectors.mutateAsync(action.request as never);
        setStatusMessage(`Quote saved · ${saved.matchStatus.replace(/_/g, ' ')} match`);
        setSelection(null);
      }
      clearRetry();
      retryPayload.current = null;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'The saved action could not be completed.';
      setError(message);
      setReloadRequired(isCasConflict(message));
    }
  };

  const doDelete = async () => {
    if (!selected || !deleteReason.trim()) return;
    const payload = { programId, sourceId: selected.id, expectedRevision: selected.revision, reason: deleteReason.trim() };
    const request = setRetry(`delete:${selected.id}`, payload, newId());
    setError('');
    setStatusMessage('');
    setReloadRequired(false);
    clearRetry();
    retryPayload.current = null;
    try {
      await remove.mutateAsync(request as never);
      setStatusMessage(`“${sourceName(selected)}” was removed from active program materials. Its historical source record remains available.`);
      setConfirmDelete(false);
      setDeleteReason('');
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'The source could not be deleted.';
      setError(message);
      setReloadRequired(isCasConflict(message));
      retryPayload.current = { kind: 'delete', request };
      setRetryAction({ label: 'Retry same removal', run: retryMutation });
    }
  };

  const doReimport = async () => {
    if (!selected || !versionId) return;
    const payload = { programId, sourceId: selected.id, versionId, expectedRevision: selected.revision, replacementText: null };
    const request = setRetry(`reimport:${selected.id}`, payload, newId());
    setError('');
    setStatusMessage('');
    setReloadRequired(false);
    clearRetry();
    retryPayload.current = null;
    try {
      await restore.mutateAsync(request as never);
      setStatusMessage(`Version ${versions.find((item) => item.id === versionId)?.versionNumber ?? ''} was restored as the active source version.`);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'The saved version could not be restored.';
      setError(message);
      setReloadRequired(isCasConflict(message));
      retryPayload.current = { kind: 'reimport', request };
      setRetryAction({ label: 'Retry same restore', run: retryMutation });
    }
  };

  const saveSelector = async () => {
    if (!selected || !version.data?.fullText || !selection || selection.start >= selection.end) return;
    const fullText = version.data.fullText;
    const startByte = new TextEncoder().encode(fullText.slice(0, selection.start)).length;
    const endByte = new TextEncoder().encode(fullText.slice(0, selection.end)).length;
    const payload = { programId, sourceId: selected.id, sourceVersionId: versionId, startByte, endByte };
    const key = JSON.stringify(payload);
    let entry = retry.current.selector;
    if (entry?.key !== key) entry = { key, request: { ...payload, selectorId: newId(), operationId: newId() } };
    retry.current.selector = entry;
    setError('');
    setStatusMessage('');
    setReloadRequired(false);
    clearRetry();
    retryPayload.current = null;
    try {
      const saved = await selectors.mutateAsync(entry.request as never);
      delete retry.current.selector;
      setSelection(null);
      setStatusMessage(`Quote saved · ${saved.matchStatus.replace(/_/g, ' ')} match`);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'The source quote could not be saved.';
      setError(message);
      setReloadRequired(isCasConflict(message));
      retryPayload.current = { kind: 'selector', request: entry.request };
      setRetryAction({ label: 'Retry same quote', run: retryMutation });
    }
  };

  if (workspace.isLoading) {
    return <div role="status" className="rounded-xl border border-border-default bg-surface-sunken p-4 text-xs text-text-muted">Loading source history…</div>;
  }
  if (workspace.error) {
    return (
      <div className="rounded-xl border border-border-default bg-surface p-4">
        <StatusLine error={workspace.error.message} onRetry={() => void workspace.refetch()} retryLabel="Retry" retryAriaLabel="Retry" />
      </div>
    );
  }

  const activeSources = sources.filter((source) => !source.deletedAt);
  const tombstones = sources.filter((source) => Boolean(source.deletedAt));

  return (
    <section className="mt-5 min-w-0 rounded-2xl border border-border-default bg-surface p-4 sm:p-6" data-testid="source-maintenance">
      <header>
        <p className="text-xs font-semibold uppercase tracking-[.14em] text-accent">Source continuity</p>
        <h3 className="mt-1 font-serif text-xl text-text-primary">Reconnect quotes and saved versions</h3>
        <p className="mt-2 max-w-3xl text-xs leading-5 text-text-secondary">
          Find related passages in this program’s stored source versions, preserve exact quote selectors, and restore a version or remove a source with its history retained.
        </p>
      </header>

      {statusMessage && (
        <p role="status" className="mt-4 rounded-lg bg-success-muted p-3 text-xs leading-5 text-success-fg">
          {statusMessage}
        </p>
      )}

      {error && (
        <div className="mt-4">
          {reloadRequired ? (
            <div role="alert" className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-danger-muted p-3 text-xs text-danger-fg">
              <span>{error}</span>
              <button
                type="button"
                disabled={workspace.isFetching}
                onClick={() => {
                  void workspace.refetch();
                  setError('');
                  setReloadRequired(false);
                  clearRetry();
                  retryPayload.current = null;
                }}
                className="inline-flex items-center gap-1 underline"
              >
                <RotateCcw size={12} />Reload source workspace
              </button>
            </div>
          ) : (
            <StatusLine
              error={error}
              onRetry={retryAction ? () => void retryMutation() : undefined}
              retryAriaLabel={retryAction?.label}
              retryLabel="Try again"
            />
          )}
        </div>
      )}

      <div className="mt-4 grid min-w-0 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
        <div className="min-w-0 space-y-4">
          <form onSubmit={(event) => { event.preventDefault(); setSearchText(searchDraft); }} className="rounded-xl border border-border-default bg-surface-sunken p-3">
            <label className="text-xs font-medium text-text-primary" htmlFor="semantic-source-search">Search related saved material</label>
            <div className="mt-2 flex min-w-0 gap-2">
              <input
                id="semantic-source-search"
                value={searchDraft}
                onChange={(event) => setSearchDraft(event.target.value)}
                maxLength={240}
                placeholder="A concept, process, or question"
                className="min-w-0 flex-1 rounded-lg border border-border-default bg-surface px-3 py-2 text-xs text-text-primary"
              />
              <button type="submit" disabled={!searchDraft.trim() || semantic.isFetching} aria-label="Search program source versions" className="shrink-0 rounded-lg bg-accent px-3 text-accent-fg disabled:opacity-40">
                <Search size={14} />
              </button>
            </div>
            <p className="mt-2 text-xs leading-4 text-text-muted">
              Search reads only text saved in this program; each result identifies whether retrieval used hybrid search or a keyword fallback. It never fetches source URLs.
            </p>
            {semantic.isFetching && (
              <p role="status" className="mt-2 text-xs text-text-muted">
                <LoaderCircle size={12} className="mr-1 inline animate-spin" />
                {newToIndex > 0 ? `Adding ${newToIndex.toLocaleString()} new ${newToIndex === 1 ? 'reference' : 'references'} to the course’s word index, then searching…` : 'Searching saved versions…'}
              </p>
            )}
            {semantic.error && (
              <div className="mt-2">
                <StatusLine error={semantic.error.message} onRetry={() => void semantic.refetch()} retryLabel="Retry" retryAriaLabel="Retry" />
              </div>
            )}
            {semantic.data && (
              <ol className="mt-3 max-h-64 space-y-2 overflow-auto">
                {semantic.data.map((result, index) => (
                  <li key={`${result.sourceId}:${result.version.id}:${index}`} className="rounded-lg border border-border-default bg-surface p-3">
                    <p className="text-xs font-medium text-text-primary">{result.version.title}</p>
                    <blockquote className="mt-1 border-l-2 border-accent pl-2 text-xs leading-4 text-text-secondary">{result.excerpt}</blockquote>
                    <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                      <span className="text-xs text-text-muted">{result.retrievalKind.replace(/_/g, ' ')} · Result {index + 1}</span>
                      <button
                        type="button"
                        onClick={() => {
                          const src = sources.find((item) => item.id === result.sourceId);
                          if (src) {
                            setSelectedId(src.id);
                            setVersionId(result.version.id);
                            setSelection(null);
                          }
                        }}
                        className="text-xs font-semibold text-accent underline"
                      >
                        Open this exact version
                      </button>
                    </div>
                  </li>
                ))}
              </ol>
            )}
            {semantic.data?.length === 0 && <p className="mt-3 text-xs text-text-muted">No matching saved passages.</p>}
          </form>

          <section className="rounded-xl border border-border-default bg-surface-sunken p-3">
            <div className="flex items-center justify-between gap-2">
              <h4 className="text-xs font-semibold text-text-primary">Program sources</h4>
              <span className="text-xs text-text-muted">{activeSources.length} active · {tombstones.length} removed</span>
            </div>
            {sources.length ? (
              <div className="mt-2 max-h-72 space-y-1 overflow-auto">
                {sources.map((src) => (
                  <button
                    type="button"
                    key={src.id}
                    aria-pressed={selected?.id === src.id}
                    onClick={() => chooseSource(src)}
                    className={`flex w-full min-w-0 items-start gap-2 rounded-lg px-3 py-2.5 text-left ${selected?.id === src.id ? 'bg-accent-muted text-accent' : 'hover:bg-surface'}`}
                  >
                    <BookOpenText size={14} className="mt-0.5 shrink-0" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-xs font-medium">{sourceName(src)}</span>
                      <span className="mt-0.5 block text-xs text-text-muted">
                        {src.deletedAt ? `Removed · ${new Date(src.deletedAt).toLocaleDateString()}` : 'Active in source library'} · {src.versions.length} saved version{src.versions.length === 1 ? '' : 's'}
                      </span>
                    </span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="mt-3 rounded-lg border border-dashed border-border-default p-4 text-xs text-text-muted">
                Add a saved material in Sources before creating quote selectors or searching this program.
              </p>
            )}
          </section>
        </div>

        <section className="min-w-0 rounded-xl border border-border-default bg-surface-sunken p-3 sm:p-4">
          {!selected ? (
            <div className="grid min-h-48 place-items-center rounded-lg border border-dashed border-border-default p-5 text-center">
              <div>
                <BookOpenText size={20} className="mx-auto text-text-muted" />
                <p className="mt-2 text-xs text-text-muted">Choose a source to review its immutable versions.</p>
              </div>
            </div>
          ) : (
            <>
              <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <span className={`rounded-full px-2.5 py-1 text-xs font-medium ${selected.deletedAt ? 'bg-warning-muted text-warning-fg' : 'bg-success-muted text-success-fg'}`}>
                    {selected.deletedAt ? 'Historical tombstone' : 'Active source'}
                  </span>
                  <h4 className="mt-2 wrap-break-word font-serif text-lg text-text-primary">{sourceName(selected)}</h4>
                  {selected.deletedAt && (
                    <p className="mt-1 wrap-break-word text-xs text-text-muted">
                      Removed {new Date(selected.deletedAt).toLocaleString()}
                      {selected.deletionReason ? ` · ${selected.deletionReason}` : ''}
                    </p>
                  )}
                </div>
                <label className="min-w-0 max-w-full text-xs font-medium text-text-muted">
                  Saved version
                  <select
                    value={versionId}
                    onChange={(event) => {
                      setVersionId(event.target.value);
                      setSelection(null);
                    }}
                    className="mt-1 block w-full min-w-0 max-w-full rounded-lg border border-border-default bg-surface px-2 py-2 text-xs text-text-primary"
                  >
                    {versions.map((item) => (
                      <option key={item.id} value={item.id}>v{item.versionNumber} · {item.title}</option>
                    ))}
                  </select>
                </label>
              </div>

              {version.isLoading && <p role="status" className="mt-3 text-xs text-text-muted">Loading the exact saved version…</p>}
              {version.error && (
                <div className="mt-3">
                  <StatusLine error={version.error.message} onRetry={() => void version.refetch()} retryLabel="Retry" retryAriaLabel="Retry" />
                </div>
              )}
              {version.data && (
                <>
                  <div className="mt-3 grid gap-2 sm:grid-cols-3">
                    <div className="rounded-lg border border-border-default bg-surface p-2.5">
                      <span className="block text-xs text-text-muted">Acquired</span>
                      <span className="mt-1 block wrap-break-word text-xs text-text-secondary">{new Date(version.data.version.acquiredAt).toLocaleString()}</span>
                    </div>
                    <div className="rounded-lg border border-border-default bg-surface p-2.5">
                      <span className="block text-xs text-text-muted">Content checksum</span>
                      <code className="mt-1 block break-all text-xs text-text-secondary">{version.data.version.contentSha256}</code>
                    </div>
                    <div className="rounded-lg border border-border-default bg-surface p-2.5">
                      <span className="block text-xs text-text-muted">Usage references</span>
                      <span className="mt-1 block text-xs text-text-secondary">{version.data.usage.length}</span>
                    </div>
                  </div>

                  <div className="mt-3 rounded-lg border border-border-default bg-surface p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <h5 className="text-xs font-semibold text-text-primary">Exact saved text</h5>
                      <span className="text-xs text-text-muted">
                        {version.data.version.wordCount} words · {version.data.version.truncated ? 'bounded extraction' : 'full stored text'}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-text-muted">Select a short passage to save an exact quote with surrounding context.</p>
                    <textarea
                      aria-label={`Saved source text for ${version.data.version.title}`}
                      readOnly
                      value={version.data.fullText}
                      onSelect={(event) => {
                        const target = event.currentTarget;
                        setSelection({ start: target.selectionStart, end: target.selectionEnd });
                      }}
                      className="mt-2 block h-64 w-full resize-y whitespace-pre-wrap rounded-lg border border-border-default bg-surface-sunken p-3 font-sans text-xs leading-5 text-text-secondary sm:h-80"
                    />
                    {selection && (
                      <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                        <span className="min-w-0 wrap-break-word text-xs text-text-muted">
                          “{version.data.fullText.slice(selection.start, selection.end).slice(0, 100)}
                          {version.data.fullText.slice(selection.start, selection.end).length > 100 ? '…' : ''}”
                        </span>
                        <button
                          type="button"
                          disabled={selectors.isPending}
                          onClick={() => void saveSelector()}
                          className="inline-flex shrink-0 items-center gap-1 rounded-full bg-accent px-3 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50"
                        >
                          <BookmarkPlus size={12} />
                          {selectors.isPending ? 'Saving quote…' : 'Save exact quote'}
                        </button>
                      </div>
                    )}
                  </div>

                  {version.data.usage.length > 0 && (
                    <div className="mt-3">
                      <h5 className="text-xs font-semibold text-text-primary">Cited by</h5>
                      <ul className="mt-2 flex flex-wrap gap-2">
                        {version.data.usage.map((usage, index) => (
                          <li key={`${usage.lessonId}:${usage.referenceTitle}:${index}`} className="rounded-full border border-border-default px-3 py-1.5 text-xs text-text-secondary">
                            {usage.lessonTitle} · {usage.referenceTitle}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </>
              )}

              <div className="mt-4 flex flex-wrap gap-2 border-t border-border-default pt-4">
                {selected.deletedAt ? (
                  <button
                    type="button"
                    disabled={restore.isPending || !versionId}
                    onClick={() => void doReimport()}
                    className="inline-flex items-center gap-1.5 rounded-full bg-accent px-3.5 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50"
                  >
                    <Undo2 size={12} />
                    {restore.isPending ? 'Restoring…' : 'Re-import selected version'}
                  </button>
                ) : (
                  <button
                    type="button"
                    disabled={remove.isPending}
                    onClick={() => setConfirmDelete((value) => !value)}
                    className="inline-flex items-center gap-1.5 rounded-full border border-border-default px-3.5 py-2 text-xs font-semibold text-danger-fg disabled:opacity-50"
                  >
                    <Trash2 size={12} />
                    Remove source
                  </button>
                )}
              </div>

              {confirmDelete && (
                <div className="mt-3 rounded-lg border border-border-default bg-surface p-3">
                  <p className="flex items-center gap-1.5 text-xs font-medium text-text-primary">
                    <AlertTriangle size={13} className="text-warning-fg" />
                    Remove from active materials?
                  </p>
                  <p className="mt-1 text-xs leading-4 text-text-secondary">
                    Historical citations and source versions remain. This action creates a tombstone; you can re-import a saved version later.
                  </p>
                  <label className="mt-3 block text-xs font-medium text-text-secondary">
                    Reason for removal
                    <textarea
                      value={deleteReason}
                      onChange={(event) => setDeleteReason(event.target.value)}
                      maxLength={500}
                      rows={2}
                      className="mt-1 block w-full min-w-0 rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-xs text-text-primary"
                    />
                  </label>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <button
                      type="button"
                      disabled={remove.isPending || !deleteReason.trim()}
                      onClick={() => void doDelete()}
                      className="rounded-full bg-accent px-3.5 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50"
                    >
                      {remove.isPending ? 'Removing…' : 'Confirm removal'}
                    </button>
                    <button
                      type="button"
                      disabled={remove.isPending}
                      onClick={() => setConfirmDelete(false)}
                      className="rounded-full border border-border-default px-3.5 py-2 text-xs text-text-secondary"
                    >
                      Keep source
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </section>
      </div>
    </section>
  );
}
