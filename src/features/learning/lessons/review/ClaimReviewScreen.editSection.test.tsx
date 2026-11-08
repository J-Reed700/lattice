import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LessonClaimReviewDto } from '@/lib/bindings';

// A statement of a published lesson opens the editor for its section, at its sentence.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(), program: vi.fn(), edit: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  getLearningProgram: api.program, editLearningLessonSection: api.edit,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = { verified: 0, confirmed: 0, queued: 0, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 };

const SENTENCE = 'Water boils at 100 degrees at sea level.';
const BODY = `Boiling is a phase change. ${SENTENCE} Pressure matters.`;
const program = {
  summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [],
  modules: [{ id: 'module-1', lessons: [{ id: 'lesson-1', blocks: [{ kind: 'explanation', title: 'Boiling', body: BODY, sourceIds: [] }] }] }],
};

const statement = claimItem({
  claimId: 'claim-1', statement: SENTENCE, state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
  location: { unitIndex: 0, unitKind: 'block', unitTitle: 'Boiling', field: 'body', quote: SENTENCE, span: null },
  presentation: { statement: SENTENCE, sentence: SENTENCE, contextBefore: 'Boiling is a phase change.', contextAfter: 'Pressure matters.', place: 'Section 1 · Boiling' },
});
const inAnswerKey = claimItem({
  ...statement, claimId: 'claim-2',
  location: { unitIndex: 0, unitKind: 'question', unitTitle: 'Question 1', field: 'explanation', quote: SENTENCE, span: null },
});

function show(data: LessonClaimReviewDto) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Phase changes" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

describe('Claim review screen: editing the section of a statement', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
    api.program.mockResolvedValue(ok(program));
    api.edit.mockResolvedValue(ok({ changed: true, statementsKept: 0, statementsChanged: 1, statementsRemoved: 0, sentencesWritten: 1, checkedStatementsAffected: 0, revision: null }));
  });

  it('offers Edit the section on a statement in a published lesson and opens the editor at its sentence', async () => {
    show(review([statement], { state: 'ready', counts }));
    await userEvent.click(await screen.findByRole('button', { name: 'Edit the section' }));
    expect(await screen.findByRole('dialog', { name: 'Edit section 1' })).toBeVisible();
    expect(screen.getByLabelText('Title')).toHaveValue('Boiling');
    const text = screen.getByLabelText('Text') as HTMLTextAreaElement;
    expect(text).toHaveValue(BODY);
    const start = BODY.indexOf(SENTENCE);
    await waitFor(() => {
      expect(text.selectionStart).toBe(start);
      expect(text.selectionEnd).toBe(start + SENTENCE.length);
    });
  });

  it('announces that the section was saved', async () => {
    show(review([statement], { state: 'ready', counts }));
    await userEvent.click(await screen.findByRole('button', { name: 'Edit the section' }));
    await userEvent.type(await screen.findByLabelText('Title'), ' again');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(screen.getAllByRole('status').some((node) => node.textContent === 'Section saved.')).toBe(true);
  });

  it('does not offer it before the lesson is published', async () => {
    show(review([statement], { state: 'needs_review', counts }));
    expect(await screen.findByLabelText('Claim in focus')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit the section' })).not.toBeInTheDocument();
  });

  it('does not offer it on a statement outside a section’s text', async () => {
    show(review([inAnswerKey], { state: 'ready', counts }));
    expect(await screen.findByLabelText('Claim in focus')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit the section' })).not.toBeInTheDocument();
  });
});
