import type { FolderIndexStatus } from '@/features/explorer/stores/explorerStore';
import type { JobDto } from '@/lib/bindings';

/** A job as `jobs://status` carries it. */
export function jobFixture(overrides: Partial<JobDto> = {}): JobDto {
  return {
    id: 'job-1',
    kind: 'batch.file_import',
    subjectId: null,
    status: 'running',
    progressCurrent: 0,
    progressTotal: 1,
    progressMessage: 'Starting',
    activity: null,
    resultRef: null,
    errorCode: null,
    error: null,
    retryOfJobId: null,
    retryCount: 0,
    retryNotBefore: null,
    createdAt: 1,
    startedAt: 1,
    finishedAt: null,
    ...overrides,
  };
}

/** A folder index build reporting `activity`: running while it scans or embeds, completed after. */
export function folderBuildFixture(activity: FolderIndexStatus, overrides: Partial<JobDto> = {}): JobDto {
  const running = activity.state === 'scanning' || activity.state === 'indexing';
  return jobFixture({
    id: `build:${activity.root}`,
    kind: 'explorer.folder_index',
    subjectId: activity.indexRoot,
    status: running ? 'running' : 'completed',
    progressTotal: 100,
    activity: { ...activity },
    finishedAt: running ? null : 2,
    ...overrides,
  });
}
