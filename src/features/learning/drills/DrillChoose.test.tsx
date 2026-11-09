import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CourseDrillsDto, DrillLessonDto } from '@/lib/bindings';

import { DrillChoose } from './DrillChoose';
import { DEFAULT_OPTIONS, type DrillOptions } from './drillOptions';

const api = vi.hoisted(() => ({
  codeReviewLanguages: vi.fn(),
  courseDrills: vi.fn(),
  practiceLanguage: vi.fn(),
}));
vi.mock('@/features/learning/drills/api', () => ({ drillsApi: api }));

vi.mock('@tauri-apps/api/event', () => ({
  listen: () => Promise.resolve(() => undefined),
}));

function lesson(patch: Partial<DrillLessonDto> & Pick<DrillLessonDto, 'lessonId' | 'title'>): DrillLessonDto {
  return { standing: 'new', dueAt: null, fitsCode: true, fitsPassage: true, current: false, completed: false, ...patch };
}

function view(patch: Partial<CourseDrillsDto> = {}): CourseDrillsDto {
  return {
    language: 'python',
    nextCode: { lessonId: 'lesson-1', status: 'ready' },
    nextPassage: { lessonId: 'lesson-1', status: 'ready' },
    hasSources: true,
    modules: [
      {
        moduleId: 'module-1',
        title: 'Memory',
        lessons: [
          lesson({ lessonId: 'lesson-1', title: 'Retrieval' }),
          lesson({ lessonId: 'lesson-2', title: 'Spacing', fitsPassage: false }),
        ],
      },
    ],
    recent: [],
    ...patch,
  };
}

function show(options: Partial<DrillOptions> = {}) {
  const onChange = vi.fn();
  const onStart = vi.fn();
  render(
    <DrillChoose
      programId="course-1"
      options={{ ...DEFAULT_OPTIONS, ...options }}
      onChange={onChange}
      onStart={onStart}
      onOpenAttempt={vi.fn()}
    />,
  );
  return { onChange, onStart, user: userEvent.setup() };
}

beforeEach(() => {
  for (const mock of Object.values(api)) mock.mockReset();
  api.codeReviewLanguages.mockResolvedValue(['python', 'csharp', 'sql']);
  api.practiceLanguage.mockResolvedValue(null);
  api.courseDrills.mockImplementation((_programId: string, language: string | null) =>
    Promise.resolve(view({ language: language ?? 'python' })),
  );
});

describe('DrillChoose computed choices', () => {
  it('shows the explanation as chosen when the course has no code to drill, without changing the options', async () => {
    api.courseDrills.mockResolvedValue(view({ nextCode: null }));
    const { onChange, onStart, user } = show({ material: 'code' });

    const passage = await screen.findByRole('radio', { name: /An explanation of the lesson’s sources/ });
    await waitFor(() => expect(passage).toBeChecked());
    expect(onChange).not.toHaveBeenCalled();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Start drill' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Start drill' }));
    expect(onStart).toHaveBeenCalledWith(expect.objectContaining({ material: 'passage' }));
  });

  it('shows code as chosen when the course has no passage, without changing the options', async () => {
    api.courseDrills.mockResolvedValue(view({ nextPassage: null }));
    const { onChange } = show({ material: 'passage' });

    const code = await screen.findByRole('radio', { name: /^Code/ });
    await waitFor(() => expect(code).toBeChecked());
    expect(onChange).not.toHaveBeenCalled();
  });

  it('starts on the lesson next up when the lesson picked does not fit the material, without changing the options', async () => {
    const { onChange, onStart, user } = show({ material: 'passage', lessonId: 'lesson-2' });

    const select = await screen.findByRole('combobox', { name: 'Lesson' });
    await waitFor(() => expect(select).toBeEnabled());
    expect(select).toHaveValue('');
    expect(onChange).not.toHaveBeenCalled();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Start drill' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Start drill' }));
    expect(onStart).toHaveBeenCalledWith(expect.objectContaining({ lessonId: null }));
  });

  it('keeps a lesson that fits', async () => {
    const { onStart, user } = show({ material: 'code', lessonId: 'lesson-2' });

    const select = await screen.findByRole('combobox', { name: 'Lesson' });
    await waitFor(() => expect(select).toHaveValue('lesson-2'));
    await user.click(screen.getByRole('button', { name: 'Start drill' }));
    expect(onStart).toHaveBeenCalledWith(expect.objectContaining({ lessonId: 'lesson-2' }));
  });
});

describe('DrillChoose language default from the practice profile', () => {
  it('asks the profile and then lists the course’s drills in its language', async () => {
    api.practiceLanguage.mockResolvedValue('csharp');
    show();

    expect(await screen.findByRole('combobox', { name: 'Language' })).toBeInTheDocument();
    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledWith('course-1', 'csharp'));
    expect(api.practiceLanguage).toHaveBeenCalledWith('course-1');
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Language' })).toHaveValue('csharp'));
  });

  it('starts in the profile’s language when none was picked', async () => {
    api.practiceLanguage.mockResolvedValue('sql');
    const { onStart, user } = show();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Start drill' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Start drill' }));
    expect(onStart).toHaveBeenCalledWith(expect.objectContaining({ language: 'sql' }));
  });

  it('prefers the language the learner picked to the profile’s', async () => {
    api.practiceLanguage.mockResolvedValue('csharp');
    show({ language: 'sql' });

    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledWith('course-1', 'sql'));
    expect(api.courseDrills).not.toHaveBeenCalledWith('course-1', 'csharp');
  });

  it('falls back to the course’s own language when the profile names none', async () => {
    api.practiceLanguage.mockResolvedValue(null);
    show();

    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledWith('course-1', null));
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Language' })).toHaveValue('python'));
  });

  it('falls back when the profile cannot be read', async () => {
    api.practiceLanguage.mockRejectedValue(new Error('no profile'));
    show();

    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledWith('course-1', null));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('falls back when the profile names a language drills are not written in', async () => {
    api.practiceLanguage.mockResolvedValue('klingon');
    show();

    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledWith('course-1', null));
    expect(api.courseDrills).not.toHaveBeenCalledWith('course-1', 'klingon');
  });
});

describe('DrillChoose words', () => {
  it('calls the timed mode Timed', async () => {
    show();
    const modes = await screen.findAllByRole('radio', { name: /Practice|Timed/ });
    expect(modes.map((mode) => within(mode.closest('label') as HTMLElement).getByText(/^(Practice|Timed)$/).textContent)).toEqual([
      'Practice',
      'Timed',
    ]);
    expect(screen.queryByRole('radio', { name: /Interview/ })).not.toBeInTheDocument();
  });
});
