import type * as Wire from '@/lib/bindings';
import type { ApiResult } from '@/types';

import { learningCall } from './learningCall';

export const preparationControlApi = {
  prioritizeLearningGenerationJob: (
    request: Wire.LearningGenerationJobActionRequestDto,
  ): Promise<ApiResult<Wire.LearningGenerationJob>> =>
    learningCall('prioritize_learning_generation_job', { request }),

  prepareLearningLessons: (
    request: Wire.StartLearningLessonsJobsRequestDto,
  ): Promise<ApiResult<Wire.LearningGenerationJob[]>> =>
    learningCall('prepare_learning_lessons', { request }),

  resetLearningLessonPreparation: (
    request: Wire.ResetLearningLessonPreparationRequestDto,
  ): Promise<ApiResult<Wire.ResetLearningLessonPreparationResultDto>> =>
    learningCall('reset_learning_lesson_preparation', { request }),
};
