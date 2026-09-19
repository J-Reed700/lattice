import { useEffect, useMemo, useState } from 'react';

import { open as openExternal } from '@tauri-apps/plugin-shell';
import { formatDistanceToNow } from 'date-fns';
import { ChevronDown, ChevronRight, ExternalLink, Trash2 } from 'lucide-react';

import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { toast } from '../../stores/toastStore';
import { getSourceExternalUrl } from '../../utils/sourcePreview';

interface ConversationLinkedDocumentsPanelProps {
  conversationId: string;
  /** The named space this chat is confined to; null in General. */
  scopedSpaceName?: string | null;
  onMoveToGeneral?: () => void;
}

export function ConversationLinkedDocumentsPanel({
  conversationId,
  scopedSpaceName = null,
  onMoveToGeneral,
}: ConversationLinkedDocumentsPanelProps) {
  const {
    spaces,
    conversations,
    lastMessageSources,
    linkedDocumentsByConversationId,
    webSourcesByConversationId,
    documentSpaceMembershipsByDocumentId,
    loadConversationLinkedDocuments,
    loadConversationWebSources,
    addConversationWebSource,
    removeConversationWebSource,
    removeConversationLinkedDocument,
    loadDocumentSpaceMemberships,
    setDocumentSpaceMembership,
  } = useConversationsStore();

  const [expanded, setExpanded] = useState(false);
  const [openingDocumentId, setOpeningDocumentId] = useState<string | null>(null);
  const [removingDocumentId, setRemovingDocumentId] = useState<string | null>(null);
  const [loadingMembershipDocumentId, setLoadingMembershipDocumentId] = useState<string | null>(null);
  const [savingMembershipKey, setSavingMembershipKey] = useState<string | null>(null);
  const [ingestingSourceKey, setIngestingSourceKey] = useState<string | null>(null);
  const [linkingSourceKey, setLinkingSourceKey] = useState<string | null>(null);
  const [removingSourceId, setRemovingSourceId] = useState<string | null>(null);
  const [isIngestingAllSources, setIsIngestingAllSources] = useState(false);
  const [ingestedSourceKeys, setIngestedSourceKeys] = useState<Set<string>>(new Set());

  const linkedDocuments = useMemo(
    () => linkedDocumentsByConversationId.get(conversationId) ?? [],
    [linkedDocumentsByConversationId, conversationId]
  );
  const conversation = useMemo(
    () => conversations.find((item) => item.id === conversationId) ?? null,
    [conversations, conversationId]
  );
  const linkedWebSources = useMemo(
    () => webSourcesByConversationId.get(conversationId) ?? [],
    [webSourcesByConversationId, conversationId]
  );
  const citedWebSources = useMemo(() => {
    const byKey = new Map<string, {
      key: string;
      url: string;
      label: string;
      sourceId?: string;
      persisted: boolean;
    }>();

    for (const source of linkedWebSources) {
      const key = source.normalizedUrl?.trim() || source.url.trim().toLowerCase();
      if (!key) continue;
      byKey.set(key, {
        key,
        url: source.url,
        label: source.title?.trim() || source.url,
        sourceId: source.id,
        persisted: true,
      });
    }

    if (!conversation?.messages || conversation.messages.length === 0) {
      return [...byKey.values()];
    }

    for (const message of conversation.messages) {
      if (message.role !== 'assistant') continue;
      const sourceCandidates = [
        ...(message.sources ?? []),
        ...(lastMessageSources.get(message.id) ?? []),
      ];

      for (const source of sourceCandidates) {
        const url = getSourceExternalUrl(source);
        if (!url) continue;
        const key = url.trim().toLowerCase();
        if (byKey.has(key)) continue;
        byKey.set(key, {
          key,
          url,
          label: source.fileName?.trim() || 'Web source',
          persisted: false,
        });
      }
    }

    return [...byKey.values()];
  }, [conversation, lastMessageSources, linkedWebSources]);
  const pendingWebSources = useMemo(
    () => citedWebSources.filter((source) => !ingestedSourceKeys.has(source.key)),
    [citedWebSources, ingestedSourceKeys]
  );
  const linkedContextCount = linkedDocuments.length + citedWebSources.length;

  useEffect(() => {
    setIngestedSourceKeys(new Set());
  }, [conversationId]);

  useEffect(() => {
    void loadConversationLinkedDocuments(conversationId);
  }, [conversationId, loadConversationLinkedDocuments]);

  useEffect(() => {
    void loadConversationWebSources(conversationId);
  }, [conversationId, loadConversationWebSources]);

  const handleOpenDocument = async (documentId: string) => {
    setOpeningDocumentId(documentId);
    try {
      const result = await VaultAPI.openFileById(documentId);
      if (!result.ok) {
        toast.error("Couldn't open document", { message: result.error });
      }
    } finally {
      setOpeningDocumentId(null);
    }
  };

  const handleRemoveDocument = async (documentId: string) => {
    setRemovingDocumentId(documentId);
    try {
      await removeConversationLinkedDocument(conversationId, documentId);
      toast.success('Removed linked document');
    } finally {
      setRemovingDocumentId(null);
    }
  };

  const handleSpacePanelToggle = async (documentId: string) => {
    if (documentSpaceMembershipsByDocumentId.has(documentId)) {
      return;
    }

    setLoadingMembershipDocumentId(documentId);
    try {
      await loadDocumentSpaceMemberships(documentId);
    } finally {
      setLoadingMembershipDocumentId(null);
    }
  };

  const handleMembershipChange = async (
    documentId: string,
    spaceId: string,
    checked: boolean
  ) => {
    const key = `${documentId}:${spaceId}`;
    setSavingMembershipKey(key);
    try {
      await setDocumentSpaceMembership(documentId, spaceId, checked);
    } finally {
      setSavingMembershipKey(null);
    }
  };

  const handleOpenSourceUrl = async (url: string) => {
    try {
      await openExternal(url);
    } catch {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  };

  const handleLinkSource = async (
    source: { key: string; url: string; label: string }
  ) => {
    setLinkingSourceKey(source.key);
    try {
      const ok = await addConversationWebSource(conversationId, source.url, {
        title: source.label,
      });
      if (ok) {
        toast.success('Linked source');
      }
    } finally {
      setLinkingSourceKey(null);
    }
  };

  const handleRemoveSource = async (sourceId: string) => {
    setRemovingSourceId(sourceId);
    try {
      const ok = await removeConversationWebSource(conversationId, sourceId);
      if (ok) {
        toast.success('Removed source link');
      }
    } finally {
      setRemovingSourceId(null);
    }
  };

  const ingestSourceUrl = async (
    source: { key: string; url: string; label: string },
    options?: { skipRefresh?: boolean }
  ): Promise<boolean> => {
    setIngestingSourceKey(source.key);
    try {
      const result = await VaultAPI.ingestWebUrl(source.url, {
        conversationId,
        spaceId: conversation?.spaceId ?? undefined,
      });

      if (!result.ok) {
        toast.error(`Couldn't index ${source.label}`, { message: result.error });
        return false;
      }

      setIngestedSourceKeys((current) => {
        const next = new Set(current);
        next.add(source.key);
        return next;
      });

      if (!options?.skipRefresh) {
        await loadConversationLinkedDocuments(conversationId);
      }
      return true;
    } finally {
      setIngestingSourceKey(null);
    }
  };

  const handleIngestAllSources = async () => {
    if (pendingWebSources.length === 0 || isIngestingAllSources) {
      return;
    }

    setIsIngestingAllSources(true);
    try {
      let successCount = 0;
      for (const source of pendingWebSources) {
        const ok = await ingestSourceUrl(source, { skipRefresh: true });
        if (ok) successCount += 1;
      }

      await loadConversationLinkedDocuments(conversationId);

      if (successCount > 0) {
        toast.success(`Indexed ${successCount} source${successCount === 1 ? '' : 's'}`);
      }
    } finally {
      setIsIngestingAllSources(false);
    }
  };

  const showScopeRelease = Boolean(scopedSpaceName) && Boolean(onMoveToGeneral);

  if (linkedContextCount === 0 && !expanded && !showScopeRelease) {
    return null;
  }

  return (
    <section className="border-t border-subtle py-4">
      {/* No count when there is nothing to count: "· 0" is a stat that is always
          zero, and there is nothing behind it to open. */}
      {(linkedContextCount > 0 || expanded) && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className="flex w-full items-center gap-2 text-xs text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
          aria-expanded={expanded}
        >
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5" />
          )}
          <span>Sources in this conversation · {linkedContextCount}</span>
        </button>
      )}

      {expanded && (
        <div className="mt-3 space-y-3">
          {linkedDocuments.length === 0 && citedWebSources.length === 0 && (
            <p className="text-xs text-[hsl(var(--text-muted))]">
              Nothing linked. Sources cited in answers will appear here.
            </p>
          )}

          {linkedDocuments.map((document) => {
            const memberships = documentSpaceMembershipsByDocumentId.get(document.documentId) ?? [];
            const assignedSpaceIds = new Set(memberships.map((item) => item.spaceId));
            const spaceTriggerLabel =
              memberships.length > 0
                ? `${memberships.length} ${memberships.length === 1 ? 'space' : 'spaces'}`
                : 'Assign spaces';

            return (
              <article
                key={document.documentId}
                className="rounded-sm border border-subtle bg-surface p-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-semibold text-[hsl(var(--text-primary))]">
                      {document.fileName}
                    </p>
                    <p className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                      {document.referenceCount} reference
                      {document.referenceCount === 1 ? '' : 's'}
                      {' · '}
                      {formatDistanceToNow(new Date(document.lastReferencedAt), {
                        addSuffix: true,
                      })}
                    </p>
                  </div>

                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void handleOpenDocument(document.documentId)}
                      disabled={openingDocumentId === document.documentId}
                      className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:opacity-60"
                    >
                      <ExternalLink className="h-3 w-3" />
                      Open
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleRemoveDocument(document.documentId)}
                      disabled={removingDocumentId === document.documentId}
                      className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--danger-fg))] disabled:opacity-60"
                    >
                      <Trash2 className="h-3 w-3" />
                      Remove
                    </button>
                  </div>
                </div>

                <details
                  className="mt-3"
                  onToggle={(event) => {
                    const details = event.currentTarget;
                    if (details.open) {
                      void handleSpacePanelToggle(document.documentId);
                    }
                  }}
                >
                  <summary className="cursor-pointer list-none text-xs text-[hsl(var(--text-muted))] hover:text-[hsl(var(--text-secondary))]">
                    Scope: {spaceTriggerLabel}
                  </summary>
                  <div className="mt-2 grid gap-1 rounded-sm border border-subtle bg-surface-raised p-2">
                    {loadingMembershipDocumentId === document.documentId && (
                      <p className="text-xs text-[hsl(var(--text-muted))]">Loading spaces...</p>
                    )}
                    {spaces.map((space) => {
                      const key = `${document.documentId}:${space.id}`;
                      return (
                        <label
                          key={space.id}
                          className="flex items-center gap-2 rounded-sm px-2 py-1 text-xs text-[hsl(var(--text-secondary))] hover:bg-surface"
                        >
                          <input
                            type="checkbox"
                            checked={assignedSpaceIds.has(space.id)}
                            onChange={(event) =>
                              void handleMembershipChange(
                                document.documentId,
                                space.id,
                                event.target.checked
                              )
                            }
                            disabled={savingMembershipKey === key}
                            className="h-3.5 w-3.5 rounded-sm border-border-default bg-transparent accent-[hsl(var(--accent))]"
                          />
                          <span className="truncate">
                            {space.name}
                            {space.isArchived ? ' (archived)' : ''}
                          </span>
                        </label>
                      );
                    })}
                  </div>
                </details>
              </article>
            );
          })}

          {citedWebSources.length > 0 && (
            <article className="rounded-sm border border-subtle bg-surface p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <p className="text-sm font-semibold text-[hsl(var(--text-primary))]">Web sources</p>
                  <p className="mt-0.5 text-xs text-[hsl(var(--text-muted))]">
                    URLs cited in this conversation. Ingest to include them in search.
                  </p>
                </div>
                {pendingWebSources.length > 0 && (
                  <button
                    type="button"
                    onClick={() => void handleIngestAllSources()}
                    disabled={isIngestingAllSources}
                    className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface-raised disabled:opacity-50"
                  >
                    Index all
                  </button>
                )}
              </div>

              <div className="mt-3 space-y-2">
                {pendingWebSources.length === 0 ? (
                  <p className="text-xs text-[hsl(var(--text-muted))]">
                    Nothing pending.
                  </p>
                ) : (
                  pendingWebSources.map((source) => (
                    <div
                      key={source.key}
                      className="rounded-sm border border-subtle bg-surface-raised p-2.5"
                    >
                      <p className="text-sm font-medium text-[hsl(var(--text-primary))]">{source.label}</p>
                      <p className="mt-0.5 break-all text-xs text-[hsl(var(--text-muted))]">{source.url}</p>
                      <div className="mt-2 flex flex-wrap items-center gap-2">
                        <button
                          type="button"
                          onClick={() => void handleOpenSourceUrl(source.url)}
                          className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))]"
                        >
                          <ExternalLink className="h-3 w-3" />
                          Open URL
                        </button>
                        {source.persisted && source.sourceId ? (
                          <button
                            type="button"
                            onClick={() => void handleRemoveSource(source.sourceId!)}
                            disabled={removingSourceId === source.sourceId}
                            className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--danger-fg))] disabled:opacity-60"
                          >
                            <Trash2 className="h-3 w-3" />
                            Remove link
                          </button>
                        ) : (
                          <button
                            type="button"
                            onClick={() => void handleLinkSource(source)}
                            disabled={linkingSourceKey === source.key}
                            className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:opacity-60"
                          >
                            Link
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => void ingestSourceUrl(source)}
                          disabled={ingestingSourceKey === source.key || isIngestingAllSources}
                          className="inline-flex items-center gap-1 rounded-sm border border-border-default px-2 py-1 text-xs text-[hsl(var(--text-primary))] transition-colors duration-fast hover:bg-surface disabled:opacity-50"
                        >
                          Index
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </article>
          )}
        </div>
      )}

      {/*
        The scope is a standing narrowing of every future answer in this thread,
        so the way out of it stands with it — not only while files happen to be
        staged above the composer. It names both ends: this used to read "only
        this conversation's files … search the whole vault", which was true of
        neither the space nor General.
      */}
      {showScopeRelease && (
        <p className="mt-2 text-xs text-[hsl(var(--text-muted))]">
          Answers use only documents filed in {scopedSpaceName}.{' '}
          <button
            type="button"
            onClick={onMoveToGeneral}
            className="text-[hsl(var(--accent))] underline-offset-2 hover:underline"
          >
            Move this chat to General
          </button>
        </p>
      )}
    </section>
  );
}
