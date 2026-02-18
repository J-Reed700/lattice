import { useEffect, useMemo, useState } from 'react';

import { formatDistanceToNow } from 'date-fns';
import { ChevronDown, ChevronRight, ExternalLink, Link2, Trash2 } from 'lucide-react';

import { VaultAPI } from '../../lib/api';
import { useConversationsStore } from '../../stores/conversationsStore';
import { toast } from '../../stores/toastStore';

interface ConversationLinkedDocumentsPanelProps {
  conversationId: string;
}

export function ConversationLinkedDocumentsPanel({
  conversationId,
}: ConversationLinkedDocumentsPanelProps) {
  const {
    spaces,
    linkedDocumentsByConversationId,
    documentSpaceMembershipsByDocumentId,
    loadConversationLinkedDocuments,
    removeConversationLinkedDocument,
    loadDocumentSpaceMemberships,
    setDocumentSpaceMembership,
  } = useConversationsStore();

  const [expanded, setExpanded] = useState(false);
  const [openingDocumentId, setOpeningDocumentId] = useState<string | null>(null);
  const [removingDocumentId, setRemovingDocumentId] = useState<string | null>(null);
  const [loadingMembershipDocumentId, setLoadingMembershipDocumentId] = useState<string | null>(null);
  const [savingMembershipKey, setSavingMembershipKey] = useState<string | null>(null);

  const linkedDocuments = useMemo(
    () => linkedDocumentsByConversationId.get(conversationId) ?? [],
    [linkedDocumentsByConversationId, conversationId]
  );

  useEffect(() => {
    void loadConversationLinkedDocuments(conversationId);
  }, [conversationId, loadConversationLinkedDocuments]);

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
            {linkedDocuments.length}
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
          {linkedDocuments.length === 0 && (
            <p className="rounded-lg border border-dashed border-white/15 bg-white/[0.02] px-3 py-2 text-xs text-white/55">
              No documents are linked to this conversation yet.
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
        </div>
      )}
    </section>
  );
}
