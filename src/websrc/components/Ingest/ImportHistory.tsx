/**
 * ImportHistory
 *
 * Past import jobs as hairline rows. Row actions (Retry / Remove) appear on
 * hover as text buttons; status is plain muted text, never a badge.
 * See `.design/UX-OVERHAUL-BRIEF.md` §1.
 */

import { type FC, useState, useEffect, useCallback, useRef } from 'react';

import { useToast } from '@/hooks/useToast';
import { cn } from '@/lib/utils';
import type { BatchJobSummary, BatchJobStatus } from '@/types/api/batch';
import {
  listBatchJobs,
  deleteBatchJob,
  retryFailedItems,
  getBatchJobDetails,
} from '@/utils/batchHistory';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

interface ImportHistoryProps {
  onRefresh?: () => void;
}

interface ExpandedJob {
  jobId: string;
  details: BatchJobStatus | null;
  loading: boolean;
}

function humanizeJobType(jobType: string): string {
  const cleaned = jobType.replace(/[_-]+/g, ' ').trim();
  if (!cleaned) return 'Import';
  const withUrl = cleaned.replace(/\burl\b/gi, 'URL');
  return withUrl.charAt(0).toUpperCase() + withUrl.slice(1);
}

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const seconds = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (seconds < 60) return 'just now';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(then).toLocaleDateString();
}

function jobMeta(job: BatchJobSummary): string {
  const parts = [`${job.totalItems} ${job.totalItems === 1 ? 'item' : 'items'}`];
  if (job.failedItems > 0) {
    parts.push(`${job.failedItems} failed`);
  }
  const when = relativeTime(job.createdAt);
  if (when) parts.push(when);
  return parts.join(' · ');
}

function jobStatusLabel(job: BatchJobSummary): string {
  switch (job.status) {
    case 'running':
      return `Running ${Math.round(job.progress)}%`;
    case 'pending':
      return 'Pending';
    case 'failed':
      return 'Failed';
    case 'cancelled':
      return 'Cancelled';
    default:
      return '';
  }
}

function itemStatusLabel(status: string, errorMessage: string | null): string {
  const normalized = status.toLowerCase();
  if (normalized === 'completed') return 'Imported';
  if (normalized === 'failed') return errorMessage ? `Failed — ${errorMessage}` : 'Failed';
  if (normalized === 'skipped') return 'Skipped';
  if (normalized === 'processing' || normalized === 'running') return 'Importing…';
  return 'Pending';
}

export const ImportHistory: FC<ImportHistoryProps> = ({ onRefresh }) => {
  const [jobs, setJobs] = useState<BatchJobSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedJobs, setExpandedJobs] = useState<Map<string, ExpandedJob>>(new Map());
  const { toast } = useToast();
  // `useToast()` hands back a fresh object every render, so depending on it
  // would recreate every callback and re-fire the load effect forever.
  const toastRef = useRef(toast);
  toastRef.current = toast;

  const loadJobs = useCallback(async () => {
    try {
      const fetchedJobs = await listBatchJobs();
      setJobs(fetchedJobs);
      setLoading(false);
    } catch (error) {
      console.error('[ImportHistory] Failed to load jobs:', error);
      toastRef.current.error("Couldn't load import history");
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadJobs();
  }, [loadJobs]);

  const hasActiveJobs = jobs.some((job) => job.status === 'running');

  useEffect(() => {
    if (!hasActiveJobs) {
      return;
    }

    const pollInterval = setInterval(() => {
      void loadJobs();
    }, 2000);

    return () => clearInterval(pollInterval);
  }, [hasActiveJobs, loadJobs]);

  const toggleJobExpansion = useCallback(async (jobId: string) => {
    setExpandedJobs((prev) => {
      const newMap = new Map(prev);
      const existing = newMap.get(jobId);

      if (existing) {
        newMap.delete(jobId);
        return newMap;
      }

      newMap.set(jobId, { jobId, details: null, loading: true });

      getBatchJobDetails(jobId)
        .then((details) => {
          setExpandedJobs((current) => {
            const updated = new Map(current);
            const entry = updated.get(jobId);
            if (entry) {
              entry.details = details;
              entry.loading = false;
            }
            return updated;
          });
        })
        .catch((error) => {
          console.error('[ImportHistory] Failed to load job details:', error);
          setExpandedJobs((current) => {
            const updated = new Map(current);
            updated.delete(jobId);
            return updated;
          });
        });

      return newMap;
    });
  }, []);

  const handleDeleteJob = useCallback(async (jobId: string) => {
    try {
      await deleteBatchJob(jobId);
      setJobs((prev) => prev.filter((job) => job.id !== jobId));
      setExpandedJobs((prev) => {
        const newMap = new Map(prev);
        newMap.delete(jobId);
        return newMap;
      });
      onRefresh?.();
    } catch (error) {
      console.error('[ImportHistory] Failed to delete job:', error);
      toastRef.current.error("Couldn't remove that import");
    }
  }, [onRefresh]);

  const handleRetryFailedItems = useCallback(async (jobId: string) => {
    try {
      await retryFailedItems(jobId);
      toastRef.current.success('Retrying failed items');
      loadJobs();
      onRefresh?.();
    } catch (error) {
      console.error('[ImportHistory] Failed to retry items:', error);
      toastRef.current.error("Couldn't retry those items");
    }
  }, [loadJobs, onRefresh]);

  const handleClearAllHistory = useCallback(async () => {
    try {
      await Promise.all(jobs.map((job) => deleteBatchJob(job.id)));
      setJobs([]);
      setExpandedJobs(new Map());
      onRefresh?.();
    } catch (error) {
      console.error('[ImportHistory] Failed to clear history:', error);
      toastRef.current.error("Couldn't clear the history");
    }
  }, [jobs, onRefresh]);

  if (loading) {
    return <p className="text-sm text-text-muted">Loading…</p>;
  }

  if (jobs.length === 0) {
    return <p className="text-sm text-text-muted">No imports yet.</p>;
  }

  return (
    <div>
      <div className="flex items-center justify-between pb-2">
        <h2 className="text-base font-medium text-text-primary">
          {jobs.length} {jobs.length === 1 ? 'import' : 'imports'}
        </h2>
        <button
          type="button"
          onClick={handleAsyncEvent(handleClearAllHistory)}
          className="text-xs text-text-tertiary transition-colors duration-fast hover:text-text-primary"
        >
          Clear all
        </button>
      </div>

      <div className="border-t border-border-subtle">
        {jobs.map((job) => (
          <JobRow
            key={job.id}
            job={job}
            expanded={expandedJobs.get(job.id)}
            onToggleExpand={handleAsyncEvent(async () => {
              await toggleJobExpansion(job.id);
            })}
            onDelete={handleAsyncEvent(async () => {
              await handleDeleteJob(job.id);
            })}
            onRetry={handleAsyncEvent(async () => {
              await handleRetryFailedItems(job.id);
            })}
          />
        ))}
      </div>
    </div>
  );
};

interface JobRowProps {
  job: BatchJobSummary;
  expanded?: ExpandedJob;
  onToggleExpand: () => void;
  onDelete: () => void;
  onRetry: () => void;
}

const rowActionClass =
  'text-xs text-text-tertiary opacity-0 transition-opacity duration-fast hover:text-text-primary focus-visible:opacity-100 group-hover:opacity-100';

const JobRow: FC<JobRowProps> = ({ job, expanded, onToggleExpand, onDelete, onRetry }) => {
  const isExpanded = !!expanded;
  const hasFailedItems = job.failedItems > 0;
  const status = jobStatusLabel(job);

  return (
    <div className="border-b border-border-subtle">
      <div className="group flex items-center gap-3 py-3">
        <button
          type="button"
          onClick={onToggleExpand}
          aria-expanded={isExpanded}
          className="min-w-0 flex-1 text-left"
        >
          <div className="truncate text-sm text-text-primary">{humanizeJobType(job.jobType)}</div>
          <div className="truncate text-xs text-text-muted tabular-nums">{jobMeta(job)}</div>
        </button>

        {status && <span className="shrink-0 text-xs text-text-muted">{status}</span>}

        <div className="flex shrink-0 items-center gap-3">
          {hasFailedItems && job.status !== 'running' && (
            <button type="button" onClick={onRetry} className={cn(rowActionClass)}>
              Retry
            </button>
          )}
          <button type="button" onClick={onDelete} className={cn(rowActionClass)}>
            Remove
          </button>
        </div>
      </div>

      {isExpanded && (
        <div className="pb-3 pl-4">
          {expanded?.loading ? (
            <p className="text-xs text-text-muted">Loading…</p>
          ) : expanded?.details?.items?.length ? (
            <div className="border-t border-border-subtle">
              {expanded.details.items.map((item) => (
                <div
                  key={item.itemId}
                  className="flex items-center gap-3 border-b border-border-subtle py-2"
                >
                  <span className="min-w-0 flex-1 truncate text-xs text-text-secondary">
                    {item.target || item.url || item.itemId}
                  </span>
                  <span className="max-w-[240px] shrink-0 truncate text-xs text-text-muted">
                    {itemStatusLabel(item.status, item.errorMessage)}
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-xs text-text-muted">No item detail for this import.</p>
          )}
        </div>
      )}
    </div>
  );
};
