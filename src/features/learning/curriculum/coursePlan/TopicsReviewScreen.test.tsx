import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { entry, ok, planView, topics } from '@/features/learning/curriculum/coursePlan/coursePlanFixtures';
import { TopicsReviewScreen } from '@/features/learning/curriculum/coursePlan/TopicsReviewScreen';
import type { CoursePlanEntryDto, LearningProgramDto } from '@/lib/bindings';

const api = vi.hoisted(() => ({ get: vi.fn(), edit: vi.fn(), change: vi.fn(), undo: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningCoursePlan: api.get,
  editLearningCoursePlanEntry: api.edit,
  changeLearningCoursePlanEntries: api.change,
  undoLearningCoursePlanEntry: api.undo,
} }));

const lesson = (id: string, title: string) => ({ id, title, objective: '', estimatedMinutes: 10, preparation: 'outline', blocks: [], completed: false });
const course = {
  summary: { id: 'program-1', title: 'Circulation', status: 'active', revision: 3 },
  modules: [
    { id: 'module-1', title: 'The pump', summary: '', outcomes: [], lessons: [lesson('lesson-1', 'The heart'), lesson('lesson-2', 'Valves')] },
    { id: 'module-2', title: 'The pipes', summary: '', outcomes: [], lessons: [lesson('lesson-3', 'Vessels')] },
  ],
  sources: [],
} as unknown as LearningProgramDto;

const entries = (): CoursePlanEntryDto[] => [
  entry('lesson-1', { topics: topics('lesson-1', { ideas: ['Beat cycle', 'Chambers'], terms: ['Systole'], actions: ['Trace a beat'], contrast: 'Pulse is not pressure' }), entryRevision: 3 }),
  entry('lesson-2', { entryRevision: 5 }),
  entry('lesson-3', { entryRevision: 2 }),
];

function show() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const onBack = vi.fn();
  render(<QueryClientProvider client={client}><TopicsReviewScreen program={course} onBack={onBack} /></QueryClientProvider>);
  return { onBack };
}
const card = (name: string) => screen.getByRole('article', { name });

describe('Review topics', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.get.mockResolvedValue(ok(planView(entries())));
  });

  it('shows one card per lesson grouped by module in course order, with every list', async () => {
    show();
    expect(await screen.findByRole('heading', { name: 'Review topics' })).toBeVisible();
    const headings = screen.getAllByRole('heading', { level: 2 }).map((heading) => heading.textContent);
    expect(headings).toEqual(['The pump', 'The pipes']);
    expect(screen.getAllByRole('article').map((item) => item.getAttribute('aria-label'))).toEqual(['The heart', 'Valves', 'Vessels']);
    const first = within(card('The heart'));
    expect(first.getByText('Beat cycle')).toBeVisible();
    expect(first.getByText('Chambers')).toBeVisible();
    expect(first.getByText('Systole')).toBeVisible();
    expect(first.getByText('Trace a beat')).toBeVisible();
    expect(first.getByText('Pulse is not pressure')).toBeVisible();
  });

  it('saves an edit with the card\'s revision and then shows what the topics were', async () => {
    const user = userEvent.setup();
    const saved = entries();
    saved[0] = entry('lesson-1', {
      topics: topics('lesson-1', { ideas: ['Beat cycle', 'Chambers', 'Rhythm'], terms: ['Systole'], actions: ['Trace a beat'], contrast: 'Pulse is not pressure' }),
      entryRevision: 4, previous: entries()[0].topics,
    });
    api.edit.mockResolvedValue(ok(planView(saved)));
    show();
    await user.click(within(await screen.findByRole('article', { name: 'The heart' })).getByRole('button', { name: 'Edit' }));
    await user.click(screen.getByRole('button', { name: 'Add idea' }));
    await user.type(screen.getByRole('textbox', { name: 'Idea 3' }), 'Rhythm');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.edit).toHaveBeenCalledWith({
      programId: 'program-1', outlineRevision: 4, expectedEntryRevision: 3,
      topics: { lessonId: 'lesson-1', ideas: ['Beat cycle', 'Chambers', 'Rhythm'], terms: ['Systole'], actions: ['Trace a beat'], contrast: 'Pulse is not pressure' },
    }));
    expect(await within(card('The heart')).findByText('Was: Beat cycle · Chambers')).toBeVisible();
    expect(within(card('The heart')).queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
  });

  it('removes and rewords items, leaves out blank ones, and needs at least one idea', async () => {
    const user = userEvent.setup();
    api.edit.mockResolvedValue(ok(planView(entries())));
    show();
    await user.click(within(await screen.findByRole('article', { name: 'The heart' })).getByRole('button', { name: 'Edit' }));
    await user.click(screen.getByRole('button', { name: 'Add term' }));
    await user.clear(screen.getByRole('textbox', { name: 'Contrast' }));
    await user.type(screen.getByRole('textbox', { name: 'Contrast' }), 'Rate is not output');
    await user.click(screen.getByRole('button', { name: 'Remove idea 2' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.edit).toHaveBeenCalledWith(expect.objectContaining({
      topics: { lessonId: 'lesson-1', ideas: ['Beat cycle'], terms: ['Systole'], actions: ['Trace a beat'], contrast: 'Rate is not output' },
    })));
    await user.click(within(card('The heart')).getByRole('button', { name: 'Edit' }));
    await user.clear(screen.getByRole('textbox', { name: 'Idea 1' }));
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('asks for a change to one lesson with that card\'s revision', async () => {
    const user = userEvent.setup();
    api.change.mockResolvedValue(ok(planView(entries())));
    show();
    await user.click(within(await screen.findByRole('article', { name: 'Valves' })).getByRole('button', { name: 'Ask for a change' }));
    await user.type(within(card('Valves')).getByRole('textbox', { name: 'What should change?' }), 'Drop the jargon');
    await user.click(within(card('Valves')).getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(api.change).toHaveBeenCalledWith({
      programId: 'program-1', outlineRevision: 4, lessons: [{ lessonId: 'lesson-2', expectedEntryRevision: 5 }], instruction: 'Drop the jargon',
    }));
  });

  it('asks for one change to every selected lesson in a single call', async () => {
    const user = userEvent.setup();
    api.change.mockResolvedValue(ok(planView(entries())));
    show();
    await user.click(await screen.findByRole('checkbox', { name: 'Select The heart' }));
    await user.click(screen.getByRole('checkbox', { name: 'Select Vessels' }));
    const bar = screen.getByRole('toolbar', { name: '2 lessons selected' });
    await user.click(within(bar).getByRole('button', { name: 'Ask for a change' }));
    await user.type(screen.getByRole('textbox', { name: 'What should change?' }), 'Shorter');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(api.change).toHaveBeenCalledTimes(1));
    expect(api.change).toHaveBeenCalledWith({
      programId: 'program-1', outlineRevision: 4,
      lessons: [{ lessonId: 'lesson-1', expectedEntryRevision: 3 }, { lessonId: 'lesson-3', expectedEntryRevision: 2 }],
      instruction: 'Shorter',
    });
  });

  it('offers Undo only on a card that has a previous version, and undoes with its revision', async () => {
    const user = userEvent.setup();
    const list = entries();
    list[1] = entry('lesson-2', { entryRevision: 6, previous: topics('lesson-2', { ideas: ['Old valve idea'] }) });
    api.get.mockResolvedValue(ok(planView(list)));
    api.undo.mockResolvedValue(ok(planView(entries())));
    show();
    await screen.findByRole('article', { name: 'Valves' });
    expect(within(card('The heart')).queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
    expect(within(card('Vessels')).queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
    expect(within(card('Valves')).getByText('Was: Old valve idea')).toBeVisible();
    await user.click(within(card('Valves')).getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(api.undo).toHaveBeenCalledWith({ programId: 'program-1', outlineRevision: 4, lessonId: 'lesson-2', expectedEntryRevision: 6 }));
  });

  it('locks a lesson whose writer has started: no actions and no way to select it', async () => {
    const list = entries();
    list[0] = entry('lesson-1', { editable: false, previous: topics('lesson-1', { ideas: ['Older'] }) });
    api.get.mockResolvedValue(ok(planView(list)));
    show();
    await screen.findByRole('article', { name: 'The heart' });
    const locked = within(card('The heart'));
    expect(locked.getByText('Being written; its topics are fixed')).toBeVisible();
    for (const name of ['Edit', 'Ask for a change', 'Undo']) expect(locked.queryByRole('button', { name })).not.toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: 'Select The heart' })).not.toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Select Valves' })).toBeInTheDocument();
  });

  it('shows a refused change plainly and reloads the plan', async () => {
    const user = userEvent.setup();
    api.edit.mockResolvedValue({ ok: false, error: 'These topics changed since you opened them.' });
    show();
    await user.click(within(await screen.findByRole('article', { name: 'The heart' })).getByRole('button', { name: 'Edit' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toBeVisible();
    await waitFor(() => expect(api.get).toHaveBeenCalledTimes(2));
  });

  it('goes back to the course', async () => {
    const user = userEvent.setup();
    const { onBack } = show();
    await user.click(await screen.findByRole('button', { name: 'Back' }));
    expect(onBack).toHaveBeenCalled();
  });
});
