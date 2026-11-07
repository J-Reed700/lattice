import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { ClaimReviewItemDto, LearningClaimEvidenceDto, LearningLessonDto, LearningLessonEvidenceDto, LearningProgramDto } from '@/lib/bindings';

// A lesson that publishes with statements verification could not settle says so on the course page
// (design-publish-unverified.md, "Wording"). The review and the evidence report are given the same
// statements, so these tests do not fix which of the two the card and the section note read.
const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn(), index: vi.fn(), evidence: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, getLearningLessonClaimReview: api.review, getLearningLessonEvidence: api.evidence, courseIndexStatus: api.index,
} }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));

type Reason = 'unchecked' | 'unsupported' | 'contradicted';
const blocks = [
  { kind: 'explanation', title: 'Boiling', body: 'Water boils when its vapour pressure meets the air’s.', sourceIds: [] },
  { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
] as LearningLessonDto['blocks'];
const lesson: LearningLessonDto = { id: 'lesson-1', title: 'Phase changes', objective: 'Explain boiling.', estimatedMinutes: 20, preparation: 'ready', blocks, completed: false };
const program = { summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [], modules: [] } as unknown as LearningProgramDto;

const STATEMENT = 'Water boils at 100 °C on Everest.';
const unverifiedItem = (index: number, reason: Reason = 'unchecked'): ClaimReviewItemDto => claimItem({
  claimId: `claim-${index}`, statement: STATEMENT, state: { state: 'unverified', at: 2, reason }, verdictKind: null, reason: null, availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
});
const verifiedItem = claimItem({ claimId: 'claim-v', state: { state: 'resolved_by_evidence' }, verdictKind: null, reason: null, availableActions: [] });

const evidenceClaim = (overrides: Partial<LearningClaimEvidenceDto>): LearningClaimEvidenceDto => ({
  sectionIndex: 0, contentQuote: 'The sentence.', claim: 'Ice floats on water.', verdict: 'supported', reason: 'Checked.', supportingQuote: null,
  passages: [], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], unverifiedReason: null, ...overrides,
});
const evidence = (teachingClaims: LearningClaimEvidenceDto[]): LearningLessonEvidenceDto => ({
  policy: 'lesson-evidence-v21', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash',
  claimCount: teachingClaims.length, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true, teachingClaims,
});
const unverifiedClaim = (reason: Reason, sectionIndex = 0) => evidenceClaim({ claim: STATEMENT, sectionIndex, resolution: 'not_verified', unverifiedReason: reason, verdict: 'unsupported' });

function show(options: { current?: boolean; onReview?: () => void } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter>
    <LessonCard lesson={lesson} moduleId="module-1" program={program} isCurrent={options.current ?? false} selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} onReview={options.onReview} />
  </MemoryRouter></QueryClientProvider>);
}

/** What a section shows outside its collapsed evidence: the note must not hide in a closed <details>. */
function visibleOutsideEvidence(section: HTMLElement): string {
  const copy = section.cloneNode(true) as HTMLElement;
  copy.querySelectorAll('details').forEach((details) => details.remove());
  return (copy.textContent ?? '').replace(/\s+/g, ' ');
}
const sectionOf = async (index: number) => {
  await screen.findByRole('heading', { name: index === 0 ? 'Boiling' : 'Key takeaways' });
  return document.getElementById(`lesson-lesson-1-section-${index}`)!;
};
const quoted = new RegExp(`Not verified:.*[“"]${STATEMENT}[”"]`);

describe('Lesson card with statements that are not verified', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useCitationDisplayStore.getState().setVisible(true);
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.index.mockResolvedValue({ ok: true, data: { state: 'ready', mode: 'hybrid' } });
    api.evidence.mockResolvedValue({ ok: true, data: null });
  });
  afterEach(() => useCitationDisplayStore.getState().setVisible(true));

  it.each([
    [1, 'Ready · 1 statement not verified'],
    [2, 'Ready · 2 statements not verified'],
  ])('says a ready lesson with %i unverified statement(s) is ready, with a Review button', async (count, status) => {
    api.review.mockResolvedValue({ ok: true, data: review([verifiedItem, ...Array.from({ length: count }, (_, index) => unverifiedItem(index + 1))], { state: 'ready' }) });
    const onReview = vi.fn();
    show({ onReview });
    const row = screen.getByRole('article');
    expect(await within(row).findByText(status)).toBeVisible();
    await userEvent.click(within(row).getByRole('button', { name: 'Review' }));
    expect(onReview).toHaveBeenCalledWith('program-1', 'lesson-1');
  });

  it.each([
    ['unchecked', /The check could not be completed\./],
    ['unsupported', /The lesson[’']s sources do not state this\./],
    ['contradicted', /A source appears to disagree with this\./],
  ] as const)('shows under the section, outside the collapsed evidence, why a %s statement is not verified', async (reason, words) => {
    api.review.mockResolvedValue({ ok: true, data: review([unverifiedItem(1, reason)], { state: 'ready' }) });
    api.evidence.mockResolvedValue({ ok: true, data: evidence([evidenceClaim({}), unverifiedClaim(reason)]) });
    show({ current: true });
    const section = await sectionOf(0);
    await vi.waitFor(() => expect(visibleOutsideEvidence(section)).toMatch(quoted));
    const text = visibleOutsideEvidence(section);
    expect(text).toMatch(words);
    // Its reason follows the statement, and the statements that passed are not listed as unverified.
    expect(text.search(words)).toBeGreaterThan(text.search(quoted));
    expect(text).not.toMatch(/Ice floats on water/);
  });

  it('shows the note also when the learner has citations turned off', async () => {
    useCitationDisplayStore.getState().setVisible(false);
    api.review.mockResolvedValue({ ok: true, data: review([unverifiedItem(1, 'unsupported')], { state: 'ready' }) });
    api.evidence.mockResolvedValue({ ok: true, data: evidence([unverifiedClaim('unsupported')]) });
    show({ current: true });
    const section = await sectionOf(0);
    await vi.waitFor(() => expect(visibleOutsideEvidence(section)).toMatch(quoted));
    expect(screen.queryByRole('region', { name: 'Lesson evidence' })).not.toBeInTheDocument();
  });

  it('puts the note under the section that holds the statement and under no other', async () => {
    api.review.mockResolvedValue({ ok: true, data: review([unverifiedItem(1)], { state: 'ready' }) });
    api.evidence.mockResolvedValue({ ok: true, data: evidence([unverifiedClaim('unchecked', 1)]) });
    show({ current: true });
    const recap = await sectionOf(1);
    await vi.waitFor(() => expect(visibleOutsideEvidence(recap)).toMatch(/Not verified:/));
    expect(visibleOutsideEvidence(await sectionOf(0))).not.toMatch(/Not verified/);
  });

  it('reads a ready lesson with nothing unverified exactly as before: Ready, no note, no Review button', async () => {
    api.review.mockResolvedValue({ ok: true, data: review([verifiedItem], { state: 'ready' }) });
    api.evidence.mockResolvedValue({ ok: true, data: evidence([evidenceClaim({}), evidenceClaim({ claim: 'Water expands as it freezes.' })]) });
    show({ current: true });
    const row = screen.getByRole('article');
    expect(await within(row).findByText('Ready')).toBeVisible();
    await vi.waitFor(() => expect(api.review).toHaveBeenCalled());
    await vi.waitFor(() => expect(api.evidence).toHaveBeenCalled());
    expect(row).not.toHaveTextContent(/not verified/i);
    expect(screen.queryByRole('button', { name: 'Review' })).not.toBeInTheDocument();
  });
});
