import { useEffect, useId, useState } from 'react';

import { ArrowRight, Check, CheckCircle2, ClipboardCheck, Clock3, Play } from 'lucide-react';

import { useLearningLessonEvidence } from '@/features/learning/api/evidenceQueries';
import { OutlineCitation } from '@/features/learning/curriculum/OutlineEvidence';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonEvidencePanel, LessonSectionEvidence } from '@/features/learning/lessons/LessonEvidencePanel';
import { currentPreparationJobs, isWaitingForYou, jobHoldingLesson, reviewHold, type ReviewHold } from '@/features/learning/lessons/review/claimReviewPresentation';
import { AuditLessonNow } from '@/features/learning/lessons/review/LessonAudit';
import { LessonClaimReviewPanel } from '@/features/learning/lessons/review/LessonClaimReviewPanel';
import { useLessonClaimReview } from '@/features/learning/lessons/review/useLessonClaimReview';
import { findOutlineCitation } from '@/features/learning/model/evidencePresentation';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { SourceLine } from '@/features/learning/sources/SourceLine';
import { useEvidenceWaitsForIndex } from '@/features/learning/sources/useCourseIndexStatus';
import { SectionTangentBar, StudyTangentSelection } from '@/features/learning/tangents/StudyTangentSelection';
import { MarkdownViewer } from '@/features/reading/components/viewers/MarkdownViewer';
import { useCitationDisplayStore } from '@/features/reading/stores/citationDisplayStore';
import type { LearningBlockKind, LearningLessonDto, LearningOutlineEvidenceDto, LearningProgramDto } from '@/lib/bindings';

const blockLabels: Record<LearningBlockKind, string> = {
  explanation: 'Understand the idea', worked_example: 'Worked example', guided_practice: 'Try it with guidance',
  independent_practice: 'Your assignment', reflection: 'Reflect & connect', recap: 'Key takeaways',
};

export function LessonCard({ lesson, moduleId, outlineEvidence, program, isCurrent, selected, pending, onPrepare, onComplete, onSelect, onPractice }: {
  lesson: LearningLessonDto;
  moduleId: string;
  outlineEvidence?: LearningOutlineEvidenceDto | null;
  program: LearningProgramDto;
  isCurrent: boolean;
  selected: boolean;
  pending: boolean;
  onPrepare: () => void;
  onComplete: () => void;
  onSelect: () => void;
  onPractice: () => void;
}) {
  const [expanded, setExpanded] = useState(isCurrent && lesson.preparation === 'ready');
  const showCitations = useCitationDisplayStore((state) => state.visible);
  // Preparing reads the course's evidence, so it waits while that indexes,
  // and says so beside the button with the time left.
  const waitsForIndex = useEvidenceWaitsForIndex(program.summary.id);
  const heldByIndex = !pending && program.summary.status === 'active' ? waitsForIndex : undefined;
  const indexWaitId = useId();
  const evidenceReport = useLearningLessonEvidence(
    program.summary.id,
    lesson.id,
    program.summary.revision,
    showCitations && expanded && lesson.preparation === 'ready',
  );
  const objectiveCitation = findOutlineCitation(
    outlineEvidence,
    'lesson_objective',
    moduleId,
    lesson.objective,
    undefined,
    lesson.id,
  );
  useEffect(() => { if (selected) setExpanded(true); }, [selected, lesson.preparation]);
  // A lesson held for review is named by its job's result; its claims come
  // from the review. While a job runs, an opened lesson lists claims live.
  // A draft course has no plan and no jobs, so it asks for none.
  const jobs = useLearningPlan(program.summary.status === 'active' ? program.summary.id : '').data?.jobs ?? [];
  const holdingJob = lesson.preparation === 'outline' ? jobHoldingLesson(jobs, lesson.id) : null;
  const activeJob = currentPreparationJobs(jobs).find((job) => job.status === 'pending' || job.status === 'running') ?? null;
  const review = useLessonClaimReview(
    program.summary.id,
    lesson.id,
    Boolean(holdingJob) || expanded,
  );
  const liveTally = activeJob?.activity?.claims ? JSON.stringify(activeJob.activity.claims) : null;
  const refetchReview = review.refetch;
  useEffect(() => { if (liveTally && expanded) void refetchReview(); }, [liveTally, expanded, refetchReview]);
  const reviewState = review.data?.state ?? (holdingJob ? 'needs_review' : null);
  const waiting = review.data?.claims.filter(isWaitingForYou).length ?? null;
  const needsReview = reviewState === 'needs_review' || reviewState === 'stopped';
  // Never "0 claims": a lesson with none waiting is held by the audit or ready to resume.
  const hold = review.data && waiting != null ? reviewHold(review.data, waiting) : 'claims';
  const holdLabel: Record<ReviewHold, string> = {
    claims: `Needs your review${waiting ? ` · ${waiting} ${waiting === 1 ? 'claim' : 'claims'}` : ''}`,
    audit_stop: 'Waiting for the Verifier audit', audit: 'Waiting for the Verifier audit', resolved: 'Ready to resume',
  };
  const statusLabel = reviewState === 'needs_review' ? holdLabel[hold]
    : reviewState === 'stopped' ? 'Stopped · see why' : lesson.preparation === 'ready' ? 'Ready' : 'Outline';
  const holdHint: Record<ReviewHold, string> = {
    claims: 'Some claims could not be verified. Open the lesson to see each one with what was compared, and resolve it.',
    audit_stop: 'No claim needs you; the Verifier audit could not finish. Open the lesson to see why and resume.',
    audit: 'No claim needs you; the Verifier audit has not finished. Open the lesson to resume it.',
    resolved: 'Every claim is resolved. Open the lesson and choose Resume verification.',
  };
  const showReview = needsReview || (Boolean(review.data) && lesson.preparation === 'outline' && Boolean(activeJob));
  const reviewPanel = showReview && <LessonClaimReviewPanel programId={program.summary.id} lessonId={lesson.id} courseRevision={program.summary.revision}
    jobActive={Boolean(activeJob)} lessonAnchor={lesson.preparation === 'ready' ? `lesson-${lesson.id}-section-` : null} />;

  return (
    <article id={`lesson-${lesson.id}`} className={`overflow-clip rounded-2xl border bg-surface transition ${isCurrent ? 'border-accent/50 shadow-[0_8px_34px_-24px_hsl(var(--accent))]' : 'border-border shadow-sm'}`}>
      <button type="button" onClick={() => { onSelect(); setExpanded((value) => !value); }} className="flex w-full items-start gap-4 p-5 text-left sm:p-6">
        <span className={`mt-0.5 grid h-9 w-9 shrink-0 place-items-center rounded-full ${lesson.completed ? 'bg-emerald-500/15 text-emerald-700' : lesson.preparation === 'ready' ? 'bg-accent/10 text-accent' : 'bg-background text-text-muted'}`}>
          {lesson.completed ? <Check size={16} /> : <span className="font-serif text-base">{lesson.preparation === 'ready' ? <Play size={14} /> : '·'}</span>}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-center gap-2"><span className="font-serif text-xl text-text-primary">{lesson.title}</span>{isCurrent && <span className="rounded-full bg-accent/10 px-2 py-0.5 text-xs font-semibold uppercase tracking-wider text-accent">Resume here</span>}</span>
          <span className="mt-1 block text-sm leading-6 text-text-secondary">
            <StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle="Lesson Objective">
              <span>{lesson.objective || (lesson.preparation === 'outline' ? 'A lesson in the program outline. Prepare it when you are ready.' : 'A focused lesson with explanation and worked example.')}</span>
            </StudyTangentSelection>
          </span>
          {needsReview && !expanded && <span className="mt-2 block text-xs leading-5 text-text-secondary">{reviewState === 'stopped' ? 'Preparation stopped with a cause to fix. Open the lesson to see it and retry.' : holdHint[hold]}</span>}
          <span className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-text-muted"><span className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap"><Clock3 size={13} />About {lesson.estimatedMinutes} min</span><span className={`inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 ${needsReview ? 'border border-accent/50 font-semibold text-text-primary' : 'bg-background'}`}>{needsReview && <ClipboardCheck size={12} aria-hidden="true" />}{statusLabel}</span><span className="shrink-0 whitespace-nowrap">{lesson.completed ? 'Self-reported complete' : 'Not marked complete'}</span></span>
        </span>
        <span className="mt-2 shrink-0 text-xs text-text-muted">{expanded ? 'Close' : 'Open'}</span>
      </button>

      {expanded && (
        <div className="border-t border-border bg-background/45 px-5 py-6 sm:px-9">
          {showCitations && <OutlineCitation citation={objectiveCitation} evidence={outlineEvidence} className="mb-4 mt-0" />}
          {lesson.preparation === 'outline' && reviewPanel}
          {lesson.preparation === 'outline' ? needsReview ? null : (
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div><h4 className="font-medium text-text-primary">Ready when you are</h4><p className="mt-1 text-sm text-text-secondary">This prepares this lesson only. The accepted outline stays in place if preparation fails.</p></div>
              <div className="flex flex-col items-start gap-1.5 sm:items-end">
                <button type="button" disabled={pending || program.summary.status !== 'active' || Boolean(waitsForIndex)} aria-describedby={heldByIndex ? indexWaitId : undefined} onClick={onPrepare} className="rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{program.summary.status !== 'active' ? 'Accept the course to begin' : pending ? 'Preparing…' : 'Prepare lesson'}</button>
                {heldByIndex && <p id={indexWaitId} className="text-xs tabular-nums text-text-secondary">{heldByIndex}</p>}
              </div>
            </div>
          ) : (
            <>
              {reviewPanel}
              <AuditLessonNow programId={program.summary.id} lessonId={lesson.id} />
              {showCitations && <LessonEvidencePanel key={lesson.id} programId={program.summary.id} lessonId={lesson.id} programRevision={program.summary.revision} report={evidenceReport} />}
              <nav aria-label={`Sections in ${lesson.title}`} className="mb-5 flex flex-wrap gap-2">
                {lesson.blocks.map((block, index) => <a key={index} href={`#lesson-${lesson.id}-section-${index}`} className="rounded-full border border-border bg-surface px-3 py-2 text-xs text-text-secondary hover:border-accent">{index + 1}. {block.title}</a>)}
              </nav>
              <div className="space-y-4">
                {lesson.blocks.map((block, index) => (
                  <section id={`lesson-${lesson.id}-section-${index}`} key={`${lesson.id}-${index}`} className="rounded-xl border border-border bg-surface p-5">
                    <div className="text-xs font-semibold uppercase tracking-[.12em] text-accent">{blockLabels[block.kind]}</div>
                    <h4 className="mt-1 font-serif text-lg text-text-primary">{block.title}</h4>
                    <div className="prose prose-sm mt-3 max-w-none text-text-secondary dark:prose-invert">
                      <StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={index} sectionTitle={block.title}>
                        <MarkdownViewer content={block.body} />
                      </StudyTangentSelection>
                    </div>
                    {showCitations && <LessonSectionEvidence report={evidenceReport} sectionIndex={index} />}
                    {showCitations && evidenceReport.isSuccess && !evidenceReport.data && <SourceLine ids={block.sourceIds} sources={program.sources} />}
                    {block.kind === 'guided_practice' && selected && program.summary.status === 'active' && <div id={`guided-${lesson.id}`}><PracticeWorkbenchPanel key={lesson.id} program={program} lesson={lesson} taskKind="guided" embedded onContinue={onPractice} /></div>}
                    {block.kind === 'independent_practice' && <button type="button" onClick={onPractice} className="mt-5 inline-flex min-h-11 items-center gap-2 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg">Work on this assignment <ArrowRight size={15} /></button>}
                    <SectionTangentBar moduleId={moduleId} lessonId={lesson.id} sectionIndex={index} sectionTitle={block.title} sectionText={`${block.title}\n\n${block.body}`} />
                  </section>
                ))}
              </div>
              <SectionTangentBar moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle={lesson.title} sectionText={`${lesson.title}\n\n${lesson.objective}`} label="Start a tangent on this lesson" />
              <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-5"><button type="button" onClick={onPractice} className="inline-flex min-h-11 items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-sm font-semibold text-accent-fg hover:brightness-95">Practice this lesson <ArrowRight size={15} /></button><div className="flex flex-wrap items-center gap-3"><p className="max-w-lg text-xs leading-5 text-text-muted">Reading the lesson does not count as completion.</p><button type="button" disabled={lesson.completed || pending} onClick={onComplete} className="inline-flex min-h-11 items-center gap-2 rounded-full border border-accent/40 px-4 py-2.5 text-sm font-medium text-accent hover:bg-accent/5 disabled:opacity-45">{lesson.completed ? <><CheckCircle2 size={15} /> Marked complete</> : pending ? 'Saving…' : <>Mark lesson complete <Check size={15} /></>}</button></div></div>
            </>
          )}
        </div>
      )}
    </article>
  );
}
