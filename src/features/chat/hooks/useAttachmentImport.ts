import { useCallback, useState, type RefObject } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { useChatFileDrop, type StagedFile } from '@/features/chat/hooks/useChatFileDrop';
import {
  attachPictures,
  isPicturePath,
  releasePictures,
  takePictures as takeStagedPictures,
  useStagedPictures,
} from '@/features/chat/images/stagedPictures';
import { pollAttachmentImport } from '@/features/chat/model/attachmentImportPoll';
import { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { useConversationsStore } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';
import type { ChatImage } from '@/types/conversation';

const BATCH_POLL_INTERVAL_MS = 1000;
const BATCH_POLL_TIMEOUT_MS = 5 * 60 * 1000;

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
  pictures: readonly ChatImage[];
  isAttachingPictures: boolean;
  removePicture: (_id: string) => void;
  takePictures: () => ChatImage[];
  chooseFiles: () => Promise<void>;
  importStaged: () => Promise<ImportedAttachments | null>;
}

interface AttachmentImportOptions {
  dropTargetRef: RefObject<HTMLElement | null>;
  conversationId: string | null;
  scopedSpaceId: string | null;
}

export function useAttachmentImport({
  dropTargetRef,
  conversationId,
  scopedSpaceId,
}: AttachmentImportOptions): AttachmentImport {
  const queryClient = useQueryClient();
  const loadConversationLinkedDocuments = useConversationsStore((state) => state.loadConversationLinkedDocuments);
  const [isImporting, setIsImporting] = useState(false);
  const [attachingPictures, setAttachingPictures] = useState(0);
  const pictures = useStagedPictures(conversationId);

  const stagePictures = useCallback(async (paths: string[]) => {
    const picturePaths = paths.filter(isPicturePath);
    if (!conversationId || picturePaths.length === 0) return;
    setAttachingPictures((count) => count + 1);
    try {
      await attachPictures(conversationId, picturePaths);
    } finally {
      setAttachingPictures((count) => count - 1);
    }
  }, [conversationId]);

  const {
    isDragging,
    staged,
    add: addStagedPaths,
    clear: clearStagedFiles,
    remove,
  } = useChatFileDrop(dropTargetRef, (paths) => void stagePictures(paths), (path) => !isPicturePath(path));

  const chooseFiles = useCallback(async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const picked = await open({ multiple: true });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      addStagedPaths(paths);
      void stagePictures(paths);
    } catch {
      // Ignore if not in Tauri webview
    }
  }, [addStagedPaths, stagePictures]);

  const removePicture = useCallback((id: string) => {
    if (conversationId) void releasePictures(conversationId, [id]);
  }, [conversationId]);

  const clear = useCallback(() => {
    clearStagedFiles();
    if (conversationId) void releasePictures(conversationId, pictures.map((picture) => picture.id));
  }, [clearStagedFiles, conversationId, pictures]);

  const takePictures = useCallback(
    () => (conversationId ? takeStagedPictures(conversationId) : []),
    [conversationId],
  );

  const importStaged = useCallback(async (): Promise<ImportedAttachments | null> => {
    if (!conversationId || staged.length === 0 || isImporting) return null;
    const stagedNames = staged.map((file) => file.name);
    setIsImporting(true);
    try {
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

      if (documentIds.length > 0 && scopedSpaceId) {
        await VaultAPI.setDocumentsSpaceMembership(documentIds, scopedSpaceId, true);
      }

      await queryClient.invalidateQueries({
        queryKey: conversationKeys.linkedDocuments(conversationId),
      });
      void loadConversationLinkedDocuments(conversationId);

      const requested = staged.length;
      if (outcome.kind === 'pending') {
        if (documentIds.length === 0) {
          toast.error('These files are taking too long to attach', {
            message: 'They stay staged, and your message was not sent.',
          });
          return null;
        }
        clearStagedFiles();
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
        clearStagedFiles();
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
    clearStagedFiles,
  ]);

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
    importStaged,
  };
}
