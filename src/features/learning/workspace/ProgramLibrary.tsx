import type { RefObject } from 'react';

import { Archive, ArrowRight, Layers3, LoaderCircle, Plus, Upload } from 'lucide-react';

import { PlainError } from '@/components/PlainError';
import { Skeleton } from '@/components/Skeleton';
import { Button } from '@/components/ui/button';
import {
  describeOutlineStage,
  formatOutlineDuration,
  useOutlineElapsedSeconds,
} from '@/features/learning/curriculum/OutlineGenerationProgress';
import { StudyHabitsBar, type FrameworkAction } from '@/features/learning/habits';
import { usePortabilityRunStore } from '@/features/learning/portability/portabilityRunStore';
import { FlashcardsSection } from '@/features/learning/recall/flashcards/FlashcardsSection';
import { CourseTangentsSection } from '@/features/learning/tangents/CourseTangentsSection';
import type { LearningOutlineProgressDto, LearningProgramSummaryDto } from '@/lib/bindings';
import type { StudyDestination } from '@/lib/studyActivity';

export interface OutlineGenerationLibraryState {
  isPending: boolean;
  progress: LearningOutlineProgressDto | null;
  goal?: string | null;
  cancelling?: boolean;
  cancelError?: string;
  error: Error | null;
  onOpenBuilder: () => void;
  onOpenDraft?: () => void;
  onCancel?: () => void;
  onDismissError?: () => void;
}

function OutlineGenerationBanner({ generation }: { generation: OutlineGenerationLibraryState }) {
  const elapsed = useOutlineElapsedSeconds(generation.progress?.elapsedSeconds);
  const stage = generation.progress?.stage;
  const stageInfo = describeOutlineStage(stage);
  const stopping = Boolean(generation.cancelling && stage !== 'saving' && stage !== 'completed');

  return <section role="region" aria-label="Course generation in progress" className="mb-5 rounded-xl border border-border-default bg-surface p-4 shadow-sm sm:p-5">
    <div className="flex flex-col items-start justify-between gap-3 sm:flex-row">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 text-sm font-semibold text-text-primary" role="status">
          <LoaderCircle size={16} className="shrink-0 animate-spin text-accent" aria-hidden="true" />
          {stopping ? 'Cancelling generation…' : stageInfo.title}
        </div>
        <p className="mt-1 text-sm leading-6 text-text-secondary">
          {stopping ? 'Stopping this request. Your inputs and completed draft corrections stay saved.' : stageInfo.detail}
        </p>
        {!stopping && generation.progress?.detail && (
          <p className="mt-1 text-xs text-text-muted" aria-label="Stage progress">{generation.progress.detail}</p>
        )}
        {generation.goal && (
          <p className="mt-1 truncate text-xs text-text-muted">Goal: {generation.goal}</p>
        )}
      </div>
      <span className="text-xs tabular-nums text-text-muted" aria-live="off">
        Elapsed <span aria-label="Elapsed time">{formatOutlineDuration(elapsed)}</span>
      </span>
    </div>
    <div className="mt-3.5 flex flex-wrap items-center gap-2">
      <Button type="button" onClick={generation.onOpenBuilder}>
        View progress
      </Button>
      {generation.progress?.programId && generation.onOpenDraft && (
        <Button variant="outline" type="button" onClick={generation.onOpenDraft}>
          Inspect saved draft
        </Button>
      )}
      {generation.onCancel && (
        <Button
          variant="outline"
          type="button"
          onClick={generation.onCancel}
          disabled={generation.cancelling || stage === 'saving' || stage === 'completed'}
        >
          {stopping ? 'Cancelling…' : 'Cancel generation'}
        </Button>
      )}
    </div>
    {generation.cancelError && <p role="alert" className="mt-3 text-sm text-danger-fg">Could not cancel: {generation.cancelError}</p>}
  </section>;
}

const PROGRESS_RING_SIZE = 44;
const PROGRESS_RING_STROKE = 3.25;
const PROGRESS_RING_RADIUS = (PROGRESS_RING_SIZE - PROGRESS_RING_STROKE) / 2;
const PROGRESS_RING_CIRCUMFERENCE = 2 * Math.PI * PROGRESS_RING_RADIUS;

function CourseProgressRing({ percent, active }: { percent: number; active: boolean }) {
  const clamped = active ? Math.max(0, Math.min(100, percent)) : 0;
  const dashOffset = PROGRESS_RING_CIRCUMFERENCE * (1 - clamped / 100);
  const center = PROGRESS_RING_SIZE / 2;

  return (
    <span aria-hidden="true" className="relative inline-grid h-11 w-11 shrink-0 place-items-center">
      <svg
        width={PROGRESS_RING_SIZE}
        height={PROGRESS_RING_SIZE}
        viewBox={`0 0 ${PROGRESS_RING_SIZE} ${PROGRESS_RING_SIZE}`}
        className="-rotate-90"
      >
        <circle
          cx={center}
          cy={center}
          r={PROGRESS_RING_RADIUS}
          fill="none"
          strokeWidth={PROGRESS_RING_STROKE}
          className="stroke-border-default"
        />
        {clamped > 0 && (
          <circle
            cx={center}
            cy={center}
            r={PROGRESS_RING_RADIUS}
            fill="none"
            strokeWidth={PROGRESS_RING_STROKE}
            strokeLinecap="round"
            strokeDasharray={PROGRESS_RING_CIRCUMFERENCE}
            strokeDashoffset={dashOffset}
            className="stroke-accent transition-[stroke-dashoffset] duration-300 ease-out motion-reduce:transition-none"
          />
        )}
      </svg>
      <span className="absolute font-sans text-[11px] font-semibold tabular-nums leading-none text-text-primary">
        {active ? `${clamped}%` : '0%'}
      </span>
    </span>
  );
}

function Progress({ program }: { program: LearningProgramSummaryDto }) {
  const active = program.status === 'active';
  const percent = program.lessonCount ? Math.min(100, Math.round((program.completedLessons / program.lessonCount) * 100)) : 0;
  return <div className="studio-program-progress">
    <span className="studio-program-progress-label">{active ? `${program.completedLessons} / ${program.lessonCount} complete` : 'Draft outline'}</span>
    {program.lessonCount > 0 && <CourseProgressRing percent={percent} active={active} />}
  </div>;
}

function PortabilityBanner({ onOpen, onImport, onExport }: {
  onOpen: (id: string) => void;
  onImport: () => void;
  onExport: () => void;
}) {
  const exportAll = usePortabilityRunStore((store) => store.exportAll);
  const importFlow = usePortabilityRunStore((store) => store.importFlow);
  const dismissExportAll = usePortabilityRunStore((store) => store.dismissExportAllBanner);
  const dismissImport = usePortabilityRunStore((store) => store.dismissImportBanner);

  const showExport = exportAll.pending || exportAll.finishedWhileAway;
  const showImport = importFlow.pendingAction === 'apply' || importFlow.finishedWhileAway;
  if (!showExport && !showImport) return null;

  const exportedCount = exportAll.result?.exported.length ?? 0;
  const failedCount = exportAll.result?.failed.length ?? 0;
  const totalCount = exportedCount + failedCount;

  return <div className="mb-5 space-y-3">
    {showExport && (
      <section role="region" aria-label="Course export status" className="rounded-xl border border-border-default bg-surface p-4 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0 flex-1">
            {exportAll.pending ? (
              <p role="status" className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                <LoaderCircle size={15} className="shrink-0 animate-spin text-accent" aria-hidden="true" />
                Exporting all courses…
              </p>
            ) : exportAll.error ? (
              <p role="alert" className="text-sm text-danger-fg">
                Course export failed: {exportAll.error}
              </p>
            ) : exportAll.result ? (
              <p role="status" className="text-sm text-text-primary">
                {totalCount === 0
                  ? 'Export complete: there are no courses to export yet.'
                  : `Exported ${exportedCount} of ${totalCount} ${totalCount === 1 ? 'course' : 'courses'} to ${exportAll.result.destinationDir}.`}
              </p>
            ) : null}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button type="button" onClick={onExport}>
              View export
            </Button>
            {!exportAll.pending && (
              <Button variant="outline" type="button" onClick={dismissExportAll}>
                Dismiss
              </Button>
            )}
          </div>
        </div>
      </section>
    )}
    {showImport && (
      <section role="region" aria-label="Course import status" className="rounded-xl border border-border-default bg-surface p-4 shadow-sm">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0 flex-1">
            {importFlow.pendingAction === 'apply' ? (
              <p role="status" className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                <LoaderCircle size={15} className="shrink-0 animate-spin text-accent" aria-hidden="true" />
                Importing course…
              </p>
            ) : importFlow.failure ? (
              <p role="alert" className="text-sm text-danger-fg">
                Course import failed: {importFlow.failure.message}
              </p>
            ) : importFlow.lastImported ? (
              <p role="status" className="text-sm text-text-primary">
                “{importFlow.lastImported.title}” is ready on this computer.
              </p>
            ) : null}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {importFlow.pendingAction !== 'apply' && importFlow.lastImported && !importFlow.failure && (
              <Button
                type="button"
                onClick={() => {
                  const id = importFlow.lastImported!.programId;
                  dismissImport();
                  onOpen(id);
                }}
              >
                Open course
              </Button>
            )}
            <Button variant="outline" type="button" onClick={onImport}>
              View import
            </Button>
            {importFlow.pendingAction !== 'apply' && (
              <Button variant="outline" type="button" onClick={dismissImport}>
                Dismiss
              </Button>
            )}
          </div>
        </div>
      </section>
    )}
  </div>;
}

export function ProgramLibrary({ programs, loading, error, onRetry, onOpen, onBuild, onImport, onExport, onSelectFramework, onOpenDestination, flashcardsRef, outlineGeneration }: {
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
  outlineGeneration?: OutlineGenerationLibraryState;
}) {
  const continuing = programs?.find((program) => program.status === 'active' && program.completedLessons < program.lessonCount);
  const orderedPrograms = programs && continuing
    ? [continuing, ...programs.filter((program) => program.id !== continuing.id)]
    : programs;
  const generatingOutline = Boolean(outlineGeneration?.isPending);

  return <div className="studio-library">
    <header className="studio-library-header">
      <div>
        <h1>Learning Studio</h1>
        <p>Focused courses, practice, and recall that stay on this device.</p>
      </div>
      <div className="studio-library-actions">
        <Button variant="outline" type="button" onClick={onImport}><Upload size={14} /> Import a course…</Button>
        <Button variant="outline" type="button" onClick={onExport}><Archive size={14} /> Export all courses…</Button>
        <Button type="button" onClick={onBuild}><Plus size={14} /> {generatingOutline ? 'View outline progress' : 'New course'}</Button>
      </div>
    </header>
    {outlineGeneration?.isPending && <OutlineGenerationBanner generation={outlineGeneration} />}
    <PortabilityBanner onOpen={onOpen} onImport={onImport} onExport={onExport} />
    {!outlineGeneration?.isPending && outlineGeneration?.error && (
      <div className="mb-5 rounded-xl border border-danger/25 bg-danger-muted/30 p-4">
        <PlainError error={outlineGeneration.error} className="text-sm text-danger-fg" />
        <div className="mt-3 flex flex-wrap gap-2">
          <Button type="button" onClick={outlineGeneration.onOpenBuilder}>
            Return to course builder
          </Button>
          {outlineGeneration.onDismissError && (
            <Button variant="outline" type="button" onClick={outlineGeneration.onDismissError}>
              Dismiss
            </Button>
          )}
        </div>
      </div>
    )}
    <StudyHabitsBar className="studio-habits" onSelectFramework={onSelectFramework} />
    <CourseTangentsSection onOpenDestination={onOpenDestination} />
    <section className="studio-programs" aria-labelledby="studio-programs-title">
      <div className="studio-collection-heading">
        <h2 id="studio-programs-title">Your programs</h2>
        <span>{programs?.length ?? 0} {programs?.length === 1 ? 'course' : 'courses'}</span>
      </div>
      {loading ? <div role="status" aria-label="Loading your programs" className="studio-library-skeleton"><span className="sr-only">Loading your programs…</span>{[0, 1, 2].map((row) => <div key={row} className="studio-library-skeleton-row"><Skeleton width={40} height={50} /><div><Skeleton width={`${65 - row * 12}%`} height={16} /><Skeleton width="85%" height={11} className="mt-3" /><Skeleton width="40%" height={10} className="mt-2" /></div></div>)}</div>
        : error ? <div role="alert" className="studio-library-empty"><h3>Programs are unavailable</h3><p>{error}</p><Button variant="outline" type="button" onClick={onRetry}>Retry</Button></div>
          : orderedPrograms?.length ? <div className="studio-program-list">{orderedPrograms.map((program, index) => {
            const isFeatured = program.id === continuing?.id;
            const created = new Date(program.createdAt);
            const createdLabel = Number.isNaN(created.getTime()) ? 'date unavailable' : created.toLocaleDateString();
            return <button type="button" key={program.id} onClick={() => onOpen(program.id)} className={`studio-program-row border border-border-subtle bg-surface shadow-sm ${isFeatured ? 'studio-program-row--featured' : ''}`}>
              <span className="studio-program-cover" aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>
              <span className="studio-program-copy">
                {isFeatured && <span className="studio-program-badge">Continue studying</span>}
                <span className="studio-program-title font-sans font-semibold tracking-[-0.025em]">{program.title}</span>
                <span className="studio-program-description">{program.goal}</span>
                <span className="studio-program-meta">{program.moduleCount} {program.moduleCount === 1 ? 'module' : 'modules'} · {program.lessonCount} {program.lessonCount === 1 ? 'lesson' : 'lessons'} · {program.status === 'active' ? 'Active' : 'Review outline'}<span className="studio-program-created"> · created {createdLabel}</span></span>
              </span>
              <Progress program={program} />
              <span className={`studio-program-action ${isFeatured ? 'studio-program-action--resume' : ''}`}>
                {isFeatured ? <><span>Resume</span><ArrowRight size={14} aria-hidden="true" /></> : <ArrowRight size={16} className="studio-program-arrow" aria-hidden="true" />}
              </span>
            </button>;
          })}</div> : <div className="studio-library-empty"><Layers3 size={24} className="text-accent" /><h3>What would you like to understand?</h3><p>Start with a goal and your experience. Add materials if you have them, then review the outline before preparing your first lesson.</p><div className="studio-library-actions"><Button type="button" onClick={onBuild}>{generatingOutline ? 'View outline progress' : 'Build your first program'} <ArrowRight size={14} /></Button><Button variant="outline" type="button" onClick={onImport}><Upload size={14} /> Import a course</Button></div></div>}
      {Boolean(programs?.some((program) => program.status === 'active')) && <p className="studio-progress-caption">Lesson completion is self-reported. Quiz and assessment results are recorded inside each program.</p>}
    </section>
    <FlashcardsSection ref={flashcardsRef} />
    <footer className="studio-library-footer"><span>Topic or source-based · Review the outline before lessons</span><span>Programs stay on this device</span></footer>
  </div>;
}
