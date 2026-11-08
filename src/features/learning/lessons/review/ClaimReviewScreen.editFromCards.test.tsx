import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ClaimReviewScreen } from '@/features/learning/lessons/review/ClaimReviewScreen';
import { claimItem, review, teachingFinding } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, LessonClaimReviewDto, TeachingFindingDto } from '@/lib/bindings';

// Teaching notes and taken-out sentences of a published lesson open the editor for their section.
const api = vi.hoisted(() => ({ review: vi.fn(), audit: vi.fn(), plan: vi.fn(), trust: vi.fn(), program: vi.fn(), edit: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningLessonClaimReview: api.review, getLearningLessonAudit: api.audit, getLearningPlan: api.plan, getLearningSourceTrust: api.trust,
  getLearningProgram: api.program, editLearningLessonSection: api.edit,
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
const program = {
  summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [],
  modules: [{ id: 'module-1', lessons: [{ id: 'lesson-1', blocks: [
    { kind: 'explanation', title: 'Boiling', body: BODIES[0], sourceIds: [] },
    { kind: 'explanation', title: 'Freezing', body: BODIES[1], sourceIds: [] },
  ] }] }],
};

const note = (overrides: Partial<TeachingFindingDto> = {}) => teachingFinding({
  findingKey: 'teaching:section-1', unitIndex: 1, unitTitle: 'Section 2: Freezing', sentence: FREEZING, passage: null, ...overrides,
});

const cut = (claimId: string, sentence: string, unitIndex: number, putBackUnit: number | null, contextBefore: string): ClaimReviewItemDto => claimItem({
  claimId, state: { state: 'removed', at: 5, by: 'machine' }, availableActions: ['put_back'], verdictKind: null, reason: null,
  place: putBackUnit === null ? 'choose' : 'known', putBackUnit,
  location: { unitIndex, unitKind: 'block', unitTitle: '', field: 'body', quote: sentence, span: null },
  presentation: { statement: sentence, sentence, contextBefore, contextAfter: '', place: '' },
});

const statement = claimItem({
  claimId: 'claim-1', statement: FREEZING, state: { state: 'unverified', at: 2, reason: 'unsupported' }, verdictKind: null, reason: null,
  availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
  location: { unitIndex: 1, unitKind: 'block', unitTitle: 'Freezing', field: 'body', quote: FREEZING, span: null },
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
    api.audit.mockResolvedValue(ok(null));
    api.plan.mockResolvedValue(ok({ jobs: [] }));
    api.trust.mockResolvedValue(ok({ sources: [], claim: null }));
    api.program.mockResolvedValue(ok(program));
  });

  it('opens the editor on the section of a teaching note, at the note sentence', async () => {
    show(review([], { state: 'ready', counts, teachingFindings: [note()] }));
    const notes = await openNotes();
    await userEvent.click(await within(notes).findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit section 2' })).toBeVisible();
    expect(screen.getByLabelText('Title')).toHaveValue('Freezing');
    const text = screen.getByLabelText('Text') as HTMLTextAreaElement;
    expect(text).toHaveValue(BODIES[1]);
    const start = BODIES[1].indexOf(FREEZING);
    await waitFor(() => {
      expect(text.selectionStart).toBe(start);
      expect(text.selectionEnd).toBe(start + FREEZING.length);
    });
  });

  it('opens the editor with nothing selected when the section no longer holds the note text', async () => {
    show(review([], { state: 'ready', counts, teachingFindings: [note({ sentence: 'A sentence that is gone.' })] }));
    const notes = await openNotes();
    await userEvent.click(await within(notes).findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit section 2' })).toBeVisible();
    const text = screen.getByLabelText('Text') as HTMLTextAreaElement;
    expect(text).toHaveValue(BODIES[1]);
    expect(text.selectionStart).toBe(text.selectionEnd);
  });

  it('offers it on a note about a section only, not on a note about a question or the whole lesson', async () => {
    show(review([], { state: 'ready', counts, teachingFindings: [
      note(),
      note({ findingKey: 'teaching:question-0', site: 'question', unitIndex: 0, unitTitle: 'Question 1', sentence: null }),
      note({ findingKey: 'teaching:lesson', site: 'lesson', unitIndex: null, unitTitle: 'Whole lesson', sentence: null }),
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

  it('opens the editor on the section a sentence was taken out of, at the sentence before it', async () => {
    show(review([cut('a', BOILING, 0, 0, BEFORE_BOILING)], { state: 'ready', counts, sections: [{ index: 0, title: 'Boiling' }] }));
    const list = await openTakenOut();
    await userEvent.click(await within(list).findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit section 1' })).toBeVisible();
    expect(screen.getByLabelText('Title')).toHaveValue('Boiling');
    const text = screen.getByLabelText('Text') as HTMLTextAreaElement;
    expect(text).toHaveValue(BODIES[0]);
    await waitFor(() => {
      expect(text.selectionStart).toBe(0);
      expect(text.selectionEnd).toBe(BEFORE_BOILING.length);
    });
  });

  it('offers it only on a taken-out sentence whose section the course has', async () => {
    show(review([cut('a', BOILING, 0, 0, ''), cut('b', 'A sentence of a section that is gone.', 7, 7, '')], { state: 'ready', counts }));
    const list = await openTakenOut();
    expect(await within(list).findAllByRole('button', EDIT)).toHaveLength(1);
    expect(within(within(list).getByText(BOILING).closest('li') as HTMLElement).getByRole('button', EDIT)).toBeVisible();
  });

  it('opens the section the sentence goes back into, not the one its old place names', async () => {
    show(review([cut('a', FREEZING, 1, 0, BEFORE_BOILING)], { state: 'ready', counts }));
    const list = await openTakenOut();
    await userEvent.click(await within(list).findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit section 1' })).toBeVisible();
    expect(screen.getByLabelText('Title')).toHaveValue('Boiling');
    expect(screen.getByLabelText('Text')).toHaveValue(BODIES[0]);
  });

  it('does not offer it on a taken-out sentence with no known place', async () => {
    show(review([cut('a', BOILING, 0, null, '')], { state: 'ready', counts }));
    const list = await openTakenOut();
    expect(within(list).getByText(BOILING)).toBeVisible();
    expect(within(list).queryByRole('button', EDIT)).not.toBeInTheDocument();
  });

  it('still opens the section from the statement card', async () => {
    show(review([statement], { state: 'ready', counts }));
    await userEvent.click(await screen.findByRole('button', EDIT));
    expect(await screen.findByRole('dialog', { name: 'Edit section 2' })).toBeVisible();
    expect(screen.getByLabelText('Title')).toHaveValue('Freezing');
    expect(screen.getByLabelText('Text')).toHaveValue(BODIES[1]);
  });
});
