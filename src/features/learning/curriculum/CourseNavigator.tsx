import type { ReactNode } from 'react';

import { Check, Circle } from 'lucide-react';

import { SearchableSelect } from '@/components/ui/SearchableSelect';
import type { LearningModuleDto } from '@/lib/bindings';

/** Course-specific labels and progress belong here; the pickers are shared UI. */
export function CourseNavigator({ modules, moduleId, lessonId, disabled, onModuleChange, onLessonChange, children }: {
  modules: LearningModuleDto[];
  moduleId: string;
  lessonId: string;
  disabled: boolean;
  onModuleChange: (module: LearningModuleDto) => void;
  onLessonChange: (id: string) => void;
  children?: ReactNode;
}) {
  const current = modules.find((module) => module.id === moduleId);
  const moduleOptions = modules.map((module, index) => ({
    value: module.id,
    label: `Module ${index + 1}: ${module.title}`,
    description: `${module.lessons.filter((lesson) => lesson.completed).length} of ${module.lessons.length} lessons complete`,
    keywords: module.lessons.map((lesson) => lesson.title),
    indicator: module.lessons.length > 0 && module.lessons.every((lesson) => lesson.completed)
      ? <Check size={14} aria-hidden="true" />
      : <span className="font-mono text-[10px]" aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>,
  }));
  const lessonOptions = [
    { value: '', label: 'No lesson selected', description: 'Work without a lesson context' },
    ...(current?.lessons.map((lesson, index) => ({
      value: lesson.id,
      label: `${index + 1}. ${lesson.title}`,
      description: lesson.completed ? 'Marked complete' : lesson.preparation === 'ready' ? `Ready to study · ${lesson.estimatedMinutes} min` : 'Outline · prepare when ready',
      keywords: [lesson.objective],
      indicator: lesson.completed ? <Check size={14} aria-hidden="true" /> : <Circle size={12} aria-hidden="true" />,
    })) ?? []),
  ];
  return <div className="studio-course-pickers">
    <div className="studio-course-picker">
      <label htmlFor="module-select" className="shrink-0">Module</label>
      <SearchableSelect id="module-select" label="Module" value={moduleId} disabled={disabled || !modules.length} searchPlaceholder="Find a module…" emptyMessage="No modules match your search." options={moduleOptions} onValueChange={(id) => { const next = modules.find((module) => module.id === id); if (next) onModuleChange(next); }} />
    </div>
    <div className="studio-course-picker">
      <label htmlFor="lesson-select" className="shrink-0">Lesson</label>
      <SearchableSelect id="lesson-select" label="Lesson context" value={lessonId} disabled={disabled || !current?.lessons.length} searchPlaceholder="Find a lesson…" emptyMessage="No lessons match your search." options={lessonOptions} onValueChange={onLessonChange} />
    </div>
    {children}
  </div>;
}
