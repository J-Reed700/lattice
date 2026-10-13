import { MessageCircle } from 'lucide-react';

/** What the chat shows with no conversation open: a way to start one. */
export function NoConversationOpen({
  isCreatingConversation,
  onCreate,
}: {
  isCreatingConversation: boolean;
  onCreate: () => Promise<void>;
}) {
  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 items-center justify-center bg-bg">
      <div className="max-w-md px-8 text-center">
        <div className="mx-auto mb-4 flex h-11 w-11 items-center justify-center rounded-xl bg-[hsl(var(--text-primary)/0.05)] text-text-tertiary shadow-[inset_0_0_0_1px_hsl(var(--text-primary)/0.05)]">
          <MessageCircle className="h-5 w-5" strokeWidth={1.6} />
        </div>
        <p className="font-serif text-[19px] font-medium tracking-[-0.015em] text-text-primary">No conversation open.</p>
        <p className="mt-1.5 text-ui leading-relaxed text-text-muted">Pick one from the list, or start a new one and ask your library a question.</p>
        <button
          type="button"
          onClick={() => { void onCreate(); }}
          disabled={isCreatingConversation}
          aria-label="Create new conversation"
          className="pressable mt-5 inline-flex h-8 items-center justify-center gap-2 rounded-md bg-action px-3 text-ui font-medium text-action-fg shadow-action transition-[background-color,scale] duration-fast hover:bg-action-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          New conversation
          <kbd className="kbd bg-[hsl(var(--action-fg)/0.14)] text-[hsl(var(--action-fg)/0.8)] shadow-none">⌘N</kbd>
        </button>
      </div>
    </div>
  );
}
