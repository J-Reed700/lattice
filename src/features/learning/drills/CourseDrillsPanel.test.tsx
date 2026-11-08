import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CodeReviewResultDto, CourseDrillAttemptDto, CourseDrillsDto, DrillLessonDto, DrillModuleDto, JobDto, StartedCodeReviewDto } from '@/lib/bindings';
import { deferred } from '@/tests/deferred';
import { jobFixture } from '@/tests/fixtures/jobs';

import { CourseDrillsPanel } from './CourseDrillsPanel';
import { momentOf } from './reveal/revealCopy';

const api = vi.hoisted(() => ({
  codeReviewLanguages: vi.fn(),
  courseDrills: vi.fn(),
  startCourseDrill: vi.fn(),
  submitCodeReview: vi.fn(),
  codeReviewResult: vi.fn(),
  regradeCodeReview: vi.fn(),
}));
vi.mock('@/features/learning/drills/api', () => ({ drillsApi: api }));

const events = vi.hoisted(() => ({ handlers: new Set<(event: { payload: unknown }) => void>() }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: (_name: string, handler: (event: { payload: unknown }) => void) => {
    events.handlers.add(handler);
    return Promise.resolve(() => {
      events.handlers.delete(handler);
    });
  },
}));

type Lines = { startLine: number; endLine: number } | null;
const lines = (range: Lines) => (range ? `${range.startLine}-${range.endLine}` : '');

// CodeMirror's gutter needs a layout engine; this stands in with buttons that pick lines.
vi.mock('@/features/explorer/components/CodeViewer', () => ({
  CodeViewer: (props: {
    path: string;
    text: string;
    selection: Lines;
    highlight: Lines;
    highlightNonce: number | null;
    onSelectLines: (range: Lines) => void;
  }) => (
    <div data-testid="code-viewer" data-path={props.path} data-selection={lines(props.selection)} data-highlight={lines(props.highlight)} data-nonce={props.highlightNonce ?? ''}>
      <pre>{props.text}</pre>
      <button type="button" onClick={() => props.onSelectLines({ startLine: 2, endLine: 3 })}>Pick lines 2 to 3</button>
      <button type="button" onClick={() => props.onSelectLines({ startLine: 99, endLine: 99 })}>Pick line 99</button>
      <button type="button" onClick={() => props.onSelectLines(null)}>Pick nothing</button>
    </div>
  ),
}));

vi.mock('@/features/learning/practice/practical/LearningCodeEditor', () => ({
  LearningCodeEditor: (props: { ariaLabel?: string; value: string; onChange: (value: string) => void }) => (
    <textarea aria-label={props.ariaLabel} value={props.value} onChange={(event) => props.onChange(event.target.value)} />
  ),
}));

// The reveal is built apart from this screen.
vi.mock('@/features/learning/drills/reveal/DrillReveal', () => ({
  DrillReveal: (props: { result: { attemptId: string; mode: string }; onNextDrill: () => void; onRegrade: () => void; regrading: boolean }) => (
    <div data-testid="reveal" data-regrading={String(props.regrading)}>
      <p>Revealed {props.result.attemptId}</p>
      <button type="button" onClick={props.onNextDrill}>Next drill</button>
      <button type="button" onClick={props.onRegrade}>Grade again from the reveal</button>
    </div>
  ),
}));

const FILES = [
  { path: 'src/limiter.py', text: 'import time\n\nclass Limiter:\n    def allow(self, key):\n        return True\n' },
  { path: 'src/store.py', text: 'DATA = {}\n' },
];
const DRILL = { title: 'A rate limiter', material: { kind: 'code', language: 'python' } as const, files: FILES, lesson: null };
const LESSON = { programId: 'course-1', lessonId: 'lesson-2', title: 'Spacing' };
const LESSON_DRILL = { ...DRILL, lesson: LESSON };

function started(overrides: Partial<Extract<StartedCodeReviewDto, { status: 'started' }>> = {}): StartedCodeReviewDto {
  return { status: 'started', attemptId: 'attempt-1', mode: 'practice', startedAt: Date.now(), drill: DRILL, ...overrides };
}

const SCORE = { found: 0, halfFound: 0, missed: 0, validExtras: 0, falsePositives: 0, bySeverity: [], weightedRecall: 0, precision: null, ledWithTheMostSevere: false };
const REVEAL: CodeReviewResultDto['reveal'] = {
  issues: [],
  findings: [],
  score: SCORE,
  ratings: [],
  fix: null,
  cleanFiles: [],
  source: null,
};

/** An attempt as the backend reads it: open unless the test says it was handed in. */
function attempt(overrides: Partial<CodeReviewResultDto> = {}): CodeReviewResultDto {
  return {
    attemptId: 'attempt-1',
    mode: 'practice',
    status: 'open',
    startedAt: Date.now(),
    submittedAt: null,
    timeSpentMs: null,
    drill: DRILL,
    findings: [],
    fixFiles: null,
    error: null,
    graderModel: null,
    gradedAt: null,
    reveal: null,
    ...overrides,
  };
}
const submitted = (overrides: Partial<CodeReviewResultDto> = {}) => attempt({ status: 'submitted', submittedAt: Date.now(), timeSpentMs: 5_000, ...overrides });
const graded = (overrides: Partial<CodeReviewResultDto> = {}) => submitted({ status: 'graded', reveal: REVEAL, gradedAt: Date.now(), ...overrides });

const CODE_SHELF = 'drill:course-1:code:python';
const PASSAGE_SHELF = 'drill:course-1:passage';
const buildJob = (overrides: Partial<JobDto>) => jobFixture({ id: 'job-9', kind: 'learning.drill_build', subjectId: CODE_SHELF, ...overrides });
const gradingJob = (overrides: Partial<JobDto>) => jobFixture({ id: 'job-g', kind: 'learning.drill_grading', subjectId: 'attempt-1', ...overrides });
function report(job: JobDto) {
  act(() => {
    for (const handler of [...events.handlers]) handler({ payload: job });
  });
}

const DUE_AT = new Date(2026, 10, 20, 9, 0).getTime();
const DUE_TEXT = new Date(DUE_AT).toLocaleDateString([], { day: 'numeric', month: 'short' });

function lesson(overrides: Partial<DrillLessonDto> & Pick<DrillLessonDto, 'lessonId' | 'title'>): DrillLessonDto {
  return { standing: 'new', dueAt: null, fitsCode: true, fitsPassage: true, current: false, completed: false, ...overrides };
}

const MODULES: DrillModuleDto[] = [
  {
    moduleId: 'module-1',
    title: 'Memory',
    lessons: [
      lesson({ lessonId: 'lesson-1', title: 'Retrieval', standing: 'due', completed: true }),
      lesson({ lessonId: 'lesson-2', title: 'Spacing', standing: 'new', current: true }),
      lesson({ lessonId: 'lesson-3', title: 'Sleep', standing: 'rested', dueAt: DUE_AT, fitsCode: false, completed: true }),
    ],
  },
  {
    moduleId: 'module-2',
    title: 'Habits',
    lessons: [lesson({ lessonId: 'lesson-4', title: 'Interleaving', standing: 'pending', fitsPassage: false })],
  },
];

function courseDrills(overrides: Partial<CourseDrillsDto> = {}): CourseDrillsDto {
  return {
    language: 'python',
    nextCode: { lessonId: 'lesson-2', status: 'ready' },
    nextPassage: { lessonId: 'lesson-3', status: 'to_build' },
    hasSources: true,
    modules: MODULES,
    recent: [],
    ...overrides,
  };
}

const STARTED_AT = new Date(2026, 10, 3, 9, 30).getTime();
const startedText = momentOf(STARTED_AT)?.text;

function recentAttempt(overrides: Partial<CourseDrillAttemptDto> = {}): CourseDrillAttemptDto {
  return {
    attemptId: 'attempt-1',
    status: 'open',
    mode: 'practice',
    startedAt: STARTED_AT,
    title: 'A rate limiter',
    material: { kind: 'code', language: 'python' },
    lesson: LESSON,
    score: null,
    ...overrides,
  };
}

/** What the course's drills say besides the defaults, whatever language is asked for. */
let served: Partial<CourseDrillsDto> = {};
const serve = (overrides: Partial<CourseDrillsDto>) => {
  served = overrides;
};

function show() {
  const rendered = render(<CourseDrillsPanel programId="course-1" />);
  return { ...rendered, user: userEvent.setup() };
}
type User = ReturnType<typeof userEvent.setup>;

const COMMENT = 'What is wrong, why it matters and what to do';

/** The Start button once the course's lessons have been read. */
async function clickStart(user: User) {
  const button = await screen.findByRole('button', { name: 'Start drill' });
  await waitFor(() => expect(button).toBeEnabled());
  await user.click(button);
}

async function startDrill(user: User) {
  await clickStart(user);
  await screen.findByRole('heading', { name: 'A rate limiter' });
}

/** Opens an attempt from the list of recent drills. */
async function openRecent(user: User, title = 'A rate limiter') {
  await user.click(await screen.findByRole('button', { name: new RegExp(title) }));
}

async function comment(user: User, text: string, severity?: string) {
  await user.type(screen.getByRole('textbox', { name: COMMENT }), text);
  if (severity) await user.selectOptions(screen.getByRole('combobox', { name: 'Severity' }), severity);
  await user.click(screen.getByRole('button', { name: 'Add comment' }));
}

const commentTexts = () => screen.getAllByRole('listitem').map((item) => item.querySelector('p')?.textContent);

beforeEach(() => {
  localStorage.clear();
  events.handlers.clear();
  for (const mock of Object.values(api)) mock.mockReset();
  api.codeReviewLanguages.mockResolvedValue(['python', 'csharp', 'sql']);
  served = {};
  api.courseDrills.mockImplementation((_programId: string, language: string | null) =>
    Promise.resolve(courseDrills({ language: language ?? 'python', ...served })));
});

afterEach(() => {
  vi.useRealTimers();
});

describe('choosing a drill', () => {
  it('has a heading and says what the drills are, and says the lessons are being read until they are', async () => {
    const reading = deferred<CourseDrillsDto>();
    api.courseDrills.mockReturnValueOnce(reading.promise);
    show();
    expect(screen.getByRole('heading', { name: 'Review drills' })).toBeVisible();
    expect(screen.getByText(/Review something you did not write, built from a lesson of this course/)).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('Reading the course’s lessons…');
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeDisabled();
    await act(async () => {
      reading.resolve(courseDrills());
    });
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeEnabled();
  });

  it('lists the languages the command gives, named for reading, with the course’s own chosen', async () => {
    api.codeReviewLanguages.mockResolvedValue(['python', 'csharp', 'sql', 'zig']);
    show();
    await screen.findByRole('option', { name: 'C#' });
    const select = screen.getByRole('combobox', { name: 'Language' });
    expect(within(select).getAllByRole('option').map((option) => option.textContent)).toEqual(['Python', 'C#', 'SQL', 'zig']);
    expect(select).toHaveValue('python');
    expect(api.codeReviewLanguages).toHaveBeenCalledTimes(1);
  });

  it('offers the choices last used', async () => {
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ material: 'code', mode: 'interview', size: 'long', minutes: 25 }));
    localStorage.setItem('lattice.drills.language.course-1', 'sql');
    show();
    expect(await screen.findByRole('combobox', { name: 'Language' })).toHaveValue('sql');
    expect(screen.getByRole('radio', { name: /Interview/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Long/ })).toBeChecked();
    expect(screen.getByRole('spinbutton', { name: 'Time limit' })).toHaveValue(25);
  });

  it('says how long each size is', async () => {
    show();
    expect(await screen.findByRole('radio', { name: /Short.*About 40 lines, 3 issues/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Standard.*About 80 lines, 4 issues/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Long.*About 140 lines, 5 issues/ })).toBeInTheDocument();
  });

  it('asks for the length of an interview in whole minutes, and starts only with one', async () => {
    const { user } = show();
    await user.click(await screen.findByRole('radio', { name: /Interview/ }));
    const limit = screen.getByRole('spinbutton', { name: 'Time limit' });
    expect(limit).toHaveValue(10);
    fireEvent.change(limit, { target: { value: '' } });
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a whole number of minutes, at least 1.');
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeDisabled();
    fireEvent.change(limit, { target: { value: '15' } });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start drill' })).toBeEnabled());
  });

  it('shows the backend message when the languages cannot be read, and reads them again', async () => {
    api.codeReviewLanguages.mockRejectedValueOnce(new Error('The catalogue is unavailable.')).mockResolvedValue(['python', 'rust']);
    const { user } = show();
    expect(await screen.findByRole('alert')).toHaveTextContent('The catalogue is unavailable.');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('option', { name: 'Rust' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});

describe('choosing a lesson', () => {
  /** The lesson select, once the course's lessons have been read into it. */
  const lessonSelect = async () => {
    const select = await screen.findByRole('combobox', { name: 'Lesson' });
    await waitFor(() => expect(select).toBeEnabled());
    return select;
  };
  const optionTexts = (select: HTMLElement) => within(select).getAllByRole('option').map((option) => option.textContent);

  it('asks for the course’s drills in no language to begin with, and in the one remembered for the course when there is one', async () => {
    const first = show();
    await lessonSelect();
    expect(api.courseDrills).toHaveBeenCalledWith('course-1', null);
    first.unmount();

    localStorage.setItem('lattice.drills.language.course-1', 'csharp');
    show();
    await waitFor(() => expect(api.courseDrills).toHaveBeenLastCalledWith('course-1', 'csharp'));
    expect(await screen.findByRole('combobox', { name: 'Language' })).toHaveValue('csharp');
  });

  it('lists the lessons by module with their standing and the current lesson marked, with the one next up chosen first', async () => {
    show();
    const select = await lessonSelect();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    expect(select).toHaveValue('');
    expect(within(select).getAllByRole('group').map((group) => group.getAttribute('label'))).toEqual(['Memory', 'Habits']);
    expect(optionTexts(select)).toEqual([
      'Next: Spacing',
      'Retrieval — Due now',
      'Spacing — New, current lesson',
      `Sleep — Due again ${DUE_TEXT} (no code drill fits it)`,
      'Interleaving — Not yet graded',
    ]);
    expect(within(within(select).getAllByRole('group')[1]).getAllByRole('option')).toHaveLength(1);
  });

  it('says a rested lesson is due again without a date when it has none', async () => {
    serve({ modules: [{ moduleId: 'module-1', title: 'Memory', lessons: [lesson({ lessonId: 'lesson-1', title: 'Retrieval', standing: 'rested' })] }] });
    show();
    await screen.findByRole('option', { name: 'Retrieval — Due again' });
  });

  it('disables the lessons the material does not fit, with why, and follows the material', async () => {
    const { user } = show();
    const select = await lessonSelect();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    expect(screen.getByRole('option', { name: /^Sleep — .*no code drill fits it/ })).toBeDisabled();
    expect(screen.getByRole('option', { name: /^Interleaving/ })).toBeEnabled();
    expect(screen.getByRole('option', { name: /^Retrieval/ })).toBeEnabled();

    await user.click(screen.getByRole('radio', { name: /An explanation of the lesson’s sources/ }));
    await screen.findByRole('option', { name: 'Next: Sleep' });
    expect(select).toHaveValue('');
    expect(screen.getByRole('option', { name: /^Interleaving — Not yet graded \(no passage in the sources fits it\)/ })).toBeDisabled();
    expect(screen.getByRole('option', { name: /^Sleep — Due again/ })).toBeEnabled();
    expect(screen.getByRole('option', { name: /^Sleep — Due again/ })).not.toHaveTextContent('no code drill');
  });

  it('goes back to the lesson next up when the lesson picked stops fitting the material', async () => {
    const { user } = show();
    const select = await lessonSelect();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    await user.selectOptions(select, 'lesson-4');
    expect(select).toHaveValue('lesson-4');

    await user.click(screen.getByRole('radio', { name: /An explanation of the lesson’s sources/ }));
    await waitFor(() => expect(select).toHaveValue(''));
    expect(screen.getByRole('option', { name: 'Next: Sleep' })).toBeInTheDocument();
  });

  it('goes back to the lesson next up when the lesson picked is not in the course’s lessons any more', async () => {
    api.courseDrills.mockImplementation((_programId: string, language: string | null) =>
      Promise.resolve(courseDrills({
        language: language ?? 'python',
        nextCode: { lessonId: language === 'csharp' ? 'lesson-4' : 'lesson-2', status: 'ready' },
        modules: language === 'csharp' ? [{ moduleId: 'module-2', title: 'Habits', lessons: [MODULES[1].lessons[0]] }] : MODULES,
      })));
    const { user } = show();
    const select = await lessonSelect();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    await user.selectOptions(select, 'lesson-1');
    expect(select).toHaveValue('lesson-1');

    await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'csharp');
    await waitFor(() => expect(select).toHaveValue(''));
    expect(optionTexts(select)).toEqual(['Next: Interleaving', 'Interleaving — Not yet graded']);
  });

  it.each([
    ['ready', 'A drill of it is ready.'],
    ['building', 'A drill of it is being written.'],
    ['to_build', 'A drill of it is written when you start, usually in a minute or two.'],
  ] as const)('says under the select what starting on the lesson next up does when its drill is %s', async (status, line) => {
    serve({ nextCode: { lessonId: 'lesson-2', status } });
    show();
    expect(await screen.findByText(line)).toBeVisible();
  });

  it('says under the select that a drill is written when none is ready, once a lesson is picked', async () => {
    const { user } = show();
    await user.selectOptions(await lessonSelect(), 'lesson-1');
    expect(screen.getByText('If no drill of it is ready, one is written when you start, usually in a minute or two.')).toBeVisible();
    expect(screen.queryByText('A drill of it is ready.')).not.toBeInTheDocument();
  });

  it('says nothing under the select while the lessons are being read', async () => {
    api.courseDrills.mockReturnValueOnce(deferred<CourseDrillsDto>().promise);
    show();
    expect(screen.queryByText(/A drill of it/)).not.toBeInTheDocument();
    expect(screen.queryByText(/No lesson of this course can be drilled yet/)).not.toBeInTheDocument();
  });

  it('starts on the lesson next up with no lesson named, in the material, mode and size chosen', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const { user } = show();
    await screen.findByRole('option', { name: 'C#' });
    await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'csharp');
    await user.click(screen.getByRole('radio', { name: /Interview/ }));
    await user.click(screen.getByRole('radio', { name: /Short/ }));
    await clickStart(user);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'csharp' }, 'interview', 40, 3);
  });

  it('starts on a lesson picked with its id', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const { user } = show();
    await user.selectOptions(await lessonSelect(), 'lesson-1');
    await user.click(screen.getByRole('radio', { name: /Long/ }));
    await clickStart(user);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', 'lesson-1', { kind: 'code', language: 'python' }, 'practice', 140, 5);
  });

  it('shows the language the course offers, asks again in the one picked, and starts in it', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    api.courseDrills.mockImplementation((_programId: string, language: string | null) =>
      Promise.resolve(courseDrills({ language: language ?? 'sql' })));
    const { user } = show();
    const language = await screen.findByRole('combobox', { name: 'Language' });
    await waitFor(() => expect(language).toHaveValue('sql'));
    expect(within(language).getAllByRole('option').map((option) => option.textContent)).toContain('SQL');

    await user.selectOptions(language, 'csharp');
    await waitFor(() => expect(api.courseDrills).toHaveBeenLastCalledWith('course-1', 'csharp'));
    expect(language).toHaveValue('csharp');
    await clickStart(user);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'csharp' }, 'practice', 80, 4);
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBe('csharp');
  });

  it('starts in the language the course offers when none is picked', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    api.courseDrills.mockImplementation((_programId: string, language: string | null) =>
      Promise.resolve(courseDrills({ language: language ?? 'sql' })));
    const { user } = show();
    await clickStart(user);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'sql' }, 'practice', 80, 4);
  });

  it('disables the explanation when the course has no source with text, says so, and keeps code chosen', async () => {
    serve({ nextPassage: null, hasSources: false });
    show();
    const passage = await screen.findByRole('radio', { name: /An explanation of the lesson’s sources/ });
    await waitFor(() => expect(passage).toBeDisabled());
    expect(screen.getByText('The course has no source with text yet.')).toBeVisible();
    expect(screen.getByRole('radio', { name: /^Code/ })).toBeChecked();
  });

  it('disables the explanation when no source holds a passage to explain, and says so', async () => {
    serve({ nextPassage: null, hasSources: true });
    show();
    const passage = await screen.findByRole('radio', { name: /An explanation of the lesson’s sources/ });
    await waitFor(() => expect(passage).toBeDisabled());
    expect(screen.getByText('None of the lessons’ sources holds a passage to explain.')).toBeVisible();
    expect(screen.queryByText('The course has no source with text yet.')).not.toBeInTheDocument();
  });

  it('disables code when no lesson can be drilled in it, says so, and chooses the explanation', async () => {
    serve({ nextCode: null });
    show();
    const code = await screen.findByRole('radio', { name: /^Code/ });
    await waitFor(() => expect(code).toBeDisabled());
    expect(screen.getByText('No lesson of this course can be drilled in code yet.')).toBeVisible();
    await waitFor(() => expect(screen.getByRole('radio', { name: /An explanation of the lesson’s sources/ })).toBeChecked());
    expect(screen.queryByRole('combobox', { name: 'Language' })).not.toBeInTheDocument();
    expect(await screen.findByRole('option', { name: 'Next: Sleep' })).toBeInTheDocument();
  });

  it('does not start when no lesson can be drilled, and says so', async () => {
    serve({ nextCode: null, nextPassage: null, hasSources: false });
    show();
    expect(await screen.findByText('No lesson of this course can be drilled yet.')).toBeVisible();
    expect(screen.getByRole('option', { name: 'No lesson can be drilled' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /^Code/ })).toBeChecked();
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeDisabled();
  });

  it('starts on a lesson picked when none is next up', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    serve({ nextCode: null, nextPassage: null });
    const { user } = show();
    await screen.findByText('No lesson of this course can be drilled yet.');
    await user.selectOptions(screen.getByRole('combobox', { name: 'Lesson' }), 'lesson-1');
    await clickStart(user);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', 'lesson-1', { kind: 'code', language: 'python' }, 'practice', 80, 4);
  });

  it('does not start code before it has a language', async () => {
    api.courseDrills.mockImplementation(() => Promise.resolve(courseDrills({ language: '' })));
    show();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeDisabled();
  });

  it('shows the backend message when the lessons cannot be read, and reads them again on request', async () => {
    api.courseDrills.mockRejectedValueOnce(new Error('The course is not there.'));
    const { user } = show();
    expect(await screen.findByRole('alert')).toHaveTextContent('The course is not there.');
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('option', { name: 'Next: Spacing' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(api.courseDrills).toHaveBeenCalledTimes(2);
  });

  it('reads the lessons again when a build of this course finishes, and not for another course’s or another kind of job', async () => {
    show();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    expect(api.courseDrills).toHaveBeenCalledTimes(1);

    report(buildJob({ id: 'job-other', subjectId: 'drill:course-2:code:python', status: 'completed', finishedAt: 9 }));
    report(buildJob({ id: 'job-grading', kind: 'learning.drill_grading', status: 'completed', finishedAt: 9 }));
    report(buildJob({ id: 'job-running', status: 'running' }));
    expect(api.courseDrills).toHaveBeenCalledTimes(1);

    report(buildJob({ id: 'job-passage', subjectId: PASSAGE_SHELF, status: 'completed', finishedAt: 9 }));
    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledTimes(2));
    report(buildJob({ id: 'job-code', status: 'completed', finishedAt: 10 }));
    await waitFor(() => expect(api.courseDrills).toHaveBeenCalledTimes(3));
  });
});

describe('the recent drills', () => {
  it('lists each with its lesson, material, mode and date, and how it fared', async () => {
    serve({
      recent: [
        recentAttempt({ attemptId: 'a1', title: 'Open one' }),
        recentAttempt({ attemptId: 'a2', title: 'Handed in', status: 'submitted', mode: 'interview', material: { kind: 'passage' }, lesson: null }),
        recentAttempt({ attemptId: 'a3', title: 'Graded one', status: 'graded', mode: 'rapid', lesson: { ...LESSON, title: 'Sleep' }, score: { ...SCORE, found: 2, halfFound: 1, missed: 1 } }),
        recentAttempt({ attemptId: 'a4', title: 'Graded without a score', status: 'graded' }),
        recentAttempt({ attemptId: 'a5', title: 'Not graded', status: 'couldnt_grade', material: { kind: 'code', language: 'csharp' } }),
      ],
    });
    show();
    const list = within((await screen.findByRole('heading', { name: 'Recent drills' })).closest('section') as HTMLElement);
    const items = list.getAllByRole('button');
    expect(items).toHaveLength(5);

    const open = within(items[0]);
    expect(open.getByText('Open one')).toBeVisible();
    expect(open.getByText(`Spacing · Python · Practice · ${startedText}`)).toBeVisible();
    expect(open.getByText('Continue')).toBeVisible();

    const handedIn = within(items[1]);
    expect(handedIn.getByText(`Explanation · Interview · ${startedText}`)).toBeVisible();
    expect(handedIn.getByText('Waiting for its grade')).toBeVisible();

    const scored = within(items[2]);
    expect(scored.getByText(`Sleep · Python · Rapid · ${startedText}`)).toBeVisible();
    expect(scored.getByText('Found 2 of 4')).toBeVisible();

    expect(within(items[3]).getByText('Graded')).toBeVisible();
    expect(within(items[4]).getByText(`Spacing · C# · Practice · ${startedText}`)).toBeVisible();
    expect(within(items[4]).getByText('Could not be graded')).toBeVisible();
  });

  it('is not there when there are none', async () => {
    show();
    await screen.findByRole('option', { name: 'Next: Spacing' });
    expect(screen.queryByRole('heading', { name: 'Recent drills' })).not.toBeInTheDocument();
  });

  it('is read again on coming back to the choices from a drill', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const { user } = show();
    await startDrill(user);
    expect(api.courseDrills).toHaveBeenCalledTimes(1);

    serve({ recent: [recentAttempt()] });
    await user.click(screen.getByRole('button', { name: 'Review drills' }));
    expect(await screen.findByRole('heading', { name: 'Recent drills' })).toBeVisible();
    expect(api.courseDrills).toHaveBeenCalledTimes(2);
  });
});

describe('starting a drill', () => {
  it('opens the review of a drill that is ready, in the size and mode chosen, and remembers the choices', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const { user } = show();
    await screen.findByRole('option', { name: 'C#' });
    await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'csharp');
    await user.click(screen.getByRole('radio', { name: /Short/ }));
    await clickStart(user);

    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'csharp' }, 'practice', 40, 3);
    expect(screen.getByRole('tab', { name: 'src/limiter.py' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/limiter.py');
    expect(JSON.parse(localStorage.getItem('lattice.drills.choices') ?? '{}')).toEqual({ material: 'code', mode: 'practice', size: 'short', minutes: 10 });
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBe('csharp');
  });

  it('says a drill is being written when none is ready, shows the job, and opens the review when the build completes', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started());
    const { user } = show();
    await clickStart(user);

    expect(await screen.findByText('Writing a drill for you, usually a minute or two')).toBeVisible();
    expect(screen.getByText(/keeps the next drill of this kind ready for the course, so later starts are usually instant/)).toBeVisible();
    report(buildJob({ progressMessage: 'Planting the defects (3 of 6)', progressCurrent: 2, progressTotal: 6 }));
    expect(await screen.findByText('Planting the defects (3 of 6)')).toBeVisible();
    expect(screen.getByRole('progressbar', { name: 'Drill build progress' })).toHaveAttribute('aria-valuenow', '33');
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);

    report(buildJob({ status: 'completed', finishedAt: 9 }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCourseDrill).toHaveBeenCalledTimes(2);
  });

  it('starts again when any build of the course’s shelf completes, and ignores the builds of other languages, other courses and other jobs', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started());
    const { user } = show();
    await clickStart(user);
    await screen.findByText('Writing a drill for you, usually a minute or two');

    report(buildJob({ id: 'job-rust', subjectId: 'drill:course-1:code:rust', status: 'completed', finishedAt: 9 }));
    report(buildJob({ id: 'job-course', subjectId: 'drill:course-2:code:python', status: 'completed', finishedAt: 9 }));
    report(buildJob({ id: 'job-other', kind: 'learning.drill_grading', status: 'completed', finishedAt: 9 }));
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);

    report(buildJob({ id: 'job-next', status: 'completed', finishedAt: 9 }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('shows a failed build’s error, asks again only when told to, and then opens the drill', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started());
    const { user } = show();
    await clickStart(user);
    await screen.findByText('Writing a drill for you, usually a minute or two');

    report(buildJob({ status: 'failed', error: 'The model is not loaded.', finishedAt: 9 }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The model is not loaded.');
    expect(screen.queryByText('Writing a drill for you, usually a minute or two')).not.toBeInTheDocument();
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCourseDrill).toHaveBeenCalledTimes(2);
  });

  it.each([
    ['cancelled', 'The drill build was cancelled.'],
    ['failed', 'The drill could not be built.'],
    ['interrupted', 'The drill could not be built.'],
  ] as const)('says what happened to a build that ends as %s and gives no reason', async (status, message) => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await clickStart(user);
    await screen.findByText('Writing a drill for you, usually a minute or two');
    report(buildJob({ status, finishedAt: 9 }));
    expect(await screen.findByRole('alert')).toHaveTextContent(message);
  });

  it('shows the error of a build that tries again, and goes on waiting', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await clickStart(user);
    await screen.findByText('Writing a drill for you, usually a minute or two');
    report(buildJob({ status: 'pending', error: 'The model disconnected.' }));
    expect(await screen.findByText('The model disconnected.')).toBeVisible();
    expect(screen.getByText('The last try failed, so it is trying again.')).toBeVisible();
    expect(screen.getByText('Writing a drill for you, usually a minute or two')).toBeVisible();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows the backend message when a drill cannot be started, and asks again on request', async () => {
    api.startCourseDrill.mockRejectedValueOnce(new Error('python is not a language drills are written in.')).mockResolvedValueOnce(started());
    const { user } = show();
    await clickStart(user);
    expect(await screen.findByRole('alert')).toHaveTextContent('python is not a language drills are written in.');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('stops waiting for a build and goes back to the choices, which are kept', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await user.click(await screen.findByRole('radio', { name: /Long/ }));
    await clickStart(user);
    await user.click(await screen.findByRole('button', { name: 'Stop waiting' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.getByRole('radio', { name: /Long/ })).toBeChecked();
    report(buildJob({ status: 'completed', finishedAt: 9 }));
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);
  });

  it('takes one drill, however many reports of a finished build there are while it asks', async () => {
    const first = deferred<StartedCodeReviewDto>();
    api.startCourseDrill.mockReturnValueOnce(first.promise).mockResolvedValueOnce(started());
    const { user } = show();
    await clickStart(user);
    report(buildJob({ status: 'completed', finishedAt: 9 }));
    report(buildJob({ status: 'completed', finishedAt: 9 }));
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);
    await act(async () => {
      first.resolve({ status: 'building', jobId: 'job-9' });
    });
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCourseDrill).toHaveBeenCalledTimes(2);
  });
});

describe('writing the review', () => {
  beforeEach(() => {
    api.startCourseDrill.mockResolvedValue(started());
    api.submitCodeReview.mockResolvedValue(submitted());
  });

  it('shows the drill’s files as tabs over a read-only view', async () => {
    const { user } = show();
    await startDrill(user);
    const tabs = within(screen.getByRole('tablist', { name: 'Files of the drill' })).getAllByRole('tab');
    expect(tabs.map((tab) => tab.textContent)).toEqual(['src/limiter.py', 'src/store.py']);
    await user.click(tabs[1]);
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/store.py');
    expect(screen.getByText('DATA = {}')).toBeVisible();
  });

  it('moves between the file tabs with the arrow keys, Home and End', async () => {
    const { user } = show();
    await startDrill(user);
    const limiter = () => screen.getByRole('tab', { name: 'src/limiter.py' });
    const store = () => screen.getByRole('tab', { name: 'src/store.py' });
    await user.click(limiter());

    await user.keyboard('{ArrowRight}');
    expect(store()).toHaveAttribute('aria-selected', 'true');
    expect(store()).toHaveFocus();
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/store.py');
    await user.keyboard('{ArrowRight}');
    expect(limiter()).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowLeft}');
    expect(store()).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{Home}');
    expect(limiter()).toHaveFocus();
    await user.keyboard('{End}');
    expect(store()).toHaveFocus();
  });

  it('makes a comment on the selected lines, and a general one on none', async () => {
    const { user } = show();
    await startDrill(user);
    expect(screen.getByText(/A general comment/)).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    expect(screen.getByText('Lines 2–3 of src/limiter.py')).toBeVisible();
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-selection', '2-3');
    await comment(user, 'The check and the write are not atomic.', 'major');

    const first = screen.getByRole('listitem', { name: 'Comment 1' });
    expect(within(first).getByRole('button', { name: /Lines 2–3 of src\/limiter\.py/ })).toBeVisible();
    expect(within(first).getByText('Major')).toBeVisible();
    expect(within(first).getByText('The check and the write are not atomic.')).toBeVisible();
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-selection', '');

    await comment(user, 'Nothing is tested.');
    expect(within(screen.getByRole('listitem', { name: 'Comment 2' })).getByText('General comment')).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenCalledWith(
      'attempt-1',
      [
        { span: { path: 'src/limiter.py', start: 2, end: 3 }, text: 'The check and the write are not atomic.', severity: 'major' },
        { span: null, text: 'Nothing is tested.', severity: null },
      ],
      null,
      expect.any(Number),
    );
  });

  it('puts the lines of a comment in the file they were picked in, and does not pick lines the file has not', async () => {
    const { user } = show();
    await startDrill(user);
    await user.click(screen.getByRole('tab', { name: 'src/store.py' }));
    await user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    expect(screen.getByText(/A general comment/)).toBeVisible();

    await user.click(screen.getByRole('tab', { name: 'src/limiter.py' }));
    await user.click(screen.getByRole('button', { name: 'Pick line 99' }));
    expect(screen.getByText(/A general comment/)).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    await user.click(screen.getByRole('tab', { name: 'src/store.py' }));
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-selection', '');
    expect(screen.getByText('Lines 2–3 of src/limiter.py')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Clear the selected lines' }));
    expect(screen.getByText(/A general comment/)).toBeVisible();

    await user.click(screen.getByRole('tab', { name: 'src/limiter.py' }));
    await user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-selection', '2-3');
    await user.click(screen.getByRole('button', { name: 'Pick nothing' }));
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-selection', '');
    expect(screen.getByText(/A general comment/)).toBeVisible();
  });

  it('adds a comment with Ctrl+Enter, and only one that says something', async () => {
    const { user } = show();
    await startDrill(user);
    expect(screen.getByRole('button', { name: 'Add comment' })).toBeDisabled();
    await user.type(screen.getByRole('textbox', { name: COMMENT }), '   ');
    expect(screen.getByRole('button', { name: 'Add comment' })).toBeDisabled();
    await user.type(screen.getByRole('textbox', { name: COMMENT }), 'Retries forever.{Control>}{Enter}{/Control}');
    expect(commentTexts()).toEqual(['Retries forever.']);
    expect(screen.getByRole('textbox', { name: COMMENT })).toHaveValue('');
  });

  it('tells the learner to put the most severe issue first', async () => {
    const { user } = show();
    await startDrill(user);
    expect(screen.getByText('Put what you would fix first at the top: leading with the most severe issue counts.')).toBeVisible();
  });

  it('edits, deletes and reorders comments, and hands them in as they stand', async () => {
    const { user } = show();
    await startDrill(user);
    await comment(user, 'first');
    await comment(user, 'second');
    await comment(user, 'third');
    expect(commentTexts()).toEqual(['first', 'second', 'third']);
    expect(screen.getByRole('button', { name: 'Move comment 1 up' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Move comment 3 down' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: 'Move comment 3 up' }));
    expect(commentTexts()).toEqual(['first', 'third', 'second']);
    await user.click(screen.getByRole('button', { name: 'Move comment 1 down' }));
    expect(commentTexts()).toEqual(['third', 'first', 'second']);

    await user.click(screen.getByRole('button', { name: 'Edit comment 2' }));
    expect(screen.getByRole('heading', { name: 'Edit comment' })).toBeVisible();
    const box = screen.getByRole('textbox', { name: COMMENT });
    expect(box).toHaveValue('first');
    await user.clear(box);
    await user.type(box, 'first, edited');
    await user.selectOptions(screen.getByRole('combobox', { name: 'Severity' }), 'critical');
    await user.click(screen.getByRole('button', { name: 'Save comment' }));
    expect(commentTexts()).toEqual(['third', 'first, edited', 'second']);
    expect(within(screen.getByRole('listitem', { name: 'Comment 2' })).getByText('Critical')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Add a comment' })).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Delete comment 1' }));
    expect(commentTexts()).toEqual(['first, edited', 'second']);

    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenCalledWith(
      'attempt-1',
      [
        { span: null, text: 'first, edited', severity: 'critical' },
        { span: null, text: 'second', severity: null },
      ],
      null,
      expect.any(Number),
    );
  });

  it('moves the lines of a comment when they are picked again while editing it', async () => {
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Not atomic.');
    await user.click(screen.getByRole('button', { name: 'Edit comment 1' }));
    await user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    await user.click(screen.getByRole('button', { name: 'Save comment' }));
    expect(within(screen.getByRole('listitem', { name: 'Comment 1' })).getByRole('button', { name: /Lines 2–3 of src\/limiter\.py/ })).toBeVisible();
  });

  it('leaves editing without changing the comment', async () => {
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Keep me.');
    await user.click(screen.getByRole('button', { name: 'Edit comment 1' }));
    await user.type(screen.getByRole('textbox', { name: COMMENT }), ' And more.');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(commentTexts()).toEqual(['Keep me.']);
    expect(screen.getByRole('heading', { name: 'Add a comment' })).toBeVisible();
  });

  it('takes the code view to the lines of a comment when it is clicked in the list', async () => {
    const { user } = show();
    await startDrill(user);
    await user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    await comment(user, 'Not atomic.');
    await user.click(screen.getByRole('tab', { name: 'src/store.py' }));
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/store.py');

    const show1 = screen.getByRole('button', { name: /Lines 2–3 of src\/limiter\.py/ });
    await user.click(show1);
    const viewer = screen.getByTestId('code-viewer');
    expect(viewer).toHaveAttribute('data-path', 'src/limiter.py');
    expect(viewer).toHaveAttribute('data-highlight', '2-3');
    expect(viewer).toHaveAttribute('data-nonce', '1');
    await user.click(show1);
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-nonce', '2');

    await user.click(screen.getByRole('tab', { name: 'src/store.py' }));
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-highlight', '');
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-nonce', '');
  });

  it('leaves editing when the comment being edited is deleted', async () => {
    const { user } = show();
    await startDrill(user);
    await comment(user, 'one');
    await comment(user, 'two');
    await user.click(screen.getByRole('button', { name: 'Edit comment 1' }));
    await user.click(screen.getByRole('button', { name: 'Delete comment 1' }));
    expect(commentTexts()).toEqual(['two']);
    expect(screen.getByRole('heading', { name: 'Add a comment' })).toBeVisible();
  });
});

describe('writing a fix', () => {
  beforeEach(() => {
    api.startCourseDrill.mockResolvedValue(started());
    api.submitCodeReview.mockResolvedValue(submitted());
  });

  it('sends no fix when nothing was changed', async () => {
    const { user } = show();
    await startDrill(user);
    await user.click(screen.getByRole('button', { name: 'Write a fix (optional)' }));
    expect(screen.getByRole('textbox', { name: 'Edit src/limiter.py' })).toHaveValue(FILES[0].text);
    await comment(user, 'Needs a lock.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenCalledWith('attempt-1', expect.any(Array), null, expect.any(Number));
  });

  it('sends only the files that were changed, and none once they are changed back', async () => {
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Needs a lock.');
    await user.click(screen.getByRole('button', { name: 'Write a fix (optional)' }));
    await user.click(screen.getByRole('tab', { name: 'src/store.py' }));
    const editor = screen.getByRole('textbox', { name: 'Edit src/store.py' });

    fireEvent.change(editor, { target: { value: 'DATA = {}\nLOCK = None\n' } });
    expect(screen.getByText('1 file changed')).toBeVisible();
    expect(screen.getByRole('tab', { name: /src\/store\.py/ })).toHaveTextContent('changed in your fix');

    fireEvent.change(editor, { target: { value: FILES[1].text } });
    expect(screen.queryByText('1 file changed')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenLastCalledWith('attempt-1', expect.any(Array), null, expect.any(Number));
  });

  it('hands in the changed files by path, and takes the changes back on request', async () => {
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Needs a lock.');
    await user.click(screen.getByRole('button', { name: 'Write a fix (optional)' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Edit src/limiter.py' }), { target: { value: 'fixed\n' } });
    await user.click(screen.getByRole('tab', { name: 'src/store.py' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Edit src/store.py' }), { target: { value: 'also fixed\n' } });
    expect(screen.getByText('2 files changed')).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Discard my changes' }));
    expect(screen.queryByText('2 files changed')).not.toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Edit src/store.py' })).toHaveValue(FILES[1].text);

    fireEvent.change(screen.getByRole('textbox', { name: 'Edit src/store.py' }), { target: { value: 'again\n' } });
    await user.click(screen.getByRole('button', { name: 'Back to the code' }));
    expect(screen.getByTestId('code-viewer')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenCalledWith('attempt-1', expect.any(Array), [{ path: 'src/store.py', text: 'again\n' }], expect.any(Number));
  });
});

describe('handing the review in', () => {
  beforeEach(() => {
    api.startCourseDrill.mockResolvedValue(started());
  });

  it('asks before handing in a review with no comments, and hands it in when told to', async () => {
    api.submitCodeReview.mockResolvedValue(submitted());
    const { user } = show();
    await startDrill(user);
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('Submit with no comments?');
    await user.click(within(dialog).getByRole('button', { name: 'Keep reviewing' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(api.submitCodeReview).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: /Submit anyway/ }));
    await waitFor(() => expect(api.submitCodeReview).toHaveBeenCalledWith('attempt-1', [], null, expect.any(Number)));
    expect(await screen.findByText('Grading your review…')).toBeVisible();
  });

  it('counts the time spent from when the attempt started', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-11-04T12:10:00Z'));
    api.startCourseDrill.mockResolvedValue(started({ startedAt: Date.now() - 125_000 }));
    api.submitCodeReview.mockResolvedValue(submitted());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Slow.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenCalledWith('attempt-1', expect.any(Array), null, 125_000);
  });

  it('shows the backend message when the hand-in fails, keeps the comments, and hands in again on request', async () => {
    api.submitCodeReview.mockRejectedValueOnce(new Error('Finding 1 is on lines 40 to 41 of src/limiter.py, which has 5 lines.')).mockResolvedValueOnce(submitted());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Past the end.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Finding 1 is on lines 40 to 41 of src/limiter.py, which has 5 lines.');
    expect(commentTexts()).toEqual(['Past the end.']);
    expect(screen.getByRole('button', { name: 'Submit review' })).toBeEnabled();

    await user.click(screen.getByRole('button', { name: 'Try handing in again' }));
    expect(await screen.findByText('Grading your review…')).toBeVisible();
    expect(api.submitCodeReview).toHaveBeenCalledTimes(2);
  });

  it('shows Submitting while the review is on its way', async () => {
    const sent = deferred<CodeReviewResultDto>();
    api.submitCodeReview.mockReturnValue(sent.promise);
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Slow.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(await screen.findByRole('button', { name: 'Submitting…' })).toBeDisabled();
    await act(async () => {
      sent.resolve(submitted());
    });
    expect(await screen.findByText('Grading your review…')).toBeVisible();
  });

  it('waits for the grade, follows the grading job, and shows the reveal when it is graded', async () => {
    api.submitCodeReview.mockResolvedValue(submitted());
    api.codeReviewResult.mockResolvedValue(graded());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'The lock is never released.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));

    expect(await screen.findByText('Grading your review…')).toBeVisible();
    expect(api.codeReviewResult).not.toHaveBeenCalled();
    report(gradingJob({ status: 'running', progressMessage: 'Grading the findings (1 of 3)' }));
    expect(await screen.findByText('Grading the findings (1 of 3)')).toBeVisible();
    report(gradingJob({ subjectId: 'another-attempt', status: 'completed', finishedAt: 9 }));
    expect(api.codeReviewResult).not.toHaveBeenCalled();

    report(gradingJob({ status: 'completed', finishedAt: 9 }));
    expect(await screen.findByText('Revealed attempt-1')).toBeVisible();
    expect(api.codeReviewResult).toHaveBeenCalledWith('attempt-1');
    expect(screen.getByTestId('reveal')).toHaveAttribute('data-regrading', 'false');
  });

  it('keeps waiting when the grading job ends and the attempt is still waiting', async () => {
    api.submitCodeReview.mockResolvedValue(submitted());
    api.codeReviewResult.mockResolvedValueOnce(submitted()).mockResolvedValueOnce(graded());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Late.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    report(gradingJob({ status: 'completed', finishedAt: 9 }));
    await waitFor(() => expect(api.codeReviewResult).toHaveBeenCalledTimes(1));
    expect(screen.getByText('Grading your review…')).toBeVisible();
    report(gradingJob({ status: 'completed', finishedAt: 10 }));
    expect(await screen.findByText('Revealed attempt-1')).toBeVisible();
  });

  it('says why a review could not be graded, and grades it again on request', async () => {
    api.submitCodeReview.mockResolvedValue(submitted());
    api.codeReviewResult.mockResolvedValue(submitted({ status: 'couldnt_grade', error: 'The model returned no assessment.' }));
    const again = deferred<CodeReviewResultDto>();
    api.regradeCodeReview.mockReturnValue(again.promise);
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Something.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    report(gradingJob({ status: 'failed', error: 'The model returned no assessment.', finishedAt: 9 }));

    expect(await screen.findByRole('heading', { name: 'Your review could not be graded' })).toBeVisible();
    expect(screen.getByText('The model returned no assessment.')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Grade again' }));
    expect(api.regradeCodeReview).toHaveBeenCalledWith('attempt-1');
    expect(await screen.findByRole('button', { name: 'Grading again…' })).toBeDisabled();

    await act(async () => {
      again.resolve(submitted());
    });
    expect(await screen.findByText('Grading your review…')).toBeVisible();
    expect(screen.queryByRole('heading', { name: 'Your review could not be graded' })).not.toBeInTheDocument();
  });

  it('shows the backend message when grading again fails, and tries again on request', async () => {
    api.submitCodeReview.mockResolvedValue(submitted());
    api.codeReviewResult.mockResolvedValue(submitted({ status: 'couldnt_grade', error: 'No assessment.' }));
    api.regradeCodeReview.mockRejectedValueOnce(new Error('No model is loaded.')).mockResolvedValueOnce(submitted());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Something.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    report(gradingJob({ status: 'failed', finishedAt: 9 }));
    await user.click(await screen.findByRole('button', { name: 'Grade again' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('No model is loaded.');
    await user.click(screen.getByRole('button', { name: 'Try grading again' }));
    expect(await screen.findByText('Grading your review…')).toBeVisible();
    expect(api.regradeCodeReview).toHaveBeenCalledTimes(2);
  });

  it('gives the reveal a way to grade again, and shows what the backend says when that is refused', async () => {
    api.codeReviewResult.mockResolvedValue(graded());
    const pending = deferred<CodeReviewResultDto>();
    api.regradeCodeReview.mockReturnValue(pending.promise);
    serve({ recent: [recentAttempt({ status: 'graded' })] });
    const { user } = show();
    await openRecent(user);
    await user.click(await screen.findByRole('button', { name: 'Grade again from the reveal' }));
    await waitFor(() => expect(screen.getByTestId('reveal')).toHaveAttribute('data-regrading', 'true'));
    expect(api.regradeCodeReview).toHaveBeenCalledWith('attempt-1');

    await act(async () => {
      pending.reject(new Error('Only a review that could not be graded can be graded again.'));
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('Only a review that could not be graded can be graded again.');
    expect(screen.getByTestId('reveal')).toHaveAttribute('data-regrading', 'false');
  });

  it('starts the next drill in the same language, mode and size from the reveal, on the lesson next up', async () => {
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ material: 'code', mode: 'interview', size: 'long', minutes: 20 }));
    localStorage.setItem('lattice.drills.language.course-1', 'rust');
    api.codeReviewLanguages.mockResolvedValue(['python', 'rust']);
    const rust = { ...DRILL, material: { kind: 'code', language: 'rust' } as const };
    api.startCourseDrill.mockResolvedValueOnce(started({ mode: 'interview', drill: rust })).mockResolvedValueOnce(started({ attemptId: 'attempt-2' }));
    api.submitCodeReview.mockResolvedValue(submitted({ mode: 'interview', drill: rust }));
    api.codeReviewResult.mockResolvedValue(graded({ mode: 'interview', drill: rust }));
    const { user } = show();
    await user.selectOptions(await screen.findByRole('combobox', { name: 'Lesson' }), 'lesson-1');
    await startDrill(user);
    expect(api.startCourseDrill).toHaveBeenLastCalledWith('course-1', 'lesson-1', { kind: 'code', language: 'rust' }, 'interview', 140, 5);
    await comment(user, 'Something.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    report(gradingJob({ status: 'completed', finishedAt: 9 }));
    await user.click(await screen.findByRole('button', { name: 'Next drill' }));

    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCourseDrill).toHaveBeenCalledTimes(2);
    expect(api.startCourseDrill).toHaveBeenLastCalledWith('course-1', null, { kind: 'code', language: 'rust' }, 'interview', 140, 5);
    expect(screen.queryByText('Revealed attempt-1')).not.toBeInTheDocument();
  });
});

describe('an attempt from the recent drills', () => {
  it('is read again from the backend and reopens its review with its drill', async () => {
    serve({ recent: [recentAttempt({ attemptId: 'attempt-7' })] });
    api.codeReviewResult.mockResolvedValue(attempt({ attemptId: 'attempt-7' }));
    const { user } = show();
    await openRecent(user);
    expect(screen.getByText('Opening your review…')).toBeVisible();
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.codeReviewResult).toHaveBeenCalledWith('attempt-7');
    expect(api.startCourseDrill).not.toHaveBeenCalled();
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/limiter.py');
  });

  it('is shown waiting for its grade when it was handed in', async () => {
    serve({ recent: [recentAttempt({ status: 'submitted' })] });
    api.codeReviewResult.mockResolvedValue(submitted());
    const { user } = show();
    await openRecent(user);
    expect(await screen.findByText('Grading your review…')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('is shown as its reveal when it was graded', async () => {
    serve({ recent: [recentAttempt({ status: 'graded' })] });
    api.codeReviewResult.mockResolvedValue(graded());
    const { user } = show();
    await openRecent(user);
    expect(await screen.findByText('Revealed attempt-1')).toBeVisible();
  });

  it('is shown with the reason it could not be graded', async () => {
    serve({ recent: [recentAttempt({ status: 'couldnt_grade' })] });
    api.codeReviewResult.mockResolvedValue(submitted({ status: 'couldnt_grade', error: 'The model was not loaded.' }));
    const { user } = show();
    await openRecent(user);
    expect(await screen.findByText('The model was not loaded.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Grade again' })).toBeEnabled();
  });

  it('shows the backend message when it cannot be read, and reads it again on request', async () => {
    serve({ recent: [recentAttempt()] });
    api.codeReviewResult.mockRejectedValueOnce(new Error('This review is not there.')).mockResolvedValueOnce(attempt());
    const { user } = show();
    await openRecent(user);
    expect(await screen.findByRole('alert')).toHaveTextContent('This review is not there.');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('can be left for a new drill when it is not there', async () => {
    serve({ recent: [recentAttempt({ attemptId: 'gone' })] });
    api.codeReviewResult.mockRejectedValue(new Error('This review is not there.'));
    const { user } = show();
    await openRecent(user);
    await user.click(await screen.findByRole('button', { name: 'Start a new drill' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Recent drills' })).toBeVisible();
  });

  it('brings back the comments and the fix written before the learner left', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const first = show();
    await startDrill(first.user);
    await first.user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    await comment(first.user, 'Not atomic.', 'major');
    await comment(first.user, 'No tests.');
    await first.user.click(screen.getByRole('button', { name: 'Write a fix (optional)' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Edit src/limiter.py' }), { target: { value: 'fixed\n' } });
    first.unmount();

    serve({ recent: [recentAttempt()] });
    api.codeReviewResult.mockResolvedValue(attempt());
    api.submitCodeReview.mockResolvedValue(submitted());
    const second = show();
    await openRecent(second.user);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(commentTexts()).toEqual(['Not atomic.', 'No tests.']);
    expect(within(screen.getByRole('listitem', { name: 'Comment 1' })).getByText('Major')).toBeVisible();
    expect(screen.getByText('1 file changed')).toBeVisible();

    await second.user.click(screen.getByRole('button', { name: 'Submit review' }));
    expect(api.submitCodeReview).toHaveBeenCalledWith(
      'attempt-1',
      [
        { span: { path: 'src/limiter.py', start: 2, end: 3 }, text: 'Not atomic.', severity: 'major' },
        { span: null, text: 'No tests.', severity: null },
      ],
      [{ path: 'src/limiter.py', text: 'fixed\n' }],
      expect.any(Number),
    );
  });

  it('forgets what was written once it is handed in', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    api.submitCodeReview.mockResolvedValue(submitted());
    const first = show();
    await startDrill(first.user);
    await comment(first.user, 'Handed in.');
    await first.user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    first.unmount();
    expect(localStorage.getItem('lattice.drafts') ?? '').not.toContain('Handed in.');
  });
});

describe('naming the lesson and leaving a drill', () => {
  it('names the lesson, the material and the mode on the review screen', async () => {
    api.startCourseDrill.mockResolvedValue(started({ drill: LESSON_DRILL }));
    const { user } = show();
    await startDrill(user);
    expect(screen.getByText('Spacing · Python · Practice')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'A rate limiter', level: 2 })).toBeVisible();
  });

  it('names the lesson on the grading screen too', async () => {
    api.startCourseDrill.mockResolvedValue(started({ drill: LESSON_DRILL }));
    api.submitCodeReview.mockResolvedValue(submitted({ drill: LESSON_DRILL }));
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Something.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    expect(screen.getByRole('heading', { name: 'A rate limiter', level: 2 })).toBeVisible();
    expect(screen.getByText('Spacing · Python · Practice')).toBeVisible();
  });

  it('goes back to the choices from a drill that is being written', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await clickStart(user);
    await screen.findByText('Writing a drill for you, usually a minute or two');
    await user.click(screen.getByRole('button', { name: 'Review drills' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.queryByText('Writing a drill for you, usually a minute or two')).not.toBeInTheDocument();
  });

  it('goes back to the choices from a drill that failed to start', async () => {
    api.startCourseDrill.mockRejectedValueOnce(new Error('No model is loaded.'));
    const { user } = show();
    await clickStart(user);
    expect(await screen.findByRole('alert')).toHaveTextContent('No model is loaded.');
    await user.click(screen.getByRole('button', { name: 'Back to the choices' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('goes back to the choices while an attempt is being opened', async () => {
    serve({ recent: [recentAttempt()] });
    api.codeReviewResult.mockReturnValue(deferred<CodeReviewResultDto>().promise);
    const { user } = show();
    await openRecent(user);
    await screen.findByText('Opening your review…');
    await user.click(screen.getByRole('button', { name: 'Review drills' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
  });

  it('goes back to the choices from the grading', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    api.submitCodeReview.mockResolvedValue(submitted());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Something.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    await user.click(screen.getByRole('button', { name: 'Review drills' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.queryByText('Grading your review…')).not.toBeInTheDocument();
  });

  it('goes back from the reveal, to the choices', async () => {
    serve({ recent: [recentAttempt({ status: 'graded' })] });
    api.codeReviewResult.mockResolvedValue(graded());
    const { user } = show();
    await openRecent(user);
    await screen.findByText('Revealed attempt-1');
    await user.click(screen.getByRole('button', { name: 'Review drills' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.queryByTestId('reveal')).not.toBeInTheDocument();
  });

  it('lists a review left that way as one to continue, and reopens it with what was written', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const { user } = show();
    await startDrill(user);
    await comment(user, 'Not atomic.');
    serve({ recent: [recentAttempt()] });
    await user.click(screen.getByRole('button', { name: 'Review drills' }));

    const item = await screen.findByRole('button', { name: /A rate limiter/ });
    expect(within(item).getByText('Continue')).toBeVisible();
    api.codeReviewResult.mockResolvedValue(attempt());
    await user.click(item);
    await screen.findByRole('heading', { name: 'A rate limiter' });
    expect(commentTexts()).toEqual(['Not atomic.']);
  });
});

describe('the interview countdown', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-11-04T12:00:00Z'));
  });

  it('counts down the length the learner chose from the start of the attempt', async () => {
    api.startCourseDrill.mockResolvedValue(started({ mode: 'interview' }));
    const { user } = show();
    await user.click(await screen.findByRole('radio', { name: /Interview/ }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Time limit' }), { target: { value: '15' } });
    await clickStart(user);

    expect(await screen.findByRole('timer')).toHaveTextContent('Time left 15:00');
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'code', language: 'python' }, 'interview', 80, 4);
  });

  it('is not there when practising', async () => {
    api.startCourseDrill.mockResolvedValue(started());
    const { user } = show();
    await startDrill(user);
    expect(screen.queryByRole('timer')).not.toBeInTheDocument();
    expect(screen.queryByText(/Time is up/)).not.toBeInTheDocument();
  });

  it('keeps counting from the start when the attempt is reopened, in the length chosen for it', async () => {
    api.startCourseDrill.mockResolvedValue(started({ mode: 'interview' }));
    const first = show();
    await first.user.click(await screen.findByRole('radio', { name: /Interview/ }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Time limit' }), { target: { value: '15' } });
    await clickStart(first.user);
    await screen.findByRole('timer');
    first.unmount();

    vi.setSystemTime(new Date('2026-11-04T12:14:00Z'));
    api.codeReviewResult.mockResolvedValue(attempt({ mode: 'interview', startedAt: new Date('2026-11-04T12:00:00Z').getTime() }));
    serve({ recent: [recentAttempt({ mode: 'interview' })] });
    const second = show();
    await openRecent(second.user);
    expect(await screen.findByRole('timer')).toHaveTextContent('Time left 1:00');
  });

  it('says when time is up and leaves the hand-in to the learner', async () => {
    api.codeReviewResult.mockResolvedValue(attempt({ mode: 'interview', startedAt: Date.now() - 11 * 60_000 }));
    serve({ recent: [recentAttempt({ mode: 'interview' })] });
    const { user } = show();
    await openRecent(user);
    expect((await screen.findByText(/Time is up/)).closest('[role="status"]')).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Submit review' })).toBeEnabled();
    expect(api.submitCodeReview).not.toHaveBeenCalled();
  });
});

describe('an explanation of the lesson’s sources', () => {
  const SOURCE_TEXT = 'Students who spaced their study recalled 14% more a week later.';
  const EXPLANATION = 'Spacing helps recall.\nIt helped by 40% a week later.\nThe benefit was larger for known material.\n';
  const PASSAGE_DRILL = {
    title: 'Why spacing helps',
    material: { kind: 'passage', sourceTitle: 'Spacing and memory' } as const,
    files: [{ path: 'explanation.md', text: EXPLANATION }],
    lesson: null,
  };

  async function choosePassage(user: User) {
    await user.click(await screen.findByRole('radio', { name: /An explanation of the lesson’s sources/ }));
    await screen.findByRole('option', { name: 'Next: Sleep' });
  }

  it('offers code and an explanation of the lesson’s sources, with code chosen and a language asked for', async () => {
    show();
    expect(await screen.findByRole('radio', { name: /^Code/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /An explanation of the lesson’s sources/ })).not.toBeChecked();
    expect(screen.getByRole('combobox', { name: 'Language' })).toBeVisible();
  });

  it('asks for no language, and sizes in sentences', async () => {
    const { user } = show();
    await choosePassage(user);
    expect(screen.queryByRole('combobox', { name: 'Language' })).not.toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Short.*About 6 sentences, 2 errors/ })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Standard.*About 10 sentences, 3 errors/ })).toBeChecked();
  });

  it('starts on the passage with no language, remembers the material, and leaves the course’s language alone', async () => {
    api.startCourseDrill.mockResolvedValue(started({ drill: PASSAGE_DRILL }));
    const { user } = show();
    await choosePassage(user);
    await clickStart(user);

    expect(await screen.findByRole('heading', { name: 'Why spacing helps' })).toBeVisible();
    expect(api.startCourseDrill).toHaveBeenCalledWith('course-1', null, { kind: 'passage' }, 'practice', 10, 3);
    expect(JSON.parse(localStorage.getItem('lattice.drills.choices') ?? '{}')).toMatchObject({ material: 'passage' });
    expect(localStorage.getItem('lattice.drills.language.course-1')).toBeNull();
  });

  it('waits for the build of the course’s passages, not of a language or another course', async () => {
    api.startCourseDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started({ drill: PASSAGE_DRILL }));
    const { user } = show();
    await choosePassage(user);
    await clickStart(user);
    expect(await screen.findByText(/keeps the next drill of this kind ready for the course/)).toBeVisible();

    report(buildJob({ id: 'job-python', subjectId: CODE_SHELF, status: 'completed', finishedAt: 9 }));
    report(buildJob({ id: 'job-other', subjectId: 'drill:course-2:passage', status: 'completed', finishedAt: 9 }));
    expect(api.startCourseDrill).toHaveBeenCalledTimes(1);
    report(buildJob({ id: 'job-next', subjectId: PASSAGE_SHELF, status: 'completed', finishedAt: 9 }));
    expect(await screen.findByRole('heading', { name: 'Why spacing helps' })).toBeVisible();
  });

  describe('reviewing the explanation', () => {
    beforeEach(() => {
      api.startCourseDrill.mockResolvedValue(started({ drill: PASSAGE_DRILL }));
      api.submitCodeReview.mockResolvedValue(submitted({ drill: PASSAGE_DRILL }));
    });

    async function review(user: User) {
      await choosePassage(user);
      await clickStart(user);
      await screen.findByRole('heading', { name: 'Why spacing helps' });
    }

    it('shows the explanation as prose and nothing of the source', async () => {
      const { user } = show();
      await review(user);
      expect(screen.getByRole('group', { name: 'The explanation, sentence by sentence' })).toBeVisible();
      expect(screen.getByRole('button', { name: /Sentence 2: .*It helped by 40% a week later\./ })).toBeVisible();
      expect(screen.getByText('Explanation of Spacing and memory · Practice')).toBeVisible();
      expect(screen.queryByTestId('code-viewer')).not.toBeInTheDocument();
      expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
      expect(document.body).not.toHaveTextContent(SOURCE_TEXT);
      expect(screen.getByText('A general comment. Click a sentence in the explanation to put it on sentences.')).toBeVisible();
    });

    it('puts a comment on the sentences picked, named as sentences, and hands them in by line', async () => {
      const { user } = show();
      await review(user);
      await user.click(screen.getByRole('button', { name: /^Sentence 2:/ }));
      expect(screen.getByText('Sentence 2')).toBeVisible();
      fireEvent.click(screen.getByRole('button', { name: /^Sentence 3:/ }), { shiftKey: true });
      expect(screen.getByText('Sentences 2–3')).toBeVisible();
      await comment(user, 'The figure is 14%, not 40%.');
      expect(screen.getByRole('button', { name: /Sentences 2–3/ })).toBeVisible();
      await user.click(screen.getByRole('button', { name: 'Submit review' }));
      expect(api.submitCodeReview).toHaveBeenCalledWith(
        'attempt-1',
        [{ span: { path: 'explanation.md', start: 2, end: 3 }, text: 'The figure is 14%, not 40%.', severity: null }],
        null,
        expect.any(Number),
      );
    });

    it('clears the sentences picked', async () => {
      const { user } = show();
      await review(user);
      await user.click(screen.getByRole('button', { name: /^Sentence 1:/ }));
      await user.click(screen.getByRole('button', { name: 'Clear the selected sentences' }));
      expect(screen.queryByText('Sentence 1')).not.toBeInTheDocument();
      expect(screen.getByRole('button', { name: /^Sentence 1:/ })).toHaveAttribute('aria-pressed', 'false');
    });

    it('writes the fix in a plain editor of the explanation and hands in the rewrite', async () => {
      const { user } = show();
      await review(user);
      await comment(user, 'The figure is wrong.');
      await user.click(screen.getByRole('button', { name: 'Write a fix (optional)' }));
      const editor = screen.getByRole('textbox', { name: 'Edit the explanation' });
      expect(editor).toHaveValue(EXPLANATION);
      fireEvent.change(editor, { target: { value: EXPLANATION.replace('40%', '14%') } });
      expect(screen.getByText('Explanation changed')).toBeVisible();
      await user.click(screen.getByRole('button', { name: 'Back to the explanation' }));
      expect(screen.getByRole('group', { name: 'The explanation, sentence by sentence' })).toBeVisible();
      await user.click(screen.getByRole('button', { name: 'Submit review' }));
      expect(api.submitCodeReview).toHaveBeenCalledWith(
        'attempt-1',
        expect.any(Array),
        [{ path: 'explanation.md', text: EXPLANATION.replace('40%', '14%') }],
        expect.any(Number),
      );
    });

    it('starts the next drill on a passage after the reveal, with no lesson named', async () => {
      api.codeReviewResult.mockResolvedValue(graded({ drill: PASSAGE_DRILL }));
      api.startCourseDrill.mockResolvedValueOnce(started({ drill: PASSAGE_DRILL })).mockResolvedValueOnce(started({ attemptId: 'attempt-2', drill: PASSAGE_DRILL }));
      const { user } = show();
      await choosePassage(user);
      await user.selectOptions(screen.getByRole('combobox', { name: 'Lesson' }), 'lesson-1');
      await clickStart(user);
      await screen.findByRole('heading', { name: 'Why spacing helps' });
      expect(api.startCourseDrill).toHaveBeenLastCalledWith('course-1', 'lesson-1', { kind: 'passage' }, 'practice', 10, 3);
      await comment(user, 'The figure is wrong.');
      await user.click(screen.getByRole('button', { name: 'Submit review' }));
      await screen.findByText('Grading your review…');
      report(gradingJob({ status: 'completed', finishedAt: 9 }));
      await user.click(await screen.findByRole('button', { name: 'Next drill' }));
      await waitFor(() => expect(api.startCourseDrill).toHaveBeenCalledTimes(2));
      expect(api.startCourseDrill).toHaveBeenLastCalledWith('course-1', null, { kind: 'passage' }, 'practice', 10, 3);
    });
  });
});
