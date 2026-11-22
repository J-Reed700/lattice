import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { useChatFileDrop, type StagedFile } from '@/features/chat/hooks/useChatFileDrop';
import {
  attachPictures,
  isPicturePath,
  releasePictures,
  takePictures as takeStagedPictures,
  useStagedPictures,
} from '@/features/chat/images/stagedPictures';
import { pollAttachmentImport } from '@/features/chat/model/attachmentImportPoll';
import {
  useIsStagedFilesImporting,
  useStagedFilesStore,
} from '@/features/chat/stores/stagedFilesStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { ChatImage } from '@/types/conversation';

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
  /** Documents, imported when the message is sent. */
  staged: readonly StagedFile[];
  isImporting: boolean;
  remove: (_path: string) => void;
  /** Drops every staged document and picture: the draft's attachments are discarded. */
  clear: () => void;
  /** Pictures, stored as soon as they are staged and claimed by the send. */
  pictures: readonly ChatImage[];
  isAttachingPictures: boolean;
  removePicture: (_id: string) => void;
  /** The staged pictures, handed to a send and cleared from the draft without release. */
  takePictures: () => ChatImage[];
  /** The file dialog, for people who would rather not drag. */
  chooseFiles: () => Promise<void>;
  /** Cancel an in-progress attachment import, leaving staged files intact. */
  cancelImport?: () => void;
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
  const api = useApi();
  const queryClient = useQueryClient();
  const loadConversationLinkedDocuments = useConversationsStore((state) => state.loadConversationLinkedDocuments);
  const isImporting = useIsStagedFilesImporting(conversationId);
  const [attachingPictures, setAttachingPictures] = useState(0);
  const pictures = useStagedPictures(conversationId);
  const importAbortRef = useRef<AbortController | null>(null);
  const conversationIdRef = useRef(conversationId);
  conversationIdRef.current = conversationId;
  const activeJobRef = useRef<{
    jobId: string | null;
    targetConversationId: string;
    controller: AbortController;
    explicitlyCancelled: boolean;
  } | null>(null);

  useEffect(() => () => {
    importAbortRef.current?.abort();
    importAbortRef.current = null;
    if (conversationId) {
      useStagedFilesStore.getState().setImporting(conversationId, false);
    }
  }, [conversationId]);

  const cancelImport = useCallback(() => {
    const active = activeJobRef.current;
    if (active) {
      active.explicitlyCancelled = true;
      useStagedFilesStore.getState().setImporting(active.targetConversationId, false);
      if (active.jobId) {
        void Promise.resolve(api.cancelBatchJob(active.jobId)).catch(() => undefined);
      }
    }
    importAbortRef.current?.abort();
    importAbortRef.current = null;
  }, [api]);

  // Pictures go to the backend now, not at send: the composer shows the
  // stored file, and the turn reads the rows the send names.
  const stagePictures = useCallback(async (paths: string[]) => {
    const picturePaths = paths.filter(isPicturePath);
    if (!conversationId || picturePaths.length === 0) return;
    setAttachingPictures((count) => count + 1);
    try {
      await attachPictures(conversationId, picturePaths, api);
    } finally {
      setAttachingPictures((count) => count - 1);
    }
  }, [api, conversationId]);

  const {
    isDragging,
    staged,
    add: addStagedPaths,
    clear: clearStagedFiles,
    remove,
  } = useChatFileDrop(
    dropTargetRef,
    (paths) => void stagePictures(paths),
    (path) => !isPicturePath(path),
    conversationId,
  );

  const chooseFiles = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const picked = await open({ multiple: true });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      addStagedPaths(paths);
      void stagePictures(paths);
    } catch {
      // Not in a Tauri webview: the drop target is still there.
    }
  }, [addStagedPaths, stagePictures]);

  const removePicture = useCallback((id: string) => {
    if (conversationId) void releasePictures(conversationId, [id], api);
  }, [api, conversationId]);

  const clear = useCallback(() => {
    clearStagedFiles();
    if (conversationId) void releasePictures(conversationId, pictures.map((picture) => picture.id), api);
  }, [api, clearStagedFiles, conversationId, pictures]);

  const takePictures = useCallback(
    () => (conversationId ? takeStagedPictures(conversationId) : []),
    [conversationId],
  );

  const importStaged = useCallback(async (): Promise<ImportedAttachments | null> => {
    const targetConversationId = conversationId;
    if (!targetConversationId || staged.length === 0 || isImporting) return null;
    const stagedSnapshot = [...staged];
    const stagedNames = stagedSnapshot.map((file) => file.name);
    const controller = new AbortController();
    importAbortRef.current?.abort();
    importAbortRef.current = controller;
    const jobState = {
      jobId: null as string | null,
      targetConversationId,
      controller,
      explicitlyCancelled: false,
    };
    activeJobRef.current = jobState;
    useStagedFilesStore.getState().setImporting(targetConversationId, true);
    try {
      // Attached, not filed. These documents belong to this conversation: the
      // turn can read and cite them, but they stay out of the library and out
      // of every other chat's searches, and they go when this chat goes. The
      // "Add to library" action on the attachment is what files them for good.
      const started = await api.startBatchFileImport(
        stagedSnapshot.map((file) => file.path),
        targetConversationId
      );
      if (!started.ok) {
        if (!controller.signal.aborted) {
          toast.error("Couldn't add these files", { message: started.error });
        }
        return null;
      }
      jobState.jobId = started.data;
      if (controller.signal.aborted) {
        if (jobState.explicitlyCancelled) {
          void Promise.resolve(api.cancelBatchJob(started.data)).catch(() => undefined);
        }
        return null;
      }

      const outcome = await pollAttachmentImport(started.data, {
        getStatus: api.getBatchJobStatus,
        intervalMs: BATCH_POLL_INTERVAL_MS,
        timeoutMs: BATCH_POLL_TIMEOUT_MS,
        signal: controller.signal,
      });
      if (outcome.kind === 'cancelled' || controller.signal.aborted || conversationIdRef.current !== targetConversationId) {
        return null;
      }
      if (outcome.kind === 'unreachable') {
        toast.error("Couldn't confirm these files were attached", { message: outcome.error });
        return null;
      }
      const { documentIds } = outcome;

      // Scoping only applies to a conversation that already has its own space.
      // Creating one behind the user's back would silently narrow every future
      // answer in this thread.
      if (documentIds.length > 0 && scopedSpaceId) {
        await api.setDocumentsSpaceMembership(documentIds, scopedSpaceId, true);
      }

      await queryClient.invalidateQueries({
        queryKey: conversationKeys.linkedDocuments(targetConversationId),
      });
      void loadConversationLinkedDocuments(targetConversationId);

      // Keep staged files intact if the user cancelled or switched conversations before send.
      if (controller.signal.aborted || conversationIdRef.current !== targetConversationId) {
        return null;
      }

      const clearTarget = () => {
        if (targetConversationId === conversationIdRef.current) {
          clearStagedFiles();
        } else {
          useStagedFilesStore.getState().clear(targetConversationId);
        }
      };

      const requested = stagedSnapshot.length;
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
        clearTarget();
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
        clearTarget();
        toast.success(`Attached ${outcome.added} file${outcome.added !== 1 ? 's' : ''}`, {
          message:
            outcome.failed > 0
              ? `${outcome.failed} couldn't be read. The rest are indexing now.`
              : 'Only this conversation can see them. Add them to your library from Sources.',
        });
        return { names: stagedNames, documentIds };
      }
    } finally {
      if (activeJobRef.current === jobState) {
        activeJobRef.current = null;
      }
      if (importAbortRef.current === controller) {
        importAbortRef.current = null;
      }
      useStagedFilesStore.getState().setImporting(targetConversationId, false);
    }
  }, [conversationId, staged, isImporting, api, scopedSpaceId, queryClient, loadConversationLinkedDocuments, clearStagedFiles]);

  return {
    isDragging,
    staged,
    isImporting,
    remove,
    clear,
    pictures,
    isAttachingPictures: attachingPictures > 0,
    removePicture,
    takePictures,
    chooseFiles,
    cancelImport,
    importStaged,
  };
}
