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
  const statusLabel = reviewState === 'stopped' ? 'Stopped with a cause to fix'
    : needsReview ? holdLabel[hold]
    : lesson.preparation === 'ready' ? teachingWaiting > 0 ? `Ready · ${teachingNotesForYou(teachingWaiting)}` : 'Ready'
    : preparingThis ? preparingLine(activeJob?.activity?.claims)
    : 'Outline';
  const reviewable = Boolean(onReview) && (needsReview || teachingWaiting > 0);
  const openReview = () => onReview?.(program.summary.id, lesson.id);

  return (
    <article
      id={`lesson-${lesson.id}`}
      className={`overflow-clip rounded-xl border bg-surface transition-colors ${
        selected && expanded
          ? 'border-border-strong ring-1 ring-accent/20 shadow-sm'
          : isCurrent
            ? 'border-accent/40 shadow-xs'
            : 'border-border-default shadow-xs hover:border-border-strong'
      }`}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3 sm:px-5">
        <span
          className={`grid h-7 w-7 shrink-0 place-items-center rounded-full ${
            lesson.completed
              ? 'bg-success-muted text-success-fg'
              : lesson.preparation === 'ready'
                ? 'bg-accent-muted text-accent'
                : 'bg-surface-sunken text-text-muted border border-border-subtle'
          }`}
        >
          {lesson.completed ? (
            <Check size={14} />
          ) : (
            <span className="font-serif text-sm">{lesson.preparation === 'ready' ? <Play size={11} /> : '·'}</span>
          )}
        </span>
        <button
          type="button"
          aria-expanded={expanded}
          aria-controls={bodyId}
          onClick={() => {
            onSelect();
            setExpanded((value) => !value);
          }}
          className="flex min-w-0 flex-1 items-center gap-2 text-left focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
        >
          <span className="min-w-0 truncate font-serif text-base font-medium text-text-primary">{lesson.title}</span>
          {isCurrent && (
            <span className="shrink-0 rounded-full bg-accent-muted px-2 py-0.5 text-xxs font-semibold uppercase tracking-wider text-accent">
              Resume here
            </span>
          )}
        </button>
        <span
          className={`inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2.5 py-0.5 text-xs ${
            needsReview || reviewable
              ? 'border border-accent/40 bg-accent-muted/30 font-semibold text-text-primary'
              : 'border border-border-subtle bg-surface-sunken text-text-muted'
          }`}
        >
          {needsReview && <ClipboardCheck size={12} aria-hidden="true" />}
          {statusLabel}
        </span>
        {reviewable && (
          <button
            type="button"
            onClick={openReview}
            className="min-h-8 shrink-0 rounded-lg bg-accent px-3.5 py-1 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors shadow-2xs"
          >
            Review
          </button>
        )}
      </div>

      {expanded && (
        <div id={bodyId} className="border-t border-border-subtle bg-surface px-5 py-6 sm:px-8">
          <div className="max-w-[68ch] text-sm leading-6 text-text-secondary">
            <StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle="Lesson Objective">
              <span>{lesson.objective || (lesson.preparation === 'outline' ? 'A lesson in the program outline. Prepare it when you are ready.' : 'A focused lesson with explanation and worked example.')}</span>
            </StudyTangentSelection>
          </div>
          <p className="mb-4 mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-muted">
            <span className="inline-flex items-center gap-1.5">
              <Clock3 size={12} />
              About {lesson.estimatedMinutes} min
            </span>
            <span>{lesson.completed ? 'Self-reported complete' : 'Not marked complete'}</span>
          </p>
          {showCitations && <OutlineCitation citation={objectiveCitation} evidence={outlineEvidence} className="mb-4 mt-0" />}
          {lesson.preparation === 'outline' ? (
            needsReview ? (
              <div className="space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-accent/30 bg-accent-muted/20 p-4">
                  <p className="max-w-xl text-xs leading-5 text-text-secondary">
                    {reviewState === 'stopped'
                      ? 'Preparation stopped with a cause to fix. The review shows it and lets you retry.'
                      : hold === 'claims'
                        ? 'Some claims could not be verified. The review shows each one with its evidence, one at a time.'
                        : hold === 'resolved'
                          ? 'Every claim is resolved. Resume verification from the review to publish the lesson.'
                          : 'No claim needs you; the independent check has not finished. The review shows why.'}
                  </p>
                  {onReview && (
                    <button
                      type="button"
                      onClick={openReview}
                      className="min-h-8 shrink-0 rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors shadow-2xs"
                    >
                      {hold === 'claims' && waiting ? `Review ${claimsCount(waiting)}` : 'Open the review'}
                    </button>
                  )}
                </div>
                {lesson.blocks.length > 0 && (
                  <div aria-label="Draft sections" className="divide-y divide-border-subtle rounded-xl border border-border-default bg-surface-sunken/40 px-4">
                    {lesson.blocks.map((block, index) => (
                      <section key={`${lesson.id}-draft-${index}`} className="py-4 first:pt-4 last:pb-4 opacity-90">
                        <div className="text-xxs font-semibold uppercase tracking-wider text-text-muted">
                          Draft · {blockLabels[block.kind]}
                        </div>
                        <h4 className="mt-1 font-serif text-base text-text-primary">{block.title}</h4>
                        <div className="prose prose-sm mt-2.5 max-w-[68ch] text-text-secondary dark:prose-invert">
                          <MarkdownViewer content={block.body} />
                        </div>
                      </section>
                    ))}
                  </div>
                )}
              </div>
            ) : (
              <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-border-subtle bg-surface-sunken/40 p-4">
                <div>
                  <h4 className="text-sm font-medium text-text-primary">Ready when you are</h4>
                  <p className="mt-1 text-xs text-text-secondary">
                    This prepares this lesson only. The accepted outline stays in place if preparation fails.
                  </p>
                </div>
                <div className="flex flex-col items-start gap-1.5 sm:items-end">
                  <button
                    type="button"
                    disabled={pending || program.summary.status !== 'active' || Boolean(waitsForIndex)}
                    aria-describedby={heldByIndex ? indexWaitId : undefined}
                    onClick={onPrepare}
                    className="min-h-9 rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors disabled:opacity-50 shadow-2xs"
                  >
                    {program.summary.status !== 'active' ? 'Accept the course to begin' : pending ? 'Preparing…' : 'Prepare lesson'}
                  </button>
                  {heldByIndex && <p id={indexWaitId} className="text-xs tabular-nums text-text-secondary">{heldByIndex}</p>}
                </div>
              </div>
            )
          ) : (
            <>
              <AuditLessonNow programId={program.summary.id} lessonId={lesson.id} />
              {showCitations && (
                <LessonEvidencePanel
                  key={lesson.id}
                  programId={program.summary.id}
                  lessonId={lesson.id}
                  programRevision={program.summary.revision}
                  report={evidenceReport}
                />
              )}
              {lesson.blocks.length >= 2 && (
                <nav aria-label={`Sections in ${lesson.title}`} className="mb-6 flex flex-wrap items-center gap-1.5 rounded-lg border border-border-subtle bg-surface-sunken/60 p-1.5 text-xs">
                  <span className="px-2 text-xxs font-semibold uppercase tracking-wider text-text-muted">Jump to:</span>
                  {lesson.blocks.map((block, index) => (
                    <a
                      key={index}
                      href={`#lesson-${lesson.id}-section-${index}`}
                      className="rounded-md px-2.5 py-1 text-xs text-text-secondary hover:bg-surface hover:text-text-primary transition-colors"
                    >
                      {index + 1}. {block.title}
                    </a>
                  ))}
                </nav>
              )}
              <div className="divide-y divide-border-subtle">
                {lesson.blocks.map((block, index) => (
                  <section id={`lesson-${lesson.id}-section-${index}`} key={`${lesson.id}-${index}`} className="py-6 first:pt-2 last:pb-2">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="rounded-md bg-surface-sunken px-2 py-0.5 text-xxs font-medium uppercase tracking-wider text-text-secondary border border-border-subtle">
                        Section {index + 1} · {blockLabels[block.kind]}
                      </span>
                      <SectionTangentBar moduleId={moduleId} lessonId={lesson.id} sectionIndex={index} sectionTitle={block.title} sectionText={`${block.title}\n\n${block.body}`} />
                    </div>
                    <h4 className="mt-2 font-serif text-xl leading-snug text-text-primary">{block.title}</h4>
                    <div className="prose prose-sm mt-3 max-w-[68ch] text-text-secondary dark:prose-invert">
                      <StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={index} sectionTitle={block.title}>
                        <MarkdownViewer content={block.body} />
                      </StudyTangentSelection>
                    </div>
                    {showCitations && <LessonSectionEvidence report={evidenceReport} sectionIndex={index} />}
                    {showCitations && evidenceReport.isSuccess && !evidenceReport.data && <SourceLine ids={block.sourceIds} sources={program.sources} />}
                    {block.kind === 'guided_practice' && selected && program.summary.status === 'active' && (
                      <div id={`guided-${lesson.id}`} className="mt-4">
                        <PracticeWorkbenchPanel key={lesson.id} program={program} lesson={lesson} taskKind="guided" embedded onContinue={onPractice} />
                      </div>
                    )}
                    {block.kind === 'independent_practice' && (
                      <button
                        type="button"
                        onClick={onPractice}
                        className="mt-4 inline-flex min-h-9 items-center gap-2 rounded-lg bg-accent px-3.5 py-1.5 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors shadow-2xs"
                      >
                        Work on this assignment <ArrowRight size={14} />
                      </button>
                    )}
                  </section>
                ))}
              </div>
              <div className="mt-6 flex flex-wrap items-center justify-between gap-3 border-t border-border-subtle pt-4">
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    onClick={onPractice}
                    className="inline-flex min-h-9 items-center gap-2 rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-accent-fg hover:bg-accent-hover transition-colors shadow-2xs"
                  >
                    Practice this lesson <ArrowRight size={14} />
                  </button>
                  <SectionTangentBar moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle={lesson.title} sectionText={`${lesson.title}\n\n${lesson.objective}`} label="Start a tangent on this lesson" />
                </div>
                <div className="flex flex-wrap items-center gap-3">
                  <p className="max-w-lg text-xs leading-5 text-text-muted">Reading the lesson does not count as completion.</p>
                  <button
                    type="button"
                    disabled={lesson.completed || pending}
                    onClick={onComplete}
                    className="inline-flex min-h-9 items-center gap-2 rounded-lg border border-border-default bg-surface px-3.5 py-1.5 text-xs font-medium text-text-primary hover:bg-surface-raised transition-colors disabled:opacity-45 shadow-2xs"
                  >
                    {lesson.completed ? (
                      <>
                        <CheckCircle2 size={14} className="text-success" /> Marked complete
                      </>
                    ) : pending ? (
                      'Saving…'
                    ) : (
                      <>
                        Mark lesson complete <Check size={14} />
                      </>
                    )}
                  </button>
                </div>
              </div>
            </>
          )}
        </div>
      )}
    </article>
  );
}
