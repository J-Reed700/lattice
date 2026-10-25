import { useCallback, useEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';

import { ExternalLink, GitBranch, Search, X } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { CourseTangentChat } from '@/features/learning/tangents/CourseTangentChat';
import {
  StudyTangentsContext,
  type StartStudyTangentParams,
  type StudyTangentsContextValue,
  useStudyTangentsContext,
} from '@/features/learning/tangents/StudyTangentSelection';
import {
  useCreateLearningTangent,
  useLearningTangents,
} from '@/features/learning/tangents/useLearningTangents';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import type { LearningProgramDto, LearningTangentDto } from '@/lib/bindings';
import { useOptionalConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

export interface ProgramTangentsProps {
  program: LearningProgramDto;
  currentModuleId?: string | null;
  currentLessonId?: string | null;
  initialTangentId?: string | null;
  onSelectLesson?: (moduleId: string, lessonId: string) => void;
  children: ReactNode;
}

export function ProgramTangents({
  program,
  currentModuleId,
  currentLessonId,
  initialTangentId,
  onSelectLesson,
  children,
}: ProgramTangentsProps) {
  const [panelOpen, setPanelOpen] = useState(Boolean(initialTangentId));
  const [activeTangentId, setActiveTangentId] = useState<string | null>(initialTangentId ?? null);
  const [scopeFilter, setScopeFilter] = useState<'lesson' | 'all'>('lesson');
  const [searchQuery, setSearchQuery] = useState('');

  const tangentsQuery = useLearningTangents(program.summary.id);
  const createMutation = useCreateLearningTangent();
  const tangents = useMemo(() => tangentsQuery.data ?? [], [tangentsQuery.data]);

  useEffect(() => {
    if (initialTangentId) {
      setActiveTangentId(initialTangentId);
      setPanelOpen(true);
    }
  }, [initialTangentId]);

  const activeTangent = useMemo(
    () => tangents.find((t) => t.conversationId === activeTangentId) ?? null,
    [tangents, activeTangentId],
  );

  const startTangent = useCallback(
    async (params: StartStudyTangentParams) => {
      try {
        const created = await createMutation.mutateAsync({
          programId: program.summary.id,
          moduleId: params.moduleId,
          lessonId: params.lessonId,
          sectionIndex: params.sectionIndex ?? null,
          sectionTitle: params.sectionTitle ?? null,
          selectedText: params.selectedText,
        });
        setActiveTangentId(created.conversationId);
        setPanelOpen(true);
      } catch (error) {
        toast.error("Couldn't start the tangent", {
          message: error instanceof Error ? error.message : String(error),
        });
      }
    },
    [createMutation, program.summary.id],
  );

  const openTangent = useCallback((tangentId: string) => {
    setActiveTangentId(tangentId);
    setPanelOpen(true);
  }, []);

  const openTangentsDrawer = useCallback((filterLessonId?: string | null) => {
    if (filterLessonId) {
      setScopeFilter('lesson');
    }
    setActiveTangentId(null);
    setPanelOpen(true);
  }, []);

  const closeTangentsDrawer = useCallback(() => {
    setPanelOpen(false);
  }, []);

  const contextValue: StudyTangentsContextValue = useMemo(
    () => ({
      programId: program.summary.id,
      tangents,
      activeTangentId,
      isCreating: createMutation.isPending,
      isPanelOpen: panelOpen,
      startTangent,
      openTangent,
      openTangentsDrawer,
      closeTangentsDrawer,
    }),
    [
      activeTangentId,
      closeTangentsDrawer,
      createMutation.isPending,
      openTangent,
      openTangentsDrawer,
      panelOpen,
      program.summary.id,
      startTangent,
      tangents,
    ],
  );

  const lessonTangents = useMemo(
    () => tangents.filter((t) => t.lessonId === currentLessonId),
    [currentLessonId, tangents],
  );

  const filteredTangents = useMemo(() => {
    const base = scopeFilter === 'lesson' && currentLessonId ? lessonTangents : tangents;
    const query = searchQuery.trim().toLowerCase();
    if (!query) return base;
    return base.filter(
      (t) =>
        t.title.toLowerCase().includes(query) ||
        t.selectedText.toLowerCase().includes(query) ||
        (t.sectionTitle ?? '').toLowerCase().includes(query) ||
        t.lessonTitle.toLowerCase().includes(query) ||
        t.moduleTitle.toLowerCase().includes(query),
    );
  }, [currentLessonId, lessonTangents, scopeFilter, searchQuery, tangents]);

  // Group tangents hierarchically by Module -> Lesson
  const groupedModules = useMemo(() => {
    const moduleMap = new Map<
      string,
      {
        moduleId: string;
        moduleTitle: string;
        lessons: Map<string, { lessonId: string; lessonTitle: string; tangents: LearningTangentDto[] }>;
      }
    >();

    // Seed order from program curriculum
    for (const mod of program.modules) {
      moduleMap.set(mod.id, {
        moduleId: mod.id,
        moduleTitle: mod.title,
        lessons: new Map(),
      });
      for (const les of mod.lessons) {
        moduleMap.get(mod.id)!.lessons.set(les.id, {
          lessonId: les.id,
          lessonTitle: les.title,
          tangents: [],
        });
      }
    }

    // Populate with matching tangents
    for (const t of filteredTangents) {
      let mod = moduleMap.get(t.moduleId);
      if (!mod) {
        mod = { moduleId: t.moduleId, moduleTitle: t.moduleTitle, lessons: new Map() };
        moduleMap.set(t.moduleId, mod);
      }
      let les = mod.lessons.get(t.lessonId);
      if (!les) {
        les = { lessonId: t.lessonId, lessonTitle: t.lessonTitle, tangents: [] };
        mod.lessons.set(t.lessonId, les);
      }
      les.tangents.push(t);
    }

    // Filter down to modules with at least one tangent
    return Array.from(moduleMap.values())
      .map((mod) => ({
        ...mod,
        lessons: Array.from(mod.lessons.values()).filter((l) => l.tangents.length > 0),
      }))
      .filter((mod) => mod.lessons.length > 0);
  }, [filteredTangents, program.modules]);

  return (
    <StudyTangentsContext.Provider value={contextValue}>
      <div className="conversation-with-tangents" data-panel-open={panelOpen}>
        <div className="tangent-main">{children}</div>

        {panelOpen && (
          <aside
            id={`course-tangents-${program.summary.id}`}
            aria-label="Course tangents"
            className="tangent-sidebar tangent-panel-slot"
            onKeyDown={(event) => {
              if (event.key === 'Escape' && !event.defaultPrevented) {
                event.stopPropagation();
                closeTangentsDrawer();
              }
            }}
          >
            {activeTangent ? (
              <CourseTangentChat
                key={activeTangent.conversationId}
                tangent={activeTangent}
                currentModuleId={currentModuleId}
                currentLessonId={currentLessonId}
                onBackToList={() => setActiveTangentId(null)}
                onClose={closeTangentsDrawer}
                onJumpToLesson={(modId, lesId) => {
                  onSelectLesson?.(modId, lesId);
                }}
              />
            ) : (
              <div className="flex h-full min-h-0 flex-1 flex-col bg-surface">
                <header className="flex h-12 shrink-0 items-center justify-between gap-2 border-b border-border-subtle px-3.5">
                  <div className="flex items-center gap-2">
                    <GitBranch className="h-4 w-4 text-accent" aria-hidden="true" />
                    <h2 className="font-serif text-sm font-medium text-text-primary">
                      Course Tangents
                    </h2>
                    {tangents.length > 0 && (
                      <span className="rounded-full bg-accent/10 px-2 py-0.5 text-xs font-semibold text-accent">
                        {tangents.length}
                      </span>
                    )}
                  </div>
                  <IconButton label="Close tangents" onClick={closeTangentsDrawer}>
                    <X className="h-4 w-4" />
                  </IconButton>
                </header>

                <div className="shrink-0 border-b border-border-subtle p-3 space-y-2.5">
                  <div className="flex items-center gap-1 rounded-lg bg-background p-0.5 text-xs">
                    <button
                      type="button"
                      onClick={() => setScopeFilter('lesson')}
                      className={`flex-1 rounded-md py-1.5 font-medium transition ${
                        scopeFilter === 'lesson'
                          ? 'bg-surface text-accent shadow-xs'
                          : 'text-text-muted hover:text-text-primary'
                      }`}
                    >
                      This Lesson ({lessonTangents.length})
                    </button>
                    <button
                      type="button"
                      onClick={() => setScopeFilter('all')}
                      className={`flex-1 rounded-md py-1.5 font-medium transition ${
                        scopeFilter === 'all'
                          ? 'bg-surface text-accent shadow-xs'
                          : 'text-text-muted hover:text-text-primary'
                      }`}
                    >
                      All in Course ({tangents.length})
                    </button>
                  </div>

                  {tangents.length > 3 && (
                    <div className="relative">
                      <Search
                        className="pointer-events-none absolute left-2.5 top-2.5 h-3.5 w-3.5 text-text-muted"
                        aria-hidden="true"
                      />
                      <input
                        type="text"
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        placeholder="Search tangents…"
                        aria-label="Search tangents"
                        className="w-full rounded-md border border-border bg-background py-1.5 pl-8 pr-3 text-xs outline-hidden focus:border-accent"
                      />
                    </div>
                  )}
                </div>

                <div className="min-h-0 flex-1 overflow-y-auto p-3">
                  {tangentsQuery.isPending && (
                    <p role="status" className="p-4 text-center text-sm text-text-muted">
                      Loading tangents…
                    </p>
                  )}

                  {tangentsQuery.isError && (
                    <div role="alert" className="p-4 text-sm text-text-secondary">
                      <p>Couldn’t load course tangents.</p>
                      <button
                        type="button"
                        onClick={() => void tangentsQuery.refetch()}
                        className="mt-2 text-accent underline"
                      >
                        Try again
                      </button>
                    </div>
                  )}

                  {tangentsQuery.isSuccess && filteredTangents.length === 0 && (
                    <div className="space-y-3 p-4 text-center text-xs leading-relaxed text-text-muted">
                      <p>
                        Highlight any passage in the study guide or click{' '}
                        <strong className="font-semibold text-text-secondary">
                          Start tangent
                        </strong>{' '}
                        at the end of any section to branch a side conversation.
                      </p>
                    </div>
                  )}

                  <div className="space-y-4">
                    {groupedModules.map((mod) => (
                      <div key={mod.moduleId} className="space-y-2">
                        <h3 className="px-1 text-[11px] font-semibold uppercase tracking-wider text-accent">
                          {mod.moduleTitle}
                        </h3>

                        {mod.lessons.map((les) => (
                          <div key={les.lessonId} className="space-y-1.5">
                            <h4 className="px-1 text-xs font-medium text-text-muted">
                              {les.lessonTitle}
                            </h4>

                            <div className="space-y-2">
                              {les.tangents.map((t) => (
                                <CourseTangentCard
                                  key={t.conversationId}
                                  tangent={t}
                                  onOpen={() => openTangent(t.conversationId)}
                                />
                              ))}
                            </div>
                          </div>
                        ))}
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </aside>
        )}
      </div>
    </StudyTangentsContext.Provider>
  );
}

function CourseTangentCard({
  tangent,
  onOpen,
}: {
  tangent: LearningTangentDto;
  onOpen: () => void;
}) {
  const navigate = useAppNavigate();
  const store = useOptionalConversationsStore();
  const replies = Math.max(0, tangent.messageCount - tangent.contextMessageCount);
  const sectionLabel =
    tangent.sectionTitle ??
    (tangent.sectionIndex != null ? `Section ${tangent.sectionIndex + 1}` : 'Lesson overview');

  const handleOpenInChat = (event: React.MouseEvent) => {
    event.stopPropagation();
    store?.setSelectedSpace(tangent.spaceId);
    void store?.selectConversation(tangent.conversationId);
    navigate(`/chat?conversationId=${encodeURIComponent(tangent.conversationId)}`);
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onOpen();
        }
      }}
      className="group relative flex flex-col rounded-xl border border-border bg-surface p-3 text-left transition hover:border-accent/50 hover:shadow-xs focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="rounded-full bg-accent/10 px-2 py-0.5 text-[10px] font-medium text-accent">
          {sectionLabel}
        </span>
        <button
          type="button"
          title="Open in Chat"
          onClick={handleOpenInChat}
          className="rounded p-1 text-text-muted opacity-0 transition group-hover:opacity-100 hover:text-accent focus-visible:opacity-100 focus-visible:outline-hidden"
        >
          <ExternalLink className="h-3.5 w-3.5" />
        </button>
      </div>

      <h5 className="mt-1.5 line-clamp-1 font-serif text-sm font-medium text-text-primary">
        {tangent.title}
      </h5>

      <p className="mt-1 line-clamp-2 font-serif text-xs leading-relaxed text-text-muted">
        {tangent.selectedText}
      </p>

      <div className="mt-2.5 flex items-center justify-between text-[11px] text-text-muted">
        <span>
          {replies} {replies === 1 ? 'reply' : 'replies'}
        </span>
        <span className="text-accent group-hover:underline">Open tangent →</span>
      </div>
    </div>
  );
}

export function ProgramTangentsTrigger() {
  const { tangents, openTangentsDrawer, closeTangentsDrawer, isPanelOpen } =
    useStudyTangentsContext();

  const handleClick = () => {
    if (isPanelOpen) {
      closeTangentsDrawer?.();
    } else {
      openTangentsDrawer();
    }
  };

  return (
    <button
      type="button"
      aria-expanded={isPanelOpen}
      aria-label="Course tangents"
      title="Explore side topics in tangents"
      onClick={handleClick}
      className={`inline-flex min-h-9 items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-accent ${
        isPanelOpen
          ? 'border-accent bg-accent/10 text-accent'
          : tangents.length > 0
            ? 'border-accent/40 bg-surface text-accent hover:bg-accent/5'
            : 'border-border text-text-secondary hover:border-accent/40 hover:text-text-primary'
      }`}
    >
      <GitBranch className="h-3.5 w-3.5 text-accent" aria-hidden="true" />
      <span>Tangents</span>
      {tangents.length > 0 && (
        <span className="rounded-full bg-accent/15 px-1.5 py-0.2 text-[10px] font-semibold text-accent">
          {tangents.length}
        </span>
      )}
    </button>
  );
}
