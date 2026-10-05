import { CheckCircle2, Wrench } from 'lucide-react';


import { OutlineGenerationProgress } from '@/features/learning/curriculum/OutlineGenerationProgress';
import type { useRepairLearningOutline } from '@/features/learning/workspace/useLearningStudio';
import type { LearningProgramDto } from '@/lib/bindings';

export type OutlineOperation = Pick<ReturnType<typeof useRepairLearningOutline>, 'isPending' | 'progress' | 'cancelling' | 'cancel' | 'cancelError'>;

function location(program: LearningProgramDto, path: string) {
  const parts = path.split('/');
  const module = parts[1] === 'modules' ? program.modules[Number(parts[2])] : undefined;
  if (!module) return 'Course plan';
  const lesson = parts[3] === 'lessons' ? module.lessons[Number(parts[4])] : undefined;
  return lesson ? `${module.title} · ${lesson.title}` : module.title;
}

export function OutlineReviewPanel({ program, generation, repair }: {
  program: LearningProgramDto; generation?: OutlineOperation; repair: ReturnType<typeof useRepairLearningOutline>;
}) {
  const review = program.outlineReview;
  const operation = generation?.isPending ? generation : repair;
  const pending = operation.isPending;
  if (!review) return null;
  const passed = review.status === 'passed';
  return <section aria-label="Outline review" className={`mt-4 rounded-2xl border p-5 ${passed ? 'border-emerald-500/30 bg-emerald-500/5' : 'border-amber-500/30 bg-amber-500/5'}`}>
    <div className="flex flex-col items-start gap-4 sm:flex-row sm:justify-between"><div className="min-w-0 w-full flex-1"><h2 className="flex items-center gap-2 font-serif text-xl text-text-primary">{passed ? <CheckCircle2 size={19} /> : <Wrench size={19} />}{passed ? 'Outline checks passed' : review.status === 'unchecked' ? 'Saved draft · review incomplete' : `Saved draft · ${review.issues.length} unresolved ${review.issues.length === 1 ? 'finding' : 'findings'}`}</h2><p className="mt-2 text-sm leading-6 text-text-secondary">{review.note}</p><p className="mt-1 text-xs text-text-muted">{review.repairPasses} repair {review.repairPasses === 1 ? 'pass' : 'passes'} saved. {passed ? 'This is an AI review; lesson content receives separate checks when prepared.' : 'You can inspect this draft now. Resolve its findings before accepting the course.'}</p></div>
      {!passed && !pending && program.summary.status === 'draft' && <button type="button" onClick={() => {
        repair.mutate({ programId: program.summary.id, expectedRevision: program.summary.revision });
      }} className="inline-flex min-h-11 max-w-full shrink-0 items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg"><Wrench size={16} />Repair remaining issues</button>}
    </div>
    {!passed && <p className="mt-4 text-xs leading-5 text-text-muted">Supporting references are researched automatically when findings remain. Searches use your learning goal and affected module titles; captured pages are saved and checked alongside your references.</p>}
    {pending && <OutlineGenerationProgress progress={operation.progress} cancelling={operation.cancelling} onCancel={() => void operation.cancel()} cancelError={operation.cancelError} deepDive={false} />}
    {repair.error && <p role="alert" className="mt-3 text-sm text-rose-700">Repair stopped: {repair.error.message}. Your saved draft remains available.</p>}
    {review.issues.length > 0 && <details className="mt-4" open><summary className="cursor-pointer text-sm font-semibold text-text-primary">{review.status === 'unchecked' ? 'Previous findings' : 'Findings to resolve'} ({review.issues.length}){review.status === 'unchecked' ? ' · awaiting recheck' : ''}</summary>{review.status === 'unchecked' && <p className="mt-2 text-xs leading-5 text-text-muted">These findings are from the last completed review. Saved corrections are awaiting verification.</p>}<ul className="mt-3 space-y-3">{review.issues.map((issue, index) => <li key={`${issue.path}:${index}`} className="rounded-xl border border-border bg-background p-4"><p className="text-xs font-semibold text-accent">{location(program, issue.path)} · {issue.kind === 'quote' ? 'Unmatched quotation' : 'Content review'}</p>{issue.claim && <p className="mt-2 text-sm text-text-primary">{issue.claim}</p>}{issue.quote && <blockquote className="mt-2 whitespace-pre-wrap break-words border-l-2 border-accent/40 pl-3 text-sm text-text-secondary">{issue.quote}</blockquote>}<p className="mt-2 text-sm leading-6 text-text-secondary">{issue.message}</p>{issue.sourceId && <p className="mt-2 text-xs text-text-muted">Reference: {program.sources.find((source) => source.id === issue.sourceId)?.title ?? 'Saved reference'}</p>}</li>)}</ul></details>}
  </section>;
}
