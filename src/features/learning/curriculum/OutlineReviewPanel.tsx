import { CheckCircle2, Wrench } from 'lucide-react';


import { OutlineGenerationProgress } from '@/features/learning/curriculum/OutlineGenerationProgress';
import type { useRepairLearningOutline } from '@/features/learning/workspace/useLearningStudio';
import type { LearningOutlineIssueKind, LearningProgramDto } from '@/lib/bindings';

export type OutlineOperation = Pick<ReturnType<typeof useRepairLearningOutline>, 'isPending' | 'progress' | 'cancelling' | 'cancel' | 'cancelError'>;

const kindLabel: Record<LearningOutlineIssueKind, string> = { factual: 'Factual check', plan: 'Plan check', references: 'References' };
/** Repair attempts one finding may use per operation; mirrors the backend limit. */
const attemptLimit = 2;

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
  // Once the course is accepted, a passed outline is a fact, not a task: one line.
  if (passed && program.summary.status === 'active' && !pending) {
    const references = program.sources.length;
    return <section aria-label="Outline review" className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-text-secondary">
      <CheckCircle2 size={14} className="shrink-0 text-success" aria-hidden="true" />
      <span>Outline checks passed</span>
      {references > 0 && <span className="text-text-muted">· {references} {references === 1 ? 'reference' : 'references'}</span>}
      {review.issues.length > 0 && <details className="w-full"><summary className="cursor-pointer text-text-muted">Advisory suggestions ({review.issues.length})</summary><ul className="mt-2 space-y-1">{review.issues.map((issue, index) => <li key={issue.id || `${issue.path}:${index}`}><span className="font-medium text-text-primary">{location(program, issue.path)}</span> · {issue.message}</li>)}</ul></details>}
    </section>;
  }
  return <section aria-label="Outline review" className={`mt-4 rounded-xl border p-5 ${passed ? 'border-success-muted bg-success-muted/30' : 'border-warning-muted bg-warning-muted/30'}`}>
    <div className="flex flex-col items-start gap-4 sm:flex-row sm:justify-between"><div className="min-w-0 w-full flex-1"><h2 className="flex items-center gap-2 font-serif text-xl text-text-primary">{passed ? <CheckCircle2 size={19} className="text-success" /> : <Wrench size={19} className="text-warning-fg" />}{passed ? 'Outline checks passed' : review.status === 'unchecked' ? 'Saved draft · review incomplete' : `Saved draft · ${review.issues.length} unresolved ${review.issues.length === 1 ? 'finding' : 'findings'}`}</h2><p className="mt-2 text-sm leading-6 text-text-secondary">{review.note}</p><p className="mt-1 text-xs text-text-muted">{review.repairPasses} repair {review.repairPasses === 1 ? 'pass' : 'passes'} saved. {passed ? 'This is an AI review; lesson content receives separate checks when prepared.' : 'You can inspect this draft now. Resolve its findings before accepting the course.'}</p></div>
      {(!passed || review.issues.length > 0) && !pending && program.summary.status === 'draft' && <button type="button" onClick={() => {
        repair.mutate({ programId: program.summary.id, expectedRevision: program.summary.revision });
      }} className="inline-flex min-h-9 max-w-full shrink-0 items-center gap-2 rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors shadow-2xs"><Wrench size={14} />{passed ? 'Try the suggestions again' : 'Repair remaining issues'}</button>}
    </div>
    {!passed && <p className="mt-4 text-xs leading-5 text-text-muted">References were gathered from public searches planned for each subtopic; the learner's goal text is never sent. Each outline item is linked to the saved passage that addresses it; quoted words are copied from the saved page, never written by the model.</p>}
    {pending && <OutlineGenerationProgress progress={operation.progress} cancelling={operation.cancelling} onCancel={() => void operation.cancel()} cancelError={operation.cancelError} deepDive={false} />}
    {repair.error && <p role="alert" className="mt-3 text-sm text-danger-fg">Repair stopped: {repair.error.message}. Your saved draft remains available.</p>}
    {review.issues.length > 0 && <details className="mt-4" open><summary className="cursor-pointer text-sm font-semibold text-text-primary">{review.status === 'unchecked' ? 'Previous findings' : passed ? 'Advisory suggestions' : 'Findings to resolve'} ({review.issues.length}){review.status === 'unchecked' ? ' · awaiting recheck' : ''}</summary>{review.status === 'unchecked' && <p className="mt-2 text-xs leading-5 text-text-muted">These findings are from the last completed review. Saved corrections are awaiting verification.</p>}<ul className="mt-3 space-y-3">{review.issues.map((issue, index) => <li key={issue.id || `${issue.path}:${index}`} className="rounded-xl border border-border-default bg-surface p-4 shadow-2xs"><p className="text-xs font-semibold text-accent">{location(program, issue.path)} · {kindLabel[issue.kind]}</p>{issue.claim && <p className="mt-2 text-sm text-text-primary">{issue.claim}</p>}{issue.quote && <blockquote className="mt-2 whitespace-pre-wrap wrap-break-word border-l-2 border-accent/40 pl-3 text-sm text-text-secondary">{issue.quote}</blockquote>}<p className="mt-2 text-sm leading-6 text-text-secondary">{issue.message}</p>{issue.sourceId && <p className="mt-2 text-xs text-text-muted">Reference: {program.sources.find((source) => source.id === issue.sourceId)?.title ?? 'Saved reference'}</p>}{issue.kind !== 'references' && <p className="mt-2 text-xs text-text-muted">{issue.attempts} of {attemptLimit} attempts used{issue.exhausted ? ' · automatic repair has stopped on this finding; Repair starts it afresh' : ''}</p>}{issue.searched.length > 0 && <p className="mt-1 text-xs text-text-muted">Searched: {issue.searched.join('; ')}</p>}</li>)}</ul></details>}
  </section>;
}
