import type { LearningProgramDto } from '@/lib/bindings';
import type { StudyTab } from '@/lib/studyActivity';

/**
 * Where the learner was in Studio, so leaving and coming back resumes there:
 * the course left open, and for each course the module, lesson and section
 * last shown. This is a view position, not progress. The backend's
 * `currentLessonId` moves only when a lesson is completed, and it is where a
 * course opens when nothing was remembered or what was remembered is gone.
 */
const STORAGE_KEY = 'learning.studio.position';

export interface ProgramPosition {
  moduleId: string;
  /** Empty when the learner chose "No lesson selected". */
  lessonId: string;
  tab: StudyTab;
}

interface StudioPosition {
  openProgramId: string | null;
  programs: Record<string, ProgramPosition>;
}

// A draft outline only offers its lessons, sources and import & export.
const DRAFT_TABS: readonly StudyTab[] = ['lessons', 'sources', 'portability'];
const ACTIVE_TABS: readonly StudyTab[] = ['starting_point', 'lessons', 'sources', 'quick-checks', 'assess-evidence', 'plan', 'portability', 'workbench', 'practical', 'notebook', 'recall', 'canvas'];

function read(): StudioPosition {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
    if (parsed && typeof parsed === 'object') {
      const { openProgramId, programs } = parsed as Partial<StudioPosition>;
      return {
        openProgramId: typeof openProgramId === 'string' ? openProgramId : null,
        programs: programs && typeof programs === 'object' ? programs : {},
      };
    }
  } catch {
    // An unreadable position opens Studio as if nothing was remembered.
  }
  return { openProgramId: null, programs: {} };
}

function write(update: (current: StudioPosition) => StudioPosition): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(update(read())));
  } catch {
    // Studio still works when position storage is unavailable.
  }
}

export function rememberedOpenProgram(): string | null {
  return read().openProgramId;
}

export function rememberOpenProgram(programId: string | null): void {
  write((current) => ({ ...current, openProgramId: programId }));
}

export function rememberProgramPosition(programId: string, position: ProgramPosition): void {
  write((current) => ({ ...current, programs: { ...current.programs, [programId]: position } }));
}

/** The remembered position when it still fits the course, else its current lesson. */
export function resumePosition(program: LearningProgramDto): ProgramPosition {
  const saved = read().programs[program.summary.id];
  const savedModule = saved && program.modules.find((module) => module.id === saved.moduleId);
  if (saved && savedModule) {
    const lessonKnown = saved.lessonId === '' || savedModule.lessons.some((lesson) => lesson.id === saved.lessonId);
    const tabs = program.summary.status === 'active' ? ACTIVE_TABS : DRAFT_TABS;
    return {
      moduleId: savedModule.id,
      lessonId: lessonKnown ? saved.lessonId : savedModule.lessons[0]?.id ?? '',
      tab: tabs.includes(saved.tab) ? saved.tab : 'lessons',
    };
  }
  const current = program.summary.currentLessonId;
  return {
    moduleId: program.modules.find((module) => module.lessons.some((lesson) => lesson.id === current))?.id ?? program.modules[0]?.id ?? '',
    lessonId: current ?? program.modules[0]?.lessons[0]?.id ?? '',
    tab: 'lessons',
  };
}
