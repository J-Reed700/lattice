import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { programKind } from '..';
import { programApi } from '../programApi';
import { caseView, programDebrief, programRun, programSession } from './fixtures';
import { PracticeSession } from '../../../session/PracticeSession';
import { sessionApi } from '../../../session/sessionApi';

vi.mock('@/lib/api', () => {
  let count = 0;
  return { newId: () => `id-${++count}`, unwrap: <T,>(value: T) => value, default: {}, VaultAPI: {} };
});

vi.mock('../../../session/sessionApi', () => ({
  sessionApi: {
    getSession: vi.fn(),
    moveStage: vi.fn(),
    check: vi.fn(),
    askHint: vi.fn(),
    sendTurn: vi.fn(),
    respond: vi.fn(),
    observeDraft: vi.fn(),
    setClock: vi.fn(),
    setSetting: vi.fn(),
    revealSolution: vi.fn(),
    finish: vi.fn(),
  },
}));

vi.mock('../programApi', () => ({
  programApi: {
    checkExample: vi.fn(),
    removeExample: vi.fn(),
    runInput: vi.fn(),
    activityRevision: vi.fn(),
  },
}));

vi.mock('../../../practical/LearningCodeEditor', () => ({
  LearningCodeEditor: ({
    value,
    onChange,
    readOnly,
    ariaLabel,
    onRunShortcut,
  }: {
    value: string;
    onChange: (value: string) => void;
    readOnly?: boolean;
    ariaLabel?: string;
    onRunShortcut?: () => void;
  }) => (
    <textarea
      aria-label={ariaLabel ?? 'Code'}
      value={value}
      readOnly={readOnly}
      onChange={(event) => onChange(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === 'Enter' && event.ctrlKey && !event.shiftKey) onRunShortcut?.();
      }}
    />
  ),
}));

vi.mock('../../../practical/useLearningLabDraft', async () => {
  const React = await import('react');
  return {
    useLearningLabDraft: ({ starterFiles }: { starterFiles: { path: string; content: string }[] }) => {
      const [files, setFiles] = React.useState(starterFiles);
      return {
        files,
        setFileContent: (path: string, content: string) =>
          setFiles((current) => current.map((file) => (file.path === path ? { ...file, content } : file))),
        loadState: 'ready',
        saveState: 'saved',
        error: null,
        flush: async () => true,
        retryLoad: async () => true,
        keepLocalEdits: async () => true,
        useSavedVersion: async () => true,
      };
    },
  };
});

vi.mock('../../../shared/PracticeThread', () => ({
  PracticeThread: ({
    busy,
    onSend,
  }: {
    busy?: string | null;
    onSend: (content: string, spoken: boolean) => Promise<void>;
  }) => (
    <div>
      {busy ? <p role="status">{busy}</p> : null}
      <button type="button" onClick={() => void onSend('What about an empty list?', false)}>
        Send
      </button>
    </div>
  ),
}));

const api = vi.mocked(sessionApi);
const program = vi.mocked(programApi);
const kinds = { program: programKind };

const failingHidden = caseView({
  caseId: 'hidden-1',
  category: 'an input with duplicates',
  visibility: 'hidden',
  status: 'failed',
  args: null,
  expected: null,
  actual: null,
});
const passing = caseView({ caseId: 'c1', category: 'a pair at the ends' });
const passingToo = caseView({ caseId: 'c2', category: 'an empty list', status: 'passed' });

function renderSession(session: Wire.PracticeSessionDto) {
  api.getSession.mockResolvedValue(session);
  return render(
    <PracticeSession programId="program-1" sessionId={session.sessionId} kinds={kinds} onExit={vi.fn()} onTryInterview={vi.fn()} />,
  );
}

async function editor() {
  return screen.findByRole('textbox', { name: /code/i });
}

describe('PracticeSession with the program kind', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    program.activityRevision.mockResolvedValue(2);
    api.setClock.mockResolvedValue({ activeMs: 60_000, openSince: 1_700_000_000_000, budgetSeconds: null });
    api.respond.mockImplementation(async () => programSession({ cuePending: false }));
  });

  it('reads the activity revision before it opens, and not for a finished session', async () => {
    const open = renderSession(programSession());
    await editor();
    expect(program.activityRevision).toHaveBeenCalledWith('program-1', 'activity-1');
    open.unmount();

    program.activityRevision.mockClear();
    renderSession(programSession({ finished: true, debrief: programDebrief() }));
    expect(await screen.findByRole('heading', { name: /Debrief/ })).toBeInTheDocument();
    expect(program.activityRevision).not.toHaveBeenCalled();
  });

  it('names the panels Problem and Code and says it is reading the code while a reply is pending', async () => {
    api.sendTurn.mockResolvedValue(programSession({ cuePending: true }));
    api.respond.mockImplementation(() => new Promise(() => undefined));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'make' }));

    expect(await screen.findAllByRole('tab')).toHaveLength(3);
    expect(screen.getAllByRole('tab').map((tab) => tab.textContent)).toEqual(['Problem', 'Code', 'Coach']);
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Reading your code…')).toBeInTheDocument();
  });

  it('keeps the editor, runs and examples usable while a reply is pending', async () => {
    api.sendTurn.mockResolvedValue(programSession({ cuePending: true }));
    api.respond.mockImplementation(() => new Promise(() => undefined));
    api.check.mockResolvedValue(programSession({}, { runs: [programRun([passing])] }));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'make' }));

    await user.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Reading your code…')).toBeInTheDocument();

    const box = await editor();
    expect(box).not.toHaveAttribute('readonly');
    fireEvent.change(box, { target: { value: 'def find_pair(numbers, target):\n    return []\n' } });
    expect(box).toHaveValue('def find_pair(numbers, target):\n    return []\n');

    const runButton = screen.getByRole('button', { name: /Run examples/ });
    expect(runButton).toBeEnabled();
    await user.click(runButton);
    expect(api.check).toHaveBeenCalledWith(expect.objectContaining({ scope: 'shown' }));
    expect(screen.getByLabelText('numbers')).toBeEnabled();
  });

  it('runs the examples on Ctrl+Enter and shows the shortcuts', async () => {
    api.check.mockResolvedValue(programSession({}, { runs: [programRun([passing])] }));
    renderSession(programSession({ stage: 'make' }));
    const box = await editor();

    expect(screen.getByRole('button', { name: /Run examples/ })).toHaveTextContent('Ctrl+Enter');
    expect(screen.getByRole('button', { name: /Submit/ })).toHaveTextContent('Ctrl+Shift+Enter');

    fireEvent.keyDown(box, { key: 'Enter', ctrlKey: true });
    await waitFor(() =>
      expect(api.check).toHaveBeenCalledWith(
        expect.objectContaining({
          programId: 'program-1',
          sessionId: 'session-1',
          scope: 'shown',
          piece: [expect.objectContaining({ path: 'solution.py' })],
        }),
      ),
    );
  });

  it('checks every case on Ctrl+Shift+Enter and on Submit', async () => {
    api.check.mockResolvedValue(programSession({ stage: 'make' }));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'make' }));
    const box = await editor();

    fireEvent.keyDown(box, { key: 'Enter', ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(api.check).toHaveBeenCalledWith(expect.objectContaining({ scope: 'all' })));
    await waitFor(() => expect(screen.getByRole('button', { name: /Submit/ })).toBeEnabled());

    api.check.mockClear();
    await user.click(screen.getByRole('button', { name: /Submit/ }));
    expect(api.check).toHaveBeenCalledWith(expect.objectContaining({ scope: 'all' }));
  });

  it('lists failures first and announces the summary', async () => {
    renderSession(
      programSession({ stage: 'check' }, { runs: [programRun([passing, passingToo, failingHidden])] }),
    );

    expect((await screen.findAllByText(/2 of 3 passed\. Failing: an input with duplicates/)).length).toBeGreaterThan(0);
    const live = screen.getAllByRole('status').find((node) => /2 of 3 passed/.test(node.textContent ?? ''));
    expect(live).toBeDefined();

    const failingMatches = screen.getAllByText(/an input with duplicates/);
    const failing = failingMatches[failingMatches.length - 1];
    const passingMatches = screen.getAllByText(/a pair at the ends/);
    const firstPassing = passingMatches[passingMatches.length - 1];
    expect(failing.compareDocumentPosition(firstPassing) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('shows a failing hidden case by its category only in Mock, and in full in Coached', async () => {
    const mock = renderSession(
      programSession(
        { setting: 'mock', settingsUsed: ['mock'], stage: 'check' },
        { runs: [programRun([failingHidden])] },
      ),
    );
    expect((await screen.findAllByText(/an input with duplicates/)).length).toBeGreaterThan(0);
    expect(screen.queryByText('[2, 2]')).not.toBeInTheDocument();
    expect(screen.queryByText('[0, 1]')).not.toBeInTheDocument();
    mock.unmount();

    renderSession(
      programSession(
        { stage: 'check' },
        { runs: [programRun([{ ...failingHidden, args: ['[2, 2]', '4'], expected: '[0, 1]', actual: '[]' }])] },
      ),
    );
    expect((await screen.findAllByText(/\[2, 2\]/)).length).toBeGreaterThan(0);
  });

  it('shows the reference answer and what the program printed for the last run', async () => {
    renderSession(
      programSession(
        { stage: 'check' },
        { runs: [programRun([passing], { scope: 'input', referenceActual: '[0, 2]', stdout: 'hello\n', stderr: '' })] },
      ),
    );
    expect(await screen.findByText(/The reference returns/)).toHaveTextContent('[0, 2]');
    expect(screen.getByText('What your program printed')).toBeInTheDocument();
    expect(screen.getByText('Your input')).toBeInTheDocument();
  });

  it('points a line hint at the lines work.hintMarks holds for its ordinal', async () => {
    const hint: Wire.PracticeHintDto = {
      ordinal: 1,
      rung: 'point',
      text: 'Look where you update the map.',
      reason: 'The update comes too late.',
      stage: 'make',
      checkId: 'run-1',
      inRole: false,
      at: 1,
    };
    renderSession(
      programSession(
        { stage: 'make', hints: [hint], nextRung: 'point' },
        { hintMarks: [{ ordinal: 1, path: 'solution.py', lines: [3, 5] }] },
      ),
    );
    expect(await screen.findByText('Look at lines 3, 5 of solution.py')).toBeInTheDocument();
    expect(screen.getByText('Point at the line')).toBeInTheDocument();
  });

  it('runs one input of the learner\'s choosing through the kind\'s own command, and repeats a failed run under its ids', async () => {
    program.runInput
      .mockRejectedValueOnce(new Error('timed out'))
      .mockResolvedValue(programSession({}, { runs: [programRun([passing], { scope: 'input' })] }));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'make' }));
    await editor();

    await user.click(screen.getByRole('button', { name: 'Try an input' }));
    const form = screen.getByRole('form', { name: 'Try an input' });
    await user.type(within(form).getByLabelText('numbers'), '[[1, 2]');
    await user.type(within(form).getByLabelText('target'), '3');
    await user.click(within(form).getByRole('button', { name: 'Run input' }));

    expect(program.runInput).toHaveBeenCalledWith(
      expect.objectContaining({
        programId: 'program-1',
        sessionId: 'session-1',
        input: ['[1, 2]', '3'],
        files: [expect.objectContaining({ path: 'solution.py' })],
      }),
    );
    await user.click(await screen.findByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(program.runInput).toHaveBeenCalledTimes(2));
    const [first, second] = program.runInput.mock.calls.map(([request]) => request);
    expect(second.operationId).toBe(first.operationId);
    expect(second.runId).toBe(first.runId);
  });

  it('checks a learner\'s example through the kind\'s own command', async () => {
    program.checkExample.mockResolvedValue(programSession({ stage: 'examples' }));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'examples' }));
    await editor();

    await user.type(screen.getByLabelText('numbers'), '[[3, 3]');
    await user.type(screen.getByLabelText('target'), '6');
    await user.type(screen.getByLabelText('expected'), '[[0, 1]');
    await user.click(screen.getByRole('button', { name: 'Check' }));

    expect(program.checkExample).toHaveBeenCalledWith(
      expect.objectContaining({
        programId: 'program-1',
        sessionId: 'session-1',
        ordinal: null,
        args: ['[3, 3]', '6'],
        expected: '[0, 1]',
      }),
    );
  });

  it('leaves the run buttons enabled and says nothing is running while an example is being checked', async () => {
    program.checkExample.mockImplementation(() => new Promise(() => undefined));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'examples' }));
    await editor();

    await user.type(screen.getByLabelText('numbers'), '[[3, 3]');
    await user.type(screen.getByLabelText('target'), '6');
    await user.type(screen.getByLabelText('expected'), '[[0, 1]');
    await user.click(screen.getByRole('button', { name: 'Check' }));

    await waitFor(() => expect(program.checkExample).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: /Run examples/ })).toBeEnabled();
    expect(screen.getByRole('button', { name: /Submit/ })).toBeEnabled();
    expect(screen.queryByText(/Running your code/)).not.toBeInTheDocument();
  });

  it('disables the run buttons and says it is running while an input run is pending', async () => {
    program.runInput.mockImplementation(() => new Promise(() => undefined));
    const user = userEvent.setup();
    renderSession(programSession({ stage: 'make' }));
    await editor();

    await user.click(screen.getByRole('button', { name: 'Try an input' }));
    const form = screen.getByRole('form', { name: 'Try an input' });
    await user.type(within(form).getByLabelText('numbers'), '[[1, 2]');
    await user.type(within(form).getByLabelText('target'), '3');
    await user.click(within(form).getByRole('button', { name: 'Run input' }));

    await waitFor(() => expect(program.runInput).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: /Run examples/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Submit/ })).toBeDisabled();
    expect(screen.getByText(/Running your code/)).toBeInTheDocument();
  });

  it('puts the worked solution between the problem and the examples once it is open', async () => {
    renderSession(programSession({ solutionAvailable: true, solutionShown: true }, { workedSolution: 'def find_pair(): ...' }));
    const solution = await screen.findByText('def find_pair(): ...');
    expect(solution.closest('details')).not.toBeNull();
    expect(screen.queryByRole('button', { name: 'Show the worked solution' })).not.toBeInTheDocument();
  });

  it('offers the worked solution between the problem and the examples before it is open', async () => {
    renderSession(programSession({ solutionAvailable: true }));
    const button = await screen.findByRole('button', { name: 'Show the worked solution' });
    const problem = screen.getByRole('heading', { name: 'Pair with a sum', level: 2 });
    const examples = screen.getByRole('heading', { name: 'Your examples' });
    expect(problem.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(button.compareDocumentPosition(examples) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('finishes with the code and shows the debrief with the strong answer and every case', async () => {
    const user = userEvent.setup();
    api.finish.mockResolvedValue(programSession({ finished: true, debrief: programDebrief() }));
    renderSession(programSession({ stage: 'review' }));

    await user.click(await screen.findByRole('button', { name: /^Finish/ }));
    expect(api.finish).toHaveBeenCalledWith(
      expect.objectContaining({ piece: [expect.objectContaining({ path: 'solution.py' })] }),
    );
    expect(await screen.findByRole('heading', { name: /Debrief/ })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'A strong answer' })).toBeInTheDocument();
    expect(within(document.body).queryByRole('button', { name: /Run examples/ })).not.toBeInTheDocument();
  });
});
