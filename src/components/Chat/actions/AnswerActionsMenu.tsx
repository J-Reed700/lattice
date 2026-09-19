import { type ReactNode, useMemo, useState } from 'react';

import * as Popover from '@radix-ui/react-popover';
import { Columns3, Loader2, NotebookPen } from 'lucide-react';
import { useNavigate } from 'react-router';

import { VaultAPI } from '@/lib/api';
import { toast } from '@/stores/toastStore';
import type { MessageVerificationSummary, SourceWithMetadata } from '@/types/conversation';
import { answerToMarkdown, vaultDocumentIds } from '@/utils/conversationExport';

import { ACTION_HEADING_CLASS, ACTION_ROW_CLASS, ACTION_SURFACE_CLASS } from './actionSurface';

/**
 * Where a grounded answer goes next: into the journal, or onto the Compare
 * table beside the documents it came from.
 *
 * These are two different things to do with the same answer, so they share one
 * deliberate surface rather than being dropped into the action row as two more
 * anonymous verbs. Each row says what it will do, including when it cannot.
 */

export interface AnswerActionsMenuProps {
  /** The answer's cited passages, in citation order. */
  sources: readonly SourceWithMetadata[];
  /** The answer as markdown — what goes to the journal. */
  markdown: string;
  /** What the sentence check found, so the journal page carries the verdict. */
  verification?: MessageVerificationSummary | null;
  /** The thread this answer came from, named on the journal page. */
  conversationTitle?: string | null;
  /** `metadata.turn`, read defensively: it may not exist on this turn. */
  turn?: unknown;
  /** The trigger, rendered by the action row so it matches its siblings. */
  children: ReactNode;
  align?: 'start' | 'end';
}

export function AnswerActionsMenu({
  sources,
  markdown,
  verification = null,
  conversationTitle = null,
  turn,
  children,
  align = 'start',
}: AnswerActionsMenuProps) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Compare reads files, so web pages never count towards the two it needs.
  const documentIds = useMemo(() => vaultDocumentIds([...sources]), [sources]);
  const canCompare = documentIds.length >= 2;

  const handleAddToJournal = async () => {
    setIsSaving(true);
    try {
      const result = await VaultAPI.quickCapture(
        answerToMarkdown({
          content: markdown,
          sources,
          verification,
          conversationTitle,
          turn,
        }),
      );
      if (!result.ok) {
        toast.error('Could not write to the journal', { message: result.error });
        return;
      }
      const { noteId, noteTitle } = result.data;
      setOpen(false);
      // The capture lands on whichever page was written to last, so the toast
      // names it instead of implying the reader chose it.
      toast.success(`Answer saved to "${noteTitle}"`, {
        action: {
          label: 'Open',
          onClick: () => navigate(`/journals?${new URLSearchParams({ noteId }).toString()}`),
        },
      });
    } catch (error) {
      toast.error('Could not write to the journal', {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const compareDetail = canCompare
    ? `${documentIds.length} documents, side by side`
    : documentIds.length === 1
      ? 'Needs two of your documents; this answer cites one'
      : 'Needs two of your documents; this answer cites none';

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>{children}</Popover.Trigger>
      <Popover.Portal>
        <Popover.Content sideOffset={6} align={align} className={ACTION_SURFACE_CLASS}>
          <p className={ACTION_HEADING_CLASS}>Take this answer with you</p>

          <button
            type="button"
            onClick={() => void handleAddToJournal()}
            disabled={isSaving}
            className={ACTION_ROW_CLASS}
          >
            <span className="flex h-5 w-5 shrink-0 items-center justify-center text-text-tertiary">
              {isSaving ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
              ) : (
                <NotebookPen className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
              )}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block text-ui text-text-primary">Add to journal</span>
              <span className="block text-xs leading-snug text-text-muted">
                The answer with its numbered sources
              </span>
            </span>
          </button>

          <button
            type="button"
            disabled={!canCompare}
            onClick={() => {
              setOpen(false);
              navigate(`/compare?${new URLSearchParams({ ids: documentIds.join(',') }).toString()}`);
            }}
            className={ACTION_ROW_CLASS}
          >
            <span className="flex h-5 w-5 shrink-0 items-center justify-center text-text-tertiary">
              <Columns3 className="h-3.5 w-3.5" strokeWidth={1.6} aria-hidden="true" />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block text-ui text-text-primary">Compare sources</span>
              <span className="block text-xs leading-snug text-text-muted">{compareDetail}</span>
            </span>
          </button>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
