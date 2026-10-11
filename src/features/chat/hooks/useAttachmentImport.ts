import { useCallback, useState, type RefObject } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { useChatFileDrop, type StagedFile } from '@/features/chat/hooks/useChatFileDrop';
import { pollAttachmentImport } from '@/features/chat/model/attachmentImportPoll';
import { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

/** Poll a batch job until it stops moving, or five minutes elapse. */
const BATCH_POLL_INTERVAL_MS = 1000;
const BATCH_POLL_TIMEOUT_MS = 5 * 60 * 1000;

/**
 * What an import handed the send: names for the chips, ids for the turn.
 *
 * Both travel or the file is only half attached — visible in the thread and
 * invisible to the answer.
 */
export interface ImportedAttachments {
  names: string[];
  documentIds: string[];
}

export interface AttachmentImport {
  isDragging: boolean;
  staged: StagedFile[];
  isImporting: boolean;
  remove: (_path: string) => void;
  clear: () => void;
  /** The file dialog, for people who would rather not drag. */
  chooseFiles: () => Promise<void>;
  /**
   * Import the staged files into the vault and link them to this conversation.
   *
   * Returns the names and the document ids when the send may proceed —
   * including the still-indexing case, where the documents are linked and the
   * next turn catches up — and `null` when nothing made it in, so the caller
   * can abort with the composer's draft and the staged files both intact.
   *
   * The ids matter as much as the names: the names only draw the chips, while
   * the ids are what the turn reads. A file whose id never reaches the send is
   * a file the answer is written without, however plainly its chip says it was
   * attached.
   */
  importStaged: () => Promise<ImportedAttachments | null>;
}

interface AttachmentImportOptions {
  /** Where files can be dropped. */
  dropTargetRef: RefObject<HTMLElement | null>;
  conversationId: string | null;
  /** The conversation's own space, when it has one; attachments join it. */
  scopedSpaceId: string | null;
}

/** Files staged on a conversation, and the import that attaches them to it. */
export function useAttachmentImport({
  dropTargetRef,
  conversationId,
  scopedSpaceId,
}: AttachmentImportOptions): AttachmentImport {
  const queryClient = useQueryClient();
  const loadConversationLinkedDocuments = useConversationsStore((state) => state.loadConversationLinkedDocuments);
  const [isImporting, setIsImporting] = useState(false);
  const {
    isDragging,
    staged,
    add: addStagedPaths,
    clear,
    remove,
  } = useChatFileDrop(dropTargetRef, () => {
    // Staging is the hook's own state; the panel only needs to re-render.
  });

  const chooseFiles = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const picked = await open({ multiple: true });
      if (!picked) return;
      addStagedPaths(Array.isArray(picked) ? picked : [picked]);
    } catch {
      // Not in a Tauri webview: the drop target is still there.
    }
  }, [addStagedPaths]);

  const importStaged = useCallback(async (): Promise<ImportedAttachments | null> => {
    if (!conversationId || staged.length === 0 || isImporting) return null;
    const stagedNames = staged.map((file) => file.name);
    setIsImporting(true);
    try {
      // Attached, not filed. These documents belong to this conversation: the
      // turn can read and cite them, but they stay out of the library and out
      // of every other chat's searches, and they go when this chat goes. The
      // "Add to library" action on the attachment is what files them for good.
      const started = await VaultAPI.startBatchFileImport(
        staged.map((file) => file.path),
        conversationId
      );
      if (!started.ok) {
        toast.error("Couldn't add these files", { message: started.error });
        return null;
      }

      const outcome = await pollAttachmentImport(started.data, {
        getStatus: VaultAPI.getBatchJobStatus,
        intervalMs: BATCH_POLL_INTERVAL_MS,
        timeoutMs: BATCH_POLL_TIMEOUT_MS,
      });
      if (outcome.kind === 'unreachable') {
        toast.error("Couldn't confirm these files were attached", { message: outcome.error });
        return null;
      }
      const { documentIds } = outcome;

      // Scoping only applies to a conversation that already has its own space.
      // Creating one behind the user's back would silently narrow every future
      // answer in this thread.
      if (documentIds.length > 0 && scopedSpaceId) {
        await VaultAPI.setDocumentsSpaceMembership(documentIds, scopedSpaceId, true);
      }

      await queryClient.invalidateQueries({
        queryKey: conversationKeys.linkedDocuments(conversationId),
      });
      void loadConversationLinkedDocuments(conversationId);

      const requested = staged.length;
      // Report what the job actually did. Saying "Attached 4 files" after the
      // batch failed, or after we stopped waiting, is a claim we cannot make.
      // Files stay staged on a total failure so the send can be retried.
      //
      // "Attached", not "Added": these files belong to this conversation, not
      // to the library, and the wording is the only place the user learns that
      // before they go looking for them in the library.
      if (outcome.kind === 'pending') {
        // Still indexing at the deadline. Send only with ids to read: chips
        // without ids would promise files the answer never sees.
        if (documentIds.length === 0) {
          toast.error('These files are taking too long to attach', {
            message: 'They stay staged, and your message was not sent.',
          });
          return null;
        }
        clear();
        toast.info(`Still attaching ${requested} file${requested !== 1 ? 's' : ''}`, {
          message: "They'll appear in this conversation when indexing finishes.",
        });
        return { names: stagedNames, documentIds };
      } else if (outcome.added === 0) {
        toast.error("Couldn't attach these files", {
          message: `${outcome.failed || requested} failed to import.`,
        });
        return null;
      } else {
        clear();
        toast.success(`Attached ${outcome.added} file${outcome.added !== 1 ? 's' : ''}`, {
          message:
            outcome.failed > 0
              ? `${outcome.failed} couldn't be read. The rest are indexing now.`
              : 'Only this conversation can see them. Add them to your library from Sources.',
        });
        return { names: stagedNames, documentIds };
      }
    } finally {
      setIsImporting(false);
    }
  }, [
    conversationId,
    staged,
    isImporting,
    scopedSpaceId,
    queryClient,
    loadConversationLinkedDocuments,
    clear,
  ]);

  return { isDragging, staged, isImporting, remove, clear, chooseFiles, importStaged };
}
