import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ContentAddress, LessonClaimReviewDto } from '@/lib/bindings';
import { closeEditor, useEditorStore } from '@/shared/edits/editorStore';

// A statement of a published lesson opens the shared editor at its location address.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(), program: vi.fn(), resolve: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  getLearningProgram: api.program, resolveLearningClaim: api.resolve,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = { verified: 0, confirmed: 0, queued: 0, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 };

const SENTENCE = 'Water boils at 100 degrees at sea level.';
const BODY = `Boiling is a phase change. ${SENTENCE} Pressure matters.`;
const SECTION_ADDRESS: ContentAddress = {
  content: { kind: 'learning.section', key: 'lesson-1/0' },
  path: '/body',
  fragment: { exact: SENTENCE, prefix: null, suffix: null, startByte: 27, endByte: 27 + SENTENCE.length },
};
const QUESTION_ADDRESS: ContentAddress = {
  content: { kind: 'learning.question', key: 'question-1' },
  path: '/explanation',
  fragment: null,
};
const program = {
  summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [],
  modules: [{ id: 'module-1', lessons: [{ id: 'lesson-1', blocks: [{ kind: 'explanation', title: 'Boiling', body: BODY, sourceIds: [] }] }] }],
};

const statement = claimItem({
  claimId: 'claim-1', statement: SENTENCE, state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
  location: { unitIndex: 0, unitKind: 'block', unitTitle: 'Boiling', field: 'body', quote: SENTENCE, span: null, address: SECTION_ADDRESS },
  presentation: { statement: SENTENCE, sentence: SENTENCE, contextBefore: 'Boiling is a phase change.', contextAfter: 'Pressure matters.', place: 'Section 1 · Boiling' },
});
const inAnswerKey = claimItem({
  ...statement, claimId: 'claim-2',
  location: { unitIndex: 0, unitKind: 'question', unitTitle: 'Question 1', field: 'explanation', quote: SENTENCE, span: null, address: QUESTION_ADDRESS },
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
    closeEditor();
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
    api.program.mockResolvedValue(ok(program));
  });

  afterEach(() => {
    closeEditor();
  });

  it('offers Edit the section on a statement in a published lesson and opens the shared editor without rendering a dialog', async () => {
    show(review([statement], { state: 'ready', counts }));
    const button = await screen.findByRole('button', { name: 'Edit the section' });
    await userEvent.click(button);
    expect(useEditorStore.getState().current?.address).toEqual(SECTION_ADDRESS);
    expect(useEditorStore.getState().current?.options.returnFocusTo).toBe(button);
    expect(screen.queryByRole('dialog', { name: /Edit section/ })).not.toBeInTheDocument();
  });

  it('does not offer it before the lesson is published', async () => {
    show(review([statement], { state: 'needs_review', counts }));
    expect(await screen.findByLabelText('Statement in focus')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit the section' })).not.toBeInTheDocument();
  });

  it('does not offer it on a statement outside a section’s text', async () => {
    show(review([inAnswerKey], { state: 'ready', counts }));
    expect(await screen.findByLabelText('Statement in focus')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit the section' })).not.toBeInTheDocument();
  });
});
