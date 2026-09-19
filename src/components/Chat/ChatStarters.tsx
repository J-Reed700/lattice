import { useChatStartersQuery } from '@/hooks/queries/useChatStartersQuery';

/**
 * Suggested prompts for the empty chat state.
 *
 * When the backend has no model to generate them, it returns an empty list and
 * this renders one plain line. It never fabricates a question — a made-up
 * "What do my 42 PDFs say about X?" is a promise about documents nobody read.
 *
 * Clicking fills the composer and focuses it rather than sending: a question
 * the reader has not read yet should not become a turn behind their back.
 *
 * `spaceId` is the space of the chat being shown, and the panel passes it down
 * rather than letting this component guess: the questions must come from the
 * documents this chat can actually read, not from the whole vault.
 */

interface ChatStartersProps {
  onPick: (_question: string) => void;
  spaceId?: string | null;
}

export function ChatStarters({ onPick, spaceId = null }: ChatStartersProps) {
  const { data, isLoading } = useChatStartersQuery(spaceId);

  // Nothing while loading: `ChatEmptyStateIngestDelta` already holds the space.
  if (isLoading || !data) return null;
  // The delta line already says "Nothing indexed yet."
  if (data.documentCount === 0) return null;

  const lead = `Ask something about your ${data.documentCount.toLocaleString()} documents.`;

  if (data.starters.length === 0) {
    return <p className="text-sm text-[hsl(var(--text-muted))]">{lead}</p>;
  }

  return (
    <div>
      <p className="text-sm text-[hsl(var(--text-muted))]">{lead}</p>
      <ul className="mt-4 border-t border-subtle">
        {data.starters.map((starter) => (
          <li key={starter.question} className="border-b border-subtle">
            <button
              type="button"
              onClick={() => onPick(starter.question)}
              className="w-full px-1 py-2.5 text-left font-serif text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
            >
              {starter.question}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
