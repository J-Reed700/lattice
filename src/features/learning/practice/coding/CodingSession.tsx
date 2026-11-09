import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { ArrowLeft, HelpCircle, ShieldCheck } from 'lucide-react';

import { newId } from '@/lib/api';
import type * as Wire from '@/lib/bindings';
import { getErrorMessage } from '@/lib/errorUtils';

import { LearningCodeEditor } from '../practical/LearningCodeEditor';
import { useLearningLabDraft } from '../practical/useLearningLabDraft';
import { CaseResults } from '../shared/CaseResults';
import { ElapsedClock } from '../shared/ElapsedClock';
import { FileTabs } from '../shared/FileTabs';
import { PracticeThread, type ThreadMessage } from '../shared/PracticeThread';
import { StatusLine, type StatusState } from '../shared/StatusLine';
import { caseResultViews, HELP_NAMES, SETTING_NAMES } from './caseViews';
import { codingApi } from './codingApi';
import { Debrief } from './Debrief';
import { ExamplesTable } from './ExamplesTable';
import { HintLadder } from './HintLadder';
import { ProblemPane } from './ProblemPane';
import { RunBar } from './RunBar';
import { StageRail } from './StageRail';

export interface CodingSessionProps {
  programId: string;
  sessionId: string;
  onExit: () => void;
  onTryInterview: (setting: Wire.CodingSetting) => void;
}

/** How long typing must stop before a Watched interviewer looks at the changes. */
export const TYPING_PAUSE_MS = 4_000;
/** How long a Coached learner may do nothing before the coach checks in. */
export const IDLE_AFTER_MS = 180_000;
const BUSY_LABEL = 'Reading your code…';
const SETTINGS: Wire.CodingSetting[] = ['coached', 'mock', 'watched'];
const RUN_TITLES: Record<Wire.CodingRunScope, string> = {
  examples: 'Examples',
  all: 'Every case',
  input: 'Your input',
};

type Panel = 'problem' | 'code' | 'talk';
type Failure = { message: string; retry?: () => void };

/** The interview setting the course last used on the Practice page; Mock the first time. */
function lastInterviewSetting(programId: string): Wire.CodingSetting {
  try {
    const raw = localStorage.getItem(`lattice:practice:startSentence:${programId}:mock`);
    const stored = raw ? (JSON.parse(raw) as { setting?: string }).setting : null;
    return stored === 'watched' ? 'watched' : 'mock';
  } catch {
    return 'mock';
  }
}

export function CodingSession({ programId, sessionId, onExit, onTryInterview }: CodingSessionProps) {
  const [loaded, setLoaded] = useState<{ session: Wire.CodingSessionDto; activityRevision: number } | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setFailure(null);
    (async () => {
      try {
        const session = await codingApi.getSession({ programId, sessionId });
        const activityRevision = session.finished ? 0 : await codingApi.activityRevision(programId, session.activityId);
        if (!cancelled) setLoaded({ session, activityRevision });
      } catch (error) {
        if (!cancelled) setFailure(getErrorMessage(error));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [programId, sessionId, attempt]);

  if (!loaded) {
    const state: StatusState = failure
      ? { kind: 'error', message: failure, retry: () => setAttempt((value) => value + 1) }
      : { kind: 'busy', label: 'Opening your session…' };
    return (
      <div className="p-4">
        <StatusLine state={state} />
        {failure ? (
          <button
            type="button"
            onClick={onExit}
            className="mt-3 min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised"
          >
            Back to Practice
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <Workspace
      programId={programId}
      initial={loaded.session}
      activityRevision={loaded.activityRevision}
      onExit={onExit}
      onTryInterview={onTryInterview}
    />
  );
}

interface WorkspaceProps {
  programId: string;
  initial: Wire.CodingSessionDto;
  activityRevision: number;
  onExit: () => void;
  onTryInterview: (setting: Wire.CodingSetting) => void;
}

function Workspace({ programId, initial, activityRevision, onExit, onTryInterview }: WorkspaceProps) {
  const sessionId = initial.sessionId;
  const [session, setSession] = useState(initial);
  const sessionRef = useRef(initial);
  const movingRef = useRef(false);
  const [clock, setClock] = useState(initial.clock);
  const [panel, setPanel] = useState<Panel>('code');
  const [activePath, setActivePath] = useState<string | null>(null);
  const [activityAt, setActivityAt] = useState(0);
  const [running, setRunning] = useState(false);
  const [asking, setAsking] = useState(false);
  const [checking, setChecking] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [moving, setMoving] = useState(false);
  const [replyBusy, setReplyBusy] = useState(false);
  const [replyError, setReplyError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<Failure | null>(null);
  const [actionError, setActionError] = useState<Failure | null>(null);

  const alive = useRef(true);
  const responding = useRef(false);
  const drainRef = useRef<() => Promise<void>>(async () => undefined);

  const apply = useCallback((next: Wire.CodingSessionDto) => {
    sessionRef.current = next;
    setSession(next);
    if (next.cuePending) void drainRef.current();
  }, []);

  const drain = useCallback(async () => {
    if (responding.current) return;
    responding.current = true;
    setReplyBusy(true);
    setReplyError(null);
    try {
      while (alive.current && sessionRef.current.cuePending) {
        apply(await codingApi.respond({ programId, sessionId }));
      }
    } catch (error) {
      if (alive.current) setReplyError(getErrorMessage(error));
    } finally {
      responding.current = false;
      if (alive.current) setReplyBusy(false);
    }
  }, [apply, programId, sessionId]);
  drainRef.current = drain;

  useEffect(() => {
    alive.current = true;
    if (sessionRef.current.cuePending) void drainRef.current();
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      apply(await codingApi.getSession({ programId, sessionId }));
    } catch {
      /* the failure that led here is already shown */
    }
  }, [apply, programId, sessionId]);

  const touch = useCallback(() => setActivityAt((value) => value + 1), []);

  const starterFiles = useMemo(() => initial.draft, [initial]);
  const draft = useLearningLabDraft({
    programId,
    activityId: initial.activityId,
    activityRevision,
    starterFiles,
  });
  const files = draft.files;
  const filesRef = useRef(files);
  filesRef.current = files;

  const current = files.find((file) => file.path === activePath) ?? files[0];

  useEffect(() => {
    if (initial.finished) return;
    const set = (open: boolean) => {
      codingApi
        .setClock({ programId, sessionId, open })
        .then(setClock)
        .catch(() => undefined);
    };
    if (!document.hidden) set(true);
    const onVisibility = () => set(!document.hidden);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      document.removeEventListener('visibilitychange', onVisibility);
      if (!sessionRef.current.finished) set(false);
    };
  }, [initial.finished, programId, sessionId]);

  const observe = useCallback(
    async (reason: Wire.DraftObservation) => {
      try {
        apply(await codingApi.observeDraft({ programId, sessionId, files: filesRef.current, reason }));
      } catch {
        /* the next checkpoint looks again */
      }
    },
    [apply, programId, sessionId],
  );

  const codeSignature = files.map((file) => `${file.path}\u0000${file.content}`).join('\u0001');
  const lastObserved = useRef(codeSignature);
  useEffect(() => {
    if (session.setting !== 'watched' || session.finished || codeSignature === lastObserved.current) return;
    const timer = setTimeout(() => {
      lastObserved.current = codeSignature;
      void observe('pause');
    }, TYPING_PAUSE_MS);
    return () => clearTimeout(timer);
  }, [codeSignature, session.setting, session.finished, observe]);

  useEffect(() => {
    if (session.setting !== 'coached' || session.finished) return;
    const timer = setTimeout(() => void observe('idle'), IDLE_AFTER_MS);
    return () => clearTimeout(timer);
  }, [activityAt, session.setting, session.finished, observe]);

  const guarded = useCallback(
    async (work: () => Promise<Wire.CodingSessionDto>, retry?: () => void) => {
      setActionError(null);
      try {
        apply(await work());
        return true;
      } catch (error) {
        setActionError({ message: getErrorMessage(error), retry });
        void refresh();
        return false;
      }
    },
    [apply, refresh],
  );

  const run = useCallback(
    async (
      scope: Wire.CodingRunScope,
      input: string[] | null = null,
      ids: { operationId: string; runId: string } = { operationId: newId(), runId: newId() },
    ) => {
      if (running) return;
      touch();
      setRunning(true);
      await guarded(
        () =>
          codingApi.run({
            ...ids,
            programId,
            sessionId,
            scope,
            files: filesRef.current,
            input,
          }),
        () => void run(scope, input, ids),
      );
      setRunning(false);
    },
    [guarded, programId, running, sessionId, touch],
  );

  const onEditorKeys = (event: KeyboardEvent) => {
    if (event.key === 'Enter' && event.shiftKey && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      event.stopPropagation();
      void run('all');
    }
  };

  const ask = async (operationId = newId()) => {
    setAsking(true);
    await guarded(
      () =>
        codingApi.askHint({
          operationId,
          programId,
          sessionId,
          files: filesRef.current,
          expectedRevision: sessionRef.current.revision,
        }),
      () => void ask(operationId),
    );
    setAsking(false);
  };

  const move = async (stage: Wire.CodingStage) => {
    if (movingRef.current) return;
    movingRef.current = true;
    setMoving(true);
    try {
      await guarded(() =>
        codingApi.moveStage({
          operationId: newId(),
          programId,
          sessionId,
          stage,
          expectedRevision: sessionRef.current.revision,
        }),
      );
    } finally {
      movingRef.current = false;
      setMoving(false);
    }
  };

  const checkExample = async (example: { ordinal: number | null; args: string[]; expected: string }) => {
    touch();
    setChecking(true);
    const ok = await guarded(() => codingApi.checkExample({ operationId: newId(), programId, sessionId, ...example }));
    setChecking(false);
    if (!ok) throw new Error('The example could not be checked.');
  };

  const removeExample = async (ordinal: number) => {
    await guarded(() => codingApi.removeExample({ operationId: newId(), programId, sessionId, ordinal }));
  };

  const changeSetting = (setting: Wire.CodingSetting) =>
    guarded(() =>
      codingApi.setSetting({
        operationId: newId(),
        programId,
        sessionId,
        setting,
        expectedRevision: sessionRef.current.revision,
      }),
    );

  const reveal = () => guarded(() => codingApi.revealSolution({ operationId: newId(), programId, sessionId }));

  const send = async (content: string, spoken: boolean, operationId = newId()): Promise<void> => {
    touch();
    setSendError(null);
    try {
      apply(
        await codingApi.sendTurn({
          operationId,
          programId,
          sessionId,
          content,
          spoken,
          files: filesRef.current,
          expectedRevision: sessionRef.current.revision,
        }),
      );
    } catch (error) {
      setSendError({ message: getErrorMessage(error), retry: () => void send(content, spoken, operationId) });
      void refresh();
    }
  };

  const finish = async (operationId = newId()) => {
    setFinishing(true);
    await draft.flush();
    await guarded(
      () => codingApi.finish({ operationId, programId, sessionId, files: filesRef.current }),
      () => void finish(operationId),
    );
    setFinishing(false);
  };

  const elapsedMs = useMemo(
    () => clock.activeMs + (clock.openSince != null ? Math.max(0, Date.now() - clock.openSince) : 0),
    [clock],
  );

  if (session.finished && session.debrief) {
    return (
      <Debrief
        title={session.problem.title}
        debrief={session.debrief}
        onExit={onExit}
        onTryInterview={() => onTryInterview(lastInterviewSetting(programId))}
      />
    );
  }

  const { problem, setting } = session;
  const lastRun = session.runs[session.runs.length - 1] as Wire.CodingRunDto | undefined;
  const counterpart = setting === 'coached' ? 'Coach' : 'Interviewer';
  const messages: ThreadMessage[] = session.turns.map((turn) => ({
    id: turn.id,
    speaker: turn.speaker,
    content: turn.content,
  }));
  const threadError = replyError ? { message: replyError, retry: () => void drain() } : sendError;
  const saveFailure: Failure | null =
    draft.saveState === 'error' || draft.saveState === 'conflict'
      ? {
          message: draft.error ?? 'Your code could not be saved.',
          retry: () => void (draft.saveState === 'conflict' ? draft.keepLocalEdits() : draft.flush()),
        }
      : null;
  const shown = actionError ?? saveFailure;
  const status: StatusState | null = finishing
    ? { kind: 'busy', label: 'Writing your debrief…' }
    : shown
      ? { kind: 'error', message: shown.message, retry: shown.retry }
      : null;

  const panelClass = (name: Panel) =>
    `${panel === name ? 'flex' : 'hidden'} min-h-0 flex-col gap-4 lg:flex lg:overflow-y-auto`;

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 p-3 sm:p-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={onExit}
            className="inline-flex min-h-[44px] cursor-pointer items-center gap-1.5 rounded-lg px-2 text-sm font-medium text-text-secondary hover:text-text-primary"
          >
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            <span>Practice</span>
          </button>
          <h1 className="m-0 font-serif text-xl font-semibold text-text-primary">{problem.title}</h1>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2">
            <label htmlFor="coding-setting" className="text-sm text-text-secondary">
              Setting
            </label>
            <select
              id="coding-setting"
              value={setting}
              onChange={(event) => void changeSetting(event.target.value as Wire.CodingSetting)}
              className="min-h-[44px] rounded-lg border border-border-default bg-surface px-2 text-sm text-text-primary"
            >
              {SETTINGS.map((option) => (
                <option key={option} value={option}>
                  {SETTING_NAMES[option]}
                </option>
              ))}
            </select>
            <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-1 text-xs font-medium text-text-secondary">
              <ShieldCheck className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{HELP_NAMES[setting]}</span>
            </span>
          </div>
          <ElapsedClock
            elapsedMs={elapsedMs}
            running={clock.openSince != null && !session.finished}
            softLimitMs={clock.budgetSeconds != null ? clock.budgetSeconds * 1000 : null}
            label="Elapsed"
          />
          <button
            type="button"
            onClick={() => void finish()}
            disabled={finishing}
            className="min-h-[44px] cursor-pointer rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Finish
          </button>
        </div>
      </header>

      <StageRail stages={session.stages} current={session.stage} busy={moving} onSelect={(stage) => void move(stage)} />

      {status ? <StatusLine state={status} /> : null}

      <div role="tablist" aria-label="Workspace panels" className="flex gap-2 lg:hidden">
        {(
          [
            ['problem', 'Problem'],
            ['code', 'Code'],
            ['talk', counterpart],
          ] as [Panel, string][]
        ).map(([name, label]) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={panel === name}
            onClick={() => setPanel(name)}
            className={`min-h-[44px] cursor-pointer rounded-lg px-3 text-sm ${
              panel === name ? 'bg-accent font-semibold text-accent-fg' : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)_minmax(0,1fr)]">
        <div className={panelClass('problem')}>
          <ProblemPane problem={problem} workedSolution={session.workedSolution} />
          {!session.workedSolution ? (
            <div className="flex flex-col items-start gap-1">
              <button
                type="button"
                onClick={() => void reveal()}
                disabled={!session.solutionAvailable}
                aria-describedby={session.solutionAvailable ? undefined : 'coding-solution-reason'}
                className="min-h-[44px] cursor-pointer rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
              >
                Show the worked solution
              </button>
              {!session.solutionAvailable ? (
                <span id="coding-solution-reason" className="text-sm text-text-secondary">
                  {setting === 'coached' ? 'It opens after your first submit.' : 'It comes with your debrief.'}
                </span>
              ) : null}
            </div>
          ) : null}
          <ExamplesTable
            signature={problem.signature}
            examples={session.examples}
            coverage={session.coverage}
            busy={checking}
            onCheck={checkExample}
            onRemove={removeExample}
          />
        </div>

        <div className={panelClass('code')}>
          <div
            className="overflow-hidden rounded-xl border border-border-default bg-surface"
            onKeyDownCapture={onEditorKeys}
          >
            {files.length > 1 ? (
              <FileTabs
                label="Files"
                files={files.map((file) => ({ path: file.path }))}
                active={current?.path ?? ''}
                onSelect={setActivePath}
              />
            ) : null}
            {current ? (
              <LearningCodeEditor
                path={current.path}
                value={current.content}
                onChange={(value) => {
                  touch();
                  draft.setFileContent(current.path, value);
                }}
                ariaLabel="Code"
                minHeight={280}
                showPath={files.length <= 1}
                onRunShortcut={() => void run('examples')}
              />
            ) : null}
          </div>

          <RunBar
            signature={problem.signature}
            running={running}
            onRunExamples={() => void run('examples')}
            onSubmit={() => void run('all')}
            onTryInput={(args) => void run('input', args)}
          />

          <section aria-labelledby="coding-results-heading" className="flex flex-col gap-2">
            <h2 id="coding-results-heading" className="sr-only">
              Results
            </h2>
            <CaseResults
              cases={caseResultViews(lastRun?.cases ?? [])}
              title={lastRun ? RUN_TITLES[lastRun.scope] : 'Results'}
              emptyText="Run your examples to see the results here."
            />
            {lastRun?.referenceActual != null ? (
              <p className="m-0 text-sm text-text-primary">
                The reference returns <code className="font-mono">{lastRun.referenceActual}</code>
              </p>
            ) : null}
            {lastRun && (lastRun.stdout || lastRun.stderr) ? (
              <details className="rounded-lg border border-border-default bg-surface-sunken p-3 text-sm">
                <summary className="cursor-pointer font-semibold text-text-primary">What your program printed</summary>
                <pre className="m-0 mt-2 overflow-x-auto font-mono text-sm text-text-primary">
                  {lastRun.stdout}
                  {lastRun.stderr}
                </pre>
              </details>
            ) : null}
          </section>

          {setting !== 'watched' ? (
            <HintLadder hints={session.hints} nextRung={session.nextRung} asking={asking} onAsk={() => void ask()} />
          ) : null}
        </div>

        <div className={panelClass('talk')}>
          <section
            aria-labelledby="coding-thread-heading"
            className="flex min-h-0 flex-col gap-3 rounded-xl border border-border-default bg-surface p-4"
          >
            <h2 id="coding-thread-heading" className="m-0 font-serif text-lg font-semibold text-text-primary">
              {counterpart}
            </h2>
            <PracticeThread
              label={`Message the ${counterpart.toLowerCase()}`}
              messages={messages}
              onSend={send}
              busy={replyBusy ? BUSY_LABEL : null}
              error={threadError}
              placeholder={setting === 'watched' ? 'Think out loud…' : 'Ask, or explain your thinking…'}
              draftKey={`coding-thread-${sessionId}`}
              suggestion={setting === 'watched' ? 'Talk through your thinking: type or use the mic' : null}
              disabled={session.finished ? { reason: 'This session is finished.' } : null}
            />
            {setting === 'watched' ? (
              <div className="flex flex-col items-start gap-1">
                <button
                  type="button"
                  onClick={() => void ask()}
                  disabled={asking}
                  className="inline-flex min-h-[44px] cursor-pointer items-center gap-2 rounded-lg border border-border-default bg-surface px-4 text-sm font-semibold text-text-primary hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <HelpCircle className="h-4 w-4" aria-hidden="true" />
                  <span>I&apos;m stuck</span>
                </button>
                <span className="text-sm text-text-secondary">
                  The interviewer answers in the conversation. Hints are noted in your debrief.
                </span>
              </div>
            ) : null}
          </section>
        </div>
      </div>
    </div>
  );
}
