import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { claimItem, review } from '@/features/learning/lessons/review/claimReviewTestData';
import type { LearningLessonDto, LearningLessonEvidenceDto, LearningProgramDto } from '@/lib/bindings';
import type { EditableProps } from '@/shared/edits/Editable';
import { useEditMode } from '@/shared/edits/editMode';
import { closeEditor, useEditorStore } from '@/shared/edits/editorStore';

const capture = vi.hoisted(() => ({ fn: null as null | ((props: EditableProps) => void) }));
vi.mock('@/shared/edits/Editable', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/shared/edits/Editable')>();
  return {
    ...actual,
    Editable: (props: EditableProps) => {
      capture.fn?.(props);
      return <actual.Editable {...props} />;
    },
  };
});

// Every section of a ready lesson is wrapped for edit mode; a held recap is not wrapped.
const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn(), evidence: vi.fn(), index: vi.fn(), recall: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan,
  getLearningLessonClaimReview: api.review,
  getLearningLessonEvidence: api.evidence,
  courseIndexStatus: api.index,
  getLearningRecallV2: api.recall,
} }));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));
vi.mock('@/features/reading/components/viewers/MarkdownViewer', () => ({ MarkdownViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const blocks = [
  { kind: 'explanation', title: 'Boiling', body: 'Water boils when its vapour pressure meets the air pressure.', sourceIds: [] },
  { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
] as LearningLessonDto['blocks'];
const ready: LearningLessonDto = { id: 'lesson-1', title: 'Phase changes', objective: 'Explain boiling.', estimatedMinutes: 20, preparation: 'ready', blocks, completed: false };
const draft: LearningLessonDto = { ...ready, preparation: 'outline' };
const checkQuestion = 'Why does water boil faster on a mountain?';
const checkReveal = 'Air pressure is lower, so vapour pressure matches it sooner.';
const checkBody = `${checkQuestion}\n\n### Check your thinking\n\n${checkReveal}`;
const checkLesson: LearningLessonDto = {
  ...ready,
  blocks: [{
    kind: 'check',
    title: 'Boiling point',
    body: checkBody,
    check: { question: checkQuestion, reveal: checkReveal },
    sourceIds: [],
  }] as LearningLessonDto['blocks'],
};
const program = { summary: { id: 'program-1', revision: 44, status: 'active' }, sources: [], modules: [] } as unknown as LearningProgramDto;

function show(lesson: LearningLessonDto, current: boolean) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}><MemoryRouter>
    <LessonCard lesson={lesson} moduleId="module-1" program={program} isCurrent={current} selected={false} pending={false}
      onPrepare={vi.fn()} onComplete={vi.fn()} onSelect={vi.fn()} onPractice={vi.fn()} onReview={vi.fn()} />
  </MemoryRouter></QueryClientProvider>);
}

describe('Lesson card edit mode', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useEditMode.getState().set(false);
    closeEditor();
    capture.fn = null;
    api.index.mockResolvedValue({ ok: true, data: { state: 'ready', mode: 'hybrid' } });
    api.plan.mockResolvedValue({ ok: true, data: { jobs: [] } });
    api.review.mockResolvedValue({ ok: true, data: null });
    api.evidence.mockResolvedValue({ ok: true, data: null });
    api.recall.mockResolvedValue({ ok: true, data: { cards: [] } });
  });

  afterEach(() => {
    useEditMode.getState().set(false);
    closeEditor();
    capture.fn = null;
  });

  it('shows no Edit button on sections or lesson outside edit mode', () => {
    show(ready, true);
    expect(screen.queryByRole('button', { name: /^Edit section 1:/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Edit lesson:/ })).not.toBeInTheDocument();
  });

  it('in edit mode puts Edit buttons on the lesson and every shown section, and none on a held recap', async () => {
    useEditMode.getState().set(true);
    const recallFirst: LearningLessonDto = {
      ...ready,
      blocks: [
        { kind: 'explanation', title: 'Boiling', body: 'Water boils when its vapour pressure meets the air pressure.', sourceIds: [] },
        { kind: 'reflection', title: 'From memory', body: 'Explain boiling in your own words.', sourceIds: [] },
        { kind: 'recap', title: 'Key takeaways', body: '- Pressure sets the boiling point.', sourceIds: [] },
      ] as LearningLessonDto['blocks'],
    };
    show(recallFirst, true);
    expect(screen.getByRole('button', { name: 'Edit lesson: Phase changes' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Edit section 1: Boiling' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Edit section 2: From memory' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit section 3: Key takeaways' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Show them now' }));
    expect(await screen.findByRole('button', { name: 'Edit section 3: Key takeaways' })).toBeVisible();
  });

  it('in edit mode on a check section offers Edit question and, once revealed, Edit answer', async () => {
    useEditMode.getState().set(true);
    show(checkLesson, true);
    expect(screen.getByRole('button', { name: 'Edit question of section 1: Boiling point' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Edit answer of section 1: Boiling point' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /show me/i }));
    expect(await screen.findByRole('button', { name: 'Edit answer of section 1: Boiling point' })).toBeVisible();
  });

  it('passes exact source and sourceStart offsets for a check question and its revealed answer', async () => {
    const recorded: EditableProps[] = [];
    capture.fn = (props) => recorded.push(props);
    useEditMode.getState().set(true);
    show(checkLesson, true);
    await userEvent.click(screen.getByRole('button', { name: /show me/i }));

    const questionWrap = recorded.find((entry) => entry.label === 'question of section 1: Boiling point');
    const answerWrap = recorded.find((entry) => entry.label === 'answer of section 1: Boiling point');
    expect(questionWrap).toBeDefined();
    expect(questionWrap?.source).toBe(checkQuestion);
    expect(questionWrap?.sourceStart).toBe(0);
    expect(questionWrap?.path).toBe('/body');

    expect(answerWrap).toBeDefined();
    expect(answerWrap?.path).toBe('/body');
    expect(answerWrap?.source).toBe(checkReveal);
    expect(typeof answerWrap?.sourceStart).toBe('number');
    expect(answerWrap!.sourceStart! + answerWrap!.source!.length).toBe(checkBody.length);
    expect(checkBody.startsWith(answerWrap!.source!, answerWrap!.sourceStart!)).toBe(true);
  });

  it('shows no section Edit button on an outline lesson that is not prepared', () => {
    useEditMode.getState().set(true);
    show({ ...draft, blocks: [] }, false);
    expect(screen.queryByRole('button', { name: /^Edit section/ })).not.toBeInTheDocument();
  });

  it('offers Edit the section on an unverified statement with an address while keeping the claim Edit button', async () => {
    const address = { content: { kind: 'learning.section', key: 'lesson-1/0' }, path: '/body', fragment: null };
    const report: LearningLessonEvidenceDto = {
      policy: 'lesson-evidence-v21', checkedAt: 1, checkerModel: 'checker', retrievalMode: 'hybrid',
      embeddingModel: null, contentSha256: 'hash', claimCount: 1, confirmedCount: 0, executedExamples: 0,
      unexecutedLanguages: [], sourcesCurrent: true, citations: [], sections: [],
      teachingClaims: [{
        sectionIndex: 0, contentQuote: 'Unverified sentence.', claim: 'Unverified sentence.', claimId: 'claim-1',
        verdict: 'unsupported', reason: 'No source.', supportingQuote: null, passages: [], resolution: 'not_verified',
        note: null, sufficiency: null, class: 'sourced', label: null, parts: [], unverifiedReason: 'unsupported', cites: [],
      }],
    };
    const claim = claimItem({
      claimId: 'claim-1',
      location: { unitIndex: 0, unitKind: 'block', unitTitle: 'Boiling', field: 'body', quote: 'Unverified sentence.', span: null, address },
      availableActions: ['confirm', 'edit', 'remove', 'attach_evidence'],
    });
    api.evidence.mockResolvedValue({ ok: true, data: report });
    api.review.mockResolvedValue({ ok: true, data: review([claim], { state: 'ready', revision: 7 }) });

    show(ready, true);
    const editSectionButton = await screen.findByRole('button', { name: 'Edit the section' });
    await userEvent.click(editSectionButton);
    expect(useEditorStore.getState().current?.address).toEqual(address);
    expect(useEditorStore.getState().current?.options.returnFocusTo).toBe(editSectionButton);

    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(screen.getByLabelText('Your version')).toBeVisible();
  });
});
