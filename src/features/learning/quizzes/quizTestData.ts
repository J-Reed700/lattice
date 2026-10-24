import type {
  LearningAssessmentFormDto,
  LearningAssessmentFormItemDto,
  LearningAssessmentItemResultDto,
  LearningAssessmentSubmissionDto,
  LearningCriterionRationaleDto,
  LearningGradeRationaleDto,
  LearningItemFormat,
  LearningKeyRevealDto,
  LearningQuizOverviewDto,
  LearningQuizStatusDto,
} from '@/lib/bindings';

/** Test builders for the quiz screens. Every field the DTOs carry, with quiet defaults. */

export function quizStatus(overrides: Partial<LearningQuizStatusDto> = {}): LearningQuizStatusDto {
  return {
    scope: { kind: 'lesson', id: 'lesson-1' }, title: 'Enzyme kinetics', bankStatus: 'ready', bankMessage: '', bankSize: 24, freshItems: 8,
    formSize: 8, expectedMinutes: 12, passingScore: 0.7, coverage: null, activeFormId: null, activeFormStartedAt: null, activeFormUpdatedAt: null, activeFormPaused: false,
    verdict: 'not_taken', best: null, latest: null,
    attempts: 0, preparingJobId: null, ...overrides,
  };
}

export function quizOverview(overrides: Partial<LearningQuizOverviewDto> = {}): LearningQuizOverviewDto {
  return {
    programId: 'program-1',
    settings: { quizPass: 0.7, testPass: 0.75, examPass: 0.75, writtenAnswers: true },
    exam: quizStatus({ scope: { kind: 'course', id: null }, title: 'Course exam', coverage: { coveredLessons: 1, totalLessons: 2 }, passingScore: 0.75 }),
    modules: [{
      moduleId: 'module-1',
      test: quizStatus({ scope: { kind: 'module', id: 'module-1' }, title: 'Module test', coverage: { coveredLessons: 1, totalLessons: 2 }, passingScore: 0.75 }),
      lessons: [
        { lessonId: 'lesson-1', quiz: quizStatus() },
        { lessonId: 'lesson-2', quiz: quizStatus({ scope: { kind: 'lesson', id: 'lesson-2' }, title: 'Inhibition', bankStatus: 'not_prepared', bankSize: 0, freshItems: 0 }) },
      ],
    }],
    quizzesPassed: 0, quizzesTotal: 2, outcomes: [], evidence: [], followUps: [], ...overrides,
  };
}

export function reveal(overrides: Partial<LearningKeyRevealDto> = {}): LearningKeyRevealDto {
  return { correctIndices: [], acceptedAnswer: null, orderedValues: [], matches: [], blanks: [], explanation: '', distractorRationales: [], ...overrides };
}

export function formItem(format: LearningItemFormat, overrides: Partial<LearningAssessmentFormItemDto> = {}): LearningAssessmentFormItemDto {
  return {
    id: `item-${format}`, outcomeIds: ['outcome-1'], format, difficulty: 2, prompt: `A ${format} question`, options: [], artifactKind: null, rubric: [],
    points: 1, sourceVersionIds: [], sourceQuote: '', previouslyExposed: false, selectedIndices: [], blankResponses: [], matches: [],
    leftItems: [], rightItems: [], blankCount: 0, textResponse: null, orderedValues: [], artifactJson: null, responseRevision: 0, keyReveal: null, answerHint: null,
    ...overrides,
  };
}

export function itemResult(overrides: Partial<LearningAssessmentItemResultDto> = {}): LearningAssessmentItemResultDto {
  return {
    itemId: 'item-single_choice', outcomeIds: ['outcome-1'], score: 1, correct: true, gradeStatus: 'deterministic', feedback: '',
    pointsEarned: 1, pointsPossible: 1, detail: '', learnerOverride: false, rationale: null, partResults: [], contest: null, ...overrides,
  };
}

export function criterion(overrides: Partial<LearningCriterionRationaleDto> = {}): LearningCriterionRationaleDto {
  return {
    criterionId: 'accuracy', title: 'Accuracy', verdict: 'met', points: 2, maxPoints: 2, whatWasMissing: '', yourWords: [], fullCreditIncludes: '',
    sourceQuote: null, sourceVersionId: null, ...overrides,
  };
}

export function rationale(overrides: Partial<LearningGradeRationaleDto> = {}): LearningGradeRationaleDto {
  return { summary: 'You explained the binding but gave no example.', criteria: [criterion()], graderModel: 'Qwen3.5-9B', rubricVersion: 'r1', gradedAt: Date.UTC(2026, 9, 23), ...overrides };
}

export function submission(overrides: Partial<LearningAssessmentSubmissionDto> = {}): LearningAssessmentSubmissionDto {
  return {
    score: 1, passed: true, gradeStatus: 'deterministic', graderModel: null, graderDisagreement: [], feedback: '', itemResults: [], submittedAt: Date.UTC(2026, 9, 23),
    scoreWithoutOverrides: null, pendingItems: 0, gradingJobId: null, ...overrides,
  };
}

export function quizForm(overrides: Partial<LearningAssessmentFormDto> = {}): LearningAssessmentFormDto {
  return {
    id: 'form-1', programId: 'program-1', blueprintId: 'blueprint-1', blueprintRevision: 1, retakeOfFormId: null, status: 'active', revision: 1,
    title: 'Enzyme kinetics', instructions: '', expectedMinutes: 12, purpose: 'checkpoint', passingScore: 0.7, feedbackTiming: 'after_submission',
    sourceVersionIds: [], modelName: null, items: [], createdAt: 0, updatedAt: 0, submittedAt: null, submission: null,
    scope: { kind: 'lesson', id: 'lesson-1' }, mode: 'graded', ...overrides,
  };
}
