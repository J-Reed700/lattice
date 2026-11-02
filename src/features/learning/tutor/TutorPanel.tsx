import { useEffect, useRef, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { MessageSquarePlus, Trash2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { IconButton } from '@/components/ui/IconButton';
import { ChatPanel } from '@/features/chat/components/ChatPanel';
import { DeleteConversationDialog } from '@/features/chat/components/DeleteConversationDialog';
import { learningMemoryKey, learningRecallKey } from '@/features/learning/api/learningQueryKeys';
import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';

import { useTutorChats } from './useTutorChats';

/** Fills the composer rather than sending, as the library's suggested questions do. */
function TutorStarters({ subject, onPick }: { subject: string; onPick: (_text: string) => void }) {
  return (
    <ul className="border-t border-subtle">
      {[`Teach me ${subject}`, 'Quiz me on what is due', 'What should I work on next?'].map((starter) => (
        <li key={starter} className="border-b border-subtle">
          <button
            type="button"
            onClick={() => onPick(starter)}
            className="w-full px-1 py-2.5 text-left font-serif text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
          >
            {starter}
          </button>
        </li>
      ))}
    </ul>
  );
}

/**
 * The Tutor tab: the course's tutor chats beside the shared chat, which brings
 * everything a chat has here. A turn may rate cards and save notes, so when one
 * ends what Recall and the Notebook show is read again.
 */
export function TutorPanel({ program, lesson, onCourseChanged }: {
  program: LearningProgramDto;
  /** The lesson the workspace is on: a new chat can start from it. */
  lesson?: LearningLessonDto;
  onCourseChanged?: () => void;
}) {
  const programId = program.summary.id;
  const chats = useTutorChats(programId, program.summary.title);
  const queryClient = useQueryClient();
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; title: string } | null>(null);
  const sending = useConversationsStore((state) =>
    Boolean(chats.activeId && state.inFlightGenerations.has(chats.activeId)));
  const wasSending = useRef(false);

  useEffect(() => {
    if (wasSending.current && !sending) {
      void queryClient.invalidateQueries({ queryKey: learningMemoryKey(programId) });
      void queryClient.invalidateQueries({ queryKey: learningRecallKey(programId) });
      onCourseChanged?.();
    }
    wasSending.current = sending;
  }, [onCourseChanged, programId, queryClient, sending]);

  return (
    <div className="mt-4 grid h-[calc(100vh-14rem)] min-h-[28rem] grid-cols-[14rem_minmax(0,1fr)] gap-3">
      <aside aria-label="Tutor chats" className="flex min-h-0 flex-col rounded-xl border border-border bg-background">
        <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
          <h3 className="text-sm font-medium text-text-primary">Tutor chats</h3>
          <IconButton label="New tutor chat" disabled={chats.creating} onClick={() => void chats.create()}>
            <MessageSquarePlus />
          </IconButton>
        </div>
        <ul className="min-h-0 flex-1 overflow-y-auto p-1">
          {chats.chats.map((chat) => (
            <li key={chat.id} className="group flex items-center gap-1">
              <button
                type="button"
                aria-current={chat.id === chats.activeId ? 'true' : undefined}
                onClick={() => chats.select(chat.id)}
                className={cn(
                  'min-w-0 flex-1 truncate rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-surface',
                  chat.id === chats.activeId ? 'bg-surface text-text-primary' : 'text-text-secondary',
                )}
              >
                {chat.title}
              </button>
              <IconButton label={`Delete ${chat.title}`} onClick={() => setDeleteTarget({ id: chat.id, title: chat.title })}>
                <Trash2 />
              </IconButton>
            </li>
          ))}
        </ul>
      </aside>
      <div className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-xl border border-border">
        {chats.activeId ? (
          <ChatPanel renderEmptyState={(onPick) => <TutorStarters subject={lesson?.title ?? program.summary.title} onPick={onPick} />} />
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
            {chats.error
              ? <p role="alert" className="max-w-sm text-sm text-danger-fg">{chats.error}</p>
              : <p className="max-w-sm text-sm text-text-secondary">A tutor teaches from this course’s sources, cites them, and works your reviews into the conversation.</p>}
            <div className="flex flex-wrap justify-center gap-2">
              <Button type="button" variant="outline" disabled={chats.creating} onClick={() => void chats.create()}>Start a tutor chat</Button>
              {lesson && <Button type="button" variant="outline" disabled={chats.creating} onClick={() => void chats.create({ id: lesson.id, title: lesson.title })}>Start on {lesson.title}</Button>}
            </div>
          </div>
        )}
      </div>
      {deleteTarget && (
        <DeleteConversationDialog
          conversation={deleteTarget}
          onDelete={chats.remove}
          onClose={() => setDeleteTarget(null)}
        />
      )}
    </div>
  );
}
