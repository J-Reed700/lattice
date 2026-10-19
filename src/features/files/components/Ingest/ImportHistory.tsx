/** Import results and recovery, backed by durable per-file job records. */
import { type FC, useState } from 'react';

import { type QueryClient, useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';

import { BATCH_IMPORT_JOBS } from '@/features/batch/jobKinds';
import { useJobStatus } from '@/features/jobs/api';
import type { BatchJobSummary, BatchJobStatus, BatchJobItem } from '@/types/api/batch';
import { listAllBatchJobs, deleteBatchJob, retryFailedItems, getBatchJobDetails } from '@/utils/batchHistory';
import { validateIndexablePath } from '@/utils/indexingFileValidation';

interface ImportHistoryProps { onRefresh?: () => void; }
const isActive = (job: BatchJobSummary) => ['running', 'pending', 'processing'].includes(job.status);
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const itemPriority = (status: string) => ['processing', 'running'].includes(status) ? 0 : status === 'failed' ? 1 : status === 'pending' ? 2 : 3;
const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

function jobStatusLabel(job: BatchJobSummary): string {
  if (isActive(job) && job.completedItems + job.failedItems >= job.totalItems) return 'Finishing import…';
  if (isActive(job)) return `${job.completedItems} of ${job.totalItems} imported · ${Math.max(0, job.totalItems - job.completedItems - job.failedItems)} remaining`;
  if (job.failedItems > 0) return `${job.completedItems} imported, ${job.failedItems} failed`;
  if (job.status === 'cancelled') return 'Cancelled';
  if (job.status === 'failed') return 'Failed';
  return `${job.completedItems} imported`;
}

function recoveryHint(error: string | null): string {
  if (/OCR|selectable text/i.test(error ?? '')) return 'Choose a PDF with selectable text, or run OCR on it first.';
  if (/not found|no such file|does not exist/i.test(error ?? '')) return 'Choose the file again if it has moved or been renamed.';
  if (/timed? out|timeout/i.test(error ?? '')) return 'Retry the file. If it times out again, choose a smaller or re-exported PDF.';
  return 'Retry this file, or choose a corrected copy to replace it.';
}

const importHistoryKeys = {
  all: ['import-history'] as const,
  list: () => [...importHistoryKeys.all, 'list'] as const,
  job: (id: string) => [...importHistoryKeys.all, 'details', id] as const,
  /** A summary that has moved on reads fresh details by itself. */
  details: (id: string, status: string, completedItems: number, failedItems: number) =>
    [...importHistoryKeys.job(id), status, completedItems, failedItems] as const,
};
/** A failed read is tried again on this cadence until it succeeds; showing the page again rereads. */
const FAILED_READ_POLL_MS = 2000;
const readOptions = { retry: true, retryDelay: FAILED_READ_POLL_MS, staleTime: 0, refetchOnWindowFocus: 'always' } as const;
const NO_JOBS: BatchJobSummary[] = [];
const deleteJobs = (ids: string[]) => Promise.all(ids.map(id => deleteBatchJob(id)));
/** The newest details read for a job under any summary, shown while a fresher read is under way. */
const lastDetails = (client: QueryClient, id: string) => client.getQueryCache().findAll({ queryKey: importHistoryKeys.job(id) })
  .filter(query => query.state.data !== undefined)
  .sort((a, b) => b.state.dataUpdatedAt - a.state.dataUpdatedAt)[0]?.state.data as BatchJobStatus | undefined;

export const ImportHistory: FC<ImportHistoryProps> = ({ onRefresh }) => {
  const queryClient = useQueryClient();
  // Rows the user opened or closed. Others are open while importing or holding failures.
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [actionErrors, setActionErrors] = useState<Record<string, string>>({});

  const list = useQuery({ queryKey: importHistoryKeys.list(), queryFn: listAllBatchJobs, ...readOptions });
  const jobs = list.data ?? NO_JOBS;
  const listError = list.error ?? list.failureReason;
  const isOpen = (job: BatchJobSummary) => toggled[job.id] ?? (job.failedItems > 0 || isActive(job));
  // Single-file headings name their file; open and importing rows list theirs.
  const detailed = jobs.filter(job => job.totalItems === 1 || isOpen(job) || isActive(job));
  const detailQueries = useQueries({
    queries: detailed.map(job => ({
      queryKey: importHistoryKeys.details(job.id, job.status, job.completedItems, job.failedItems),
      queryFn: () => getBatchJobDetails(job.id),
      placeholderData: () => lastDetails(queryClient, job.id),
      ...readOptions,
    })),
  });
  const details = new Map(detailed.map((job, index) => [job.id, detailQueries[index]]));

  // An import's job reports each file it settles; the newest read wins.
  const refresh = () => queryClient.invalidateQueries({ queryKey: importHistoryKeys.all });
  useJobStatus((report) => {
    if (BATCH_IMPORT_JOBS.has(report.kind)) void refresh();
  });
  // A refresh asked for by the user also restarts a read waiting to retry.
  const refreshNow = async () => {
    await queryClient.cancelQueries({ queryKey: importHistoryKeys.all });
    await refresh();
  };
  const setJobs = (update: (all: BatchJobSummary[]) => BatchJobSummary[]) =>
    queryClient.setQueryData<BatchJobSummary[]>(importHistoryKeys.list(), prev => prev && update(prev));
  const retry = useMutation({
    mutationFn: (args: { jobId: string; itemId?: string; replacement?: string }) => retryFailedItems(args.jobId, args.itemId, args.replacement),
    onSuccess: id => setJobs(all => all.map(entry => entry.id === id ? { ...entry, status: 'pending' } : entry)),
    onSettled: () => { void refresh(); },
  });
  const removeOptions = {
    mutationFn: deleteJobs,
    onSuccess: (_: unknown, ids: string[]) => setJobs(all => all.filter(entry => !ids.includes(entry.id))),
    onSettled: () => { void refresh(); },
  };
  const remove = useMutation(removeOptions);
  const clearAll = useMutation(removeOptions);
  const hasActiveJobs = jobs.some(job => isActive(details.get(job.id)?.data ?? job));

  const runAction = async (job: BatchJobSummary, action: () => Promise<void>) => {
    if (busy.has(job.id)) return;
    setBusy(prev => new Set(prev).add(job.id));
    setActionErrors(prev => ({ ...prev, [job.id]: '' }));
    try { await action(); } finally {
      setBusy(prev => { const next = new Set(prev); next.delete(job.id); return next; });
    }
  };

  const recover = (job: BatchJobSummary, item?: BatchJobItem, replace = false) => runAction(job, async () => {
    if (isActive(job)) return;
    try {
      let replacement: string | undefined;
      if (replace && item) {
        const chosen = await open({ multiple: false, directory: false, title: `Replace ${fileName(item.target)}`,
          ...(item.target.toLowerCase().endsWith('.pdf') ? { filters: [{ name: 'PDF', extensions: ['pdf'] }] } : {}),
        });
        if (!chosen) return; // Cancellation preserves the failed item and its reason.
        const validation = validateIndexablePath(chosen);
        if (!validation.ok) throw new Error('Choose a supported document file.');
        replacement = chosen;
      }
      const id = await retry.mutateAsync({ jobId: job.id, itemId: item?.itemId, replacement });
      setToggled(prev => ({ ...prev, [id]: true }));
      onRefresh?.();
    } catch (error) {
      setActionErrors(prev => ({ ...prev, [job.id]: `Could not start the retry: ${message(error)}` }));
    }
  });

  const removeJob = (job: BatchJobSummary) => runAction(job, async () => {
    try {
      await remove.mutateAsync([job.id]);
      onRefresh?.();
    } catch (error) { setActionErrors(prev => ({ ...prev, [job.id]: `Could not remove this import: ${message(error)}` })); }
  });

  const clearHistory = async () => {
    try {
      await clearAll.mutateAsync(jobs.map(job => job.id));
      onRefresh?.();
    } catch { /* Shown from the mutation's error. */ }
  };

  if (list.isPending && !listError) return <p className="text-sm text-text-muted">Loading import history…</p>;
  return (
    <div>
      {listError && <div role="alert" className="mb-4 text-sm text-text-secondary">Import status is unavailable: {message(listError)}{' '}
        <button type="button" onClick={() => { void refreshNow(); }} className="text-accent underline">Refresh status</button>
      </div>}
      {clearAll.isError && <p role="alert" className="mb-4 text-sm text-text-secondary">Could not clear history: {message(clearAll.error)}</p>}
      {!listError && jobs.length === 0 && <p className="text-sm text-text-muted">No imports yet.</p>}
      {jobs.length > 0 && <>
        <div className="flex items-center justify-between pb-2">
          <h2 className="text-base font-medium text-text-primary">{jobs.length} {jobs.length === 1 ? 'import' : 'imports'}</h2>
          <button type="button" disabled={hasActiveJobs || busy.size > 0} onClick={() => { void clearHistory(); }} className="text-xs text-text-tertiary hover:text-text-primary disabled:opacity-50">Clear all</button>
        </div>
        <p className="pb-3 text-xs text-text-muted">Retrying a file updates its original import.</p>
        <div className="border-t border-border-subtle">
          {jobs.map(summary => {
            const detail = details.get(summary.id);
            const detailError = detail?.error ?? detail?.failureReason;
            const opened = isOpen(summary);
            // The heading and file rows must use the same status response. A
            // separately fetched list summary can be older or newer than it.
            const job = { ...summary, ...detail?.data };
            const singleTarget = job.totalItems === 1 ? detail?.data?.items[0]?.target : undefined;
            const title = singleTarget ? (job.jobType === 'file_import' ? fileName(singleTarget) : singleTarget)
              : job.totalItems === 1 && detail?.isPending && !detailError ? 'Loading…'
              : `${job.totalItems} ${job.jobType === 'file_import' ? (job.totalItems === 1 ? 'file' : 'files') : (job.totalItems === 1 ? 'URL' : 'URLs')}`;
            const disabled = busy.has(job.id) || isActive(job);
            const items = [...(detail?.data?.items ?? [])].sort((a, b) => itemPriority(a.status) - itemPriority(b.status));
            return <section key={job.id} aria-label={`Import ${job.id}`} className="border-b border-border-subtle">
              <div className="flex flex-wrap items-center gap-3 py-3">
                <button type="button" onClick={() => setToggled(prev => ({ ...prev, [job.id]: !opened }))} aria-expanded={opened} className="min-w-0 flex-1 text-left">
                  <div className="wrap-break-word text-sm text-text-primary">{title}</div>
                  <div className="text-xs text-text-muted">Added {new Date(job.createdAt).toLocaleString()}</div>
                </button>
                <span role="status" className="text-xs text-text-secondary">{detailError ? 'Status unavailable' : jobStatusLabel(job)}</span>
                {job.failedItems > 0 && <button type="button" disabled={disabled} onClick={() => { void recover(job); }} className="text-xs text-accent hover:underline disabled:opacity-50">Retry all failed</button>}
                {!isActive(job) && <button type="button" disabled={disabled} onClick={() => { void removeJob(job); }} className="text-xs text-text-tertiary hover:text-text-primary">Remove</button>}
              </div>
              {actionErrors[job.id] && <p role="alert" className="pb-3 text-sm text-text-secondary">{actionErrors[job.id]}</p>}
              {detailError && <p role="alert" className="pb-3 text-sm text-text-secondary">Could not load files: {message(detailError)}. Displayed results may be out of date.{' '}
                <button type="button" onClick={() => { void refreshNow(); }} className="text-accent underline">Refresh files</button>
              </p>}
              {opened && <div className="pb-3 pl-4">
                {detail?.isPending && !detailError && <p className="text-xs text-text-muted">Loading files…</p>}
                {items.map(item => <div key={item.itemId} className="border-t border-border-subtle py-3">
                  <p className="wrap-break-word text-sm text-text-primary">{job.jobType === 'file_import' ? fileName(item.target) : item.target}</p>
                  {job.jobType === 'file_import' && <p className="break-all text-xs text-text-muted">{item.target}</p>}
                  <p className="mt-1 whitespace-pre-wrap wrap-break-word text-xs text-text-secondary">{item.status === 'completed' ? 'Imported' : item.status === 'failed' ? `Failed — ${item.errorMessage || 'Import failed'}` : ['running', 'processing'].includes(item.status) ? 'Processing — not ready to search yet' : item.status === 'cancelled' ? 'Cancelled' : 'Queued'}</p>
                  {item.status === 'failed' && job.jobType === 'file_import' && <>
                    <p className="mt-1 text-xs text-text-muted">{recoveryHint(item.errorMessage ?? null)}</p>
                    <div className="mt-2 flex gap-4">
                      <button type="button" aria-label={`Retry ${fileName(item.target)}`} disabled={disabled} onClick={() => { void recover(job, item); }} className="text-xs text-accent hover:underline disabled:opacity-50">Retry file</button>
                      <button type="button" aria-label={`Choose replacement for ${fileName(item.target)}`} disabled={disabled} onClick={() => { void recover(job, item, true); }} className="text-xs text-accent hover:underline disabled:opacity-50">Choose replacement…</button>
                    </div>
                  </>}
                </div>)}
              </div>}
            </section>;
          })}
        </div>
      </>}
    </div>
  );
};
