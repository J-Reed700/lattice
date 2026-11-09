import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { codingApi } from '../codingApi';
import { CodingSession } from '../CodingSession';
import { caseView, codingRun, codingSession } from './fixtures';

vi.mock('../codingApi', () => ({
  codingApi: {
    getSession: vi.fn(),
    moveStage: vi.fn(),
    checkExample: vi.fn(),
    removeExample: vi.fn(),
    run: vi.fn(),
    askHint: vi.fn(),
    sendTurn: vi.fn(),
    respond: vi.fn(),
    observeDraft: vi.fn(),
    setClock: vi.fn(),
    setSetting: vi.fn(),
    revealSolution: vi.fn(),
    finish: vi.fn(),
    activityRevision: vi.fn(),
  },
}));

vi.mock('../../practical/LearningCodeEditor', () => ({
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

vi.mock('../../practical/useLearningLabDraft', async () => {
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

vi.mock('../../shared/PracticeThread', () => ({
  PracticeThread: ({
    messages,
    busy,
    error,
    suggestion,
    onSend,
  }: {
    messages: { id: string; content: string }[];
    busy?: string | null;
    error?: { message: string; retry?: () => void } | null;
    suggestion?: string | null;
    onSend: (content: string, spoken: boolean) => Promise<void>;
  }) => (
    <div>
      {messages.map((message) => (
        <p key={message.id}>{message.content}</p>
      ))}
      {busy ? <p role="status">{busy}</p> : null}
      {error ? (
        <div role="alert">
          {error.message}
          <button type="button" onClick={error.retry}>
            Try again
          </button>
        </div>
      ) : null}
      {suggestion ? <p>{suggestion}</p> : null}
      <button type="button" onClick={() => void onSend('What about an empty list?', false)}>
        Send
      </button>
    </div>
  ),
}));

const api = vi.mocked(codingApi);

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

function renderSession(session: Wire.CodingSessionDto) {
  api.getSession.mockResolvedValue(session);
  const onTryInterview = vi.fn();
  const view = render(
    <CodingSession programId="program-1" sessionId={session.sessionId} onExit={vi.fn()} onTryInterview={onTryInterview} />,
  );
  return { ...view, onTryInterview };
}

async function editor() {
  return screen.findByRole('textbox', { name: /code/i });
}

describe('CodingSession', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.activityRevision.mockResolvedValue(2);
    api.setClock.mockResolvedValue({ activeMs: 60_000, openSince: 1_700_000_000_000, budgetSeconds: null });
    api.respond.mockImplementation(async () => codingSession({ cuePending: false }));
  });

  afterEach(() => {
    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    vi.useRealTimers();
  });

  it('keeps the editor, runs and examples usable while a reply is pending', async () => {
    api.sendTurn.mockResolvedValue(codingSession({ cuePending: true }));
    api.respond.mockImplementation(() => new Promise(() => undefined));
    api.run.mockResolvedValue(codingSession({ runs: [codingRun([passing])] }));
    const user = userEvent.setup();
    renderSession(codingSession({ stage: 'code' }));

    await user.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Reading your code…')).toBeInTheDocument();

    const box = await editor();
    expect(box).not.toHaveAttribute('readonly');
    fireEvent.change(box, { target: { value: 'def find_pair(numbers, target):\n    return []\n' } });
    expect(box).toHaveValue('def find_pair(numbers, target):\n    return []\n');

    const runButton = screen.getByRole('button', { name: /Run examples/ });
    expect(runButton).toBeEnabled();
    await user.click(runButton);
    expect(api.run).toHaveBeenCalledWith(expect.objectContaining({ scope: 'examples' }));
    expect(screen.getByLabelText('numbers')).toBeEnabled();
  });

  it('runs the examples on Ctrl+Enter and shows the shortcuts', async () => {
    api.run.mockResolvedValue(codingSession({ runs: [codingRun([passing])] }));
    renderSession(codingSession({ stage: 'code' }));
    const box = await editor();

    expect(screen.getByRole('button', { name: /Run examples/ })).toHaveTextContent('Ctrl+Enter');
    expect(screen.getByRole('button', { name: /Submit/ })).toHaveTextContent('Ctrl+Shift+Enter');

    fireEvent.keyDown(box, { key: 'Enter', ctrlKey: true });
    await waitFor(() =>
      expect(api.run).toHaveBeenCalledWith(
        expect.objectContaining({
          programId: 'program-1',
          sessionId: 'session-1',
          scope: 'examples',
          files: [expect.objectContaining({ path: 'solution.py' })],
        }),
      ),
    );
  });

  it('lists failures first and announces the summary', async () => {
    renderSession(
      codingSession({
        stage: 'test',
        runs: [codingRun([passing, passingToo, failingHidden])],
      }),
    );

    await screen.findByText(/2 of 3 passed\. Failing: an input with duplicates/);
    const live = screen.getAllByRole('status').find((node) => /2 of 3 passed/.test(node.textContent ?? ''));
    expect(live).toBeDefined();

    const failing = screen.getAllByText(/an input with duplicates/).at(-1) as HTMLElement;
    const firstPassing = screen.getAllByText(/a pair at the ends/).at(-1) as HTMLElement;
    expect(failing.compareDocumentPosition(firstPassing) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('shows a failing hidden case by its category only in Mock, and in full in Coached', async () => {
    const mock = renderSession(
      codingSession({
        setting: 'mock',
        settingsUsed: ['mock'],
        stage: 'test',
        runs: [codingRun([failingHidden])],
      }),
    );
    await screen.findByText(/an input with duplicates/);
    expect(screen.queryByText('[2, 2]')).not.toBeInTheDocument();
    expect(screen.queryByText('[0, 1]')).not.toBeInTheDocument();
    mock.unmount();

    renderSession(
      codingSession({
        stage: 'test',
        runs: [
          codingRun([
            { ...failingHidden, args: ['[2, 2]', '4'], expected: '[0, 1]', actual: '[]' },
          ]),
        ],
      }),
    );
    expect(await screen.findByText(/\[2, 2\]/)).toBeInTheDocument();
  });

  it('shows one hint rung at a time, the next only after the current is read', async () => {
    const nudge: Wire.CodeHintDto = {
      ordinal: 0,
      rung: 'nudge',
      text: 'What do you need to remember about earlier numbers?',
      reason: 'It points at what to look at.',
      lines: [],
      path: null,
      stage: 'code',
      runId: null,
      inRole: false,
      at: 1,
    };
    api.askHint.mockResolvedValue(
      codingSession({ hints: [nudge], nextRung: 'idea', stage: 'code' }),
    );
    const user = userEvent.setup();
    renderSession(codingSession({ stage: 'code' }));

    expect(screen.queryByRole('button', { name: /Get the idea/ })).not.toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: /Get a nudge/ }));
    expect(api.askHint).toHaveBeenCalledWith(expect.objectContaining({ expectedRevision: 4 }));

    expect(await screen.findByText(nudge.text)).toBeInTheDocument();
    expect(screen.getByText(/It points at what to look at/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Get the idea/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Get a plan/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /I've read this/ }));
    expect(screen.getByRole('button', { name: /Get the idea/ })).toBeEnabled();
    expect(screen.queryByRole('button', { name: /Get a plan/ })).not.toBeInTheDocument();
  });

  it('offers I am stuck instead of the ladder in Watched', async () => {
    renderSession(codingSession({ setting: 'watched', settingsUsed: ['watched'], stage: 'code' }));
    expect(await screen.findByRole('button', { name: /I'm stuck/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Get a nudge/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Talk through your thinking: type or use the mic/)).toBeInTheDocument();
  });

  it('opens the clock on mount, closes it when the document hides and on unmount', async () => {
    const view = renderSession(codingSession());
    await editor();
    await waitFor(() =>
      expect(api.setClock).toHaveBeenCalledWith({ programId: 'program-1', sessionId: 'session-1', open: true }),
    );

    api.setClock.mockClear();
    Object.defineProperty(document, 'hidden', { value: true, configurable: true });
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    await waitFor(() =>
      expect(api.setClock).toHaveBeenCalledWith({ programId: 'program-1', sessionId: 'session-1', open: false }),
    );

    api.setClock.mockClear();
    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    await waitFor(() =>
      expect(api.setClock).toHaveBeenCalledWith({ programId: 'program-1', sessionId: 'session-1', open: true }),
    );

    api.setClock.mockClear();
    view.unmount();
    expect(api.setClock).toHaveBeenCalledWith({ programId: 'program-1', sessionId: 'session-1', open: false });
  });

  it('calls respond once for a pending cue and not at all without one', async () => {
    renderSession(codingSession({ cuePending: true }));
    await editor();
    await waitFor(() => expect(api.respond).toHaveBeenCalledTimes(1));
    expect(api.respond).toHaveBeenCalledWith({ programId: 'program-1', sessionId: 'session-1' });
    await act(async () => {
      await Promise.resolve();
    });
    expect(api.respond).toHaveBeenCalledTimes(1);
  });

  it('does not call respond when no cue is pending', async () => {
    renderSession(codingSession());
    await editor();
    await act(async () => {
      await Promise.resolve();
    });
    expect(api.respond).not.toHaveBeenCalled();
  });

  it('calls respond again only while the result still holds a cue', async () => {
    api.respond
      .mockResolvedValueOnce(codingSession({ cuePending: true }))
      .mockResolvedValueOnce(codingSession({ cuePending: false }));
    renderSession(codingSession({ cuePending: true }));
    await waitFor(() => expect(api.respond).toHaveBeenCalledTimes(2));
  });

  it('offers Try again in the thread when a reply fails, and respond runs again on it', async () => {
    api.respond.mockRejectedValueOnce(new Error('The model did not answer.'));
    const user = userEvent.setup();
    renderSession(codingSession({ cuePending: true }));

    expect(await screen.findByText('The model did not answer.')).toBeInTheDocument();
    expect(await editor()).not.toHaveAttribute('readonly');
    api.respond.mockResolvedValueOnce(codingSession({ cuePending: false }));
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(api.respond).toHaveBeenCalledTimes(2));
  });

  it('moves to the next stage with the session revision', async () => {
    api.moveStage.mockResolvedValue(codingSession({ stage: 'examples', revision: 5 }));
    const user = userEvent.setup();
    renderSession(codingSession());

    await user.click(await screen.findByRole('button', { name: /^Next/ }));
    expect(api.moveStage).toHaveBeenCalledWith(
      expect.objectContaining({ stage: 'examples', expectedRevision: 4, sessionId: 'session-1' }),
    );
  });

  it('sends a draft observation after a typing pause in Watched', async () => {
    api.observeDraft.mockResolvedValue(codingSession({ setting: 'watched' }));
    renderSession(codingSession({ setting: 'watched', settingsUsed: ['watched'], stage: 'code' }));
    const box = await editor();

    vi.useFakeTimers();
    fireEvent.change(box, { target: { value: 'def find_pair(numbers, target):\n    return {}\n' } });
    await act(async () => {
      vi.advanceTimersByTime(4_000);
    });
    expect(api.observeDraft).toHaveBeenCalledWith(
      expect.objectContaining({ reason: 'pause', files: [expect.objectContaining({ path: 'solution.py' })] }),
    );
  });

  it('sends Idle in Coached after three minutes with no activity', async () => {
    api.observeDraft.mockResolvedValue(codingSession());
    renderSession(codingSession({ stage: 'code' }));
    await editor();

    vi.useFakeTimers();
    fireEvent.change(await screen.findByRole('textbox', { name: /code/i }), { target: { value: 'x = 1\n' } });
    await act(async () => {
      vi.advanceTimersByTime(179_000);
    });
    expect(api.observeDraft).not.toHaveBeenCalled();
    await act(async () => {
      vi.advanceTimersByTime(2_000);
    });
    expect(api.observeDraft).toHaveBeenCalledWith(expect.objectContaining({ reason: 'idle' }));
  });

  it('finishes with the code and shows the debrief', async () => {
    const user = userEvent.setup();
    api.finish.mockResolvedValue(
      codingSession({
        finished: true,
        debrief: {
          scorecard: { rows: [] },
          facts: { stageMs: [], runs: 0, submits: 0, hints: [], settings: ['coached'] },
          strongAnswer: { approach: 'Scan once.', complexity: 'O(n)', reference: 'pass' },
          patterns: [],
          cases: [],
          misreadings: [],
          model: 'a model',
        },
      }),
    );
    renderSession(codingSession({ stage: 'review' }));

    await user.click(await screen.findByRole('button', { name: /^Finish/ }));
    expect(api.finish).toHaveBeenCalledWith(
      expect.objectContaining({ files: [expect.objectContaining({ path: 'solution.py' })] }),
    );
    expect(await screen.findByRole('heading', { name: /Debrief/ })).toBeInTheDocument();
    expect(within(document.body).queryByRole('button', { name: /Run examples/ })).not.toBeInTheDocument();
  });
});
