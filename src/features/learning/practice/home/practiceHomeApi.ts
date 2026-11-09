import { apiCall, type CommandName } from '@/shared/ipc/transport';

export interface LearningPracticeProfileDto {
  programId: string;
  details: string;
  language: string | null;
  interviewAt: number | null;
  company: string | null;
  role: string | null;
  revision: number;
}

export interface SaveLearningPracticeProfileRequestDto {
  programId: string;
  details: string;
  language?: string | null;
  interviewAt?: number | null;
  company?: string | null;
  role?: string | null;
  expectedRevision?: number | null;
}

export type CodingTask = 'problem' | 'bug_fix' | 'project';
export type Setting = 'coached' | 'mock' | 'watched';
export type Difficulty = 'easy' | 'medium' | 'hard';

export interface StartCodingBuildRequestDto {
  programId: string;
  lessonId?: string | null;
  task: CodingTask;
  setting: Setting;
  language: string;
  difficulty: Difficulty;
  focus?: string | null;
}

export interface StartedCodingBuildDto {
  jobId: string;
}

export interface DueItemDto {
  id: string;
  kind: 'pattern' | 'lesson';
  title: string;
  subtitle?: string;
  count?: number;
  overdueText?: string;
  isOverdue?: boolean;
}

export interface PracticeDueDto {
  items: DueItemDto[];
  totalDue: number;
}

async function invokeLearning<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const result = await apiCall<T>(command as CommandName, args);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
}

/** Wraps the backend commands for practice profiles, coding build, and practice due. */
export const practiceHomeApi = {
  getProfile: (programId: string): Promise<LearningPracticeProfileDto> =>
    invokeLearning<LearningPracticeProfileDto>('get_learning_practice_profile', {
      programId,
    }),

  saveProfile: (
    request: SaveLearningPracticeProfileRequestDto,
  ): Promise<LearningPracticeProfileDto> =>
    invokeLearning<LearningPracticeProfileDto>('save_learning_practice_profile', {
      request,
    }),

  startCodingBuild: (
    request: StartCodingBuildRequestDto,
  ): Promise<StartedCodingBuildDto> =>
    invokeLearning<StartedCodingBuildDto>('start_coding_build', {
      request,
    }),

  getDue: (programId: string): Promise<PracticeDueDto> =>
    invokeLearning<PracticeDueDto>('practice_due', {
      programId,
    }),
};
