import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';
import type { DueItemDto, LearningPracticeProfileDto } from './practiceHomeApi';
import type { HistoryItem } from './History';

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

/** Stage 1 minimal non-panicking stub: returns null. */
export function PracticeHome(_props: PracticeHomeProps): JSX.Element | null {
  return null;
}
