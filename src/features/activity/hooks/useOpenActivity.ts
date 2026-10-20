import { useCallback } from 'react';

import { useNavigate } from 'react-router';

import { useExplorerStore } from '@/features/explorer/stores/explorerStore';
import { useDownloadState } from '@/features/model/hooks/useDownloadState';
import type { ActivityEntryDto } from '@/lib/bindings';

/** Whether an item has somewhere to open: its course, lesson, folder or chat, or the downloads drawer. */
export function canOpenActivity(item: Pick<ActivityEntryDto, 'kind' | 'target'>): boolean {
  return item.target !== null || item.kind === 'download';
}

/** Opens where an item lives, then calls `onOpened` (the drawer closes). */
export function useOpenActivity(onOpened: () => void) {
  const navigate = useNavigate();
  const { openDrawer } = useDownloadState();
  return useCallback(
    (item: ActivityEntryDto) => {
      const target = item.target;
      if (item.kind === 'download') {
        openDrawer();
      } else if (!target) {
        return;
      } else if (target.type === 'course') {
        navigate(`/studio?${new URLSearchParams({ course: target.course }).toString()}`);
      } else if (target.type === 'lesson') {
        navigate(`/studio?${new URLSearchParams({ course: target.course, lesson: target.lesson }).toString()}`);
      } else if (target.type === 'folder') {
        useExplorerStore.getState().setRoot({ root: target.root, name: target.name });
        navigate('/explorer');
      } else {
        navigate(`/chat?${new URLSearchParams({ conversationId: target.conversation }).toString()}`);
      }
      onOpened();
    },
    [navigate, onOpened, openDrawer],
  );
}
