import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CourseIndexNotice, describeCourseIndex, EMBEDDING_SETTINGS } from '@/features/learning/sources/CourseIndexNotice';
import type { CourseIndexStatusDto, JobDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ status: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { courseIndexStatus: api.status } }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const indexing = (overrides: Partial<Extract<CourseIndexStatusDto, { state: 'indexing' }>> = {}): CourseIndexStatusDto =>
  ({ state: 'indexing', embedded: 120, total: 1_204, referencesNew: 3, passagesPerSecond: null, ...overrides });

function show(status: CourseIndexStatusDto) {
  api.status.mockResolvedValue(ok(status));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}><MemoryRouter>{children}</MemoryRouter></QueryClientProvider>;
  return render(<CourseIndexNotice programId="program-1" />, { wrapper });
}

describe('Course index notice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('words every state, and says nothing once the course is ready', () => {
    expect(describeCourseIndex(indexing())).toBe('Indexing references · 120 of 1,204 passages');
    expect(describeCourseIndex(indexing({ passagesPerSecond: 37.4 }))).toBe('Indexing references · 120 of 1,204 passages · 37 passages/s');
    expect(describeCourseIndex(indexing({ embedded: 0, total: 0 }))).toBe('Indexing 3 new references');
    expect(describeCourseIndex(indexing({ embedded: 0, total: 0, referencesNew: 1 }))).toBe('Indexing 1 new reference');
    expect(describeCourseIndex({ state: 'noEmbeddingModel' })).toBe(
      'References are searched by words only. Choose an embedding model in Settings to search by meaning',
    );
    expect(describeCourseIndex({ state: 'unavailable', reason: 'The index file is damaged.' })).toBe(
      'References are searched by words only. The index file is damaged.',
    );
    expect(describeCourseIndex({ state: 'ready', mode: 'hybrid' })).toBeNull();
    expect(describeCourseIndex(null)).toBeNull();
  });

  it('shows indexing progress and that evidence actions wait for it', async () => {
    show(indexing());
    const notice = await screen.findByRole('status');
    expect(notice).toHaveTextContent('Indexing references · 120 of 1,204 passages');
    expect(notice).toHaveTextContent('wait until it finishes');
  });

  it('points to the embedding model setting when none is chosen', async () => {
    show({ state: 'noEmbeddingModel' });
    expect(await screen.findByRole('status')).toHaveTextContent('References are searched by words only');
    expect(screen.getByRole('link', { name: 'Open model settings' })).toHaveAttribute('href', EMBEDDING_SETTINGS);
  });

  it('gives the reason when the index is unavailable', async () => {
    show({ state: 'unavailable', reason: 'Indexing stopped before every passage was embedded.' });
    expect(await screen.findByRole('status')).toHaveTextContent('Indexing stopped before every passage was embedded.');
  });

  it('shows nothing for a ready course', async () => {
    const { container } = show({ state: 'ready', mode: 'hybrid' });
    await waitFor(() => expect(api.status).toHaveBeenCalledWith('program-1'));
    expect(container).toBeEmptyDOMElement();
  });

  it('reads the status again when one of the course’s builds moves, and only then', async () => {
    show(indexing());
    await screen.findByRole('status');
    const handler = vi.mocked(listen).mock.calls.find(([event]) => event === 'jobs://status')?.[1];
    expect(handler).toBeDefined();
    const job = (kind: string, subjectId: string) => ({ payload: { kind, subjectId, status: 'running' } as JobDto });
    api.status.mockResolvedValue(ok({ state: 'ready', mode: 'hybrid' }));
    handler!({ ...job('learning.course_index', 'other-course'), event: 'jobs://status', id: 1 });
    handler!({ ...job('explorer.folder_index', 'program-1'), event: 'jobs://status', id: 2 });
    expect(api.status).toHaveBeenCalledTimes(1);
    handler!({ ...job('learning.course_index', 'program-1'), event: 'jobs://status', id: 3 });
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument());
    expect(api.status).toHaveBeenCalledTimes(2);
  });
});
