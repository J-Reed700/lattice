import type { RefObject } from 'react';

import { Archive, ArrowRight, BookOpenCheck, Layers3, Plus, Upload } from 'lucide-react';

import { Skeleton } from '@/components/Skeleton';
import { Button } from '@/components/ui/button';
import { StudyHabitsBar, type FrameworkAction } from '@/features/learning/habits';
import { FlashcardsSection } from '@/features/learning/recall/flashcards/FlashcardsSection';
import { CourseTangentsSection } from '@/features/learning/tangents/CourseTangentsSection';
import type { LearningProgramSummaryDto } from '@/lib/bindings';
import type { StudyDestination } from '@/lib/studyActivity';

function Progress({ program }: { program: LearningProgramSummaryDto }) {
  const percent = program.lessonCount ? Math.min(100, Math.round(program.completedLessons / program.lessonCount * 100)) : 0;
  return <div className="studio-program-progress">
    <span>{program.status === 'active' ? `${program.completedLessons} / ${program.lessonCount} complete` : 'Draft outline'}</span>
    <div className="studio-progress-track" aria-hidden="true"><span style={{ width: `${program.status === 'active' ? percent : 0}%` }} /></div>
  </div>;
}

/** A decorative course path; all progress labels come from the saved summary. */
function CoursePath() {
  return <svg className="studio-course-path" viewBox="0 0 280 190" fill="none" aria-hidden="true">
    <path className="studio-path-grid" d="M0 35H280M0 95H280M0 155H280M40 0V190M140 0V190M240 0V190" />
    <path className="studio-path-line" d="M40 135L108 55L174 125L240 45M40 135L174 125M108 55L240 45" />
    <circle className="studio-path-orbit" cx="140" cy="95" r="81" />
    <circle className="studio-path-node" cx="40" cy="135" r="8" />
    <circle className="studio-path-node" cx="108" cy="55" r="8" />
    <circle className="studio-path-node" cx="174" cy="125" r="8" />
    <circle className="studio-path-node studio-path-current" cx="240" cy="45" r="9" />
    <circle className="studio-path-halo" cx="240" cy="45" r="17" />
  </svg>;
}

export function ProgramLibrary({ programs, loading, error, onRetry, onOpen, onBuild, onImport, onExport, onSelectFramework, onOpenDestination, flashcardsRef }: {
  programs?: LearningProgramSummaryDto[];
  loading: boolean;
  error?: string;
  onRetry: () => void;
  onOpen: (id: string) => void;
  onBuild: () => void;
  onImport: () => void;
  onExport: () => void;
  onSelectFramework: (action: FrameworkAction) => void;
  onOpenDestination: (destination: StudyDestination) => void;
  flashcardsRef: RefObject<HTMLElement | null>;
}) {
  const continuing = programs?.find((program) => program.status === 'active' && program.completedLessons < program.lessonCount);
  return <div className="studio-library">
    <header className="studio-library-header">
      <div><div className="studio-eyebrow"><BookOpenCheck size={14} /> Learning Studio</div><h1>Your programs</h1><p>A place for focused study, practice, and ideas that stay with you.</p></div>
      <div className="studio-library-actions">
        <Button variant="outline" type="button" onClick={onImport} className="studio-button"><Upload size={14} /> Import a course…</Button>
        <Button variant="outline" type="button" onClick={onExport} className="studio-button"><Archive size={14} /> Export all courses…</Button>
        <Button variant="outline" type="button" onClick={onBuild} className="studio-button studio-button-primary"><Plus size={15} /> Build a program</Button>
      </div>
    </header>
    <StudyHabitsBar className="studio-habits" onSelectFramework={onSelectFramework} />
    {continuing && <section className="studio-resume" aria-label="Continue studying">
      <div className="studio-resume-copy">
        <div className="studio-eyebrow">Continue where you left off</div>
        <h2>{continuing.title}</h2>
        <p>{continuing.goal}</p>
        <div className="studio-resume-footer"><Button variant="outline" type="button" className="studio-button studio-button-primary" onClick={() => onOpen(continuing.id)}>Resume program <ArrowRight size={15} /></Button><span>{continuing.completedLessons} of {continuing.lessonCount} {continuing.lessonCount === 1 ? 'lesson' : 'lessons'} complete</span></div>
      </div>
      <div className="studio-resume-visual"><CoursePath /><span className="studio-eyebrow">{continuing.moduleCount} {continuing.moduleCount === 1 ? 'module' : 'modules'} · {continuing.lessonCount} {continuing.lessonCount === 1 ? 'lesson' : 'lessons'}</span></div>
    </section>}
    <CourseTangentsSection onOpenDestination={onOpenDestination} />
    <section className="studio-programs" aria-labelledby="studio-programs-title">
      <div className="studio-collection-heading"><h2 id="studio-programs-title">Programs</h2><span>{programs?.length ?? 0} {programs?.length === 1 ? 'program' : 'programs'}</span></div>
      {loading ? <div role="status" aria-label="Loading your programs" className="studio-library-skeleton"><span className="sr-only">Loading your programs…</span>{[0, 1, 2].map((row) => <div key={row} className="studio-library-skeleton-row"><Skeleton width={40} height={50} /><div><Skeleton width={`${65 - row * 12}%`} height={16} /><Skeleton width="85%" height={11} className="mt-3" /><Skeleton width="40%" height={10} className="mt-2" /></div></div>)}</div>
        : error ? <div role="alert" className="studio-library-empty"><h3>Programs are unavailable</h3><p>{error}</p><Button variant="outline" type="button" onClick={onRetry} className="studio-button">Retry</Button></div>
          : programs?.length ? <div className="studio-program-list">{programs.map((program, index) => {
            const created = new Date(program.createdAt);
            const createdLabel = Number.isNaN(created.getTime()) ? 'date unavailable' : created.toLocaleDateString();
            return <button type="button" key={program.id} onClick={() => onOpen(program.id)} className="studio-program-row">
              <span className="studio-program-cover" aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>
              <span className="studio-program-copy"><span className="studio-program-title">{program.title}</span><span className="studio-program-description">{program.goal}</span><span className="studio-program-meta">{program.moduleCount} {program.moduleCount === 1 ? 'module' : 'modules'} · {program.lessonCount} {program.lessonCount === 1 ? 'lesson' : 'lessons'} · {program.status === 'active' ? 'Active' : 'Review outline'}<span className="studio-program-created"> · created {createdLabel}</span></span></span>
              <Progress program={program} />
              <ArrowRight size={17} className="studio-program-arrow" />
            </button>;
          })}</div> : <div className="studio-library-empty"><Layers3 size={28} className="text-accent" /><h3>What would you like to understand?</h3><p>Start with a goal and your experience. Add materials if you have them, then review the outline before preparing your first lesson.</p><div className="studio-library-actions"><Button variant="outline" type="button" onClick={onBuild} className="studio-button studio-button-primary">Build your first program <ArrowRight size={15} /></Button><Button variant="outline" type="button" onClick={onImport} className="studio-button"><Upload size={14} /> Import a course</Button></div></div>}
      {Boolean(programs?.some((program) => program.status === 'active')) && <p className="studio-progress-caption">Lesson completion is self-reported. Quiz and assessment results are recorded inside each program.</p>}
    </section>
    <FlashcardsSection ref={flashcardsRef} />
    <footer className="studio-library-footer"><span>Topic or source-based · Review the outline before lessons</span><span>Programs stay on this device</span></footer>
  </div>;
}
