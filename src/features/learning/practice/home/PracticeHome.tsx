import { useState } from 'react';
import {

  ArrowRight,
  Bot,
  Code2,
  FileSearch,
  MessageSquare,
  Sparkles,
  Users,
} from 'lucide-react';

import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';
import { DueStrip } from './DueStrip';
import { type HistoryItem, History } from './History';
import type { DueItemDto, LearningPracticeProfileDto } from './practiceHomeApi';
import { PreparingFor } from './PreparingFor';
import { StartSentence } from './StartSentence';

export interface OpenPracticeTarget {
  id: string;
  title: string;
  kind: 'explain' | 'spot' | 'code' | 'mock' | 'role';
  settingOrHelp?: string;
  stoppedAt?: string;
  elapsedMinutes?: number;
  stageInfo?: string;
}

export interface PracticeHomeProps {
  program: LearningProgramDto;
  currentLesson?: LearningLessonDto | null;
  profile?: LearningPracticeProfileDto | null;
  openTarget?: OpenPracticeTarget | null;
  dueItems?: DueItemDto[];
  historyItems?: HistoryItem[];
  onContinue?: (target: OpenPracticeTarget) => void;
  onOpenSession?: (sessionId: string) => void;
  onOpenAttempt?: (sessionId: string) => void;
  onOpenDrill?: (drillId?: string) => void;
  onStartCard?: (cardId: 'explain' | 'spot' | 'code' | 'mock' | 'role') => void;
  className?: string;
}

export function PracticeHome({
  program,
  currentLesson,
  profile: initialProfile,
  openTarget,
  dueItems,
  historyItems,
  onContinue,
  onStartCard,
  className,
}: PracticeHomeProps) {
  const [profile, setProfile] = useState<LearningPracticeProfileDto | null>(
    initialProfile ?? null,
  );

  const activeProfile = profile ?? initialProfile;
  const programId = program.summary.id;

  const fitsInterview = Boolean(
    activeProfile?.interviewAt ||
      (activeProfile?.details &&
        /interview/i.test(activeProfile.details)),
  );

  const lessonName = currentLesson?.title ?? 'lesson';

  const courseContext = [
    program.summary.title,
    program.summary.goal,
    currentLesson?.title ?? '',
    currentLesson?.objective ?? '',
    activeProfile?.details ?? '',
  ].join(' ');
  const isCodingCourse =
    Boolean(activeProfile?.language) ||
    /\b(code|coding|python|rust|javascript|typescript|java|c\+\+|sql|algorithm|algorithmic|software|programming|data structure|strings|arrays)\b/i.test(
      courseContext,
    );
  const defaultSpotMaterial = isCodingCourse ? 'Python code' : 'explanation';
  const defaultMockRound = isCodingCourse ? 'live coding' : 'technical questions';

  return (
    <div className={`space-y-8 ${className ?? ''}`}>
      {/* 1. Preparing for */}
      <PreparingFor
        programId={programId}
        courseGoal={program.summary.goal}
        profile={activeProfile}
        onProfileChange={setProfile}
      />

      {/* 2. Continue card (when open target exists) & 3. DueStrip */}
      <div className="grid grid-cols-1 items-stretch gap-4 lg:grid-cols-2">
        {openTarget && (
          <section
            aria-labelledby="continue-heading"
            className="flex flex-col justify-between rounded-xl border border-border-default bg-surface p-5 shadow-xs"
          >
            <div className="space-y-3">
              <div
                id="continue-subhead"
                className="text-xs font-semibold uppercase tracking-wider text-text-tertiary"
              >
                Continue
              </div>
              <h2
                id="continue-heading"
                className="font-serif text-lg font-medium text-text-primary"
              >
                {openTarget.title}
              </h2>
              {openTarget.settingOrHelp && (
                <div className="text-xs text-text-secondary">
                  {openTarget.settingOrHelp}
                </div>
              )}
              {/* Progress track */}
              <div
                aria-hidden="true"
                className="flex h-1.5 w-full gap-1 overflow-hidden rounded-full bg-surface-raised"
              >
                <div className="h-full w-1/3 rounded-full bg-accent" />
                <div className="h-full w-1/4 rounded-full border border-accent bg-accent-muted" />
              </div>
              {openTarget.stoppedAt && (
                <div className="text-xs text-text-secondary">
                  {openTarget.stoppedAt}
                </div>
              )}
            </div>
            <div className="pt-4">
              <button
                type="button"
                onClick={() => onContinue?.(openTarget)}
                className="inline-flex min-h-[44px] cursor-pointer items-center justify-center gap-2 rounded-lg bg-accent px-5 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span>Continue</span>
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>
          </section>
        )}

        <div className={openTarget ? '' : 'lg:col-span-2'}>
          <DueStrip
            programId={programId}
            items={dueItems}
            onStartMostOverdue={() => onStartCard?.('code')}
          />
        </div>
      </div>

      {/* 4. Five cards in fixed order */}
      <section aria-labelledby="start-new-heading" className="space-y-4">
        <div>
          <h2
            id="start-new-heading"
            className="font-serif text-2xl font-semibold text-text-primary"
          >
            Start something new
          </h2>
          <p className="mt-1 text-sm text-text-secondary">
            Each option reads as a sentence. Click a highlighted word to change
            it.
          </p>
        </div>

        <div className="grid grid-cols-1 items-stretch gap-4 md:grid-cols-2 xl:grid-cols-3">
          {/* Card 1: Explain */}
          <div
            data-testid="card-explain"
            className="flex flex-col justify-between rounded-xl border border-border-default bg-surface p-5 shadow-xs transition-shadow hover:shadow-sm"
          >
            <div className="space-y-3">
              <div className="grid h-10 w-10 place-items-center rounded-lg bg-surface-raised text-accent">
                <MessageSquare className="h-5 w-5" aria-hidden="true" />
              </div>
              <h3 className="font-serif text-lg font-medium text-text-primary">
                Explain
              </h3>
              <p className="text-xs text-text-secondary">
                Answer in your own words, get feedback, revise.
              </p>
              <div className="py-1">
                <StartSentence
                  programId={programId}
                  cardId="explain"
                  sentencePrefix="A question on "
                  sentenceSuffix="."
                  slots={[
                    {
                      id: 'topic',
                      label: 'Topic',
                      value: lessonName,
                      options: [
                        { value: lessonName, label: lessonName },
                        {
                          value: 'Teach it to a beginner',
                          label: 'Teach it to a beginner',
                          description: 'Feynman technique',
                        },
                      ],
                    },
                    {
                      id: 'help',
                      label: 'Help',
                      value: 'hints only',
                      options: [
                        { value: 'hints only', label: 'hints only' },
                        { value: 'full help', label: 'full help' },
                        { value: 'no help', label: 'no help' },
                      ],
                    },
                  ]}
                />
              </div>
            </div>
            <div className="pt-4">
              <button
                type="button"
                onClick={() => onStartCard?.('explain')}
                className="inline-flex min-h-[44px] w-full cursor-pointer items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Start
              </button>
            </div>
          </div>

          {/* Card 2: Spot the mistakes */}
          <div
            data-testid="card-spot"
            className="flex flex-col justify-between rounded-xl border border-border-default bg-surface p-5 shadow-xs transition-shadow hover:shadow-sm"
          >
            <div className="space-y-3">
              <div className="grid h-10 w-10 place-items-center rounded-lg bg-surface-raised text-accent">
                <FileSearch className="h-5 w-5" aria-hidden="true" />
              </div>
              <h3 className="font-serif text-lg font-medium text-text-primary">
                Spot the mistakes
              </h3>
              <p className="text-xs text-text-secondary">
                Review code or an explanation with planted mistakes, then see
                what you missed.
              </p>
              <div className="py-1">
                <StartSentence
                  programId={programId}
                  cardId="spot"
                  sentencePrefix=""
                  sentenceSuffix="."
                  slots={[
                    {
                      id: 'material',
                      label: 'Material',
                      value: defaultSpotMaterial,
                      options: [
                        { value: defaultSpotMaterial, label: 'Python code' },
                        { value: 'explanation', label: 'explanation' },
                      ],
                    },
                    {
                      id: 'lesson',
                      label: 'Lesson',
                      value: lessonName,
                      options: [{ value: lessonName, label: lessonName }],
                    },
                    {
                      id: 'timing',
                      label: 'Timing',
                      value: 'untimed',
                      options: [
                        { value: 'untimed', label: 'untimed' },
                        { value: 'timed', label: 'timed' },
                      ],
                    },
                  ]}
                />
              </div>
            </div>
            <div className="pt-4">
              <button
                type="button"
                onClick={() => onStartCard?.('spot')}
                className="inline-flex min-h-[44px] w-full cursor-pointer items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Start
              </button>
            </div>
          </div>

          {/* Card 3: Code */}
          <div
            data-testid="card-code"
            className="flex flex-col justify-between rounded-xl border border-border-default bg-surface p-5 shadow-xs transition-shadow hover:shadow-sm"
          >
            <div className="space-y-3">
              <div className="grid h-10 w-10 place-items-center rounded-lg bg-surface-raised text-accent">
                <Code2 className="h-5 w-5" aria-hidden="true" />
              </div>
              <h3 className="font-serif text-lg font-medium text-text-primary">
                Code
              </h3>
              <p className="text-xs text-text-secondary">
                Solve a problem, fix a bug or build a project, and run it.
              </p>
              <div className="py-1">
                <StartSentence
                  programId={programId}
                  cardId="code"
                  sentencePrefix="An "
                  sentenceSuffix="."
                  slots={[
                    {
                      id: 'difficulty',
                      label: 'Difficulty',
                      value: 'easy',
                      options: [
                        { value: 'easy', label: 'easy' },
                        { value: 'medium', label: 'medium' },
                        { value: 'hard', label: 'hard' },
                      ],
                    },
                    {
                      id: 'language',
                      label: 'Language',
                      value: 'Python',
                      options: [
                        { value: 'Python', label: 'Python' },
                        { value: 'Rust', label: 'Rust' },
                      ],
                    },
                    {
                      id: 'task',
                      label: 'Task',
                      value: 'problem',
                      options: [
                        { value: 'problem', label: 'problem' },
                        { value: 'fix a bug', label: 'fix a bug' },
                        { value: 'project', label: 'project' },
                      ],
                    },
                    {
                      id: 'setting',
                      label: 'Setting',
                      value: 'coached',
                      options: [
                        { value: 'coached', label: 'coached' },
                        { value: 'mock', label: 'mock' },
                        { value: 'watched', label: 'watched' },
                      ],
                    },
                  ]}
                />
              </div>
            </div>
            <div className="pt-4">
              <button
                type="button"
                onClick={() => onStartCard?.('code')}
                className="inline-flex min-h-[44px] w-full cursor-pointer items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Start
              </button>
            </div>
          </div>

          {/* Card 4: Mock interview */}
          <div
            data-testid="card-mock"
            className={`flex flex-col justify-between rounded-xl border p-5 shadow-xs transition-shadow hover:shadow-sm ${
              fitsInterview
                ? 'border-accent bg-surface'
                : 'border-border-default bg-surface'
            }`}
          >
            <div className="space-y-3">
              <div className="grid h-10 w-10 place-items-center rounded-lg bg-surface-raised text-accent">
                <Bot className="h-5 w-5" aria-hidden="true" />
              </div>
              {fitsInterview && (
                <div>
                  <span className="inline-flex min-h-[28px] items-center gap-1.5 rounded-full bg-accent-muted px-2.5 text-xs font-semibold text-accent">
                    <Sparkles className="h-3.5 w-3.5" aria-hidden="true" />
                    Fits what you&apos;re preparing for
                  </span>
                </div>
              )}
              <h3 className="font-serif text-lg font-medium text-text-primary">
                Mock interview
              </h3>
              <p className="text-xs text-text-secondary">
                Rehearse with an interviewer: live coding, debugging, technical
                questions, system design or behavioral.
              </p>
              <div className="py-1">
                <StartSentence
                  programId={programId}
                  cardId="mock"
                  sentencePrefix="A "
                  sentenceSuffix="."
                  slots={[
                    {
                      id: 'round',
                      label: 'Round',
                      value: defaultMockRound,
                      options: [
                        { value: defaultMockRound, label: 'live coding' },
                        { value: 'debugging', label: 'debugging' },
                        {
                          value: 'technical questions',
                          label: 'technical questions',
                        },
                        { value: 'system design', label: 'system design' },
                        { value: 'behavioral', label: 'behavioral' },
                      ],
                    },
                    {
                      id: 'setting',
                      label: 'Setting',
                      value: 'watched',
                      options: [
                        { value: 'mock', label: 'mock' },
                        { value: 'watched', label: 'watched' },
                      ],
                    },
                    {
                      id: 'budget',
                      label: 'Time budget',
                      value: '30 min',
                      options: [
                        { value: '20 min', label: '20 min' },
                        { value: '30 min', label: '30 min' },
                        { value: '45 min', label: '45 min' },
                        { value: '60 min', label: '60 min' },
                      ],
                    },
                  ]}
                />
              </div>
            </div>
            <div className="pt-4">
              <button
                type="button"
                onClick={() => onStartCard?.('mock')}
                className="inline-flex min-h-[44px] w-full cursor-pointer items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Start
              </button>
            </div>
          </div>

          {/* Card 5: Role-play */}
          <div
            data-testid="card-role"
            className="flex flex-col justify-between rounded-xl border border-border-default bg-surface p-5 shadow-xs transition-shadow hover:shadow-sm"
          >
            <div className="space-y-3">
              <div className="grid h-10 w-10 place-items-center rounded-lg bg-surface-raised text-accent">
                <Users className="h-5 w-5" aria-hidden="true" />
              </div>
              <h3 className="font-serif text-lg font-medium text-text-primary">
                Role-play
              </h3>
              <p className="text-xs text-text-secondary">
                Talk through a scenario: an incident, a design review, a hard
                conversation.
              </p>
              <div className="py-1">
                <StartSentence
                  programId={programId}
                  cardId="role"
                  sentencePrefix=""
                  sentenceSuffix="."
                  slots={[
                    {
                      id: 'scenario',
                      label: 'Scenario',
                      value: 'An incident',
                      options: [
                        { value: 'An incident', label: 'An incident' },
                        {
                          value: 'A design review',
                          label: 'A design review',
                        },
                        {
                          value: 'A difficult feedback conversation',
                          label: 'A difficult feedback conversation',
                        },
                      ],
                    },
                    {
                      id: 'role',
                      label: 'Your role',
                      value: 'the on-call lead',
                      options: [
                        {
                          value: 'the on-call lead',
                          label: 'the on-call lead',
                        },
                        { value: 'the reviewer', label: 'the reviewer' },
                        {
                          value: 'the engineering manager',
                          label: 'the engineering manager',
                        },
                      ],
                    },
                  ]}
                />
              </div>
            </div>
            <div className="pt-4">
              <button
                type="button"
                onClick={() => onStartCard?.('role')}
                className="inline-flex min-h-[44px] w-full cursor-pointer items-center justify-center rounded-lg bg-accent px-4 text-sm font-semibold text-accent-fg transition-opacity hover:opacity-90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                Start
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* 5. History */}
      <History
        programId={programId}
        items={historyItems}
      />
    </div>
  );
}
