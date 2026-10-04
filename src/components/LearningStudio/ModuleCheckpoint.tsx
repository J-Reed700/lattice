import { useRef } from 'react';

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { ClipboardCheck } from 'lucide-react';

import VaultAPI from '@/lib/api';
import type { CreateLearningAssessmentBlueprintRequestDto, LearningAssessmentWorkspaceDto, LearningModuleDto, LearningProgramDto } from '@/lib/bindings';

import { learningAssessmentWorkspaceKey } from './useLearningAssessment';

export function ModuleCheckpoint({ program, module, workspace, onCreated }: {
  program: LearningProgramDto; module: LearningModuleDto; workspace: LearningAssessmentWorkspaceDto; onCreated: () => void;
}) {
  const client = useQueryClient();
  const retry = useRef<CreateLearningAssessmentBlueprintRequestDto | null>(null);
  const outcomes = workspace.outcomes.filter((outcome) => outcome.moduleId === module.id && !outcome.lessonId);
  const existing = workspace.blueprints.find((blueprint) => blueprint.purpose === 'module_test' && blueprint.requirements.some((requirement) => outcomes.some((outcome) => outcome.id === requirement.outcomeId)));
  const ready = module.lessons.length > 0 && module.lessons.every((lesson) => lesson.preparation === 'ready');
  const create = useMutation({
    retry: false,
    mutationFn: async () => {
      if (!retry.current) {
        const lessonSourceIds = [...new Set(module.lessons.flatMap((lesson) => [...lesson.blocks.flatMap((block) => block.sourceIds), ...lesson.questions.flatMap((question) => question.sourceIds)]))];
        const sourceVersionIds = lessonSourceIds.length ? lessonSourceIds : program.sources.map((source) => source.id);
        retry.current = {
          operationId: crypto.randomUUID(), programId: program.summary.id, blueprintId: crypto.randomUUID(), revision: 1, predecessorRevision: null,
          purpose: 'module_test', title: `${module.title} — checkpoint`.slice(0, 160),
          instructions: `Demonstrate the outcomes of ${module.title}. Explain your reasoning and apply it to a new situation. Work independently; feedback is shown after submission. ${ready ? "Build on the prepared lessons." : "This is an optional prior-knowledge challenge based on the accepted lesson objectives; provide all task data and do not assume the learner has read unprepared lessons."}`,
          expectedMinutes: Math.max(20, program.minutesPerSession), allowedAids: [], passingScore: 0.7, feedbackTiming: 'after_submission',
          rubric: [
            { id: crypto.randomUUID(), title: 'Accuracy', description: 'Use the concepts correctly and identify relevant limitations.', maxPoints: 4 },
            { id: crypto.randomUUID(), title: 'Reasoning', description: 'Explain the decisions with a clear chain of reasoning and concrete evidence.', maxPoints: 4 },
            { id: crypto.randomUUID(), title: 'Transfer', description: 'Apply the concept to the new situation and justify the result.', maxPoints: 4 },
          ],
          sourceVersionIds,
          requirements: outcomes.flatMap((outcome) => [
            { outcomeId: outcome.id, format: 'explanation' as const, count: 1, difficultyMin: 2, difficultyMax: 3 },
            { outcomeId: outcome.id, format: 'artifact' as const, count: 1, difficultyMin: 3, difficultyMax: 4 },
          ]),
          changeReason: ready ? 'Create the module checkpoint from its accepted outcomes and prepared lessons.' : 'Create an optional prior-knowledge challenge from the accepted module objectives.',
        };
      }
      const result = await VaultAPI.createLearningAssessmentBlueprint(retry.current);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    onSuccess: (data) => {
      client.setQueryData(learningAssessmentWorkspaceKey(program.summary.id), data);
      retry.current = null;
      onCreated();
    },
  });
  return <section className="mb-5 rounded-2xl border border-accent/25 bg-accent/5 p-5"><div className="flex flex-wrap items-start justify-between gap-4"><div className="max-w-2xl"><div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-accent"><ClipboardCheck size={15} /> Module checkpoint</div><h3 className="mt-2 font-serif text-xl text-text-primary">Show what you can do with {module.title.toLowerCase()}</h3><p className="mt-2 text-sm leading-6 text-text-secondary">Explain the concepts, then apply them in a new scenario. Written responses and artifacts receive rubric feedback, with a saved attempt and outcome evidence.</p><p className="mt-2 text-xs text-text-muted">{ready ? `${outcomes.length} outcomes · ${outcomes.length * 2} written and applied tasks · feedback after submission` : 'Already know this topic? Try a challenge based on the course objectives, or study the lessons first.'}</p></div><button type="button" disabled={create.isPending || (!existing && !outcomes.length)} onClick={() => existing ? onCreated() : create.mutate()} className="min-h-11 rounded-full bg-accent px-4 py-2 text-sm font-semibold text-accent-fg disabled:opacity-50">{create.isPending ? 'Preparing checkpoint…' : existing ? 'View module checkpoint' : create.isError ? 'Retry checkpoint' : ready ? 'Prepare module checkpoint' : 'Challenge this module'}</button></div>{create.isError && <p role="alert" className="mt-3 text-sm text-rose-700">{create.error.message}</p>}</section>;
}
