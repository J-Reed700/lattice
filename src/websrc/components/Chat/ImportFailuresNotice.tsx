import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { getBatchJobDetails, listAllBatchJobs } from '@/utils/batchHistory';

/** Durable import history, including failures that finished while Chat was open. */
export function ImportFailuresNotice() {
  const { data, error } = useQuery({
    queryKey: ['batch-imports', 'coverage'],
    queryFn: async () => {
      const jobs = (await listAllBatchJobs()).filter(job => job.jobType === 'file_import');
      const failed = jobs.reduce((sum, job) => sum + job.failedItems, 0);
      const active = jobs.some(job => ['pending', 'running'].includes(job.status));
      const details = await Promise.all(jobs.filter(job => job.failedItems > 0).slice(0, 3).map(job => getBatchJobDetails(job.id)));
      const names = details.flatMap(job => job.items.filter(item => item.status === 'failed').map(item => item.target.split(/[\\/]/).pop() || item.target));
      return { failed, active, names };
    },
    staleTime: 15_000,
    refetchInterval: 15_000,
    refetchOnMount: 'always',
  });
  if (!data?.failed && !data?.active && !error) return null;

  return (
    <div role="status" className="border-b border-border-subtle px-6 py-3 text-sm text-text-secondary">
      {error
        ? 'Import status is unavailable. Source coverage could not be checked.'
        : data?.failed
          ? `${data.failed} ${data.failed === 1 ? 'file failed' : 'files failed'} to import${data.names.length ? `: ${data.names.slice(0, 3).join(', ')}${data.failed > 3 ? '…' : ''}` : ''}. Review or retry the failed files.`
          : 'Files are still importing. Some sources are not ready to search yet.'}{' '}
      <Link to="/ingest?tab=history" className="text-accent underline underline-offset-2">
        Review import history
      </Link>
    </div>
  );
}
