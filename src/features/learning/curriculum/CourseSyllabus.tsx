import { useRef, useState } from 'react';

import { ArrowRight, Check, ChevronDown, Clock3, Layers3 } from 'lucide-react';

import { OutlineCitation } from '@/features/learning/curriculum/OutlineEvidence';
import { findOutlineCitation } from '@/features/learning/model/evidencePresentation';
import type {
  LearningModuleDto,
  LearningOutlineEvidenceDto,
  LearningProgramDto,
} from '@/lib/bindings';

export function CourseSyllabus({
  program,
  moduleId,
  onSelect,
  evidence,
  compact = false,
}: {
  compact?: boolean;
  program: LearningProgramDto;
  moduleId: string;
  evidence?: LearningOutlineEvidenceDto | null;
  onSelect: (module: LearningModuleDto, lessonId: string) => void;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(program.summary.status === 'draft');
  const [activeModuleId, setActiveModuleId] = useState(moduleId || program.modules[0]?.id || '');
  const lessons = program.modules.flatMap((module) => module.lessons);
  const minutes = lessons.reduce((total, lesson) => total + lesson.estimatedMinutes, 0);
  const completed = lessons.filter((lesson) => lesson.completed).length;

  const activeModule =
    program.modules.find((m) => m.id === (activeModuleId || moduleId || program.modules[0]?.id)) ??
    program.modules[0];
  const activeModuleIndex = program.modules.findIndex((m) => m.id === activeModule?.id);

  const renderModuleDetail = (module: LearningModuleDto, index: number) => {
    const summaryCitation = findOutlineCitation(evidence, 'module_summary', module.id, module.summary);
    return (
      <div key={module.id} className="min-w-0 space-y-4">
        <div>
          <span className="text-xxs font-semibold uppercase tracking-wider text-accent">
            Module {index + 1}
            {index === program.modules.length - 1 && ' · Final module'}
          </span>
          <h3 className="mt-1 font-serif text-xl leading-snug text-text-primary">{module.title}</h3>
          <p className="mt-1.5 text-xs leading-5 text-text-secondary">{module.summary}</p>
          <OutlineCitation citation={summaryCitation} evidence={evidence} />
        </div>

        {Boolean(module.prerequisiteModuleIds?.length) && (
          <p className="text-xs text-text-muted">
            Builds on{' '}
            {module.prerequisiteModuleIds
              ?.map((id) => program.modules.find((candidate) => candidate.id === id)?.title)
              .filter(Boolean)
              .join(', ')}
          </p>
        )}

        {module.project && (
          <details className="rounded-lg border border-accent/20 bg-surface-sunken/40 p-3">
            <summary className="cursor-pointer text-xs font-semibold text-accent">
              {index === program.modules.length - 1 ? 'Capstone' : 'Project milestone'} · {module.project.title}
            </summary>
            <p className="mt-2 text-xs leading-5 text-text-secondary">{module.project.brief}</p>
            <h4 className="mt-2.5 text-xs font-semibold text-text-primary">What you’ll produce</h4>
            <ul className="mt-1 list-disc space-y-0.5 pl-4 text-xs leading-5 text-text-secondary">
              {module.project.deliverables.map((deliverable) => (
                <li key={deliverable}>{deliverable}</li>
              ))}
            </ul>
            <h4 className="mt-2.5 text-xs font-semibold text-text-primary">What good work looks like</h4>
            <ul className="mt-1 list-disc space-y-0.5 pl-4 text-xs leading-5 text-text-secondary">
              {module.project.successCriteria.map((criterion) => (
                <li key={criterion}>{criterion}</li>
              ))}
            </ul>
          </details>
        )}

        <div>
          <h4 className="text-xxs font-semibold uppercase tracking-wider text-text-muted">Outcomes</h4>
          <ul className="mt-2 space-y-2">
            {module.outcomes.map((outcome, outcomeIndex) => (
              <li key={`${outcomeIndex}-${outcome}`} className="text-xs leading-5 text-text-secondary">
                <div className="flex items-start gap-2">
                  <Check size={13} className="mt-1 shrink-0 text-accent" />
                  <span>{outcome}</span>
                </div>
                <OutlineCitation
                  citation={findOutlineCitation(evidence, 'outcome', module.id, outcome, outcomeIndex)}
                  evidence={evidence}
                  className="ml-5"
                />
              </li>
            ))}
          </ul>
        </div>

        <div>
          <h4 className="text-xxs font-semibold uppercase tracking-wider text-text-muted">Lessons</h4>
          <ol className="mt-2 divide-y divide-border-subtle rounded-lg border border-border-subtle bg-surface-sunken/30">
            {module.lessons.map((lesson, lessonIndex) => (
              <li key={lesson.id} className="p-2.5">
                <button
                  type="button"
                  onClick={() => {
                    onSelect(module, lesson.id);
                    if (compact) setOpen(false);
                  }}
                  className="group flex min-h-8 w-full items-center gap-2 text-left text-xs hover:text-accent transition-colors"
                >
                  <span className="w-7 shrink-0 text-xxs tabular-nums text-text-muted">
                    {lesson.completed ? (
                      <Check size={13} className="text-success" />
                    ) : (
                      `${index + 1}.${lessonIndex + 1}`
                    )}
                  </span>
                  <span className="min-w-0 flex-1 font-medium text-text-primary group-hover:text-accent transition-colors">
                    {lesson.title}
                  </span>
                  <span className="inline-flex shrink-0 items-center gap-1 text-xxs text-text-muted">
                    <Clock3 size={11} />
                    {lesson.estimatedMinutes}m
                  </span>
                  <ArrowRight
                    size={13}
                    className="shrink-0 text-accent opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100 transition-opacity"
                  />
                </button>
                <p className="ml-9 text-xxs leading-relaxed text-text-muted">{lesson.objective}</p>
                <OutlineCitation
                  citation={findOutlineCitation(
                    evidence,
                    'lesson_objective',
                    module.id,
                    lesson.objective,
                    lessonIndex,
                    lesson.id,
                  )}
                  evidence={evidence}
                  className="ml-9"
                />
              </li>
            ))}
          </ol>
        </div>
      </div>
    );
  };

  return (
    <section
      className={compact ? 'relative min-w-0' : 'mt-4 overflow-hidden rounded-xl border border-border-default bg-surface shadow-xs'}
      aria-label="Course syllabus"
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          setOpen(false);
          trigger.current?.focus();
        }
      }}
    >
      {compact ? (
        <button
          ref={trigger}
          type="button"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
          aria-controls="course-syllabus-content"
          className="inline-flex min-h-8 items-center gap-1.5 rounded-lg border border-border-default bg-surface px-2.5 py-1 text-xs font-medium text-text-secondary hover:bg-surface-raised hover:text-text-primary shadow-2xs transition-colors"
        >
          <Layers3 size={14} className="shrink-0 text-accent" />
          <span>Syllabus</span>
          <ChevronDown
            size={13}
            className={`shrink-0 text-text-muted transition-transform ${open ? 'rotate-180' : ''}`}
          />
        </button>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle bg-surface-sunken/40 px-5 py-3">
          <div className="flex items-center gap-2.5">
            <Layers3 size={18} className="shrink-0 text-accent" />
            <div>
              <h2 className="text-sm font-semibold text-text-primary">Course syllabus</h2>
              <p className="text-xs text-text-muted">
                {program.modules.length} {program.modules.length === 1 ? 'module' : 'modules'} · {lessons.length}{' '}
                {lessons.length === 1 ? 'lesson' : 'lessons'} · about{' '}
                {minutes < 60 ? `${minutes} minutes` : `${Math.round(minutes / 60)} hours`}
                {program.summary.status === 'active' && ` · ${completed} lessons complete`}
              </p>
            </div>
          </div>
        </div>
      )}

      <div
        id="course-syllabus-content"
        hidden={!open}
        className={
          compact
            ? 'absolute right-0 top-full z-30 mt-2 max-h-[75vh] w-[min(960px,calc(100vw-3rem))] overflow-y-auto rounded-xl border border-border-default bg-surface p-5 shadow-xl'
            : 'p-0'
        }
      >
        <div className={compact ? 'mb-4' : 'border-b border-border-subtle px-5 py-3'}>
          <p className="max-w-3xl text-xs leading-5 text-text-secondary">
            {program.summary.status === 'draft'
              ? 'Review the whole progression before you begin. Open any lesson below to inspect its objective. Your pace is flexible, and you can revise upcoming work in the Plan.'
              : 'Follow the sequence or revisit a topic. Opening a lesson never marks it complete.'}
          </p>
          {evidence?.unavailableCount ? (
            <p className="mt-2 rounded-lg border border-warning-muted bg-warning-muted/20 p-2.5 text-xs leading-5 text-warning-fg">
              {evidence.unavailableCount} saved outline {evidence.unavailableCount === 1 ? 'citation is' : 'citations are'}{' '}
              hidden because the visible plan changed or the exact saved passage is unavailable.
            </p>
          ) : null}
        </div>

        {program.modules.length > 1 ? (
          <div className="flex flex-col md:flex-row">
            {/* Module Rail */}
            <div className="w-full md:w-72 shrink-0 border-b md:border-b-0 md:border-r border-border-subtle bg-surface-sunken p-3 space-y-1.5">
              <span className="px-2 text-xxs font-semibold uppercase tracking-wider text-text-muted">
                Modules
              </span>
              {program.modules.map((module, index) => {
                const isSelected = activeModule?.id === module.id;
                const done = module.lessons.filter((l) => l.completed).length;
                return (
                  <button
                    key={module.id}
                    type="button"
                    onClick={() => setActiveModuleId(module.id)}
                    className={`w-full rounded-lg p-2.5 text-left transition-colors ${
                      isSelected
                        ? 'border border-border-default bg-surface text-text-primary shadow-2xs font-medium'
                        : 'border border-transparent text-text-secondary hover:bg-surface/60 hover:text-text-primary'
                    }`}
                  >
                    <div className="flex items-center justify-between text-xxs font-semibold uppercase tracking-wider text-accent">
                      <span>Module {index + 1}</span>
                      <span className="text-text-muted font-normal tabular-nums">
                        {done}/{module.lessons.length}
                      </span>
                    </div>
                    <div className="mt-1 font-serif text-sm leading-snug line-clamp-2 text-text-primary">
                      {module.title}
                    </div>
                  </button>
                );
              })}
            </div>

            {/* Active Module Detail Panel */}
            <div className="flex-1 bg-surface p-5">
              {activeModule && renderModuleDetail(activeModule, activeModuleIndex >= 0 ? activeModuleIndex : 0)}
            </div>
          </div>
        ) : (
          <div className="bg-surface p-5">
            {program.modules[0] && renderModuleDetail(program.modules[0], 0)}
          </div>
        )}
      </div>
    </section>
  );
}
