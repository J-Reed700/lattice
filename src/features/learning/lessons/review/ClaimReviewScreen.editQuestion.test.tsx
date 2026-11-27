import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review, teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ContentAddress, LessonClaimReviewDto } from '@/lib/bindings';
import { closeEditor, useEditorStore } from '@/shared/edits/editorStore';

// A statement or a teaching note on a question of a published lesson opens the shared editor at that question's address.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(), program: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  getLearningProgram: api.program,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = { verified: 0, confirmed: 0, queued: 0, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 };

const SENTENCE = 'Header names keep their spaces exactly as written.';
const SECTIONS = [{ index: 0, title: 'Boiling' }, { index: 1, title: 'Freezing' }];
const QUESTION_ADDRESS: ContentAddress = {
  content: { kind: 'learning.question', key: 'question-1' },
  path: '/prompt',
  fragment: null,
};
const ANSWER_KEY_ADDRESS: ContentAddress = {
  content: { kind: 'learning.question', key: 'question-2' },
  path: '/explanation',
  fragment: null,
};
const SECTION_ADDRESS: ContentAddress = {
  content: { kind: 'learning.section', key: 'lesson-1/0' },
  path: '/body',
  fragment: null,
};
const program = {
  summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [],
  modules: [{ id: 'module-1', lessons: [{ id: 'lesson-1', blocks: [
    { kind: 'explanation', title: 'Boiling', body: 'Boiling is a phase change.', sourceIds: [] },
    { kind: 'explanation', title: 'Freezing', body: 'Ice forms below freezing.', sourceIds: [] },
  ] }] }],
};

const unverified = { state: 'unverified', at: 2, reason: 'unsupported' } as const;
const onUnit = (claimId: string, unitIndex: number, unitKind: 'block' | 'question' | 'answer_key', field: string, address: ContentAddress) => claimItem({
  claimId, statement: SENTENCE, state: unverified, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
  location: { unitIndex, unitKind, unitTitle: '', field, quote: SENTENCE, span: null, address },
  presentation: { statement: SENTENCE, sentence: SENTENCE, contextBefore: '', contextAfter: '', place: 'Question' },
});

function show(data: LessonClaimReviewDto) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Phase changes" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}
const published = (claims = [onUnit('claim-1', 2, 'question', 'prompt', QUESTION_ADDRESS)], rest: Partial<LessonClaimReviewDto> = {}) =>
  review(claims, { state: 'ready', counts, sections: SECTIONS, ...rest });

const EDIT = { name: 'Edit the question' };
const openNotes = async () => {
  await userEvent.click(await screen.findByRole('button', { name: /^Teaching notes/ }));
  return screen.findByRole('region', { name: 'Teaching review' });
};
const note = (overrides = {}) => teachingFinding({
  findingKey: 'answer_key:question-1', kind: 'answer_key', site: 'question', unitIndex: 1, unitTitle: 'Question 2', sentence: null, passage: null,
  address: ANSWER_KEY_ADDRESS, ...overrides,
});

describe('Claim review screen: editing the question of a statement or a note', () => {
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

  it('offers Edit the question on a statement about a question and opens the shared editor without a local dialog', async () => {
    show(published());
    const button = await screen.findByRole('button', EDIT);
    await userEvent.click(button);
    expect(useEditorStore.getState().current?.address).toEqual(QUESTION_ADDRESS);
    expect(useEditorStore.getState().current?.options.returnFocusTo).toBe(button);
    expect(screen.queryByRole('dialog', { name: /Edit question/ })).not.toBeInTheDocument();
  });

  it('offers it on a statement about an answer key, for the question address it belongs to', async () => {
    show(published([onUnit('claim-2', 3, 'answer_key', 'explanation', ANSWER_KEY_ADDRESS)]));
    await userEvent.click(await screen.findByRole('button', EDIT));
    expect(useEditorStore.getState().current?.address).toEqual(ANSWER_KEY_ADDRESS);
    expect(screen.queryByRole('dialog', { name: /Edit question/ })).not.toBeInTheDocument();
  });

  it('does not offer Edit the section on a question statement', async () => {
    show(published());
    expect(await screen.findByRole('button', EDIT)).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit the section' })).not.toBeInTheDocument();
  });

  it('keeps Edit the section on a section statement and offers no question editor there', async () => {
    show(published([onUnit('claim-3', 0, 'block', 'body', SECTION_ADDRESS)]));
    expect(await screen.findByRole('button', { name: 'Edit the section' })).toBeVisible();
    expect(screen.queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('does not offer it before the lesson is published', async () => {
    show(review([onUnit('claim-1', 2, 'question', 'prompt', QUESTION_ADDRESS)], { state: 'needs_review', counts, sections: SECTIONS }));
    expect(await screen.findByLabelText('Statement in focus')).toBeVisible();
    expect(screen.queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('offers it on a teaching note about a question and opens finding.address in the shared editor', async () => {
    show(published([], { teachingFindings: [note()] }));
    const notes = await openNotes();
    const button = await within(notes).findByRole('button', EDIT);
    await userEvent.click(button);
    expect(useEditorStore.getState().current?.address).toEqual(ANSWER_KEY_ADDRESS);
    expect(useEditorStore.getState().current?.options.returnFocusTo).toBe(button);
    expect(screen.queryByRole('dialog', { name: /Edit question/ })).not.toBeInTheDocument();
  });

  it('does not offer it on a teaching note about a section', async () => {
    show(published([], { teachingFindings: [teachingFinding({ site: 'section', unitIndex: 1, unitTitle: 'Section 2: Freezing', sentence: null, passage: null, address: SECTION_ADDRESS })] }));
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
