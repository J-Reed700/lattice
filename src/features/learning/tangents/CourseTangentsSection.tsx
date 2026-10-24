import { useMemo, useState } from 'react';

import { BookOpen, ChevronDown, ChevronUp, ExternalLink, GitBranch } from 'lucide-react';
import { useNavigate } from 'react-router';

import { useLearningTangents } from '@/features/learning/tangents/useLearningTangents';
import type { LearningTangentDto } from '@/lib/bindings';
import type { StudyDestination } from '@/lib/studyActivity';
import { useOptionalConversationsStore } from '@/shared/conversations/conversationsStore';

export interface CourseTangentsSectionProps {
  onOpenDestination: (destination: StudyDestination) => void;
}

export function CourseTangentsSection({ onOpenDestination }: CourseTangentsSectionProps) {
  const navigate = useNavigate();
  const store = useOptionalConversationsStore();
  const tangentsQuery = useLearningTangents();
  const tangents = useMemo(() => tangentsQuery.data ?? [], [tangentsQuery.data]);
  const [isExpanded, setIsExpanded] = useState(true);

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
    <section aria-label="Course tangents" className="mt-10 scroll-mt-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[.16em] text-text-muted">
            Side conversations & deep dives
          </div>
          <div className="mt-1 flex items-center gap-2.5">
            <h2 className="font-serif text-2xl text-text-primary">Course Tangents</h2>
            <span className="rounded-full bg-accent-muted px-2 py-0.5 text-xs font-semibold text-accent">
              {tangents.length}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-text-muted">
            {tangents.length} {tangents.length === 1 ? 'tangent' : 'tangents'} across your courses
          </span>
          <button
            type="button"
            onClick={() => setIsExpanded((prev) => !prev)}
            aria-expanded={isExpanded}
            className="inline-flex items-center gap-1.5 rounded-xl border border-border-default bg-surface px-3 py-1.5 text-xs font-medium text-text-primary shadow-xs transition hover:bg-surface-raised"
          >
            {isExpanded ? (
              <>
                <ChevronUp className="h-3.5 w-3.5 text-text-muted" /> Collapse
              </>
            ) : (
              <>
                <ChevronDown className="h-3.5 w-3.5 text-text-muted" /> Expand
              </>
            )}
          </button>
        </div>
      </div>

      {isExpanded && (
        <div className="mt-5 space-y-4">
          {courseGroups.map((group) => (
            <div
              key={group.programId}
              className="overflow-hidden rounded-xl border border-border-default bg-surface shadow-xs"
            >
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border-subtle bg-surface-sunken/60 px-4 py-3">
                <div className="flex items-center gap-2">
                  <GitBranch className="h-4 w-4 text-accent" aria-hidden="true" />
                  <h3 className="font-serif text-base font-medium text-text-primary">
                    {group.programTitle}
                  </h3>
                  <span className="rounded-full bg-accent-muted px-2 py-0.5 text-xs font-semibold text-accent">
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

              <div className="grid gap-3 p-4 sm:grid-cols-2 xl:grid-cols-3">
                {group.tangents.map((t) => {
                  const replies = Math.max(0, t.messageCount - t.contextMessageCount);
                  const breadcrumb = `${t.moduleTitle} › ${t.lessonTitle}${
                    t.sectionTitle ? ` › ${t.sectionTitle}` : ''
                  }`;

                  return (
                    <div
                      key={t.conversationId}
                      className="flex flex-col justify-between rounded-xl border border-border-subtle bg-surface-sunken/40 p-4 text-left transition hover:border-border-strong hover:bg-surface-raised/40"
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

                      <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border-subtle pt-2.5 text-xs">
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
                            className="inline-flex items-center gap-1 text-text-secondary transition hover:text-accent"
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
      )}
    </section>
  );
}
