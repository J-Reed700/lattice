import { quizStatusWords } from '@/features/learning/quizzes/quizCopy';
import { moduleTestStatus, useQuizOverview } from '@/features/learning/quizzes/useQuizzes';
import type { LearningModuleDto, LearningProgramDto } from '@/lib/bindings';

/**
 * "03 Check" in the module milestone strip: the module test's state in words.
 * A paused test resumes; otherwise it opens the test's row in Quizzes.
 */
export function ModuleTestCallout({ program, module, onOpenForm, onShowTest }: {
  program: LearningProgramDto; module: LearningModuleDto; onOpenForm: (formId: string) => void; onShowTest: () => void;
}) {
  const overview = useQuizOverview(program.summary.id, program.summary.status === 'active');
  const status = moduleTestStatus(overview.data, module.id);
  const words = status ? quizStatusWords(status) : null;
  const open = () => (status?.activeFormId ? onOpenForm(status.activeFormId) : onShowTest());
  return <button type="button" onClick={open} title="Take the module test in any order; it covers the lessons prepared so far."
    className="min-h-11 rounded-full px-3 text-left text-text-secondary hover:bg-background hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">
    <span className="mr-1.5 text-xs font-semibold tabular-nums text-accent">03</span>Check · Module test
    {words && <span className="ml-1.5 text-xs text-text-muted">· {words.label}</span>} →
  </button>;
}
