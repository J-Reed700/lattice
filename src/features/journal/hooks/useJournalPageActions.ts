import { useCallback, useEffect, useState } from 'react';

import { confirm as tauriConfirm } from '@tauri-apps/plugin-dialog';

import { pageTitle as rawPageTitle } from '@/features/journal/components/PageList';
import { defaultJournalTitle, UNTITLED_PAGE } from '@/features/journal/hooks/useJournalNote';
import type { JournalNotify } from '@/features/journal/hooks/useJournalNotice';
import { NEW_ITEM_EVENT } from '@/shared/newItemEvent';
import type { ConversationJournalDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import type { SetURLSearchParams } from 'react-router';

/** Selects, names, creates and deletes the pages of the open journal. */
export function useJournalPageActions(options: {
  journalSpace: ConversationJournalDto | null;
  requestedJournalSpaceId: string | null;
  isLoadingNote: boolean;
  selectPage: (noteId: string) => Promise<void>;
  createPage: (title: string) => Promise<WorkspaceNote | null>;
  renamePage: (noteId: string, title: string) => Promise<boolean>;
  deletePage: (noteId: string) => Promise<boolean>;
  notify: JournalNotify;
  searchParams: URLSearchParams;
  setSearchParams: SetURLSearchParams;
}) {
  const {
    journalSpace,
    requestedJournalSpaceId,
    isLoadingNote,
    selectPage,
    createPage,
    renamePage,
    deletePage,
    notify,
    searchParams,
    setSearchParams,
  } = options;

  const handleSelectPage = useCallback(
    (noteId: string) => {
      void selectPage(noteId);
    },
    [selectPage],
  );

  // Earlier builds titled a journal's first page "Journal · <name>", which read
  // in the list as a journal sitting among its own pages. It is an ordinary
  // page that was never named, and is shown as one.
  const displayPageTitle = useCallback(
    (page: WorkspaceNote) => {
      const title = rawPageTitle(page);
      return title === defaultJournalTitle(journalSpace?.name ?? 'Journal') ? UNTITLED_PAGE : title;
    },
    [journalSpace?.name],
  );

  // A new page arrives unnamed with the caret in its title, rather than as one
  // more "Page · Fri, Sep 18" that cannot be told from the last.
  const [pageToName, setPageToName] = useState<string | null>(null);

  const handleNewPage = useCallback(async () => {
    const created = await createPage(UNTITLED_PAGE);
    if (created) setPageToName(created.id);
  }, [createPage]);

  const handleRenamePage = useCallback(
    async (noteId: string, title: string) => {
      setPageToName(null);
      if (!(await renamePage(noteId, title))) notify('error', 'Could not rename that page.');
    },
    [notify, renamePage],
  );

  const handleDeletePage = useCallback(
    async (page: WorkspaceNote) => {
      const name = displayPageTitle(page);
      const confirmed = await tauriConfirm(
        'The page and its highlights are deleted. This cannot be undone.',
        { title: `Delete "${name}"?`, kind: 'warning' },
      );
      if (!confirmed) return;
      if (await deletePage(page.id)) notify('success', `Deleted "${name}".`);
      else notify('error', 'Could not delete that page.');
    },
    [deletePage, displayPageTitle, notify],
  );

  // ⌘N from anywhere in the app (RootLayout) and `?new=1` deep links both
  // start a new page: writing is what a journal is for. (A `?new=1` that
  // arrives before the page list has loaded waits for it.)
  useEffect(() => {
    const onNew = () => {
      void handleNewPage();
    };
    window.addEventListener(NEW_ITEM_EVENT, onNew);
    return () => window.removeEventListener(NEW_ITEM_EVENT, onNew);
  }, [handleNewPage]);

  useEffect(() => {
    if (searchParams.get('new') !== '1' || !requestedJournalSpaceId || isLoadingNote) return;
    const next = new URLSearchParams(searchParams);
    next.delete('new');
    setSearchParams(next, { replace: true });
    void handleNewPage();
  }, [handleNewPage, isLoadingNote, requestedJournalSpaceId, searchParams, setSearchParams]);

  return {
    handleSelectPage,
    displayPageTitle,
    pageToName,
    handleNewPage,
    handleRenamePage,
    handleDeletePage,
  };
}
