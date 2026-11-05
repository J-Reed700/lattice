import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CodeReviewResultDto, JobDto, StartedCodeReviewDto } from '@/lib/bindings';
import { deferred } from '@/tests/deferred';
import { jobFixture } from '@/tests/fixtures/jobs';

import { CodeReviewDrillScreen } from './CodeReviewDrillScreen';

const api = vi.hoisted(() => ({
  codeReviewLanguages: vi.fn(),
  startCodeReviewDrill: vi.fn(),
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
const DRILL = { title: 'A rate limiter', language: 'python', files: FILES };

function started(overrides: Partial<Extract<StartedCodeReviewDto, { status: 'started' }>> = {}): StartedCodeReviewDto {
  return { status: 'started', attemptId: 'attempt-1', mode: 'practice', startedAt: Date.now(), drill: DRILL, ...overrides };
}

const REVEAL: CodeReviewResultDto['reveal'] = {
  issues: [],
  findings: [],
  score: { found: 0, halfFound: 0, missed: 0, validExtras: 0, falsePositives: 0, bySeverity: [], weightedRecall: 0, precision: null, ledWithTheMostSevere: false },
  ratings: [],
  fix: null,
  cleanFiles: [],
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

const buildJob = (overrides: Partial<JobDto>) => jobFixture({ id: 'job-9', kind: 'learning.drill_build', subjectId: 'python', ...overrides });
const gradingJob = (overrides: Partial<JobDto>) => jobFixture({ id: 'job-g', kind: 'learning.drill_grading', subjectId: 'attempt-1', ...overrides });
function report(job: JobDto) {
  act(() => {
    for (const handler of [...events.handlers]) handler({ payload: job });
  });
}

function Where() {
  return <span data-testid="search">{useLocation().search}</span>;
}
const search = () => screen.getByTestId('search').textContent;
const addressIs = (expected: string) => waitFor(() => expect(search()).toBe(expected));

function show(path = '/studio?drills=open') {
  const onBack = vi.fn();
  const router = createMemoryRouter(
    [{ path: '/studio', element: <><CodeReviewDrillScreen onBack={onBack} /><Where /></> }],
    { initialEntries: [path] },
  );
  const rendered = render(<RouterProvider router={router} />);
  return { ...rendered, router, onBack, user: userEvent.setup() };
}
type User = ReturnType<typeof userEvent.setup>;

const COMMENT = 'What is wrong, why it matters and what to do';

async function startDrill(user: User) {
  await user.click(await screen.findByRole('button', { name: 'Start drill' }));
  await screen.findByRole('heading', { name: 'A rate limiter' });
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
});

afterEach(() => {
  vi.useRealTimers();
});

describe('choosing a drill', () => {
  it('lists the languages the command gives, named for reading, with Python chosen', async () => {
    api.codeReviewLanguages.mockResolvedValue(['python', 'csharp', 'sql', 'zig']);
    show();
    await screen.findByRole('option', { name: 'C#' });
    const select = screen.getByRole('combobox', { name: 'Language' });
    expect(within(select).getAllByRole('option').map((option) => option.textContent)).toEqual(['Python', 'C#', 'SQL', 'zig']);
    expect(select).toHaveValue('python');
    expect(api.codeReviewLanguages).toHaveBeenCalledTimes(1);
  });

  it('offers the choices last used', async () => {
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ language: 'sql', mode: 'interview', size: 'long', minutes: 25 }));
    show();
    expect(await screen.findByRole('combobox', { name: 'Language' })).toHaveValue('sql');
    expect(screen.getByRole('radio', { name: /Interview/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Long/ })).toBeChecked();
    expect(screen.getByRole('spinbutton', { name: 'Time limit' })).toHaveValue(25);
  });

  it('moves to Python when the language last used is not one drills are written in', async () => {
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ language: 'cobol' }));
    show();
    const select = await screen.findByRole('combobox', { name: 'Language' });
    await waitFor(() => expect(select).toHaveValue('python'));
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
    expect(screen.getByRole('button', { name: 'Start drill' })).toBeEnabled();
  });

  it('shows the backend message when the languages cannot be read, and reads them again', async () => {
    api.codeReviewLanguages.mockRejectedValueOnce(new Error('The catalogue is unavailable.')).mockResolvedValue(['python', 'rust']);
    const { user } = show();
    expect(await screen.findByRole('alert')).toHaveTextContent('The catalogue is unavailable.');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('option', { name: 'Rust' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('goes back to the studio', async () => {
    const { user, onBack } = show();
    await user.click(await screen.findByRole('button', { name: 'Studio' }));
    expect(onBack).toHaveBeenCalledOnce();
  });
});

describe('starting a drill', () => {
  it('opens the review of a drill that is ready, in the size and mode chosen, and remembers the choices', async () => {
    api.startCodeReviewDrill.mockResolvedValue(started());
    const { user } = show();
    await screen.findByRole('option', { name: 'C#' });
    await user.selectOptions(screen.getByRole('combobox', { name: 'Language' }), 'csharp');
    await user.click(screen.getByRole('radio', { name: /Short/ }));
    await user.click(screen.getByRole('button', { name: 'Start drill' }));

    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCodeReviewDrill).toHaveBeenCalledWith('csharp', 'practice', 40, 3);
    expect(screen.getByRole('tab', { name: 'src/limiter.py' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/limiter.py');
    await addressIs('?drills=open&attempt=attempt-1');
    expect(JSON.parse(localStorage.getItem('lattice.drills.choices') ?? '{}')).toMatchObject({ language: 'csharp', mode: 'practice', size: 'short' });
  });

  it('says a drill is being written when none is ready, shows the job, and opens the review when the build completes', async () => {
    api.startCodeReviewDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started());
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));

    expect(await screen.findByText('Writing a drill for you, usually a minute or two')).toBeVisible();
    expect(screen.getByText(/keeps one drill ready for each language, so later starts are instant/)).toBeVisible();
    report(buildJob({ progressMessage: 'Planting the defects (3 of 6)', progressCurrent: 2, progressTotal: 6 }));
    expect(await screen.findByText('Planting the defects (3 of 6)')).toBeVisible();
    expect(screen.getByRole('progressbar', { name: 'Drill build progress' })).toHaveAttribute('aria-valuenow', '33');
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(1);

    report(buildJob({ status: 'completed', finishedAt: 9 }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(2);
    await addressIs('?drills=open&attempt=attempt-1');
  });

  it('starts again when any build of the language completes, and ignores the builds of other languages', async () => {
    api.startCodeReviewDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started());
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));
    await screen.findByText('Writing a drill for you, usually a minute or two');

    report(buildJob({ id: 'job-rust', subjectId: 'rust', status: 'completed', finishedAt: 9 }));
    report(buildJob({ id: 'job-other', kind: 'learning.drill_grading', status: 'completed', finishedAt: 9 }));
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(1);

    report(buildJob({ id: 'job-next', status: 'completed', finishedAt: 9 }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('shows a failed build\'s error, asks again only when told to, and then opens the drill', async () => {
    api.startCodeReviewDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' }).mockResolvedValueOnce(started());
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));
    await screen.findByText('Writing a drill for you, usually a minute or two');

    report(buildJob({ status: 'failed', error: 'The model is not loaded.', finishedAt: 9 }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The model is not loaded.');
    expect(screen.queryByText('Writing a drill for you, usually a minute or two')).not.toBeInTheDocument();
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(2);
  });

  it.each([
    ['cancelled', 'The drill build was cancelled.'],
    ['failed', 'The drill could not be built.'],
    ['interrupted', 'The drill could not be built.'],
  ] as const)('says what happened to a build that ends as %s and gives no reason', async (status, message) => {
    api.startCodeReviewDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));
    await screen.findByText('Writing a drill for you, usually a minute or two');
    report(buildJob({ status, finishedAt: 9 }));
    expect(await screen.findByRole('alert')).toHaveTextContent(message);
  });

  it('shows the error of a build that tries again, and goes on waiting', async () => {
    api.startCodeReviewDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));
    await screen.findByText('Writing a drill for you, usually a minute or two');
    report(buildJob({ status: 'pending', error: 'The model disconnected.' }));
    expect(await screen.findByText('The model disconnected.')).toBeVisible();
    expect(screen.getByText('The last try failed, so it is trying again.')).toBeVisible();
    expect(screen.getByText('Writing a drill for you, usually a minute or two')).toBeVisible();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows the backend message when a drill cannot be started, and asks again on request', async () => {
    api.startCodeReviewDrill.mockRejectedValueOnce(new Error('python is not a language drills are written in.')).mockResolvedValueOnce(started());
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('python is not a language drills are written in.');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('stops waiting for a build and goes back to the choices, which are kept', async () => {
    api.startCodeReviewDrill.mockResolvedValueOnce({ status: 'building', jobId: 'job-9' });
    const { user } = show();
    await user.click(await screen.findByRole('radio', { name: /Long/ }));
    await user.click(screen.getByRole('button', { name: 'Start drill' }));
    await user.click(await screen.findByRole('button', { name: 'Stop waiting' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    expect(screen.getByRole('radio', { name: /Long/ })).toBeChecked();
    report(buildJob({ status: 'completed', finishedAt: 9 }));
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(1);
  });

  it('takes one drill, however many reports of a finished build there are while it asks', async () => {
    const first = deferred<StartedCodeReviewDto>();
    api.startCodeReviewDrill.mockReturnValueOnce(first.promise).mockResolvedValueOnce(started());
    const { user } = show();
    await user.click(await screen.findByRole('button', { name: 'Start drill' }));
    report(buildJob({ status: 'completed', finishedAt: 9 }));
    report(buildJob({ status: 'completed', finishedAt: 9 }));
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(1);
    await act(async () => {
      first.resolve({ status: 'building', jobId: 'job-9' });
    });
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(2);
  });
});

describe('writing the review', () => {
  beforeEach(() => {
    api.startCodeReviewDrill.mockResolvedValue(started());
    api.submitCodeReview.mockResolvedValue(submitted());
  });

  it('shows the drill\'s files as tabs over a read-only view', async () => {
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
    api.startCodeReviewDrill.mockResolvedValue(started());
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
    api.startCodeReviewDrill.mockResolvedValue(started());
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
    api.startCodeReviewDrill.mockResolvedValue(started({ startedAt: Date.now() - 125_000 }));
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
    const { user } = show('/studio?drills=open&attempt=attempt-1');
    await user.click(await screen.findByRole('button', { name: 'Grade again from the reveal' }));
    await waitFor(() => expect(screen.getByTestId('reveal')).toHaveAttribute('data-regrading', 'true'));
    expect(api.regradeCodeReview).toHaveBeenCalledWith('attempt-1');

    await act(async () => {
      pending.reject(new Error('Only a review that could not be graded can be graded again.'));
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('Only a review that could not be graded can be graded again.');
    expect(screen.getByTestId('reveal')).toHaveAttribute('data-regrading', 'false');
  });

  it('starts the next drill in the same language, mode and size from the reveal', async () => {
    localStorage.setItem('lattice.drills.choices', JSON.stringify({ language: 'rust', mode: 'interview', size: 'long', minutes: 20 }));
    api.codeReviewLanguages.mockResolvedValue(['python', 'rust']);
    api.startCodeReviewDrill.mockResolvedValueOnce(started({ mode: 'interview', drill: { ...DRILL, language: 'rust' } })).mockResolvedValueOnce(started({ attemptId: 'attempt-2' }));
    api.submitCodeReview.mockResolvedValue(submitted({ mode: 'interview', drill: { ...DRILL, language: 'rust' } }));
    api.codeReviewResult.mockResolvedValue(graded({ mode: 'interview', drill: { ...DRILL, language: 'rust' } }));
    const { user } = show();
    await startDrill(user);
    expect(api.startCodeReviewDrill).toHaveBeenLastCalledWith('rust', 'interview', 140, 5);
    await comment(user, 'Something.');
    await user.click(screen.getByRole('button', { name: 'Submit review' }));
    await screen.findByText('Grading your review…');
    report(gradingJob({ status: 'completed', finishedAt: 9 }));
    await user.click(await screen.findByRole('button', { name: 'Next drill' }));

    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.startCodeReviewDrill).toHaveBeenCalledTimes(2);
    expect(api.startCodeReviewDrill).toHaveBeenLastCalledWith('rust', 'interview', 140, 5);
    await addressIs('?drills=open&attempt=attempt-2');
    expect(screen.queryByText('Revealed attempt-1')).not.toBeInTheDocument();
  });
});

describe('an attempt in the address', () => {
  it('is read again from the backend and reopens its review with its drill', async () => {
    api.codeReviewResult.mockResolvedValue(attempt({ attemptId: 'attempt-7' }));
    show('/studio?drills=open&attempt=attempt-7');
    expect(screen.getByText('Opening your review…')).toBeVisible();
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.codeReviewResult).toHaveBeenCalledWith('attempt-7');
    expect(api.startCodeReviewDrill).not.toHaveBeenCalled();
    expect(screen.getByTestId('code-viewer')).toHaveAttribute('data-path', 'src/limiter.py');
    expect(search()).toBe('?drills=open&attempt=attempt-7');
  });

  it('is shown waiting for its grade when it was handed in', async () => {
    api.codeReviewResult.mockResolvedValue(submitted());
    show('/studio?drills=open&attempt=attempt-1');
    expect(await screen.findByText('Grading your review…')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('is shown as its reveal when it was graded', async () => {
    api.codeReviewResult.mockResolvedValue(graded());
    show('/studio?drills=open&attempt=attempt-1');
    expect(await screen.findByText('Revealed attempt-1')).toBeVisible();
  });

  it('is shown with the reason it could not be graded', async () => {
    api.codeReviewResult.mockResolvedValue(submitted({ status: 'couldnt_grade', error: 'The model was not loaded.' }));
    show('/studio?drills=open&attempt=attempt-1');
    expect(await screen.findByText('The model was not loaded.')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Grade again' })).toBeEnabled();
  });

  it('shows the backend message when it cannot be read, and reads it again on request', async () => {
    api.codeReviewResult.mockRejectedValueOnce(new Error('This review is not there.')).mockResolvedValueOnce(attempt());
    const { user } = show('/studio?drills=open&attempt=attempt-1');
    expect(await screen.findByRole('alert')).toHaveTextContent('This review is not there.');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
  });

  it('can be left for a new drill when it is not there', async () => {
    api.codeReviewResult.mockRejectedValue(new Error('This review is not there.'));
    const { user } = show('/studio?drills=open&attempt=gone');
    await user.click(await screen.findByRole('button', { name: 'Start a new drill' }));
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();
    await addressIs('?drills=open');
  });

  it('is left when the address stops naming it, and read again when it names one again', async () => {
    api.codeReviewResult.mockResolvedValue(attempt());
    const { router } = show('/studio?drills=open&attempt=attempt-1');
    await screen.findByRole('heading', { name: 'A rate limiter' });

    await act(async () => {
      await router.navigate('/studio?drills=open');
    });
    expect(await screen.findByRole('button', { name: 'Start drill' })).toBeVisible();

    await act(async () => {
      await router.navigate('/studio?drills=open&attempt=attempt-1');
    });
    expect(await screen.findByRole('heading', { name: 'A rate limiter' })).toBeVisible();
    expect(api.codeReviewResult).toHaveBeenCalledTimes(2);
  });

  it('brings back the comments and the fix written before the learner left', async () => {
    api.startCodeReviewDrill.mockResolvedValue(started());
    const first = show();
    await startDrill(first.user);
    await first.user.click(screen.getByRole('button', { name: 'Pick lines 2 to 3' }));
    await comment(first.user, 'Not atomic.', 'major');
    await comment(first.user, 'No tests.');
    await first.user.click(screen.getByRole('button', { name: 'Write a fix (optional)' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Edit src/limiter.py' }), { target: { value: 'fixed\n' } });
    first.unmount();

    api.codeReviewResult.mockResolvedValue(attempt());
    api.submitCodeReview.mockResolvedValue(submitted());
    const second = show('/studio?drills=open&attempt=attempt-1');
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
    api.startCodeReviewDrill.mockResolvedValue(started());
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

describe('the interview countdown', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-11-04T12:00:00Z'));
  });

  it('counts down the length the learner chose from the start of the attempt', async () => {
    api.startCodeReviewDrill.mockResolvedValue(started({ mode: 'interview' }));
    const { user } = show();
    await user.click(await screen.findByRole('radio', { name: /Interview/ }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Time limit' }), { target: { value: '15' } });
    await user.click(screen.getByRole('button', { name: 'Start drill' }));

    expect(await screen.findByRole('timer')).toHaveTextContent('Time left 15:00');
    expect(api.startCodeReviewDrill).toHaveBeenCalledWith('python', 'interview', 80, 4);
  });

  it('is not there when practising', async () => {
    api.startCodeReviewDrill.mockResolvedValue(started());
    const { user } = show();
    await startDrill(user);
    expect(screen.queryByRole('timer')).not.toBeInTheDocument();
    expect(screen.queryByText(/Time is up/)).not.toBeInTheDocument();
  });

  it('keeps counting from the start when the attempt is reopened, in the length chosen for it', async () => {
    api.startCodeReviewDrill.mockResolvedValue(started({ mode: 'interview' }));
    const first = show();
    await first.user.click(await screen.findByRole('radio', { name: /Interview/ }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Time limit' }), { target: { value: '15' } });
    await first.user.click(screen.getByRole('button', { name: 'Start drill' }));
    await screen.findByRole('timer');
    first.unmount();

    vi.setSystemTime(new Date('2026-11-04T12:14:00Z'));
    api.codeReviewResult.mockResolvedValue(attempt({ mode: 'interview', startedAt: new Date('2026-11-04T12:00:00Z').getTime() }));
    show('/studio?drills=open&attempt=attempt-1');
    expect(await screen.findByRole('timer')).toHaveTextContent('Time left 1:00');
  });

  it('says when time is up and leaves the hand-in to the learner', async () => {
    api.codeReviewResult.mockResolvedValue(attempt({ mode: 'interview', startedAt: Date.now() - 11 * 60_000 }));
    show('/studio?drills=open&attempt=attempt-1');
    expect((await screen.findByText(/Time is up/)).closest('[role="status"]')).not.toBeNull();
    expect(screen.getByRole('button', { name: 'Submit review' })).toBeEnabled();
    expect(api.submitCodeReview).not.toHaveBeenCalled();
  });
});
