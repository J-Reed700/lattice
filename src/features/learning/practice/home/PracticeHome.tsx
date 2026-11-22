import { useCallback, useId, useMemo, useState } from 'react';

import { ArrowRight, Bot, Code2, FileSearch, Loader2, MessageSquare, Sparkles, Users, type LucideIcon } from 'lucide-react';

import { PROGRAM_KIND_ID } from '@/features/learning/practice/kinds';
import { useCodingBuildStore } from '@/features/learning/practice/kinds/program/codingBuildStore';
import type { CodingBuildChoice } from '@/features/learning/practice/kinds/program/CodingEntry';
import { StatusLine } from '@/features/learning/practice/shared/StatusLine';
import type * as Wire from '@/lib/bindings';

import { DueStrip } from './DueStrip';
import { History } from './History';
import { CARD_LABELS, type HistoryItem, type HistoryTarget } from './historyItems';
import {
  DIFFICULTY_OPTIONS,
  EXPLAIN_OPTIONS,
  MOCK_MINUTES_OPTIONS,
  MOCK_SETTING_OPTIONS,
  SCENARIO_OPTIONS,
  TASK_OPTIONS,
  CODING_LANGUAGE,
  LANGUAGE_OPTIONS,
  fitState,
  isCodingRound,
  mockRoundOptions,
  startAction,
  suggestsInterview,
  visibleCards,
  type CardId,
  type StartAction,
} from './practiceChoices';
import { PreparingFor } from './PreparingFor';
import { StartSentence, type ChoiceSlot, type SentencePart } from './StartSentence';
import { usePracticeBuildJob, usePracticeDue, usePracticeFit, usePracticeHistory, usePracticeProfile } from './usePracticeData';

export interface PracticeHomeProps {
  programId: string;
  goal: string;
  lessonId: string | null;
  onStart: (action: StartAction) => void;
  onOpenTarget: (target: HistoryTarget) => void;
  onStartDue: (item: Wire.DueItemDto) => void;
  onOpenWorkbench: () => void;
  onResumeBuild?: (choice: CodingBuildChoice) => void;
}

const ICONS: Record<CardId, LucideIcon> = { explain: MessageSquare, spot: FileSearch, code: Code2, mock: Bot, role: Users };

function listOf(items: string[]): string {
  return items.length < 2 ? (items[0] ?? '') : `${items.slice(0, -1).join(', ')} or ${items[items.length - 1]}`;
}

/** What each card promises, naming only what the course can offer. */
function promises(fit: ReturnType<typeof fitState>): Record<CardId, string> {
  return {
    explain: 'Answer in your own words, get feedback, revise.',
    spot: `Review ${fit === 'fits' ? 'code or an explanation' : 'an explanation'} with planted mistakes, then see what you missed.`,
    code: 'Solve a problem, fix a bug or build a project, and run it.',
    mock: `Rehearse with an interviewer: ${listOf(mockRoundOptions(fit).map((round) => round.label))}.`,
    role: 'Talk through a scenario: an incident, a design review, a hard conversation.',
  };
}

const article = (word: string) => (/^[aeiou]/i.test(word) ? 'An ' : 'A ');

interface CardDefinition {
  slots: ChoiceSlot[];
  parts: (values: Record<string, string>) => SentencePart[];
}

function definitions(fit: ReturnType<typeof fitState>): Record<CardId, CardDefinition> {
  const rounds = mockRoundOptions(fit);
  return {
    explain: {
      slots: [{ id: 'style', label: 'How to answer', options: EXPLAIN_OPTIONS, defaultValue: 'own' }],
      parts: (values) => (values.style === 'beginner' ? ['Teach it ', { slot: 'style' }, '.'] : ['Answer ', { slot: 'style' }, '.']),
    },
    spot: {
      slots: [],
      parts: () => [`Find the planted mistakes in a lesson's ${fit === 'fits' ? 'code or explanation' : 'explanation'}.`],
    },
    code: {
      slots: [
        { id: 'difficulty', label: 'Difficulty', options: DIFFICULTY_OPTIONS, defaultValue: 'easy' },
        { id: 'language', label: 'Language', options: LANGUAGE_OPTIONS, defaultValue: CODING_LANGUAGE },
        { id: 'task', label: 'Task', options: TASK_OPTIONS, defaultValue: 'build' },
        { id: 'setting', label: 'Setting', options: [{ value: 'coached', label: 'coached' }], defaultValue: 'coached' },
      ],
      parts: (values) => [article(values.difficulty ?? 'easy'), { slot: 'difficulty' }, ' ', { slot: 'language' }, ' ', { slot: 'task' }, ', ', { slot: 'setting' }, '.'],
    },
    mock: {
      slots: [
        { id: 'round', label: 'Round', options: rounds, defaultValue: rounds[0]?.value ?? 'technical' },
        { id: 'setting', label: 'Setting', options: MOCK_SETTING_OPTIONS, defaultValue: 'mock' },
        { id: 'minutes', label: 'Time', options: MOCK_MINUTES_OPTIONS, defaultValue: '20' },
      ],
      parts: (values) =>
        isCodingRound(values.round ?? '')
          ? ['A ', { slot: 'round' }, ' round, ', { slot: 'setting' }, ', ', { slot: 'minutes' }, '.']
          : ['A ', { slot: 'round' }, ' round.'],
    },
    role: {
      slots: [{ id: 'scenario', label: 'Scenario', options: SCENARIO_OPTIONS, defaultValue: 'incident' }],
      parts: () => ['Talk through ', { slot: 'scenario' }, '.'],
    },
  };
}

interface CardProps {
  id: CardId;
  programId: string;
  definition: CardDefinition;
  promise: string;
  badge?: string;
  onStart: (values: Record<string, string>) => void;
}

function Card({ id, programId, definition, promise, badge, onStart }: CardProps) {
  const uid = useId();
  const Icon = ICONS[id];
  const [values, setValues] = useState<Record<string, string>>({});
  const title = CARD_LABELS[id];
  const handleChange = useCallback((next: Record<string, string>) => setValues(next), []);

  return (
    <article data-testid={`card-${id}`} aria-labelledby={`${uid}-title`} className="flex flex-col justify-between gap-4 rounded-xl border border-border-default bg-surface p-5">
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="grid h-10 w-10 place-items-center rounded-lg bg-surface-raised text-accent">
            <Icon className="h-5 w-5" aria-hidden="true" />
          </span>
          {badge && (
            <span className="inline-flex min-h-7 items-center gap-1.5 rounded-full bg-accent-muted px-2.5 text-xs font-semibold text-accent">
              <Sparkles className="h-3.5 w-3.5" aria-hidden="true" />
              {badge}
            </span>
          )}
        </div>
        <h3 id={`${uid}-title`} className="font-serif text-lg font-medium text-text-primary">
          {title}
        </h3>
        <p id={`${uid}-promise`} className="text-sm text-text-secondary">
          {promise}
        </p>
        <StartSentence
          programId={programId}
          cardId={id}
          legend={`${title} options`}
          slots={definition.slots}
          parts={definition.parts}
          describedBy={`${uid}-promise`}
          onChange={handleChange}
        />
      </div>
      <button
        type="button"
        aria-label={`Start ${title}`}
        onClick={() => onStart(values)}
        className="inline-flex min-h-11 w-full items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
      >
        Start
      </button>
    </article>
  );
}

/** The Practice page: what the learner is preparing for, where they stopped, what is due, five cards, and their history. */
export function PracticeHome({ programId, goal, lessonId, onStart, onOpenTarget, onStartDue, onOpenWorkbench, onResumeBuild }: PracticeHomeProps) {
  const profileState = usePracticeProfile(programId);
  const fitStatus = usePracticeFit(programId);
  const due = usePracticeDue(programId);
  const history = usePracticeHistory(programId);
  const codingBuild = useCodingBuildStore((store) => store.handles[programId]);
  const attachBackendJob = useCodingBuildStore((store) => store.attachBackendJob);
  const clearCodingBuild = useCodingBuildStore((store) => store.clearHandle);
  const activeBuildJob = usePracticeBuildJob(programId);

  const fit = fitState(fitStatus.fit, PROGRAM_KIND_ID);
  const profile = profileState.profile ?? null;
  const promiseOf = useMemo(() => promises(fit), [fit]);
  const defs = useMemo(() => definitions(fit), [fit]);
  const cards = visibleCards(fit);
  const context = { lessonId, profile };
  const continueItem: HistoryItem | null = history.continueItem;

  const fallbackChoice: CodingBuildChoice = {
    lessonId,
    form: 'build',
    setting: 'coached',
    language: CODING_LANGUAGE,
    difficulty: 'easy',
  };
  const showActiveBuild =
    codingBuild?.phase.kind === 'building' ||
    codingBuild?.phase.kind === 'starting' ||
    Boolean(activeBuildJob && !codingBuild);
  const showBuiltReady = codingBuild?.phase.kind === 'built' || codingBuild?.phase.kind === 'ready';
  const showBuildFailed = codingBuild?.phase.kind === 'failed';

  return (
    <div data-testid="practice-home" className="space-y-8">
      {(showActiveBuild || showBuiltReady || showBuildFailed) && (
        <section
          aria-label="Coding build status"
          className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border-default bg-surface p-4"
        >
          <div className="min-w-0 flex-1">
            {showActiveBuild ? (
              <p role="status" className="flex items-center gap-2 text-sm font-medium text-text-primary">
                <Loader2 className="h-4 w-4 shrink-0 animate-spin text-accent" aria-hidden="true" />
                Building your coding problem…
              </p>
            ) : showBuiltReady ? (
              <p role="status" className="text-sm font-medium text-text-primary">
                Your coding problem is ready.
              </p>
            ) : codingBuild?.phase.kind === 'failed' ? (
              <p role="alert" className="text-sm text-danger-fg">
                {codingBuild.phase.message}
              </p>
            ) : null}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {showActiveBuild && (
              <button
                type="button"
                onClick={() => {
                  if (activeBuildJob && !codingBuild) {
                    attachBackendJob(programId, activeBuildJob, fallbackChoice);
                    onResumeBuild?.(fallbackChoice);
                  } else if (codingBuild) {
                    onResumeBuild?.(codingBuild.choice);
                  }
                }}
                className="inline-flex min-h-10 items-center gap-1.5 rounded-lg bg-accent px-3.5 text-xs font-semibold text-accent-fg hover:opacity-90"
              >
                Open build
              </button>
            )}
            {showBuiltReady && codingBuild && (
              <button
                type="button"
                onClick={() => onResumeBuild?.(codingBuild.choice)}
                className="inline-flex min-h-10 items-center gap-1.5 rounded-lg bg-accent px-3.5 text-xs font-semibold text-accent-fg hover:opacity-90"
              >
                Open problem
              </button>
            )}
            {showBuildFailed && codingBuild && (
              <>
                <button
                  type="button"
                  onClick={() => onResumeBuild?.(codingBuild.choice)}
                  className="inline-flex min-h-10 items-center gap-1.5 rounded-lg bg-accent px-3.5 text-xs font-semibold text-accent-fg hover:opacity-90"
                >
                  View build
                </button>
                <button
                  type="button"
                  onClick={() => clearCodingBuild(programId)}
                  className="inline-flex min-h-10 items-center rounded-lg border border-border-default px-3 text-xs font-medium text-text-secondary hover:bg-surface-raised"
                >
                  Dismiss
                </button>
              </>
            )}
          </div>
        </section>
      )}
      <PreparingFor
        programId={programId}
        goal={goal}
        profile={profile}
        loading={profileState.loading}
        loadError={profileState.error}
        onRetryLoad={profileState.retry}
        onSaved={profileState.set}
        onReload={profileState.reload}
      />

      {continueItem && (
        <section aria-labelledby="practice-continue-heading" className="rounded-xl border border-border-default bg-surface p-5">
          <h2 id="practice-continue-heading" className="text-xs font-semibold uppercase tracking-wider text-text-muted">
            Continue
          </h2>
          <p className="mt-1 text-base font-medium text-text-primary">{continueItem.title}</p>
          <p className="text-sm text-text-secondary">
            {CARD_LABELS[continueItem.card]} · {continueItem.detail}
            {continueItem.stoppedAt ? ` · ${continueItem.stoppedAt}` : ''}
          </p>
          <button
            type="button"
            onClick={() => onOpenTarget(continueItem.target)}
            className="mt-3 inline-flex min-h-11 items-center gap-2 rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            Continue
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </button>
        </section>
      )}

      <DueStrip items={due.items} loading={due.loading} error={due.error} onRetry={due.retry} onStart={onStartDue} />

      <section aria-labelledby="practice-start-heading" className="space-y-3">
        <h2 id="practice-start-heading" className="font-serif text-xl font-semibold text-text-primary">
          Start something new
        </h2>
        {fitStatus.checking && <StatusLine state={{ kind: 'busy', label: 'Checking whether this course teaches code…' }} />}
        {fitStatus.error && <StatusLine state={{ kind: 'error', message: fitStatus.error, retry: fitStatus.retry }} />}
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {cards.map((id) => (
            <Card
              key={id}
              id={id}
              programId={programId}
              definition={defs[id]}
              promise={promiseOf[id]}
              badge={id === 'mock' && suggestsInterview(profile) ? "Fits what you're preparing for" : undefined}
              onStart={(values) => onStart(startAction(id, values, context))}
            />
          ))}
        </div>
      </section>

      <History
        items={history.items}
        loading={history.loading}
        error={history.error}
        onRetry={history.retry}
        onOpen={(item) => onOpenTarget(item.target)}
        onOpenWorkbench={onOpenWorkbench}
      />
    </div>
  );
}
