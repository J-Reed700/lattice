import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useLessonCitationSources } from '@/features/learning/lessons/useLessonCitationSources';
import type { LearningCitationDto, LearningLessonEvidenceDto } from '@/lib/bindings';

const mocks = vi.hoisted(() => ({ version: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningSourceVersion: mocks.version } }));

const FULL_TEXT: Record<string, string> = {
  'guide-v1': 'The whole field guide. A carefully chosen passage sits in the middle of it.',
  'notes-v1': 'The whole of the lecture notes, with the passage in them.',
};

const citation = (number: number, sourceId: string | null, version: string, overrides: Partial<LearningCitationDto['passage']> = {}): LearningCitationDto => ({
  number, sourceId,
  passage: { sourceVersionId: version, title: `Title ${number}`, url: null, text: `Passage ${number}`, startByte: number * 10, endByte: number * 10 + 9, retrievalKind: 'hybrid', trust: null, ...overrides },
});
const report = (citations: LearningCitationDto[]) => ({ citations } as unknown as LearningLessonEvidenceDto);

const cited = report([
  citation(1, 'guide', 'guide-v1', { url: 'https://example.org/guide' }),
  citation(2, 'notes', 'notes-v1'),
  citation(3, null, 'run-1', { retrievalKind: 'execution' }),
  citation(4, 'guide', 'guide-v1', { url: 'https://example.org/guide' }),
]);

function render(data: LearningLessonEvidenceDto | null | undefined, enabled = true) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return renderHook(() => useLessonCitationSources('course-1', data, enabled), { wrapper });
}

describe('the sources behind a lesson’s citations', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.version.mockImplementation(async ({ sourceId, versionId }: { sourceId: string; versionId: string }) => ({
      ok: true, data: { sourceId, fullText: FULL_TEXT[versionId] ?? '', usage: [], version: { id: versionId, versionNumber: 1, title: 'Saved', resolvedUrl: null } },
    }));
  });

  it('numbers each source by its citation number, in the report’s order', () => {
    const { result } = render(cited);
    expect(result.current.map((source) => source.citationId)).toEqual([1, 2, 3, 4]);
    expect(result.current.map((source) => source.fileName)).toEqual(['Title 1', 'Title 2', 'Title 3', 'Title 4']);
  });

  it('shows the cited passage until the saved text has loaded', () => {
    const { result } = render(cited);
    expect(result.current[0]?.content).toBe('Passage 1');
    expect(result.current[1]?.content).toBe('Passage 2');
  });

  it('gives a web passage its saved page to read, and keeps the passage as its content', async () => {
    const { result } = render(cited);
    await waitFor(() => expect(result.current[0]?.webSnapshot?.text).toBe(FULL_TEXT['guide-v1']));
    expect(result.current[0]?.content).toBe('Passage 1');
    expect(result.current[0]?.excerpt).toBe('Passage 1');
    // Two passages of one saved version read the same page.
    expect(result.current[3]?.webSnapshot?.text).toBe(FULL_TEXT['guide-v1']);
    expect(result.current[3]?.content).toBe('Passage 4');
    expect(result.current[1]?.content).toBe('Passage 2');
    expect(result.current[1]?.webSnapshot).toBeUndefined();
  });

  it('fetches each saved web page once, and never a passage without a page or an executed example', async () => {
    const { result } = render(cited);
    await waitFor(() => expect(result.current[0]?.webSnapshot?.text).toBe(FULL_TEXT['guide-v1']));
    expect(mocks.version).toHaveBeenCalledTimes(1);
    expect(mocks.version).toHaveBeenCalledWith({ programId: 'course-1', sourceId: 'guide', versionId: 'guide-v1' });
    expect(result.current[2]?.content).toBe('Passage 3');
  });

  it('keeps the passage for a version whose text could not be read', async () => {
    mocks.version.mockResolvedValue({ ok: false, error: 'The saved text is unavailable.' });
    const { result } = render(cited);
    await waitFor(() => expect(mocks.version).toHaveBeenCalledTimes(1));
    expect(result.current[0]?.webSnapshot?.text).toBe('Passage 1');
    expect(result.current[0]?.content).toBe('Passage 1');
  });

  it('reads nothing while it is not enabled, and has no sources without a report', async () => {
    expect(render(cited, false).result.current).toHaveLength(4);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(mocks.version).not.toHaveBeenCalled();
    expect(render(null).result.current).toEqual([]);
    expect(render(undefined).result.current).toEqual([]);
  });
});
