import type * as Wire from '@/lib/bindings';

import type { CardId } from './practiceChoices';

export type HistoryTarget =
  | { kind: 'coding'; sessionId: string }
  | { kind: 'coding-activity'; activityId: string }
  | { kind: 'activity'; activityId: string }
  | { kind: 'attempt' }
  | { kind: 'drill' };

export interface HistoryItem {
  id: string;
  card: CardId;
  title: string;
  when: number;
  detail: string;
  result: string;
  /** Work left open, which Continue can pick up. */
  resumable: boolean;
  /** Where it stopped, for Continue. */
  stoppedAt: string | null;
  target: HistoryTarget;
}

export interface HistorySources {
  coding: Wire.CodingSessionSummaryDto[];
  activities: Wire.LearningPracticalActivityDto[];
  simulations: Wire.LearningSimulationSessionDto[];
  attempts: Wire.LearningPracticeSessionSummaryDto[];
  drills: Wire.CourseDrillAttemptDto[];
}

const SETTING_LABELS: Record<Wire.CodingSetting, string> = { coached: 'Coached', mock: 'Mock', watched: 'Watched' };
const TASK_LABELS: Record<Wire.CodingTask, string> = { problem: 'Problem', bug_fix: 'Bug to fix', project: 'Project' };
const HELP_LABELS: Record<Wire.LearningPracticeMode, string> = { explore: 'Full help', practice: 'Hints only', demonstrate: 'No help' };
const DRILL_MODE_LABELS: Record<Wire.AttemptMode, string> = { practice: 'Untimed', interview: 'Timed', rapid: 'Rapid' };

const capitalize = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** The card an old practical activity belongs to under the new page. */
export function cardOfActivity(kind: Wire.LearningPracticalActivityKind): CardId {
  switch (kind) {
    case 'interview':
      return 'mock';
    case 'incident':
    case 'system_design':
    case 'conversation':
      return 'role';
    case 'code_review':
      return 'spot';
    case 'writing_revision':
      return 'explain';
    default:
      return 'code';
  }
}

export const CARD_LABELS: Record<CardId, string> = {
  explain: 'Explain',
  spot: 'Spot the mistakes',
  code: 'Code',
  mock: 'Mock interview',
  role: 'Role-play',
};

function latestSimulation(sims: Wire.LearningSimulationSessionDto[], activityId: string) {
  return sims.filter((entry) => entry.activityId === activityId).sort((a, b) => b.updatedAt - a.updatedAt)[0];
}

function drillResult(attempt: Wire.CourseDrillAttemptDto): { result: string; resumable: boolean } {
  switch (attempt.status) {
    case 'open':
      return { result: 'In progress', resumable: true };
    case 'submitted':
      return { result: 'Being graded', resumable: false };
    case 'couldnt_grade':
      return { result: 'Could not be graded', resumable: false };
    case 'graded': {
      const score = attempt.score;
      if (!score) return { result: 'Graded', resumable: false };
      return { result: `Found ${score.found} of ${score.found + score.halfFound + score.missed}`, resumable: false };
    }
  }
}

/** Every past session, attempt and drill of a course as one list, newest first. */
export function buildHistory(sources: HistorySources): HistoryItem[] {
  const items: HistoryItem[] = [];
  const codingActivityIds = new Set(sources.coding.map((entry) => entry.activityId));

  for (const entry of sources.coding) {
    items.push({
      id: `coding:${entry.sessionId}`,
      card: entry.setting === 'coached' ? 'code' : 'mock',
      title: entry.title,
      when: entry.finishedAt ?? entry.createdAt,
      detail: `${SETTING_LABELS[entry.setting]} · ${TASK_LABELS[entry.task]}`,
      result: entry.finished
        ? entry.passed !== null && entry.total !== null
          ? `${entry.passed} of ${entry.total} passed`
          : 'Finished'
        : 'In progress',
      resumable: !entry.finished,
      stoppedAt: entry.finished ? null : `At the ${entry.stage} stage`,
      target: { kind: 'coding', sessionId: entry.sessionId },
    });
  }

  for (const activity of sources.activities) {
    if (activity.kind === 'coding_problem') {
      if (codingActivityIds.has(activity.id)) continue;
      items.push({
        id: `activity:${activity.id}`,
        card: 'code',
        title: activity.title,
        when: activity.updatedAt,
        detail: 'Ready to start',
        result: 'Not started',
        resumable: false,
        stoppedAt: null,
        target: { kind: 'coding-activity', activityId: activity.id },
      });
      continue;
    }
    const simulation = latestSimulation(sources.simulations, activity.id);
    items.push({
      id: `activity:${activity.id}`,
      card: cardOfActivity(activity.kind),
      title: activity.title,
      when: simulation?.updatedAt ?? activity.updatedAt,
      detail: `${HELP_LABELS[activity.practiceMode]} · ${capitalize(activity.kind.replace('_', ' '))}`,
      result: !simulation ? 'Not started' : simulation.status === 'active' ? 'In progress' : simulation.status === 'submitted' ? 'Finished' : 'Stopped',
      resumable: simulation?.status === 'active',
      stoppedAt: simulation?.status === 'active' ? `${simulation.turns.length} turns in` : null,
      target: { kind: 'activity', activityId: activity.id },
    });
  }

  for (const attempt of sources.attempts) {
    const submitted = attempt.status === 'submitted';
    items.push({
      id: `attempt:${attempt.id}`,
      card: 'explain',
      title: attempt.lessonTitle,
      when: attempt.submittedAt ?? attempt.updatedAt,
      detail: HELP_LABELS[attempt.mode],
      result: submitted ? 'Submitted' : 'In progress',
      resumable: !submitted,
      stoppedAt: submitted ? null : 'Your answer is saved',
      target: { kind: 'attempt' },
    });
  }

  for (const attempt of sources.drills) {
    const { result, resumable } = drillResult(attempt);
    items.push({
      id: `drill:${attempt.attemptId}`,
      card: 'spot',
      title: attempt.title,
      when: attempt.startedAt,
      detail: DRILL_MODE_LABELS[attempt.mode],
      result,
      resumable,
      stoppedAt: resumable ? 'Your review is saved' : null,
      target: { kind: 'drill' },
    });
  }

  return items.sort((a, b) => b.when - a.when);
}

/** The most recent work left open. */
export function pickContinue(items: HistoryItem[]): HistoryItem | null {
  return items.find((entry) => entry.resumable) ?? null;
}
