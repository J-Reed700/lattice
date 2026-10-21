import { useEffect, useId, useState } from 'react';

import { ArrowRight, Check, CheckCircle2, ClipboardCheck, Clock3, Play } from 'lucide-react';

import { useLearningLessonEvidence } from '@/features/learning/api/evidenceQueries';
import { OutlineCitation } from '@/features/learning/curriculum/OutlineEvidence';
import { useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { LessonEvidencePanel, LessonSectionEvidence } from '@/features/learning/lessons/LessonEvidencePanel';
import { claimsCount, claimsNeedYou, preparingLine, teachingNotesForYou } from '@/features/learning/lessons/review/claimReviewCopy';
import { currentPreparationJobs, isWaitingForYou, jobHoldingLesson, reviewHold, type ReviewHold } from '@/features/learning/lessons/review/claimReviewPresentation';
import { AuditLessonNow } from '@/features/learning/lessons/review/LessonAudit';
import { isTeachingWaiting } from '@/features/learning/lessons/review/TeachingFindings';
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

export function LessonCard({ lesson, moduleId, outlineEvidence, program, isCurrent, selected, pending, onPrepare, onComplete, onSelect, onPractice, onReview }: {
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
  /** Opens the lesson's claim review screen; the course page never shows claims itself. */
  onReview?: (programId: string, lessonId: string) => void;
}) {
  const [expanded, setExpanded] = useState(isCurrent && lesson.preparation === 'ready');
  const showCitations = useCitationDisplayStore((state) => state.visible);
  // Preparing reads the course's evidence, so it waits while that indexes,
  // and says so beside the button with the time left.
  const waitsForIndex = useEvidenceWaitsForIndex(program.summary.id);
  const heldByIndex = !pending && program.summary.status === 'active' ? waitsForIndex : undefined;
  const indexWaitId = useId();
  const bodyId = useId();
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
  // A lesson held for review is named by its job's result; its counts come
  // from the review. A ready lesson may still have teaching notes waiting.
  // A draft course has no plan and no jobs, so it asks for none.
  const jobs = useLearningPlan(program.summary.status === 'active' ? program.summary.id : '').data?.jobs ?? [];
  const holdingJob = lesson.preparation === 'outline' ? jobHoldingLesson(jobs, lesson.id) : null;
  const activeJob = currentPreparationJobs(jobs).find((job) => job.status === 'pending' || job.status === 'running') ?? null;
  const review = useLessonClaimReview(
    program.summary.id,
    lesson.id,
    Boolean(holdingJob) || expanded || lesson.preparation === 'ready',
  );
  const reviewState = review.data?.state ?? (holdingJob ? 'needs_review' : null);
  const waiting = review.data?.claims.filter(isWaitingForYou).length ?? null;
  const teachingWaiting = review.data?.teachingFindings.filter(isTeachingWaiting).length ?? 0;
  const needsReview = reviewState === 'needs_review' || reviewState === 'stopped';
  // Never "0 claims": a lesson with none waiting is held by the audit or ready to resume.
  const hold = review.data && waiting != null ? reviewHold(review.data, waiting) : 'claims';
  const holdLabel: Record<ReviewHold, string> = {
    claims: waiting ? claimsNeedYou(waiting) : 'Waiting for you',
    audit_stop: 'Waiting for the independent check', audit: 'Waiting for the independent check', resolved: 'Ready to resume',
  };
  const preparingThis = lesson.preparation === 'outline' && activeJob?.activity?.lessonTitle === lesson.title;
  // The first that applies, as §4.4 orders them.
  const statusLabel = reviewState === 'stopped' ? 'Stopped with a cause to fix'
    : needsReview ? holdLabel[hold]
    : lesson.preparation === 'ready' ? teachingWaiting > 0 ? `Ready · ${teachingNotesForYou(teachingWaiting)}` : 'Ready'
    : preparingThis ? preparingLine(activeJob?.activity?.claims)
    : 'Outline';
  const reviewable = Boolean(onReview) && (needsReview || teachingWaiting > 0);
  const openReview = () => onReview?.(program.summary.id, lesson.id);

  return (
    <article id={`lesson-${lesson.id}`} className={`overflow-clip rounded-2xl border bg-surface transition ${isCurrent ? 'border-accent/50 shadow-[0_8px_34px_-24px_hsl(var(--accent))]' : 'border-border shadow-sm'}`}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3 sm:px-5">
        <span className={`grid h-8 w-8 shrink-0 place-items-center rounded-full ${lesson.completed ? 'bg-emerald-500/15 text-emerald-700' : lesson.preparation === 'ready' ? 'bg-accent/10 text-accent' : 'bg-background text-text-muted'}`}>
          {lesson.completed ? <Check size={15} /> : <span className="font-serif text-base">{lesson.preparation === 'ready' ? <Play size={13} /> : '·'}</span>}
        </span>
        <button type="button" aria-expanded={expanded} aria-controls={bodyId} onClick={() => { onSelect(); setExpanded((value) => !value); }} className="flex min-w-0 flex-1 items-center gap-2 text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent">
          <span className="min-w-0 truncate font-serif text-lg text-text-primary">{lesson.title}</span>
          {isCurrent && <span className="shrink-0 rounded-full bg-accent/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-accent">Resume here</span>}
        </button>
        <span className={`inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2 py-0.5 text-xs ${needsReview || reviewable ? 'border border-accent/50 font-semibold text-text-primary' : 'bg-background text-text-muted'}`}>{needsReview && <ClipboardCheck size={12} aria-hidden="true" />}{statusLabel}</span>
        {reviewable && <button type="button" onClick={openReview} className="min-h-9 shrink-0 rounded-full bg-accent px-4 text-xs font-semibold text-accent-fg">Review</button>}
      </div>

      {expanded && (
        <div id={bodyId} className="border-t border-border bg-background/45 px-5 py-6 sm:px-9">
          <div className="text-sm leading-6 text-text-secondary">
            <StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle="Lesson Objective">
              <span>{lesson.objective || (lesson.preparation === 'outline' ? 'A lesson in the program outline. Prepare it when you are ready.' : 'A focused lesson with explanation and worked example.')}</span>
            </StudyTangentSelection>
          </div>
          <p className="mb-4 mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-muted"><span className="inline-flex items-center gap-1.5"><Clock3 size={13} />About {lesson.estimatedMinutes} min</span><span>{lesson.completed ? 'Self-reported complete' : 'Not marked complete'}</span></p>
          {showCitations && <OutlineCitation citation={objectiveCitation} evidence={outlineEvidence} className="mb-4 mt-0" />}
          {lesson.preparation === 'outline' ? needsReview ? (
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-accent/30 bg-accent/5 p-4">
                <p className="max-w-xl text-sm leading-6 text-text-secondary">{reviewState === 'stopped' ? 'Preparation stopped with a cause to fix. The review shows it and lets you retry.' : hold === 'claims' ? 'Some claims could not be verified. The review shows each one with its evidence, one at a time.' : hold === 'resolved' ? 'Every claim is resolved. Resume verification from the review to publish the lesson.' : 'No claim needs you; the independent check has not finished. The review shows why.'}</p>
                {onReview && <button type="button" onClick={openReview} className="min-h-10 shrink-0 rounded-full bg-accent px-5 text-sm font-semibold text-accent-fg">{hold === 'claims' && waiting ? `Review ${claimsCount(waiting)}` : 'Open the review'}</button>}
              </div>
              {lesson.blocks.length > 0 && <div aria-label="Draft sections" className="space-y-3">
                {lesson.blocks.map((block, index) => (
                  <section key={`${lesson.id}-draft-${index}`} className="rounded-xl border border-border bg-surface p-5 opacity-90">
                    <div className="text-xs font-semibold uppercase tracking-[.12em] text-text-muted">Draft · {blockLabels[block.kind]}</div>
                    <h4 className="mt-1 font-serif text-lg text-text-primary">{block.title}</h4>
                    <div className="prose prose-sm mt-3 max-w-none text-text-secondary dark:prose-invert"><MarkdownViewer content={block.body} /></div>
                  </section>
                ))}
              </div>}
            </div>
          ) : (
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div><h4 className="font-medium text-text-primary">Ready when you are</h4><p className="mt-1 text-sm text-text-secondary">This prepares this lesson only. The accepted outline stays in place if preparation fails.</p></div>
              <div className="flex flex-col items-start gap-1.5 sm:items-end">
                <button type="button" disabled={pending || program.summary.status !== 'active' || Boolean(waitsForIndex)} aria-describedby={heldByIndex ? indexWaitId : undefined} onClick={onPrepare} className="rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{program.summary.status !== 'active' ? 'Accept the course to begin' : pending ? 'Preparing…' : 'Prepare lesson'}</button>
                {heldByIndex && <p id={indexWaitId} className="text-xs tabular-nums text-text-secondary">{heldByIndex}</p>}
              </div>
            </div>
          ) : (
            <>
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
