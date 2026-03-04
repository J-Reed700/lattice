import { useEffect, useMemo, useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { ChevronDown, ChevronRight, ExternalLink, Link2, Trash2 } from 'lucide-react';
import { open as openExternal } from '@tauri-apps/plugin-shell';

import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { toast } from '../../stores/toastStore';
import { getSourceExternalUrl } from '../../utils/sourcePreview';

interface ConversationLinkedDocumentsPanelProps {
  conversationId: string;
}

export function ConversationLinkedDocumentsPanel({
  conversationId,
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
        toast.error('Failed to open document', { message: result.error });
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
        toast.error(`Failed to ingest ${source.label}`, { message: result.error });
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
        toast.success(`Ingested ${successCount} source${successCount === 1 ? '' : 's'}`);
      }
    } finally {
      setIsIngestingAllSources(false);
    }
  };

  return (
    <section className="border-b border-white/10 bg-white/[0.03]">
      <button
        type="button"
        onClick={() => setExpanded((value) => !value)}
        className="flex w-full items-center justify-between px-5 py-3 text-left transition hover:bg-white/[0.04]"
      >
        <span className="inline-flex items-center gap-2 text-sm font-medium text-white/85">
          <Link2 className="h-4 w-4" />
          Linked Documents
          <span className="rounded-full border border-white/15 bg-white/[0.08] px-2 py-0.5 text-xs text-white/75">
            {linkedContextCount}
          </span>
        </span>
        {expanded ? (
          <ChevronDown className="h-4 w-4 text-white/50" />
        ) : (
          <ChevronRight className="h-4 w-4 text-white/50" />
        )}
      </button>

      {expanded && (
        <div className="space-y-2 px-4 pb-4">
          {linkedDocuments.length === 0 && citedWebSources.length === 0 && (
            <p className="rounded-lg border border-dashed border-white/15 bg-white/[0.02] px-3 py-2 text-xs text-white/55">
              No sources are linked to this conversation yet.
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
                className="rounded-xl border border-white/10 bg-white/[0.02] p-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium text-white/90">
                      {document.fileName}
                    </p>
                    <p className="mt-1 text-xs text-white/55">
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
                      className="inline-flex items-center gap-1 rounded-md border border-white/15 px-2 py-1 text-xs text-white/75 transition hover:border-white/30 hover:text-white disabled:opacity-60"
                    >
                      <ExternalLink className="h-3.5 w-3.5" />
                      Open
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleRemoveDocument(document.documentId)}
                      disabled={removingDocumentId === document.documentId}
                      className="inline-flex items-center gap-1 rounded-md border border-rose-400/25 px-2 py-1 text-xs text-rose-200 transition hover:border-rose-300/40 hover:text-rose-100 disabled:opacity-60"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
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
                  <summary className="cursor-pointer list-none text-xs text-white/60 hover:text-white/80">
                    Scope: {spaceTriggerLabel}
                  </summary>
                  <div className="mt-2 grid gap-1 rounded-lg border border-white/10 bg-black/20 p-2">
                    {loadingMembershipDocumentId === document.documentId && (
                      <p className="text-xs text-white/55">Loading spaces...</p>
                    )}
                    {spaces.map((space) => {
                      const key = `${document.documentId}:${space.id}`;
                      return (
                        <label
                          key={space.id}
                          className="flex items-center gap-2 rounded px-2 py-1 text-xs text-white/75 hover:bg-white/[0.05]"
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
                            className="h-3.5 w-3.5 rounded border-white/30 bg-transparent"
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

          <article className="rounded-xl border border-cyan-300/20 bg-cyan-500/[0.06] p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <p className="text-sm font-medium text-cyan-100">Chat Web Sources</p>
                <p className="mt-1 text-xs text-cyan-100/70">
                  URL sources linked to this conversation context. Ingest only when you want them indexed.
                </p>
              </div>
              <button
                type="button"
                onClick={() => void handleIngestAllSources()}
                disabled={pendingWebSources.length === 0 || isIngestingAllSources}
                className="inline-flex items-center gap-1 rounded-md border border-cyan-300/35 bg-cyan-500/15 px-2 py-1 text-xs text-cyan-100 transition hover:bg-cyan-500/25 disabled:opacity-50"
              >
                Ingest All Sources
              </button>
            </div>

            <div className="mt-3 space-y-2">
              {pendingWebSources.length === 0 ? (
                <p className="text-xs text-cyan-100/70">
                  No pending web citation sources.
                </p>
              ) : (
                pendingWebSources.map((source) => (
                  <div
                    key={source.key}
                    className="rounded-lg border border-white/10 bg-black/20 p-2.5"
                  >
                    <p className="text-xs font-medium text-white/90">{source.label}</p>
                    <p className="mt-1 break-all text-[11px] text-white/60">{source.url}</p>
                    <div className="mt-2 flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => void handleOpenSourceUrl(source.url)}
                        className="inline-flex items-center gap-1 rounded-md border border-white/15 px-2 py-1 text-xs text-white/75 transition hover:border-white/30 hover:text-white"
                      >
                        <ExternalLink className="h-3.5 w-3.5" />
                        Open URL
                      </button>
                      {source.persisted && source.sourceId ? (
                        <button
                          type="button"
                          onClick={() => void handleRemoveSource(source.sourceId!)}
                          disabled={removingSourceId === source.sourceId}
                          className="inline-flex items-center gap-1 rounded-md border border-rose-400/25 px-2 py-1 text-xs text-rose-200 transition hover:border-rose-300/40 hover:text-rose-100 disabled:opacity-60"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          Remove Link
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={() => void handleLinkSource(source)}
                          disabled={linkingSourceKey === source.key}
                          className="inline-flex items-center gap-1 rounded-md border border-white/15 px-2 py-1 text-xs text-white/75 transition hover:border-white/30 hover:text-white disabled:opacity-60"
                        >
                          Link Source
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => void ingestSourceUrl(source)}
                        disabled={ingestingSourceKey === source.key || isIngestingAllSources}
                        className="inline-flex items-center gap-1 rounded-md border border-cyan-300/35 bg-cyan-500/15 px-2 py-1 text-xs text-cyan-100 transition hover:bg-cyan-500/25 disabled:opacity-50"
                      >
                        Ingest
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </article>
        </div>
      )}
    </section>
  );
}
