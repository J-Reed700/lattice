import { useEffect, useId, useState, type ReactNode } from 'react';

import { ArrowRight, Check, CheckCircle2, ChevronDown, ClipboardCheck, Clock3 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { ResizableColumns } from '@/components/ui/ResizableColumns';
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
import { handleAsyncEvent } from '@/utils/promiseHandlers';

const blockLabels: Record<LearningBlockKind, string> = {
  explanation: 'Understand the idea', worked_example: 'Worked example', guided_practice: 'Try it with guidance',
  independent_practice: 'Your assignment', reflection: 'Reflect & connect', recap: 'Key takeaways',
};

export function LessonCard({ lesson, moduleId, outlineEvidence, program, isCurrent, selected, pending, onPrepare, onComplete, onSelect, onPractice, onReview, quizCallout, number, notebook, contextVisible = true, onSources }: {
  lesson: LearningLessonDto;
  number?: string;
  notebook?: ReactNode;
  contextVisible?: boolean;
  onSources?: () => void;
  moduleId: string;
  outlineEvidence?: LearningOutlineEvidenceDto | null;
  program: LearningProgramDto;
  isCurrent: boolean;
  selected: boolean;
  pending: boolean;
  onPrepare: () => void;
  onComplete: () => void;
  onSelect: () => void | Promise<boolean>;
  onPractice: () => void;
  /** Opens the lesson's claim review screen; the course page never shows claims itself. */
  onReview?: (programId: string, lessonId: string) => void;
  /** The lesson quiz callout, shown after the last section and before "Mark lesson complete". */
  quizCallout?: ReactNode;
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

  const hasMargin = lesson.preparation === 'ready' && contextVisible;
  const lessonSourceIds = new Set(lesson.blocks.flatMap((block) => block.sourceIds));
  const lessonSources = program.sources.filter((source) => lessonSourceIds.has(source.id));

  return (
    <article id={`lesson-${lesson.id}`} className={`studio-lesson ${expanded ? 'is-expanded' : ''}`}>
      <div className="studio-lesson-header">
        <span className={`studio-lesson-marker ${lesson.completed ? 'is-complete' : ''}`} aria-hidden="true">{lesson.completed ? <Check size={14} /> : number ?? '—'}</span>
        <button type="button" aria-label={lesson.title} aria-expanded={expanded} aria-controls={bodyId} onClick={handleAsyncEvent(async () => {
          const nextExpanded = !expanded;
          if (await onSelect() === false) return;
          setExpanded(nextExpanded);
        })} className="studio-lesson-toggle">
          <span className="studio-lesson-row-title">{expanded ? 'Lesson' : lesson.title}</span>
          {isCurrent && <span className="studio-resume-tag">Resume here</span>}
          <ChevronDown size={13} className={`shrink-0 text-text-muted transition ${expanded ? 'rotate-180' : ''}`} />
        </button>
        <span className="studio-lesson-status">{needsReview && <ClipboardCheck size={12} aria-hidden="true" className="mr-1 inline" />}{statusLabel}</span>
        {reviewable && <button type="button" onClick={openReview} className="studio-lesson-review">Review</button>}
      </div>
      {expanded && <ResizableColumns id={bodyId} label="Lesson notes width" storageKey="lattice.studio.lesson-margin-width" className={`studio-lesson-body studio-lesson-layout ${hasMargin ? '' : 'without-margin'}`} sidebar={hasMargin ? (<aside className="studio-lesson-margin" aria-label={`Lesson outline and notes for ${lesson.title}`}><div className="studio-margin-sticky">
          <section className="studio-margin-block"><h4 className="studio-margin-heading">In this lesson</h4><nav aria-label={`Sections in ${lesson.title}`} className="studio-lesson-toc">{lesson.blocks.map((block, index) => <a key={index} href={`#lesson-${lesson.id}-section-${index}`}><span>{String(index + 1).padStart(2, '0')}</span><span>{block.title}</span></a>)}</nav></section>
          {notebook && <div className="studio-margin-block">{notebook}</div>}
          {lessonSources.length > 0 && <section className="studio-margin-block studio-margin-sources"><h4 className="studio-margin-heading">Source material</h4>{lessonSources.map((source) => <div key={source.id}><button type="button" onClick={onSources}>{source.title} ↗</button><small>Saved reference · {new Date(source.acquiredAt).toLocaleDateString()}</small></div>)}</section>}
        </div></aside>) : null}>
        <div className="studio-lesson-reading">
          <h3 className="studio-lesson-heading">{lesson.title}</h3>
          <div className="studio-lesson-objective"><StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle="Lesson Objective"><span>{lesson.objective || (lesson.preparation === 'outline' ? 'A lesson in the program outline. Prepare it when you are ready.' : 'A focused lesson with explanation and worked example.')}</span></StudyTangentSelection></div>
          <p className="studio-lesson-meta"><span><Clock3 size={13} />About {lesson.estimatedMinutes} min</span><span>{lesson.blocks.length} {lesson.blocks.length === 1 ? 'section' : 'sections'}</span><span>{lesson.completed ? 'Self-reported complete' : 'Not marked complete'}</span></p>
          {showCitations && <OutlineCitation citation={objectiveCitation} evidence={outlineEvidence} className="mb-4 mt-0" />}
          {lesson.preparation === 'outline' ? needsReview ? (
            <div className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-accent/30 bg-accent/5 p-4">
                <p className="max-w-xl text-sm leading-6 text-text-secondary">{reviewState === 'stopped' ? 'Preparation stopped with a cause to fix. The review shows it and lets you retry.' : hold === 'claims' ? 'Some claims could not be verified. The review shows each one with its evidence, one at a time.' : hold === 'resolved' ? 'Every claim is resolved. Resume verification from the review to publish the lesson.' : 'No claim needs you; the independent check has not finished. The review shows why.'}</p>
                {onReview && <Button variant="outline" type="button" onClick={openReview} className="studio-button studio-button-primary">{hold === 'claims' && waiting ? `Review ${claimsCount(waiting)}` : 'Open the review'}</Button>}
              </div>
              {lesson.blocks.length > 0 && <div aria-label="Draft sections" className="studio-lesson-sections">
                {lesson.blocks.map((block, index) => <section key={`${lesson.id}-draft-${index}`} className="studio-lesson-section"><div className="studio-section-label">Draft · {blockLabels[block.kind]}</div><h4>{block.title}</h4><div className="studio-lesson-prose"><MarkdownViewer content={block.body} /></div></section>)}
              </div>}
            </div>
          ) : (
            <div className="flex flex-wrap items-center justify-between gap-4 border-t border-border pt-6">
              <div><h4 className="font-medium text-text-primary">Ready when you are</h4><p className="mt-1 text-sm text-text-secondary">This prepares this lesson only. The accepted outline stays in place if preparation fails.</p></div>
              <div className="flex flex-col items-start gap-1.5 sm:items-end">
                <Button variant="outline" type="button" disabled={pending || program.summary.status !== 'active' || Boolean(waitsForIndex)} aria-describedby={heldByIndex ? indexWaitId : undefined} onClick={onPrepare} className="studio-button studio-button-primary disabled:opacity-50">{program.summary.status !== 'active' ? 'Accept the course to begin' : pending ? 'Preparing…' : 'Prepare lesson'}</Button>
                {heldByIndex && <p id={indexWaitId} className="text-xs tabular-nums text-text-secondary">{heldByIndex}</p>}
              </div>
            </div>
          ) : (
            <>
              <AuditLessonNow programId={program.summary.id} lessonId={lesson.id} />
              {showCitations && <LessonEvidencePanel key={lesson.id} programId={program.summary.id} lessonId={lesson.id} programRevision={program.summary.revision} report={evidenceReport} />}
              <div className="studio-lesson-sections">
                {lesson.blocks.map((block, index) => <section id={`lesson-${lesson.id}-section-${index}`} key={`${lesson.id}-${index}`} className="studio-lesson-section">
                  <div className="studio-section-label">{blockLabels[block.kind]}</div>
                  <h4>{block.title}</h4>
                  <div className="studio-lesson-prose"><StudyTangentSelection moduleId={moduleId} lessonId={lesson.id} sectionIndex={index} sectionTitle={block.title}><MarkdownViewer content={block.body} /></StudyTangentSelection></div>
                  {showCitations && <LessonSectionEvidence report={evidenceReport} sectionIndex={index} />}
                  {showCitations && evidenceReport.isSuccess && !evidenceReport.data && <SourceLine ids={block.sourceIds} sources={program.sources} />}
                  {block.kind === 'guided_practice' && selected && program.summary.status === 'active' && <div id={`guided-${lesson.id}`}><PracticeWorkbenchPanel key={lesson.id} program={program} lesson={lesson} taskKind="guided" embedded onContinue={onPractice} /></div>}
                  {block.kind === 'independent_practice' && <Button variant="outline" type="button" onClick={onPractice} className="studio-button studio-button-primary mt-5">Work on this assignment <ArrowRight size={15} /></Button>}
                  <SectionTangentBar moduleId={moduleId} lessonId={lesson.id} sectionIndex={index} sectionTitle={block.title} sectionText={`${block.title}\n\n${block.body}`} />
                </section>)}
              </div>
              <SectionTangentBar moduleId={moduleId} lessonId={lesson.id} sectionIndex={null} sectionTitle={lesson.title} sectionText={`${lesson.title}\n\n${lesson.objective}`} label="Start a tangent on this lesson" />
              {quizCallout}
              <div className="studio-lesson-actions"><Button variant="outline" type="button" onClick={onPractice} className="studio-button studio-button-primary">Practice this lesson <ArrowRight size={15} /></Button><div><p>Reading the lesson does not count as completion.</p><Button variant="outline" type="button" disabled={lesson.completed || pending} onClick={onComplete} className="studio-button disabled:opacity-45">{lesson.completed ? <><CheckCircle2 size={15} /> Marked complete</> : pending ? 'Saving…' : <>Mark lesson complete <Check size={15} /></>}</Button></div></div>
            </>
          )}
        </div>
      </ResizableColumns>}
    </article>
  );
}
