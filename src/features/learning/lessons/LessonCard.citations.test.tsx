import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningLessonDto, LearningLessonEvidenceDto, LearningProgramDto } from '@/lib/bindings';
import { cites, said } from '@/tests/replyFixtures';

const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn(), index: vi.fn(), evidence: vi.fn(), version: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, getLearningLessonClaimReview: api.review, getLearningLessonEvidence: api.evidence, courseIndexStatus: api.index,
  getLearningSourceVersion: api.version,
} }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));

const BOILING = 'Water boils when its vapour pressure meets the air’s.';
const blocks = [
  { kind: 'explanation', title: 'Boiling', body: BOILING, sourceIds: ['source-1'] },
  { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
] as LearningLessonDto['blocks'];
const lesson: LearningLessonDto = { id: 'lesson-1', title: 'Phase changes', objective: 'Explain boiling.', estimatedMinutes: 20, preparation: 'ready', blocks, completed: false };
const program = { summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [], modules: [] } as unknown as LearningProgramDto;

const report: LearningLessonEvidenceDto = {
  policy: 'lesson-evidence-v22', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash',
  claimCount: 1, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true,
  teachingClaims: [{
    sectionIndex: 0, contentQuote: BOILING, claim: BOILING, verdict: 'supported', reason: 'Checked.', supportingQuote: null,
    passages: [], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], unverifiedReason: null, cites: [1],
  }],
  citations: [{ number: 1, sourceId: 'source-1', passage: { sourceVersionId: 'version-1', title: 'Steam tables', url: null, text: 'Boiling happens at equal pressures.', startByte: 0, endByte: 35, retrievalKind: 'hybrid', trust: null } }],
  sections: [
    { sectionIndex: 0, pieces: [said(BOILING), cites(1)] },
    { sectionIndex: 1, pieces: [said('- Pressure sets the boiling point.')] },
  ],
};

function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter>
    <LessonCard lesson={lesson} moduleId="module-1" program={program} isCurrent selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} onReview={vi.fn()} />
  </MemoryRouter></QueryClientProvider>);
}
const sectionOf = async (index: number) => {
  await screen.findByRole('heading', { name: index === 0 ? 'Boiling' : 'Key takeaways' });
  return document.getElementById(`lesson-lesson-1-section-${index}`)!;
};

describe('Lesson card with citations drawn in its text', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useCitationDisplayStore.getState().setVisible(true);
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue({ ok: true, data: null });
    api.index.mockResolvedValue({ ok: true, data: { state: 'ready', mode: 'hybrid' } });
    api.version.mockResolvedValue({ ok: false, error: 'The saved text is unavailable.' });
    api.evidence.mockResolvedValue({ ok: true, data: report });
  });
  afterEach(() => useCitationDisplayStore.getState().setVisible(true));

  it('draws a chip and a Sources row in the section whose statement cites a passage, and in no other', async () => {
    show();
    const section = await sectionOf(0);
    await waitFor(() => expect(section.querySelector('[data-cite="1"]')).not.toBeNull());
    expect(within(section).getByText('Sources')).toBeVisible();
    expect(within(section).getByRole('button', { name: 'Citation 1: Steam tables' })).toBeInTheDocument();
    const recap = await sectionOf(1);
    expect(recap.querySelector('[data-cite]')).toBeNull();
    expect(within(recap).queryByText('Sources')).not.toBeInTheDocument();
  });

  it('no longer lists a section’s evidence beneath it', async () => {
    show();
    const section = await sectionOf(0);
    await waitFor(() => expect(section.querySelector('[data-cite="1"]')).not.toBeNull());
    expect(screen.queryByText(/Evidence · \d+ checked/)).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Citations in this section')).not.toBeInTheDocument();
  });

  it('draws the section as it was before when the lesson has no saved report', async () => {
    api.evidence.mockResolvedValue({ ok: true, data: null });
    show();
    const section = await sectionOf(0);
    await waitFor(() => expect(api.evidence).toHaveBeenCalled());
    expect(section).toHaveTextContent(BOILING);
    expect(section.querySelector('[data-cite]')).toBeNull();
    expect(within(section).queryByText('Sources')).not.toBeInTheDocument();
  });

  it('draws no chip and no Sources row with citations turned off', async () => {
    useCitationDisplayStore.getState().setVisible(false);
    show();
    const section = await sectionOf(0);
    await waitFor(() => expect(api.evidence).toHaveBeenCalled());
    expect(section).toHaveTextContent(BOILING);
    expect(section.querySelector('[data-cite]')).toBeNull();
    expect(within(section).queryByText('Sources')).not.toBeInTheDocument();
  });
});
