import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { learningClaimReviewKey, learningLessonEvidencePrefix, learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningClaimEvidenceDto, LearningLessonDto, LearningLessonEvidenceDto, LearningProgramDto } from '@/lib/bindings';

// The learner settles a published lesson's unverified statements from the note under their section
// (design-settle-published.md). The evidence report names each statement's claim (`claimId`); Confirm and Edit
// go through the review's claim action with the review's revision, and the review, evidence and program refresh.
const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn(), index: vi.fn(), evidence: vi.fn(), resolve: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan, getLearningLessonClaimReview: api.review, getLearningLessonEvidence: api.evidence, courseIndexStatus: api.index,
  resolveLearningClaim: api.resolve,
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
// The sentence the review holds for the claim, which neither the statement nor the evidence's quote repeats.
const SENTENCE = 'Water boils at 100 °C on Everest, as the guide notes.';
const REVISION = 9;
const ok = <T,>(data: T) => ({ ok: true, data });

const evidenceClaim = (overrides: Record<string, unknown>) => ({
  sectionIndex: 0, contentQuote: 'The sentence.', claim: 'Ice floats on water.', verdict: 'supported', reason: 'Checked.', supportingQuote: null,
  passages: [], resolution: 'verified', note: null, sufficiency: null, class: 'sourced', label: null, parts: [], unverifiedReason: null, claimId: 'claim-v', ...overrides,
}) as LearningClaimEvidenceDto;
const unverified = (reason: Reason, overrides: Record<string, unknown> = {}) => evidenceClaim({
  claim: STATEMENT, contentQuote: 'Everest water boils at 100 °C.', resolution: 'not_verified', unverifiedReason: reason, verdict: 'unsupported', claimId: 'claim-1', ...overrides,
});
const confirmed = () => evidenceClaim({ claim: STATEMENT, contentQuote: 'Everest water boils at 100 °C.', resolution: 'confirmed_by_you', claimId: 'claim-1' });
const evidence = (teachingClaims: LearningClaimEvidenceDto[]): LearningLessonEvidenceDto => ({
  policy: 'lesson-evidence-v21', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid', embeddingModel: null, contentSha256: 'hash',
  claimCount: teachingClaims.length, confirmedCount: 0, executedExamples: 0, unexecutedLanguages: [], sourcesCurrent: true, teachingClaims,
});

/** The evidence reads as settled once the claim action has run, as the backend's evidence follows each claim's state. */
function serve(reason: Reason, overrides: Record<string, unknown> = {}, reviewed = true) {
  let settled = false;
  api.review.mockResolvedValue(ok(review(reviewed ? [claimItem({ claimId: 'claim-1', location: { unitIndex: 0, unitKind: 'block', unitTitle: 'Boiling', field: 'body', quote: SENTENCE, span: null }, state: { state: 'unverified', at: 2, reason }, verdictKind: null, availableActions: ['confirm', 'edit', 'attach_evidence'] })] : [], { state: 'ready', revision: REVISION })));
  api.evidence.mockImplementation(async () => ok(evidence(settled ? [confirmed()] : [unverified(reason, overrides)])));
  api.resolve.mockImplementation(async () => { settled = true; return ok({ revision: REVISION + 1 }); });
}

function show(onReview = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  render(<QueryClientProvider client={client}><MemoryRouter>
    <LessonCard lesson={lesson} moduleId="module-1" program={program} isCurrent selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} onReview={onReview} />
  </MemoryRouter></QueryClientProvider>);
  return { onReview, invalidate };
}

const note = async () => within(await screen.findByRole('note'));
const noteGone = () => waitFor(() => expect(screen.queryByText('Not verified:')).not.toBeInTheDocument());

describe('Settling a published lesson’s unverified statements from the note', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useCitationDisplayStore.getState().setVisible(true);
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.index.mockResolvedValue(ok({ state: 'ready', mode: 'hybrid' }));
  });

  it('offers "This is right", "Edit" and "Check the sources" on each statement', async () => {
    serve('unsupported');
    show();
    const inNote = await note();
    for (const name of ['This is right', 'Edit', 'Check the sources']) expect(inNote.getByRole('button', { name })).toBeVisible();
  });

  // Guard: nothing is offered today, so this cannot fail until the actions exist.
  it('offers no action on a statement from a report that names no claim', async () => {
    serve('unsupported', { claimId: null });
    show();
    const inNote = await note();
    expect(inNote.getByText(/Water boils at 100 °C on Everest\./)).toBeVisible();
    for (const name of ['This is right', 'Edit', 'Check the sources']) expect(inNote.queryByRole('button', { name })).not.toBeInTheDocument();
  });

  it('confirms that claim with the review’s revision, and the statement leaves the note once the evidence refreshes', async () => {
    serve('unsupported');
    show();
    await userEvent.click((await note()).getByRole('button', { name: 'This is right' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledTimes(1));
    expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1', lessonId: 'lesson-1', claimId: 'claim-1', expectedRevision: REVISION,
      action: expect.objectContaining({ action: 'confirm', note: null }),
    }));
    await noteGone();
  });

  it('asks a contradicted statement for the reason first and sends it as the confirm’s note', async () => {
    serve('contradicted');
    show();
    await userEvent.click((await note()).getByRole('button', { name: 'This is right' }));
    expect(await screen.findByText('A source disagrees with this. Why is it right?')).toBeVisible();
    expect(api.resolve).not.toHaveBeenCalled();
    const inNote = await note();
    await userEvent.click(inNote.getByRole('button', { name: 'Save' }));
    expect(api.resolve).not.toHaveBeenCalled();
    await userEvent.type(inNote.getByRole('textbox'), 'The 100 °C figure is for sea level, which the section states.');
    await userEvent.click(inNote.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledTimes(1));
    expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({
      claimId: 'claim-1', expectedRevision: REVISION,
      action: expect.objectContaining({ action: 'confirm', note: 'The 100 °C figure is for sea level, which the section states.' }),
    }));
    await noteGone();
  });

  // Guard: nothing is offered today, so this cannot fail until the actions exist.
  it('does not ask a statement that is not contradicted for a reason', async () => {
    serve('unchecked');
    show();
    await userEvent.click((await note()).getByRole('button', { name: 'This is right' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledTimes(1));
    expect(screen.queryByText('A source disagrees with this. Why is it right?')).not.toBeInTheDocument();
  });

  it('offers neither "This is right" nor "Edit" on a statement whose claim the review does not hold', async () => {
    serve('unsupported', {}, false);
    show();
    const inNote = await note();
    expect(inNote.getByText(/Water boils at 100 °C on Everest\./)).toBeVisible();
    for (const name of ['This is right', 'Edit']) expect(inNote.queryByRole('button', { name })).not.toBeInTheDocument();
  });

  it('opens the edit box holding the claim’s sentence from the review, and Save sends the user’s text', async () => {
    serve('unsupported');
    show();
    await userEvent.click((await note()).getByRole('button', { name: 'Edit' }));
    const box = screen.getByLabelText('Your version');
    expect(box).toHaveValue(SENTENCE);
    await userEvent.clear(box);
    await userEvent.type(box, 'Water boils at about 70 °C on Everest.');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.resolve).toHaveBeenCalledTimes(1));
    expect(api.resolve).toHaveBeenCalledWith(expect.objectContaining({
      claimId: 'claim-1', expectedRevision: REVISION,
      action: { action: 'edit', before: SENTENCE, after: 'Water boils at about 70 °C on Everest.' },
    }));
    await noteGone();
  });

  it('closes the edit box on Cancel and sends nothing', async () => {
    serve('unsupported');
    show();
    await userEvent.click((await note()).getByRole('button', { name: 'Edit' }));
    await userEvent.type(screen.getByLabelText('Your version'), ' Changed.');
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByLabelText('Your version')).not.toBeInTheDocument();
    expect(api.resolve).not.toHaveBeenCalled();
    expect(screen.getByText('Not verified:')).toBeVisible();
  });

  it.each([
    ['a confirm', async () => { await userEvent.click((await note()).getByRole('button', { name: 'This is right' })); }],
    ['an edit', async () => {
      await userEvent.click((await note()).getByRole('button', { name: 'Edit' }));
      await userEvent.type(screen.getByLabelText('Your version'), ' Changed.');
      await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    }],
  ])('refreshes the review, the evidence and the program after %s', async (_, act) => {
    serve('unsupported');
    const { invalidate } = show();
    await act();
    await waitFor(() => expect(api.resolve).toHaveBeenCalledTimes(1));
    await waitFor(() => {
      const keys = invalidate.mock.calls.map(([filters]) => JSON.stringify((filters as { queryKey?: unknown } | undefined)?.queryKey));
      expect(keys).toContain(JSON.stringify(learningProgramKey('program-1')));
      expect(keys).toContain(JSON.stringify(learningLessonEvidencePrefix('program-1')));
      expect(keys).toContain(JSON.stringify(learningClaimReviewKey('program-1', 'lesson-1')));
    });
  });

  it('opens the review on that claim from "Check the sources"', async () => {
    serve('unsupported');
    const { onReview } = show();
    await userEvent.click((await note()).getByRole('button', { name: 'Check the sources' }));
    expect(onReview).toHaveBeenCalledWith('program-1', 'lesson-1', 'claim-1');
    expect(api.resolve).not.toHaveBeenCalled();
  });

  it('shows no note and no action for a ready lesson with nothing unverified', async () => {
    api.review.mockResolvedValue(ok(review([], { state: 'ready', revision: REVISION })));
    api.evidence.mockResolvedValue(ok(evidence([evidenceClaim({})])));
    show();
    await waitFor(() => expect(api.evidence).toHaveBeenCalled());
    await screen.findByText('Ready');
    expect(screen.queryByText('Not verified:')).not.toBeInTheDocument();
    for (const name of ['This is right', 'Check the sources']) expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Edit' })).not.toBeInTheDocument();
  });
});
