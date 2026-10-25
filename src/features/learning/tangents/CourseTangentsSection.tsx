import { useMemo } from 'react';

import { BookOpen, ExternalLink, GitBranch } from 'lucide-react';

import { useLearningTangents } from '@/features/learning/tangents/useLearningTangents';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import type { LearningTangentDto } from '@/lib/bindings';
import type { StudyDestination } from '@/lib/studyActivity';
import { useOptionalConversationsStore } from '@/shared/conversations/conversationsStore';

export interface CourseTangentsSectionProps {
  onOpenDestination: (destination: StudyDestination) => void;
}

export function CourseTangentsSection({ onOpenDestination }: CourseTangentsSectionProps) {
  const navigate = useAppNavigate();
  const store = useOptionalConversationsStore();
  const tangentsQuery = useLearningTangents();
  const tangents = useMemo(() => tangentsQuery.data ?? [], [tangentsQuery.data]);

  const courseGroups = useMemo(() => {
    const map = new Map<
      string,
      {
        programId: string;
        programTitle: string;
        spaceId: string;
        tangents: LearningTangentDto[];
      }
    >();

    for (const t of tangents) {
      let group = map.get(t.programId);
      if (!group) {
        group = {
          programId: t.programId,
          programTitle: t.programTitle,
          spaceId: t.spaceId,
          tangents: [],
        };
        map.set(t.programId, group);
      }
      group.tangents.push(t);
    }

    return Array.from(map.values());
  }, [tangents]);

  if (tangents.length === 0) return null;

  const handleOpenSpaceInChat = (spaceId: string) => {
    store?.setSelectedSpace(spaceId);
    navigate('/chat');
  };

  const handleOpenTangentInChat = (spaceId: string, conversationId: string) => {
    store?.setSelectedSpace(spaceId);
    void store?.selectConversation(conversationId);
    navigate(`/chat?conversationId=${encodeURIComponent(conversationId)}`);
  };

  return (
    <section aria-label="Course tangents" className="mt-8 space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">
            Side conversations & deep dives
          </div>
          <h2 className="mt-1 font-serif text-2xl text-text-primary">Course Tangents</h2>
        </div>
        <div className="text-xs text-text-muted">
          {tangents.length} {tangents.length === 1 ? 'tangent' : 'tangents'} across your courses
        </div>
      </div>

      <div className="space-y-6">
        {courseGroups.map((group) => (
          <div
            key={group.programId}
            className="rounded-2xl border border-border bg-surface p-5 shadow-xs"
          >
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 pb-3.5">
              <div className="flex items-center gap-2">
                <GitBranch className="h-4 w-4 text-accent" aria-hidden="true" />
                <h3 className="font-serif text-lg font-medium text-text-primary">
                  {group.programTitle}
                </h3>
                <span className="rounded-full bg-accent/10 px-2 py-0.5 text-xs font-semibold text-accent">
                  {group.tangents.length}
                </span>
              </div>
              <button
                type="button"
                onClick={() => handleOpenSpaceInChat(group.spaceId)}
                className="inline-flex items-center gap-1.5 text-xs font-medium text-accent hover:underline"
              >
                <ExternalLink className="h-3.5 w-3.5" />
                Open Course Space in Chat
              </button>
            </div>

            <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
              {group.tangents.map((t) => {
                const replies = Math.max(0, t.messageCount - t.contextMessageCount);
                const breadcrumb = `${t.moduleTitle} › ${t.lessonTitle}${
                  t.sectionTitle ? ` › ${t.sectionTitle}` : ''
                }`;

                return (
                  <div
                    key={t.conversationId}
                    className="flex flex-col justify-between rounded-xl border border-border bg-background p-4 text-left transition hover:border-accent/40"
                  >
                    <div>
                      <div className="truncate text-[10px] font-medium text-text-muted">
                        {breadcrumb}
                      </div>
                      <h4 className="mt-1 line-clamp-1 font-serif text-base font-medium text-text-primary">
                        {t.title}
                      </h4>
                      <p className="mt-1.5 line-clamp-2 font-serif text-xs leading-relaxed text-text-muted">
                        {t.selectedText}
                      </p>
                    </div>

                    <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border/50 pt-2.5 text-xs">
                      <span className="text-[11px] text-text-muted">
                        {replies} {replies === 1 ? 'reply' : 'replies'}
                      </span>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() =>
                            onOpenDestination({
                              programId: t.programId,
                              moduleId: t.moduleId,
                              lessonId: t.lessonId,
                              tab: 'study',
                              tangentId: t.conversationId,
                            })
                          }
                          className="inline-flex items-center gap-1 font-medium text-accent hover:underline"
                        >
                          <BookOpen className="h-3 w-3" />
                          Study Guide
                        </button>
                        <button
                          type="button"
                          onClick={() => handleOpenTangentInChat(t.spaceId, t.conversationId)}
                          className="inline-flex items-center gap-1 text-text-secondary hover:text-accent"
                        >
                          <ExternalLink className="h-3 w-3" />
                          Chat
                        </button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
