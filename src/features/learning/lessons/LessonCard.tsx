import { useEffect, useState } from 'react';

import { ArrowRight, Check, CheckCircle2, Clock3, Play } from 'lucide-react';

import { useLearningLessonEvidence } from '@/features/learning/api/evidenceQueries';
import { OutlineCitation } from '@/features/learning/curriculum/OutlineEvidence';
import { LessonEvidencePanel, LessonSectionEvidence } from '@/features/learning/lessons/LessonEvidencePanel';
import { findOutlineCitation } from '@/features/learning/model/evidencePresentation';
import { PracticeWorkbenchPanel } from '@/features/learning/practice/PracticeWorkbenchPanel';
import { SourceLine } from '@/features/learning/sources/SourceLine';
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

  return (
    <article id={`lesson-${lesson.id}`} className={`overflow-hidden rounded-2xl border bg-surface transition ${isCurrent ? 'border-accent/50 shadow-[0_8px_34px_-24px_hsl(var(--accent))]' : 'border-border shadow-sm'}`}>
      <button type="button" onClick={() => { onSelect(); setExpanded((value) => !value); }} className="flex w-full items-start gap-4 p-5 text-left sm:p-6">
        <span className={`mt-0.5 grid h-9 w-9 shrink-0 place-items-center rounded-full ${lesson.completed ? 'bg-emerald-500/15 text-emerald-700' : lesson.preparation === 'ready' ? 'bg-accent/10 text-accent' : 'bg-background text-text-muted'}`}>
          {lesson.completed ? <Check size={16} /> : <span className="font-serif text-base">{lesson.preparation === 'ready' ? <Play size={14} /> : '·'}</span>}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-center gap-2"><span className="font-serif text-xl text-text-primary">{lesson.title}</span>{isCurrent && <span className="rounded-full bg-accent/10 px-2 py-0.5 text-xs font-semibold uppercase tracking-wider text-accent">Resume here</span>}</span>
          <span className="mt-1 block text-sm leading-6 text-text-secondary">{lesson.objective || (lesson.preparation === 'outline' ? 'A lesson in the program outline. Prepare it when you are ready.' : 'A focused lesson with explanation and worked example.')}</span>
          <span className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-text-muted"><span className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap"><Clock3 size={13} />About {lesson.estimatedMinutes} min</span><span className="shrink-0 whitespace-nowrap rounded-full bg-background px-2 py-0.5">{lesson.preparation === 'ready' ? 'Ready' : 'Outline'}</span><span className="shrink-0 whitespace-nowrap">{lesson.completed ? 'Self-reported complete' : 'Not marked complete'}</span></span>
        </span>
        <span className="mt-2 shrink-0 text-xs text-text-muted">{expanded ? 'Close' : 'Open'}</span>
      </button>

      {expanded && (
        <div className="border-t border-border bg-background/45 px-5 py-6 sm:px-9">
          {showCitations && <OutlineCitation citation={objectiveCitation} evidence={outlineEvidence} className="mb-4 mt-0" />}
          {lesson.preparation === 'outline' ? (
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div><h4 className="font-medium text-text-primary">Ready when you are</h4><p className="mt-1 text-sm text-text-secondary">This prepares this lesson only. The accepted outline stays in place if preparation fails.</p></div>
              <button type="button" disabled={pending || program.summary.status !== 'active'} onClick={onPrepare} className="rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50">{program.summary.status !== 'active' ? 'Accept the course to begin' : pending ? 'Preparing…' : 'Prepare lesson'}</button>
            </div>
          ) : (
            <>
              {showCitations && <LessonEvidencePanel key={lesson.id} programId={program.summary.id} lessonId={lesson.id} programRevision={program.summary.revision} report={evidenceReport} />}
              <nav aria-label={`Sections in ${lesson.title}`} className="mb-5 flex flex-wrap gap-2">
                {lesson.blocks.map((block, index) => <a key={index} href={`#lesson-${lesson.id}-section-${index}`} className="rounded-full border border-border bg-surface px-3 py-2 text-xs text-text-secondary hover:border-accent">{index + 1}. {block.title}</a>)}
              </nav>
              <div className="space-y-4">
                {lesson.blocks.map((block, index) => (
                  <section id={`lesson-${lesson.id}-section-${index}`} key={`${lesson.id}-${index}`} className="rounded-xl border border-border bg-surface p-5">
                    <div className="text-xs font-semibold uppercase tracking-[.12em] text-accent">{blockLabels[block.kind]}</div>
                    <h4 className="mt-1 font-serif text-lg text-text-primary">{block.title}</h4>
                    <div className="prose prose-sm mt-3 max-w-none text-text-secondary dark:prose-invert"><MarkdownViewer content={block.body} /></div>
                    {showCitations && <LessonSectionEvidence report={evidenceReport} sectionIndex={index} />}
                    {showCitations && evidenceReport.isSuccess && !evidenceReport.data && <SourceLine ids={block.sourceIds} sources={program.sources} />}
                    {block.kind === 'guided_practice' && selected && program.summary.status === 'active' && <div id={`guided-${lesson.id}`}><PracticeWorkbenchPanel key={lesson.id} program={program} lesson={lesson} taskKind="guided" embedded onContinue={onPractice} /></div>}
                    {block.kind === 'independent_practice' && <button type="button" onClick={onPractice} className="mt-5 inline-flex min-h-11 items-center gap-2 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg">Work on this assignment <ArrowRight size={15} /></button>}
                  </section>
                ))}
              </div>
              <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-5"><button type="button" onClick={onPractice} className="inline-flex min-h-11 items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-sm font-semibold text-accent-fg hover:brightness-95">Practice this lesson <ArrowRight size={15} /></button><div className="flex flex-wrap items-center gap-3"><p className="max-w-lg text-xs leading-5 text-text-muted">Reading the lesson does not count as completion.</p><button type="button" disabled={lesson.completed || pending} onClick={onComplete} className="inline-flex min-h-11 items-center gap-2 rounded-full border border-accent/40 px-4 py-2.5 text-sm font-medium text-accent hover:bg-accent/5 disabled:opacity-45">{lesson.completed ? <><CheckCircle2 size={15} /> Marked complete</> : pending ? 'Saving…' : <>Mark lesson complete <Check size={15} /></>}</button></div></div>
            </>
          )}
        </div>
      )}
    </article>
  );
}
