import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { PracticeSession } from '../PracticeSession';
import { sessionApi } from '../sessionApi';
import { createFakeKind, flushDraft } from './fakeKind';
import { practiceCheck, practiceDebrief, practiceHint, practiceSession } from './fixtures';

vi.mock('@/lib/api', () => {
  let count = 0;
  return { newId: () => `id-${++count}`, unwrap: <T,>(value: T) => value, default: {}, VaultAPI: {} };
});

vi.mock('../sessionApi', () => ({
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
      <button type="button" onClick={() => void onSend('What about an empty word?', false)}>
        Send
      </button>
    </div>
  ),
}));

const api = vi.mocked(sessionApi);
const ownCommand = vi.fn<() => Promise<Wire.PracticeSessionDto>>();
const kinds = { fake: createFakeKind(() => ownCommand()) };

function renderSession(session: Wire.PracticeSessionDto, registry = kinds) {
  api.getSession.mockResolvedValue(session);
  const onTryInterview = vi.fn();
  const onExit = vi.fn();
  const view = render(
    <PracticeSession
      programId="program-1"
      sessionId={session.sessionId}
      kinds={registry}
      onExit={onExit}
      onTryInterview={onTryInterview}
    />,
  );
  return { ...view, onTryInterview, onExit };
}

async function workBox() {
  return screen.findByRole('textbox', { name: 'Work' });
}

describe('PracticeSession', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.setClock.mockResolvedValue({ activeMs: 60_000, openSince: 1_700_000_000_000, budgetSeconds: null });
    api.respond.mockImplementation(async () => practiceSession({ cuePending: false }));
  });

  afterEach(() => {
    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    vi.useRealTimers();
  });

  it('keeps the work, the checks and the kind\'s own commands usable while a reply is pending', async () => {
    api.sendTurn.mockResolvedValue(practiceSession({ cuePending: true }));
    api.respond.mockImplementation(() => new Promise(() => undefined));
    api.check.mockResolvedValue(practiceSession({ checks: [practiceCheck()] }));
    const user = userEvent.setup();
    renderSession(practiceSession({ stage: 'make' }));

    await user.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Reading the work…')).toBeInTheDocument();

    const box = await workBox();
    expect(box).not.toHaveAttribute('readonly');
    fireEvent.change(box, { target: { value: 'edited' } });
    expect(box).toHaveValue('edited');

    const checkButton = screen.getByRole('button', { name: 'Check shown' });
    expect(checkButton).toBeEnabled();
    await user.click(checkButton);
    expect(api.check).toHaveBeenCalledWith(expect.objectContaining({ scope: 'shown' }));
    expect(screen.getByRole('button', { name: /own thing/ })).toBeEnabled();
  });

  it('sends the work as it stands with every call that carries it', async () => {
    const user = userEvent.setup();
    api.check.mockResolvedValue(practiceSession({ stage: 'make' }));
    api.askHint.mockResolvedValue(practiceSession({ stage: 'make' }));
    api.sendTurn.mockResolvedValue(practiceSession({ stage: 'make' }));
    api.finish.mockResolvedValue(practiceSession({ stage: 'make' }));
    renderSession(practiceSession({ stage: 'make' }));

    fireEvent.change(await workBox(), { target: { value: 'my work' } });

    await user.click(screen.getByRole('button', { name: 'Check all' }));
    expect(api.check).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'all', piece: 'my work' }));
    await user.click(screen.getByRole('button', { name: /Get a nudge/ }));
    expect(api.askHint).toHaveBeenLastCalledWith(expect.objectContaining({ piece: 'my work' }));
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(api.sendTurn).toHaveBeenLastCalledWith(expect.objectContaining({ piece: 'my work' }));
    await user.click(screen.getByRole('button', { name: /^Finish/ }));
    expect(api.finish).toHaveBeenLastCalledWith(expect.objectContaining({ piece: 'my work' }));
  });

  it('checks the shown and the whole scope under the session ids', async () => {
    api.check.mockResolvedValue(practiceSession({ stage: 'make' }));
    const user = userEvent.setup();
    renderSession(practiceSession({ stage: 'make' }));

    await user.click(await screen.findByRole('button', { name: 'Check shown' }));
    await user.click(screen.getByRole('button', { name: 'Check all' }));
    expect(api.check).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ programId: 'program-1', sessionId: 'session-1', scope: 'shown', piece: 'start' }),
    );
    expect(api.check).toHaveBeenNthCalledWith(2, expect.objectContaining({ scope: 'all' }));
  });

  it('draws the stage rail from the labels the wire sent', async () => {
    renderSession(practiceSession({ stage: 'make' }));
    const rail = await screen.findByRole('navigation', { name: 'Stages' });

    expect(within(rail).getByRole('button', { name: /Make it/ })).toHaveAttribute('aria-current', 'step');
    for (const label of ['Understand', 'Examples', 'Plan', 'Review']) {
      expect(within(rail).getByRole('button', { name: new RegExp(label) })).toBeInTheDocument();
    }
    expect(within(rail).queryByText('Code')).not.toBeInTheDocument();
    expect(within(rail).getByRole('button', { name: /Next: Test/ })).toBeInTheDocument();
    expect(within(rail).getByText('Do the make stage. Any stage can be skipped.')).toBeInTheDocument();
  });

  it('names the kind\'s panels on the small-screen tabs and the session\'s title in the header', async () => {
    renderSession(practiceSession());
    expect(await screen.findByRole('heading', { name: 'Reverse a word' })).toBeInTheDocument();
    const tabs = screen.getAllByRole('tab').map((tab) => tab.textContent);
    expect(tabs).toEqual(['Brief', 'Piece', 'Coach']);
    expect(screen.getByText('Give the word back in reverse.')).toBeInTheDocument();
  });

  it('shows an error with the way back for a kind the registry lacks', async () => {
    const user = userEvent.setup();
    const { onExit } = renderSession(practiceSession({ kind: 'sketch' }));

    expect(
      await screen.findByText('This kind of practice is not available in this version of the app.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Back to Practice' }));
    expect(onExit).toHaveBeenCalled();
  });

  it('puts the failure of a kind\'s own command on screen with its retry, and reads the session again', async () => {
    ownCommand.mockRejectedValueOnce(new Error('The kind could not do that.'));
    const user = userEvent.setup();
    renderSession(practiceSession());

    await user.click(await screen.findByRole('button', { name: /own thing/ }));
    expect(await screen.findByText('The kind could not do that.')).toBeInTheDocument();
    await waitFor(() => expect(api.getSession).toHaveBeenCalledTimes(2));

    ownCommand.mockResolvedValueOnce(practiceSession({ revision: 5 }));
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(ownCommand).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByText('The kind could not do that.')).not.toBeInTheDocument());
  });

  it('does not count a pending own command of the kind as a check by the engine', async () => {
    ownCommand.mockImplementation(() => new Promise(() => undefined));
    const user = userEvent.setup();
    renderSession(practiceSession());

    await user.click(await screen.findByRole('button', { name: /own thing/ }));
    await waitFor(() => expect(ownCommand).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: 'Check shown' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Check all' })).toBeEnabled();
  });

  it('applies the session a kind\'s own command answers with', async () => {
    ownCommand.mockResolvedValue(practiceSession({ checks: [practiceCheck({ checkId: 'own-1', scope: 'shown', passed: 2, total: 2 })] }));
    const user = userEvent.setup();
    renderSession(practiceSession());

    await user.click(await screen.findByRole('button', { name: /own thing/ }));
    expect(await screen.findByText('shown 2/2')).toBeInTheDocument();
  });

  it('lists what the kind\'s results part draws from the session\'s checks', async () => {
    renderSession(practiceSession({ checks: [practiceCheck()] }));
    expect(await screen.findByText('all 1/3')).toBeInTheDocument();
  });

  it('shows one hint rung at a time, the next only after the current is read', async () => {
    const nudge = practiceHint({ stage: 'make' });
    api.askHint.mockResolvedValue(practiceSession({ hints: [nudge], nextRung: 'idea', stage: 'make' }));
    const user = userEvent.setup();
    renderSession(practiceSession({ stage: 'make' }));

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

  it('draws the kind\'s mark beside each hint it is given', async () => {
    renderSession(practiceSession({ stage: 'make', hints: [practiceHint({ ordinal: 2, rung: 'point' })], nextRung: 'point' }));
    expect(await screen.findByText('Points at part 2')).toBeInTheDocument();
  });

  it('offers the worked solution only when it is available and not yet open, and says why not', async () => {
    const user = userEvent.setup();
    api.revealSolution.mockResolvedValue(practiceSession({ solutionAvailable: true, solutionShown: true }));
    const closed = renderSession(practiceSession());
    const button = await screen.findByRole('button', { name: 'Show the worked solution' });
    expect(button).toBeDisabled();
    expect(screen.getByText('It opens after your first submit.')).toBeInTheDocument();
    closed.unmount();

    const available = renderSession(practiceSession({ solutionAvailable: true }));
    await user.click(await screen.findByRole('button', { name: 'Show the worked solution' }));
    expect(api.revealSolution).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Show the worked solution' })).not.toBeInTheDocument());
    available.unmount();

    renderSession(practiceSession({ solutionAvailable: true, solutionShown: true }));
    await workBox();
    expect(screen.queryByRole('button', { name: 'Show the worked solution' })).not.toBeInTheDocument();
  });

  it('offers I am stuck instead of the ladder in Watched', async () => {
    renderSession(practiceSession({ setting: 'watched', settingsUsed: ['watched'], stage: 'make' }));
    expect(await screen.findByRole('button', { name: /I'm stuck/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Get a nudge/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Talk through your thinking: type or use the mic/)).toBeInTheDocument();
  });

  it('opens the clock on mount, closes it when the document hides and on unmount', async () => {
    const view = renderSession(practiceSession());
    await workBox();
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
    renderSession(practiceSession({ cuePending: true }));
    await workBox();
    await waitFor(() => expect(api.respond).toHaveBeenCalledTimes(1));
    expect(api.respond).toHaveBeenCalledWith({ programId: 'program-1', sessionId: 'session-1' });
    await act(async () => {
      await Promise.resolve();
    });
    expect(api.respond).toHaveBeenCalledTimes(1);
  });

  it('does not call respond when no cue is pending', async () => {
    renderSession(practiceSession());
    await workBox();
    await act(async () => {
      await Promise.resolve();
    });
    expect(api.respond).not.toHaveBeenCalled();
  });

  it('calls respond again only while the result still holds a cue', async () => {
    api.respond
      .mockResolvedValueOnce(practiceSession({ cuePending: true }))
      .mockResolvedValueOnce(practiceSession({ cuePending: false }));
    renderSession(practiceSession({ cuePending: true }));
    await waitFor(() => expect(api.respond).toHaveBeenCalledTimes(2));
  });

  it('offers Try again in the thread when a reply fails, and respond runs again on it', async () => {
    api.respond.mockRejectedValueOnce(new Error('The model did not answer.'));
    const user = userEvent.setup();
    renderSession(practiceSession({ cuePending: true }));

    expect(await screen.findByText('The model did not answer.')).toBeInTheDocument();
    expect(await workBox()).not.toHaveAttribute('readonly');
    api.respond.mockResolvedValueOnce(practiceSession({ cuePending: false }));
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(api.respond).toHaveBeenCalledTimes(2));
  });

  it('moves to the next stage with the session revision', async () => {
    api.moveStage.mockResolvedValue(practiceSession({ stage: 'examples', revision: 5 }));
    const user = userEvent.setup();
    renderSession(practiceSession());

    await user.click(await screen.findByRole('button', { name: /^Next/ }));
    expect(api.moveStage).toHaveBeenCalledWith(
      expect.objectContaining({ stage: 'examples', expectedRevision: 4, sessionId: 'session-1' }),
    );
  });

  it('sends a draft observation after a typing pause in Watched', async () => {
    api.observeDraft.mockResolvedValue(practiceSession({ setting: 'watched' }));
    renderSession(practiceSession({ setting: 'watched', settingsUsed: ['watched'], stage: 'make' }));
    const box = await workBox();

    vi.useFakeTimers();
    fireEvent.change(box, { target: { value: 'changed text' } });
    await act(async () => {
      vi.advanceTimersByTime(4_000);
    });
    expect(api.observeDraft).toHaveBeenCalledWith(expect.objectContaining({ reason: 'pause', piece: 'changed text' }));
  });

  it('sends Idle in Coached after three minutes with no activity', async () => {
    api.observeDraft.mockResolvedValue(practiceSession());
    renderSession(practiceSession({ stage: 'make' }));
    const box = await workBox();

    vi.useFakeTimers();
    fireEvent.change(box, { target: { value: 'x = 1\n' } });
    await act(async () => {
      vi.advanceTimersByTime(179_000);
    });
    expect(api.observeDraft).not.toHaveBeenCalled();
    await act(async () => {
      vi.advanceTimersByTime(2_000);
    });
    expect(api.observeDraft).toHaveBeenCalledWith(expect.objectContaining({ reason: 'idle', piece: 'x = 1\n' }));
  });

  it('saves the draft, then finishes with the work and shows the debrief', async () => {
    const user = userEvent.setup();
    api.finish.mockResolvedValue(practiceSession({ finished: true, debrief: practiceDebrief() }));
    renderSession(practiceSession({ stage: 'review' }));

    await user.click(await screen.findByRole('button', { name: /^Finish/ }));
    expect(flushDraft).toHaveBeenCalledTimes(1);
    expect(api.finish).toHaveBeenCalledWith(expect.objectContaining({ piece: 'start' }));
    expect(await screen.findByRole('heading', { name: /Debrief/ })).toBeInTheDocument();
    expect(screen.getByText('Kind debrief text.')).toBeInTheDocument();
    expect(within(document.body).queryByRole('button', { name: 'Check shown' })).not.toBeInTheDocument();
  });

  describe('retries reuse the ids of the action', () => {
    function ids(mock: { mock: { calls: unknown[][] } }, call: number) {
      return mock.mock.calls[call][0] as { operationId: string; checkId?: string };
    }

    it('repeats a failed check under the same operation and check ids', async () => {
      api.check.mockRejectedValueOnce(new Error('timed out')).mockResolvedValue(practiceSession({ stage: 'make' }));
      const user = userEvent.setup();
      renderSession(practiceSession({ stage: 'make' }));

      await user.click(await screen.findByRole('button', { name: 'Check shown' }));
      await user.click(await screen.findByRole('button', { name: 'Try again' }));

      await waitFor(() => expect(api.check).toHaveBeenCalledTimes(2));
      expect(ids(api.check, 1).operationId).toBe(ids(api.check, 0).operationId);
      expect(ids(api.check, 1).checkId).toBe(ids(api.check, 0).checkId);
    });

    it('repeats a failed hint under the same operation id', async () => {
      api.askHint.mockRejectedValueOnce(new Error('timed out')).mockResolvedValue(practiceSession({ stage: 'make' }));
      const user = userEvent.setup();
      renderSession(practiceSession({ stage: 'make' }));

      await user.click(await screen.findByRole('button', { name: /Get a nudge/ }));
      await user.click(await screen.findByRole('button', { name: 'Try again' }));

      await waitFor(() => expect(api.askHint).toHaveBeenCalledTimes(2));
      expect(ids(api.askHint, 1).operationId).toBe(ids(api.askHint, 0).operationId);
    });

    it('repeats a failed message under the same operation id', async () => {
      api.sendTurn.mockRejectedValueOnce(new Error('timed out')).mockResolvedValue(practiceSession());
      const user = userEvent.setup();
      renderSession(practiceSession());

      await user.click(await screen.findByRole('button', { name: 'Send' }));
      await user.click(await screen.findByRole('button', { name: 'Try again' }));

      await waitFor(() => expect(api.sendTurn).toHaveBeenCalledTimes(2));
      expect(ids(api.sendTurn, 1).operationId).toBe(ids(api.sendTurn, 0).operationId);
    });

    it('repeats a failed finish under the same operation id', async () => {
      api.finish.mockRejectedValueOnce(new Error('timed out')).mockResolvedValue(practiceSession());
      const user = userEvent.setup();
      renderSession(practiceSession({ stage: 'review' }));

      await user.click(await screen.findByRole('button', { name: /^Finish/ }));
      await user.click(await screen.findByRole('button', { name: 'Try again' }));

      await waitFor(() => expect(api.finish).toHaveBeenCalledTimes(2));
      expect(ids(api.finish, 1).operationId).toBe(ids(api.finish, 0).operationId);
    });

    it('gives each fresh click new ids', async () => {
      api.check.mockResolvedValue(practiceSession({ stage: 'make' }));
      const user = userEvent.setup();
      renderSession(practiceSession({ stage: 'make' }));

      await user.click(await screen.findByRole('button', { name: 'Check shown' }));
      await waitFor(() => expect(api.check).toHaveBeenCalledTimes(1));
      await user.click(screen.getByRole('button', { name: 'Check shown' }));
      await waitFor(() => expect(api.check).toHaveBeenCalledTimes(2));

      expect(ids(api.check, 1).operationId).not.toBe(ids(api.check, 0).operationId);
      expect(ids(api.check, 1).checkId).not.toBe(ids(api.check, 0).checkId);
    });
  });

  it('sends one stage move while the first is pending', async () => {
    api.moveStage.mockImplementation(() => new Promise(() => undefined));
    const user = userEvent.setup();
    renderSession(practiceSession());

    const next = await screen.findByRole('button', { name: /^Next/ });
    await user.click(next);
    await user.click(next);
    expect(api.moveStage).toHaveBeenCalledTimes(1);
  });
});
