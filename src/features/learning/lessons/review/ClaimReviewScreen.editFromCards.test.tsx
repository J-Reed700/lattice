import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review, teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, ContentAddress, LessonClaimReviewDto, TeachingFindingDto } from '@/lib/bindings';
import { closeEditor, useEditorStore } from '@/shared/edits/editorStore';

// Teaching notes and taken-out sentences of a published lesson open the shared editor for their section.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(), program: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  getLearningProgram: api.program,
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
} }));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const ok = <T,>(data: T) => ({ ok: true, data });
const counts = { verified: 0, confirmed: 0, queued: 0, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 };

const BOILING = 'Water boils at 100 degrees at sea level.';
const FREEZING = 'Water freezes at 0 degrees at sea level.';
const BEFORE_BOILING = 'Boiling is a phase change.';
const BODIES = [
  `${BEFORE_BOILING} Pressure matters.`,
  `Ice forms below freezing. ${FREEZING} Salt lowers it.`,
];
const NOTE_ADDRESS: ContentAddress = {
  content: { kind: 'learning.section', key: 'lesson-1/1' },
  path: '/body',
  fragment: { exact: FREEZING, prefix: null, suffix: null, startByte: 26, endByte: 26 + FREEZING.length },
};
const QUESTION_ADDRESS: ContentAddress = {
  content: { kind: 'learning.question', key: 'question-0' },
  path: '/prompt',
  fragment: null,
};
const program = {
  summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [],
  modules: [{ id: 'module-1', lessons: [{ id: 'lesson-1', blocks: [
    { kind: 'explanation', title: 'Boiling', body: BODIES[0], sourceIds: [] },
    { kind: 'explanation', title: 'Freezing', body: BODIES[1], sourceIds: [] },
  ] }] }],
};

const note = (overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: 'teaching:section-1', unitIndex: 1, unitTitle: 'Section 2: Freezing', sentence: FREEZING, passage: null, address: NOTE_ADDRESS, ...overrides,
});

const cut = (claimId: string, sentence: string, unitIndex: number, putBackUnit: number | null, putBackBeside: string | null): ClaimReviewItemDto => claimItem({
  claimId, state: { state: 'removed', at: 5, by: 'machine' }, availableActions: ['put_back'], verdictKind: null, reason: null,
  place: putBackUnit === null ? 'choose' : 'known', putBackUnit, putBackBeside,
  location: { unitIndex, unitKind: 'block', unitTitle: '', field: 'body', quote: sentence, span: null, address: null },
  presentation: { statement: sentence, sentence, contextBefore: '', contextAfter: '', place: '' },
});

const statement = claimItem({
  claimId: 'claim-1', statement: FREEZING, state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
  location: { unitIndex: 1, unitKind: 'block', unitTitle: 'Freezing', field: 'body', quote: FREEZING, span: null, address: NOTE_ADDRESS },
  presentation: { statement: FREEZING, sentence: FREEZING, contextBefore: 'Ice forms below freezing.', contextAfter: 'Salt lowers it.', place: 'Section 2 · Freezing' },
});

function show(data: LessonClaimReviewDto) {
  api.review.mockResolvedValue(ok(data));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ClaimReviewScreen programId="program-1" lessonId="lesson-1" lessonTitle="Phase changes" courseRevision={44} onBack={vi.fn()} />
  </QueryClientProvider>);
}

const openNotes = async () => {
  await userEvent.click(await screen.findByRole('button', { name: /^Teaching notes/ }));
  return screen.findByRole('region', { name: 'Teaching review' });
};
const openTakenOut = async () => {
  await userEvent.click(await screen.findByRole('button', { name: /Taken out of the lesson/ }));
  return screen.findByRole('region', { name: 'Statements taken out of the lesson' });
};
const EDIT = { name: 'Edit the section' };

describe('Claim review screen: editing the section from a note or a taken-out sentence', () => {
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

  it('opens the shared editor on finding.address for a teaching note without rendering a dialog', async () => {
    show(review([], { state: 'ready', counts, teachingFindings: [note()] }));
    const notes = await openNotes();
    const button = await within(notes).findByRole('button', EDIT);
    await userEvent.click(button);
    expect(useEditorStore.getState().current?.address).toEqual(NOTE_ADDRESS);
    expect(useEditorStore.getState().current?.options.returnFocusTo).toBe(button);
    expect(screen.queryByRole('dialog', { name: /Edit section/ })).not.toBeInTheDocument();
  });

  it('offers it on a note about a section only, not on a note about a question or the whole lesson', async () => {
    show(review([], { state: 'ready', counts, teachingFindings: [
      note(),
      note({ findingKey: 'teaching:question-0', site: 'question', unitIndex: 0, unitTitle: 'Question 1', sentence: null, address: QUESTION_ADDRESS }),
      note({ findingKey: 'teaching:lesson', site: 'lesson', unitIndex: null, unitTitle: 'Whole lesson', sentence: null, address: null }),
    ] }));
    const notes = await openNotes();
    expect(await within(notes).findAllByRole('button', EDIT)).toHaveLength(1);
    expect(within(screen.getByRole('article', { name: /^Section 2: Freezing/ })).getByRole('button', EDIT)).toBeVisible();
    expect(within(screen.getByRole('article', { name: /^Question 1/ })).queryByRole('button', EDIT)).not.toBeInTheDocument();
    expect(within(screen.getByRole('article', { name: /^Whole lesson/ })).queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('does not offer it on a note while the lesson is still being prepared', async () => {
    show(review([], { state: 'needs_review', counts, teachingFindings: [note()] }));
    const notes = await openNotes();
    expect(within(notes).getByRole('article', { name: /^Section 2: Freezing/ })).toBeVisible();
    expect(within(notes).queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('opens the section of putBackUnit in the shared editor for a taken-out sentence', async () => {
    show(review([cut('a', FREEZING, 1, 0, BEFORE_BOILING)], { state: 'ready', counts, sections: [{ index: 0, title: 'Boiling' }] }));
    const list = await openTakenOut();
    const button = await within(list).findByRole('button', EDIT);
    await userEvent.click(button);
    expect(useEditorStore.getState().current?.address).toEqual({
      content: { kind: 'learning.section', key: 'lesson-1/0' },
      path: '/body',
      fragment: null,
    });
    expect(useEditorStore.getState().current?.options.returnFocusTo).toBe(button);
    expect(screen.queryByRole('dialog', { name: /Edit section/ })).not.toBeInTheDocument();
  });

  it('does not offer it on a taken-out sentence with no known place', async () => {
    show(review([cut('a', BOILING, 0, null, null)], { state: 'ready', counts }));
    const list = await openTakenOut();
    expect(within(list).getByText(BOILING)).toBeVisible();
    expect(within(list).queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('still opens the section from the statement card', async () => {
    show(review([statement], { state: 'ready', counts }));
    await userEvent.click(await screen.findByRole('button', EDIT));
    expect(useEditorStore.getState().current?.address).toEqual(NOTE_ADDRESS);
  });
});
