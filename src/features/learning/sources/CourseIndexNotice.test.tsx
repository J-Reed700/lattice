import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CourseIndexNotice, describeCourseIndex, describeIndexingMeanwhile, EMBEDDING_SETTINGS } from '@/features/learning/sources/CourseIndexNotice';
import type { CourseIndexStatusDto, JobDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ status: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { courseIndexStatus: api.status } }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
type Indexing = Extract<CourseIndexStatusDto, { state: 'indexing' }>;
const indexing = (overrides: Partial<Indexing> = {}): Indexing =>
  ({ state: 'indexing', embedded: 120, total: 1_204, referencesNew: 3, passagesPerSecond: null, waitingTurn: false, ...overrides });

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
    expect(describeCourseIndex(indexing())).toBe('Indexing this course’s references');
    expect(describeCourseIndex(indexing({ waitingTurn: true }))).toBe('Waiting to index this course’s references');
    expect(describeCourseIndex({ state: 'noEmbeddingModel' })).toBe(
      'References are searched by words only. Choose an embedding model in Settings to search by meaning',
    );
    expect(describeCourseIndex({ state: 'unavailable', reason: 'The index file is damaged.' })).toBe(
      'References are searched by words only. The index file is damaged.',
    );
    expect(describeCourseIndex({ state: 'ready', mode: 'hybrid' })).toBeNull();
    expect(describeCourseIndex(null)).toBeNull();
    expect(describeIndexingMeanwhile(indexing())).toBe(
      'Search covers the whole course by its words and sharpens as passages embed; lesson preparation and verification wait until it finishes.',
    );
    expect(describeIndexingMeanwhile(indexing({ waitingTurn: true }))).toMatch(/^Another course’s references are indexing first, and this course starts when they finish\. Search covers/);
  });

  it('shows a bar of the passages embedded, their counts and the time left at the measured pace', async () => {
    show(indexing({ embedded: 4_210, total: 13_457, passagesPerSecond: 14 }));
    const notice = await screen.findByRole('status', { name: 'Course index' });
    expect(notice).toHaveTextContent('Indexing this course’s references');
    expect(notice).toHaveTextContent('about 11 min left');
    expect(notice).toHaveTextContent('4,210 of 13,457 passages');
    expect(notice).toHaveTextContent('lesson preparation and verification wait until it finishes');
    const bar = screen.getByRole('progressbar', { name: 'Passages embedded' });
    expect(bar).toHaveAttribute('aria-valuenow', '4210');
    expect(bar).toHaveAttribute('aria-valuemax', '13457');
    expect(bar).toHaveAttribute('aria-valuetext', '4,210 of 13,457 passages');
    expect(bar.firstElementChild).toHaveStyle({ width: '31%' });
  });

  it('shows the counts and that it is measuring speed before the build has a pace, with no estimate', async () => {
    show(indexing());
    const notice = await screen.findByRole('status', { name: 'Course index' });
    expect(notice).toHaveTextContent('120 of 1,204 passages');
    expect(notice).toHaveTextContent('measuring speed…');
    expect(notice).not.toHaveTextContent(/min left|less than a minute/);
    expect(screen.getByRole('progressbar', { name: 'Passages embedded' })).toHaveAttribute('aria-valuenow', '120');
  });

  it('says which new references it adds before any passage is counted', async () => {
    show(indexing({ embedded: 0, total: 0, referencesNew: 402 }));
    const notice = await screen.findByRole('status', { name: 'Course index' });
    expect(notice).toHaveTextContent('Adding 402 new references');
    expect(notice).toHaveTextContent('measuring speed…');
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });

  it('says in a plain sentence that the course waits its turn behind another index, with no estimate', async () => {
    show(indexing({ embedded: 0, total: 13_457, waitingTurn: true }));
    const notice = await screen.findByRole('status', { name: 'Course index' });
    expect(notice).toHaveTextContent('Waiting to index this course’s references');
    expect(notice).toHaveTextContent('0 of 13,457 passages');
    expect(notice).toHaveTextContent('Another course’s references are indexing first, and this course starts when they finish.');
    expect(notice).not.toHaveTextContent(/measuring speed|left/);
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

  it('moves the counts and the time left with each embedded batch the build reports', async () => {
    show(indexing({ embedded: 4_210, total: 13_457, passagesPerSecond: 14 }));
    const notice = await screen.findByRole('status', { name: 'Course index' });
    expect(notice).toHaveTextContent('about 11 min left');
    const handler = vi.mocked(listen).mock.calls.find(([event]) => event === 'jobs://status')?.[1];
    expect(handler).toBeDefined();
    api.status.mockResolvedValue(ok(indexing({ embedded: 13_200, total: 13_457, passagesPerSecond: 14 })));
    handler!({ payload: { kind: 'learning.course_index', subjectId: 'program-1', status: 'running' } as JobDto, event: 'jobs://status', id: 4 });
    await waitFor(() => expect(notice).toHaveTextContent('less than a minute left'));
    expect(notice).toHaveTextContent('13,200 of 13,457 passages');
  });
});
