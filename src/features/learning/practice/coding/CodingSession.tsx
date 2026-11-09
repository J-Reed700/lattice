import { useCallback, useEffect, useRef, useState } from 'react';
import { Bot, HelpCircle, MessageSquare } from 'lucide-react';
import { LearningCodeEditor } from '../practical/LearningCodeEditor';
import { ElapsedClock } from '../shared/ElapsedClock';
import { FileTabs, type FileTabItem } from '../shared/FileTabs';
import { PracticeThread, type PracticeThreadTurn } from '../shared/PracticeThread';
import {
  codingApi,
  type CodingSessionDto,
  type CodingStage,
  type RunScope,
} from './codingApi';
import { ExamplesTable } from './ExamplesTable';
import { HintLadder } from './HintLadder';
import { ProblemPane } from './ProblemPane';
import { RunBar } from './RunBar';
import { StageRail } from './StageRail';

export interface CodingSessionProps {
  session: CodingSessionDto;
  onUpdateSession?: (session: CodingSessionDto) => void;
  onFinish?: (session: CodingSessionDto) => void;
  onExit?: () => void;
  className?: string;
}

type MobileTab = 'problem' | 'code' | 'counterpart';

export function CodingSession({
  session,
  onUpdateSession,
  onFinish,
  onExit,
  className = '',
}: CodingSessionProps) {
  const [sessionState, setSessionState] = useState<CodingSessionDto>(session);
  const [activeFile, setActiveFile] = useState<string>('solution.py');
  const [mobileTab, setMobileTab] = useState<MobileTab>('code');
  const [counterpartBusy, setCounterpartBusy] = useState<string | null>(null);
  const [running, setRunning] = useState<boolean>(false);
  const [askingHint, setAskingHint] = useState<boolean>(false);
  const [checkingExample, setCheckingExample] = useState<boolean>(false);

  const draftTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Sync prop updates if parent passes new session
  useEffect(() => {
    setSessionState(session);
  }, [session]);

  const updateSession = useCallback(
    (next: CodingSessionDto) => {
      setSessionState(next);
      onUpdateSession?.(next);
    },
    [onUpdateSession],
  );

  // Visibility and window focus tracking for clock pause / resume
  useEffect(() => {
    const handleVisibility = () => {
      const isOpen = !document.hidden;
      void codingApi.setCodingClock({
        sessionId: sessionState.id,
        open: isOpen,
      });
    };

    const handleFocus = () => {
      void codingApi.setCodingClock({
        sessionId: sessionState.id,
        open: true,
      });
    };

    const handleBlur = () => {
      void codingApi.setCodingClock({
        sessionId: sessionState.id,
        open: false,
      });
    };

    document.addEventListener('visibilitychange', handleVisibility);
    window.addEventListener('focus', handleFocus);
    window.addEventListener('blur', handleBlur);

    return () => {
      document.removeEventListener('visibilitychange', handleVisibility);
      window.removeEventListener('focus', handleFocus);
      window.removeEventListener('blur', handleBlur);
    };
  }, [sessionState.id]);

  // Active file content
  const activeFileObj = sessionState.draftFiles.find((f) => f.path === activeFile) ?? sessionState.draftFiles[0];
  const fileContent = activeFileObj?.content ?? '';

  // File change handler with debounced draft observation in Watched mode
  const handleCodeChange = (newContent: string) => {
    const nextFiles = sessionState.draftFiles.map((f) =>
      f.path === (activeFileObj?.path ?? 'solution.py') ? { ...f, content: newContent } : f,
    );
    setSessionState((prev) => ({ ...prev, draftFiles: nextFiles }));

    if (sessionState.setting === 'watched') {
      if (draftTimerRef.current) {
        clearTimeout(draftTimerRef.current);
      }
      draftTimerRef.current = setTimeout(() => {
        void codingApi
          .observeCodingDraft({
            sessionId: sessionState.id,
            files: nextFiles,
          })
          .then((updated) => {
            if (updated) updateSession(updated);
          })
          .catch(() => {});
      }, 2000);
    }
  };

  // Stage changes
  const handleStageSelect = async (newStage: CodingStage) => {
    try {
      const updated = await codingApi.moveCodingStage({
        operationId: `stage-${Date.now()}`,
        sessionId: sessionState.id,
        stage: newStage,
        expectedRevision: sessionState.revision,
      });
      updateSession(updated);
    } catch {
      // Revert or keep
    }
  };

  // Example checking
  const handleCheckExample = async (argsJson: string, expectedJson: string) => {
    setCheckingExample(true);
    try {
      const checked = await codingApi.checkCodingExample({
        operationId: `ex-${Date.now()}`,
        sessionId: sessionState.id,
        argsJson,
        expectedJson,
      });
      const nextExamples = [...sessionState.examples, checked];
      updateSession({ ...sessionState, examples: nextExamples });
    } finally {
      setCheckingExample(false);
    }
  };

  // Running code with scope
  const handleRun = async (scope: RunScope, customInputJson?: string) => {
    setRunning(true);
    try {
      const updated = await codingApi.runCodingSession({
        operationId: `run-${Date.now()}`,
        sessionId: sessionState.id,
        scope,
        files: sessionState.draftFiles,
        customInputJson,
      });
      updateSession(updated);
    } finally {
      setRunning(false);
    }
  };

  // Asking for a hint
  const handleAskHint = async () => {
    setAskingHint(true);
    try {
      const updated = await codingApi.askCodingHint({
        operationId: `hint-${Date.now()}`,
        sessionId: sessionState.id,
        expectedRevision: sessionState.revision,
      });
      updateSession(updated);
    } finally {
      setAskingHint(false);
    }
  };

  // Counterpart thread turns
  const handleSendTurn = async (content: string, spoken: boolean) => {
    setCounterpartBusy('Reading your code…');
    try {
      const updated = await codingApi.sendCodingTurn({
        operationId: `turn-${Date.now()}`,
        sessionId: sessionState.id,
        content,
        spoken,
        expectedRevision: sessionState.revision,
      });
      updateSession(updated);
    } finally {
      setCounterpartBusy(null);
    }
  };

  // Finish session
  const handleFinish = async () => {
    try {
      const finished = await codingApi.finishCodingSession({
        operationId: `finish-${Date.now()}`,
        sessionId: sessionState.id,
        expectedRevision: sessionState.revision,
      });
      updateSession(finished);
      onFinish?.(finished);
    } catch {
      // Handle error
    }
  };

  const counterpartName = sessionState.setting === 'coached' ? 'Coach' : 'Interviewer';
  const helpBadge = sessionState.setting === 'coached' ? 'Full help' : 'Hints only';
  const settingLabel =
    sessionState.setting === 'coached'
      ? 'Coached'
      : sessionState.setting === 'mock'
      ? 'Mock'
      : 'Watched';

  const threadTurns: PracticeThreadTurn[] = sessionState.turns.map((t) => ({
    id: t.id,
    speaker: t.speaker === 'learner' ? 'learner' : sessionState.setting === 'coached' ? 'coach' : 'counterpart',
    name: t.speaker === 'learner' ? 'You' : counterpartName,
    content: t.content,
    createdAt: t.createdAt,
  }));

  const fileTabsList: FileTabItem[] = sessionState.draftFiles.map((f) => ({
    path: f.path,
    label: f.path,
  }));

  const lastRun = sessionState.runs[sessionState.runs.length - 1] ?? null;

  return (
    <div className={`flex flex-col min-h-screen bg-bg text-text-primary ${className}`}>
      {/* Top Header */}
      <header className="border-b border-border-default bg-surface px-4 py-3 sm:px-6">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-4">
          <div className="flex flex-wrap items-center gap-3">
            {onExit && (
              <button
                type="button"
                onClick={onExit}
                className="text-xs font-medium text-text-secondary hover:text-text-primary cursor-pointer"
              >
                ← Practice
              </button>
            )}
            <h1 className="font-serif text-lg font-semibold text-text-primary sm:text-xl m-0">
              {sessionState.problem.title}
            </h1>
            <span className="inline-flex items-center rounded-md bg-accent-muted px-2.5 py-1 text-xs font-semibold text-accent">
              {settingLabel}
            </span>
            <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-raised px-2.5 py-0.5 text-xs font-medium text-text-secondary">
              <Bot className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{helpBadge}</span>
            </span>
          </div>

          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2 text-xs text-text-secondary">
              <span>Elapsed</span>
              <ElapsedClock
                elapsedMs={sessionState.activeMs}
                running={!sessionState.finishedAt}
              />
              {sessionState.budgetSeconds && (
                <span className="text-text-tertiary">
                  of {Math.round(sessionState.budgetSeconds / 60)} min
                </span>
              )}
            </div>
            <button
              type="button"
              onClick={handleFinish}
              className="inline-flex min-h-[36px] cursor-pointer items-center justify-center rounded-lg border border-border-default bg-surface px-4 text-xs font-semibold text-text-primary hover:bg-surface-raised"
            >
              Finish
            </button>
          </div>
        </div>

        {/* Stage Rail */}
        <div className="mx-auto max-w-7xl pt-3">
          <StageRail
            currentStage={sessionState.stage}
            onSelectStage={handleStageSelect}
          />
        </div>
      </header>

      {/* Mobile / Narrow Screen Tab Switcher */}
      <div className="lg:hidden border-b border-border-default bg-surface px-4 py-2">
        <div role="tablist" aria-label="Workspace panels" className="flex items-center gap-2">
          <button
            type="button"
            role="tab"
            aria-selected={mobileTab === 'problem'}
            onClick={() => setMobileTab('problem')}
            className={`min-h-[36px] rounded-lg px-3 text-xs font-medium ${
              mobileTab === 'problem'
                ? 'bg-accent text-accent-fg font-semibold'
                : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            Problem
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mobileTab === 'code'}
            onClick={() => setMobileTab('code')}
            className={`min-h-[36px] rounded-lg px-3 text-xs font-medium ${
              mobileTab === 'code'
                ? 'bg-accent text-accent-fg font-semibold'
                : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            Code
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mobileTab === 'counterpart'}
            onClick={() => setMobileTab('counterpart')}
            className={`min-h-[36px] rounded-lg px-3 text-xs font-medium ${
              mobileTab === 'counterpart'
                ? 'bg-accent text-accent-fg font-semibold'
                : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            {counterpartName}
          </button>
        </div>
      </div>

      {/* Main Workspace Body: 3-column on lg+, tabs on mobile */}
      <main className="mx-auto max-w-7xl w-full flex-1 p-4 sm:p-6">
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
          {/* Column 1: Problem & Examples (4 cols on lg) */}
          <div
            className={`space-y-6 lg:col-span-4 ${
              mobileTab === 'problem' ? 'block' : 'hidden lg:block'
            }`}
          >
            <ProblemPane
              problem={sessionState.problem}
              setting={sessionState.setting}
            />
            <ExamplesTable
              examples={sessionState.examples}
              totalMisreadings={sessionState.problem.totalMisreadings}
              onCheckExample={handleCheckExample}
              checking={checkingExample}
              paramNames={sessionState.problem.signature.params.map((p) => p.name)}
            />
          </div>

          {/* Column 2: Code Editor, RunBar, & HintLadder (5 cols on lg) */}
          <div
            className={`space-y-6 lg:col-span-5 ${
              mobileTab === 'code' ? 'block' : 'hidden lg:block'
            }`}
          >
            <div className="rounded-xl border border-border-default bg-surface shadow-xs overflow-hidden">
              <FileTabs
                files={fileTabsList}
                activePath={activeFile}
                onSelect={setActiveFile}
              />
              <div
                className="bg-surface-sunken"
                onInput={(e) => {
                  const target = e.target as HTMLElement | null;
                  if (
                    target?.classList?.contains('cm-content') &&
                    !target.querySelector('.cm-line') &&
                    typeof target.textContent === 'string'
                  ) {
                    handleCodeChange(target.textContent);
                  }
                }}
              >
                <LearningCodeEditor
                  path={activeFileObj?.path ?? 'solution.py'}
                  value={fileContent}
                  onChange={handleCodeChange}
                  readOnly={false}
                  onRunShortcut={() => handleRun('examples')}
                  minHeight={320}
                />
              </div>
            </div>

            <RunBar
              onRunExamples={() => handleRun('examples')}
              onRunCustom={(customInput) => handleRun('input', customInput)}
              onSubmit={() => handleRun('all')}
              running={running}
              lastRun={lastRun}
            />

            {/* HintLadder is visible ONLY in Coached mode */}
            {sessionState.setting === 'coached' && (
              <HintLadder
                hints={sessionState.hints}
                availableNextRung={sessionState.availableNextRung}
                onAskHint={handleAskHint}
                asking={askingHint}
              />
            )}
          </div>

          {/* Column 3: Counterpart Thread (3 cols on lg) */}
          <div
            className={`lg:col-span-3 ${
              mobileTab === 'counterpart' ? 'block' : 'hidden lg:block'
            }`}
          >
            <div className="rounded-xl border border-border-default bg-surface p-4 shadow-xs">
              <div className="mb-3 flex items-center gap-2 border-b border-border-subtle pb-3">
                <MessageSquare className="h-4 w-4 text-accent" aria-hidden="true" />
                <h2 className="font-serif text-lg font-semibold text-text-primary m-0">
                  {counterpartName}
                </h2>
              </div>

              <PracticeThread
                draftKey={`coding-session-${sessionState.id}`}
                turns={threadTurns}
                onSend={handleSendTurn}
                busy={counterpartBusy}
                busyLabel={counterpartBusy}
                placeholder={
                  sessionState.setting === 'watched'
                    ? 'Type your thinking, or use the mic…'
                    : 'Ask, or explain your thinking…'
                }
                hint={
                  sessionState.setting === 'watched'
                    ? 'Talk through your thinking: type or use the mic'
                    : undefined
                }
                footerActions={
                  sessionState.setting === 'watched' ? (
                    <div className="flex items-center gap-2 pt-2">
                      <button
                        type="button"
                        onClick={handleAskHint}
                        disabled={askingHint}
                        className="inline-flex min-h-[36px] cursor-pointer items-center gap-1.5 rounded-lg border border-border-default bg-surface px-3 text-xs font-semibold text-text-primary hover:bg-surface-raised"
                      >
                        <HelpCircle className="h-3.5 w-3.5 text-text-secondary" aria-hidden="true" />
                        <span>I&apos;m stuck</span>
                      </button>
                      <span className="text-xxs text-text-tertiary">
                        Hints are noted in your debrief.
                      </span>
                    </div>
                  ) : undefined
                }
              />
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
