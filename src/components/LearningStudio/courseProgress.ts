import type { LearningAssessmentWorkspaceDto, LearningPlanDto, LearningPracticeWorkspaceDto, LearningProgramDto } from '@/lib/bindings';

export type CourseStep = { kind: 'lesson' | 'guided' | 'assignment' | 'assessment' | 'starting_point' | 'recall'; title: string; reason: string; lessonId?: string; sessionId?: string; formId?: string };

/** Recommendations describe evidence and never silently skip or complete work. */
export function nextCourseStep(program: LearningProgramDto, practice?: LearningPracticeWorkspaceDto, assessment?: LearningAssessmentWorkspaceDto, plan?: LearningPlanDto, dueCount = 0): CourseStep {
  const lessons = program.modules.flatMap((module) => module.lessons);
  const activeSession = [...(practice?.sessions ?? [])].filter((s) => s.status === 'active' && lessons.some((l) => l.id === s.lessonId)).sort((a, b) => b.updatedAt - a.updatedAt)[0];
  const activeForm = [...(assessment?.forms ?? [])].filter((f) => f.status === 'active').sort((a, b) => b.createdAt - a.createdAt)[0];
  if (activeForm && (!activeSession || activeForm.createdAt > activeSession.updatedAt)) return { kind: 'assessment', title: 'Resume your assessment', reason: activeForm.title, formId: activeForm.id };
  if (activeSession) return { kind: activeSession.taskKind === 'guided' ? 'guided' : 'assignment', title: 'Continue your saved response', reason: activeSession.lessonTitle, lessonId: activeSession.lessonId, sessionId: activeSession.id };
  const followup = [...(assessment?.followUps ?? [])].filter((f) => f.status === 'accepted' || f.status === 'pending').sort((a, b) => b.createdAt - a.createdAt).find((f) => {
    const outcome = assessment?.outcomes.find((o) => o.id === f.outcomeId);
    const module = program.modules.find((m) => m.id === outcome?.moduleId);
    const newerSuccess = assessment?.evidence.some((e) => e.outcomeId === f.outcomeId && e.observedAt > f.createdAt && e.result === 'observed');
    const newerPractice = practice?.sessions.some((s) => s.status === 'submitted' && (s.submittedAt ?? 0) > f.createdAt && (s.lessonId === f.actionRef || outcome?.lessonId === s.lessonId || (!outcome?.lessonId && module?.lessons.some((l) => l.id === s.lessonId))));
    return !newerSuccess && !newerPractice;
  });
  if (followup) {
    const outcome = assessment?.outcomes.find((o) => o.id === followup.outcomeId);
    const lesson = lessons.find((l) => l.id === followup.actionRef || l.id === outcome?.lessonId) ?? program.modules.find((m) => m.id === outcome?.moduleId)?.lessons[0];
    if (followup.actionKind === 'recall') return { kind: 'recall', title: 'Refresh an earlier idea', reason: followup.explanation };
    if (lesson) return { kind: followup.actionKind === 'assessment' ? 'assessment' : lesson.preparation === 'ready' ? 'assignment' : 'lesson', title: 'Work on a gap from your feedback', reason: followup.explanation, lessonId: lesson.id };
  }
  const diagnostic = plan?.latestDiagnostic;
  if (plan && !diagnostic && !(practice?.sessions.length) && !(program.attempts?.length) && !lessons.some((l) => l.completed)) return { kind: 'starting_point', title: 'Check your starting point', reason: 'Optional short tasks help you choose a useful starting level. You can also begin with the first lesson.' };
  if (diagnostic?.status === 'active') return { kind: 'starting_point', title: 'Finish your starting-point check', reason: 'Use a few short tasks to choose where to begin.' };
  const finding = diagnostic?.findings?.find((f) => f.signal === 'needs_practice' && !assessment?.evidence.some((e) => e.outcomeId === f.outcomeId && e.result === 'observed' && e.observedAt > (diagnostic.submittedAt ?? 0)) && !practice?.sessions.some((s) => {
    const module = plan?.acceptedRevision?.modules.find((m) => m.outcomeIds.includes(f.outcomeId));
    return s.status === 'submitted' && (s.submittedAt ?? 0) > (diagnostic.submittedAt ?? 0) && module?.lessons.some((l) => l.id === s.lessonId);
  }));
  if (finding) {
    const module = plan?.acceptedRevision?.modules.find((m) => m.outcomeIds.includes(finding.outcomeId));
    const lesson = lessons.find((l) => l.id === module?.lessons[0]?.id);
    if (lesson) return { kind: 'lesson', title: 'Strengthen your starting point', reason: finding.feedback, lessonId: lesson.id };
  }
  const current = lessons.find((l) => l.id === program.summary.currentLessonId && !l.completed) ?? lessons.find((l) => !l.completed);
  if (current) {
    if (current.preparation !== 'ready') return { kind: 'lesson', title: 'Prepare your next lesson', reason: current.title, lessonId: current.id };
    const submissions = practice?.sessions.filter((s) => s.lessonId === current.id && s.status === 'submitted') ?? [];
    if (current.blocks.some((b) => b.kind === 'guided_practice') && !submissions.some((s) => s.taskKind === 'guided')) return { kind: 'guided', title: 'Try the guided exercise', reason: 'Work through the idea with feedback and one hint at a time.', lessonId: current.id };
    if (!submissions.some((s) => (s.taskKind ?? 'independent') === 'independent')) return { kind: 'assignment', title: 'Apply it independently', reason: 'Create your own response and get feedback against the assignment criteria.', lessonId: current.id };
    return { kind: 'lesson', title: 'Review and finish this lesson', reason: 'Review your feedback, revise if useful, then mark the lesson complete.', lessonId: current.id };
  }
  if (dueCount > 0) return { kind: 'recall', title: 'Review what is due', reason: `${dueCount} recall ${dueCount === 1 ? 'card is' : 'cards are'} ready for review.` };
  return { kind: 'assessment', title: 'Check what you can apply', reason: 'Complete module checkpoints and review your evidence before attempting the capstone.' };
}
