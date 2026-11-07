import type { LearningLessonDto, LearningModuleDto, LearningProgramDto, LearningProgramSummaryDto } from '@/lib/bindings';

const source = { id: 'source-1', title: 'Field guide', url: 'https://example.org/guide', excerpt: 'A carefully chosen passage.', acquiredAt: 1_790_000_000_000 };

export function lesson(id: string, title: string, ready = false): LearningLessonDto {
  return {
    id, title, objective: ready ? `Explain ${title.toLowerCase()}` : '', estimatedMinutes: 20,
    preparation: ready ? 'ready' : 'outline', completed: false,
    blocks: ready ? [
      { kind: 'explanation', title: 'The idea', body: 'A source-backed explanation.', sourceIds: ['source-1'] },
      { kind: 'worked_example', title: 'Worked through', body: 'A short worked example.', sourceIds: ['source-1'] },
    ] : [],
  } as LearningLessonDto;
}

function module(id: string, title: string, lessons: LearningLessonDto[]): LearningModuleDto {
  return { id, title, summary: `${title} gives the course a useful next step.`, outcomes: [`Describe ${title}`, 'Apply the core idea'], lessons };
}

export function program(status: 'draft' | 'active' = 'draft'): LearningProgramDto {
  const modules = [
    module('module-1', 'Foundations', [lesson('lesson-1', 'Start with the idea'), lesson('lesson-2', 'Read a worked example')]),
    module('module-2', 'Putting it together', [lesson('lesson-3', 'Compare approaches'), lesson('lesson-4', 'Make a decision')]),
  ];
  const summary: LearningProgramSummaryDto = {
    id: 'program-1', title: 'A thoughtful course', goal: 'Learn to reason about these materials and use them well.',
    status, revision: status === 'active' ? 1 : 0, moduleCount: 2, lessonCount: 4, completedLessons: 0,
    currentLessonId: 'lesson-1', createdAt: 1_790_000_000_000,
  };
  return { summary, priorKnowledge: 'Some experience', minutesPerSession: 30, modelName: 'Selected model', modules, sources: [source] } as LearningProgramDto;
}

export const ok = <T>(data: T) => ({ ok: true as const, data });
