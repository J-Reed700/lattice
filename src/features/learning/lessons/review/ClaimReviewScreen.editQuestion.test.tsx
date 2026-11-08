import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review, teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LessonClaimReviewDto } from '@/lib/bindings';

// A statement or a teaching note on a question of a published lesson opens the editor for that question.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(), program: vi.fn(), get: vi.fn(), edit: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  getLearningProgram: api.program, getLearningLessonQuestion: api.get, editLearningLessonQuestion: api.edit,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = { verified: 0, confirmed: 0, queued: 0, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 };

const SENTENCE = 'Header names keep their spaces exactly as written.';
const SECTIONS = [{ index: 0, title: 'Boiling' }, { index: 1, title: 'Freezing' }];
const QUESTION = {
  kind: 'single_choice', prompt: 'What does DictReader do with spaces in a header name?',
  choices: ['It keeps them.', 'It trims them.'], right: 0, explanation: SENTENCE,
};
const program = {
  summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [],
  modules: [{ id: 'module-1', lessons: [{ id: 'lesson-1', blocks: [
    { kind: 'explanation', title: 'Boiling', body: 'Boiling is a phase change.', sourceIds: [] },
    { kind: 'explanation', title: 'Freezing', body: 'Ice forms below freezing.', sourceIds: [] },
  ] }] }],
};

const unverified = { state: 'unverified', at: 2, reason: 'unsupported' } as const;
const onUnit = (claimId: string, unitIndex: number, unitKind: 'block' | 'question' | 'answer_key', field: string) => claimItem({
  claimId, statement: SENTENCE, state: unverified, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
  location: { unitIndex, unitKind, unitTitle: '', field, quote: SENTENCE, span: null },
  presentation: { statement: SENTENCE, sentence: SENTENCE, contextBefore: '', contextAfter: '', place: 'Question' },
});

function show(data: LessonClaimReviewDto) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Phase changes" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}
const published = (claims = [onUnit('claim-1', 2, 'question', 'prompt')], rest: Partial<LessonClaimReviewDto> = {}) =>
  review(claims, { state: 'ready', counts, sections: SECTIONS, ...rest });

const EDIT = { name: 'Edit the question' };
const openNotes = async () => {
  await userEvent.click(await screen.findByRole('button', { name: /^Teaching notes/ }));
  return screen.findByRole('region', { name: 'Teaching review' });
};
const note = (overrides = {}) => teachingFinding({
  findingKey: 'answer_key:question-1', kind: 'answer_key', site: 'question', unitIndex: 1, unitTitle: 'Question 2', sentence: null, passage: null, ...overrides,
});

describe('Claim review screen: editing the question of a statement or a note', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
    api.program.mockResolvedValue(ok(program));
    api.get.mockResolvedValue(ok(QUESTION));
    api.edit.mockResolvedValue(ok({ changed: true, statementsKept: 0, statementsChanged: 1, statementsRemoved: 0, sentencesWritten: 1, checkedStatementsAffected: 0, revision: null }));
  });

  it('offers Edit the question on a statement about a question and opens the editor on it, counting past the sections', async () => {
    show(published());
    await userEvent.click(await screen.findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit question 1' })).toBeVisible();
    expect(api.get).toHaveBeenCalledWith('program-1', 'lesson-1', 0);
    expect(await screen.findByRole('textbox', { name: 'Question' })).toHaveValue(QUESTION.prompt);
  });

  it('offers it on a statement about an answer key, for the question it belongs to', async () => {
    show(published([onUnit('claim-2', 3, 'answer_key', 'explanation')]));
    await userEvent.click(await screen.findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit question 2' })).toBeVisible();
    expect(api.get).toHaveBeenCalledWith('program-1', 'lesson-1', 1);
  });

  it('does not offer Edit the section on a question statement', async () => {
    show(published());
    expect(await screen.findByRole('button', EDIT)).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit the section' })).not.toBeInTheDocument();
  });

  it('keeps Edit the section on a section statement and offers no question editor there', async () => {
    show(published([onUnit('claim-3', 0, 'block', 'body')]));
    expect(await screen.findByRole('button', { name: 'Edit the section' })).toBeVisible();
    expect(screen.queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('announces that the question was saved', async () => {
    show(published());
    await userEvent.click(await screen.findByRole('button', EDIT));
    await userEvent.type(await screen.findByRole('textbox', { name: 'Explanation' }), ' Really.');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(screen.getAllByRole('status').some((node) => node.textContent === 'Question saved.')).toBe(true);
  });

  it('does not offer it before the lesson is published', async () => {
    show(review([onUnit('claim-1', 2, 'question', 'prompt')], { state: 'needs_review', counts, sections: SECTIONS }));
    expect(await screen.findByLabelText('Claim in focus')).toBeVisible();
    expect(screen.queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('offers it on a teaching note about a question and opens the editor on that question', async () => {
    show(published([], { teachingFindings: [note()] }));
    const notes = await openNotes();
    await userEvent.click(await within(notes).findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit question 2' })).toBeVisible();
    expect(api.get).toHaveBeenCalledWith('program-1', 'lesson-1', 1);
  });

  it('does not offer it on a teaching note about a section', async () => {
    show(published([], { teachingFindings: [teachingFinding({ site: 'section', unitIndex: 1, unitTitle: 'Section 2: Freezing', sentence: null, passage: null })] }));
    const notes = await openNotes();
    expect(within(notes).queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('does not offer it on a note while the lesson is still being prepared', async () => {
    show(review([], { state: 'needs_review', counts, sections: SECTIONS, teachingFindings: [note()] }));
    const notes = await openNotes();
    expect(notes).toBeVisible();
    expect(within(notes).queryByRole('button', EDIT)).not.toBeInTheDocument();
  });
});
