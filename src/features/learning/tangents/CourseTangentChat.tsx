import { useCallback, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, ExternalLink, GitBranch, Trash2, X } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { ChatPanel } from '@/features/chat/components/ChatPanel';
import { TangentConversation } from '@/features/chat/components/tangents/TangentConversation';
import { TangentExpandButton } from '@/features/chat/components/tangents/TangentDock';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import { CitationVisibilityToggle } from '@/features/reading/components/CitationVisibilityToggle';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import type { LearningTangentDto } from '@/lib/bindings';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

const STARTER_PROMPTS = [
  'Explain this intuitively',
  'Give me a concrete example',
  'How does this connect to the module goal?',
];
const STARTERS_NOTE = 'Ask a question or explore a side-topic about this study guide passage.';

export interface CourseTangentChatProps {
  tangent: LearningTangentDto;
  currentModuleId?: string | null;
  currentLessonId?: string | null;
  onBackToList: () => void;
  onClose: () => void;
  onJumpToLesson?: (moduleId: string, lessonId: string) => void;
}

export function CourseTangentChat({
  tangent,
  currentModuleId,
  currentLessonId,
  onBackToList,
  onClose,
  onJumpToLesson,
}: CourseTangentChatProps) {
  const id = tangent.conversationId;
  const navigate = useAppNavigate();
  const base = useConversationsStore();
  const queryClient = useQueryClient();
  const [mutating, setMutating] = useState(false);
  const isSending = base.inFlightGenerations.has(id);

  const refreshTangents = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: learningTangentsKey(tangent.programId) });
  }, [queryClient, tangent.programId]);

  const handleOpenInChat = async () => {
    try {
      base.setSelectedSpace(tangent.spaceId);
      await base.selectConversation(tangent.conversationId);
      navigate(`/chat?conversationId=${encodeURIComponent(tangent.conversationId)}`);
    } catch (error) {
      toast.error("Couldn't open in chat", {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const handleDelete = async () => {
    if (isSending || mutating) return;
    if (!window.confirm('Delete this tangent and its messages?')) return;
    setMutating(true);
    try {
      const deleted = await base.deleteConversation(id);
      if (!deleted) throw new Error(conversationUiStore.getState().error ?? 'Please try again.');
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(tangent.programId),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(null),
      });
      void queryClient.invalidateQueries({
        queryKey: learningTangentsKey(),
      });
      toast.success('Tangent deleted');
      onBackToList();
    } catch (error) {
      toast.error("Couldn't delete the tangent", {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setMutating(false);
    }
  };

  const isDifferentLesson = Boolean(
    onJumpToLesson &&
      (tangent.moduleId !== currentModuleId || tangent.lessonId !== currentLessonId),
  );

  const breadcrumb = `${tangent.moduleTitle} › ${tangent.lessonTitle}${
    tangent.sectionTitle ? ` › ${tangent.sectionTitle}` : ''
  }`;

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col bg-surface">
      <header className="tangent-panel-header flex h-12 shrink-0 items-center gap-2 border-b border-border-subtle px-3">
        <IconButton label="All course tangents" onClick={onBackToList}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5 text-xs font-medium text-text-primary">
            <GitBranch className="h-3.5 w-3.5 text-accent shrink-0" aria-hidden="true" />
            <span className="truncate">
              Tangent · {tangent.sectionTitle ?? tangent.lessonTitle}
            </span>
          </div>
          <p className="truncate text-[11px] text-text-muted">
            {tangent.programTitle} · {tangent.moduleTitle}
          </p>
        </div>
        <CitationVisibilityToggle compact className="h-8 min-h-8 w-8 justify-center px-0" />
        <IconButton label="Open in full Chat" onClick={() => void handleOpenInChat()}>
          <ExternalLink className="h-4 w-4" />
        </IconButton>
        <TangentExpandButton />
        <IconButton label="Close tangents drawer" onClick={onClose}>
          <X className="h-4 w-4" />
        </IconButton>
      </header>

      <div className="tangent-starting-point shrink-0 border-b border-border-subtle bg-surface/50 p-3.5">
        <div className="flex items-center justify-between gap-2">
          <span className="truncate text-[11px] font-medium text-text-muted">
            {breadcrumb}
          </span>
          {isDifferentLesson && (
            <button
              type="button"
              onClick={() => onJumpToLesson?.(tangent.moduleId, tangent.lessonId)}
              className="shrink-0 text-xs text-accent hover:underline"
            >
              Jump to lesson
            </button>
          )}
        </div>
        <blockquote className="tangent-starting-point-quote mt-2 max-h-28 overflow-y-auto whitespace-pre-wrap wrap-break-word border-l-2 border-accent pl-3 font-serif text-xs leading-relaxed text-text-secondary">
          {tangent.selectedText}
        </blockquote>
        <div className="mt-2.5 flex items-center justify-between gap-2 border-t border-border-subtle/60 pt-2 text-xs">
          <button
            type="button"
            onClick={() => void handleOpenInChat()}
            className="inline-flex items-center gap-1 text-[11px] text-text-secondary hover:text-accent"
          >
            <ExternalLink className="h-3 w-3" />
            Open in Chat
          </button>
          <IconButton
            label="Delete tangent"
            disabled={isSending || mutating}
            onClick={() => void handleDelete()}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </IconButton>
        </div>
      </div>

      <TangentConversation
        conversationId={id}
        inheritedCount={Math.max(1, tangent.contextMessageCount)}
        onTurnEnd={refreshTangents}
      >
        <ChatPanel embedded autoFocus starters={STARTER_PROMPTS} startersNote={STARTERS_NOTE} />
      </TangentConversation>
    </div>
  );
}
