import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, GitBranch, Loader2, X } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { unwrap } from '@/features/chat/api/conversationQueryData';
import { conversationUiStore, useConversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { CitationVisibilityToggle } from '@/features/reading/components/CitationVisibilityToggle';
import { VaultAPI } from '@/lib/api';
import type { ConversationTangentDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { toast } from '@/stores/toastStore';
import type { ToolPreferences } from '@/types';

import { TangentPanel } from './TangentPanel';
import { TangentSelectionContext, type TangentSelectionData } from './TangentSelection';

import './tangents.css';

/** Mounted once in ChatPanel, so Chat and Explorer share the whole feature. */
export function ConversationTangents({ conversationId, toolPreferences, unavailable, children }: {
  conversationId: string;
  toolPreferences: ToolPreferences;
  unavailable: boolean;
  children: ReactNode;
}) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const creatingRef = useRef(false);
  const requestedTangent = useConversationUiStore(state => state.requestedTangent);
  useEffect(() => {
    if (requestedTangent?.parentId !== conversationId) return;
    setSelectedId(requestedTangent.tangentId);
    setOpen(true);
    conversationUiStore.setState({ requestedTangent: null });
  }, [conversationId, requestedTangent]);
  // Switching or closing a tangent preserves its unsent question.
  const drafts = useRef(new Map<string, string>());
  const toggleRef = useRef<HTMLButtonElement>(null);
  const tangents = useQuery({
    queryKey: conversationKeys.tangents(conversationId),
    queryFn: async () => unwrap(await VaultAPI.listConversationTangents(conversationId)),
    staleTime: 10_000,
  });
  const selected = tangents.data?.find(item => item.conversationId === selectedId);
  const close = () => { setOpen(false); toggleRef.current?.focus(); };

  const create = useCallback((selection: TangentSelectionData) => {
    if (creatingRef.current) return;
    creatingRef.current = true;
    setCreating(true);
    void (async () => {
      try {
        const tangent = unwrap(await VaultAPI.createConversationTangent(selection));
        queryClient.setQueryData<ConversationTangentDto[]>(conversationKeys.tangents(conversationId), current =>
          [tangent, ...(current ?? []).filter(item => item.conversationId !== tangent.conversationId)]);
        setSelectedId(tangent.conversationId);
        setOpen(true);
      } catch (error) {
        toast.error("Couldn't start the tangent", { message: error instanceof Error ? error.message : String(error) });
      } finally {
        creatingRef.current = false;
        setCreating(false);
      }
    })();
  }, [conversationId, queryClient]);
  const context = useMemo(() => ({ create, creating }), [create, creating]);

  return (
    <TangentSelectionContext.Provider value={context}>
      <div className="conversation-with-tangents">
        <div className="tangent-main">
          <div className="flex h-9 shrink-0 items-center justify-between gap-2 border-b border-border-subtle px-3">
            <CitationVisibilityToggle />
            <button ref={toggleRef} type="button" aria-expanded={open} aria-controls={`tangents-${conversationId}`}
              title="Explore an idea alongside this conversation"
              onClick={() => { if (open) close(); else setOpen(true); }}
              className="inline-flex items-center gap-1.5 rounded px-2 py-1 text-xs text-text-secondary hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring">
              {creating ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <GitBranch className="h-3.5 w-3.5" aria-hidden="true" />}
              Tangents{tangents.data?.length ? ` · ${tangents.data.length}` : ''}
            </button>
          </div>
          {children}
        </div>
        {open && <aside id={`tangents-${conversationId}`} aria-label="Tangents" className="tangent-sidebar" onKeyDown={(event) => {
          if (event.key === 'Escape' && !event.defaultPrevented) { event.stopPropagation(); close(); }
        }}>
          <header className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle px-3">
            {selected && <IconButton label="All tangents" onClick={() => setSelectedId(null)}><ArrowLeft /></IconButton>}
            <GitBranch className="h-4 w-4 shrink-0 text-accent" aria-hidden="true" />
            <h2 className="min-w-0 flex-1 truncate text-sm font-medium">{selected ? 'Tangent' : 'Tangents'}</h2>
            <CitationVisibilityToggle compact className="h-8 min-h-8 w-8 justify-center px-0" />
            <IconButton label="Close tangents" onClick={close}><X /></IconButton>
          </header>
          {selected ? <TangentPanel key={selected.conversationId} tangent={selected} toolPreferences={toolPreferences}
            unavailable={unavailable} drafts={drafts.current} onPromoted={() => { setSelectedId(null); close(); }} /> : (
            <div className="min-h-0 flex-1 overflow-y-auto p-4">
              {tangents.isPending && <p role="status" className="text-sm text-text-muted">Loading tangents…</p>}
              {tangents.isError && <div role="alert" className="text-sm"><p>Couldn’t load tangents.</p><button type="button" className="mt-2 text-accent" onClick={() => void tangents.refetch()}>Try again</button></div>}
              {tangents.isSuccess && tangents.data.length === 0 && <div className="space-y-3 text-sm leading-relaxed text-text-muted">
                <p>Explore an idea without losing your place. Tangents stay here with this conversation.</p>
                <p>Choose <strong className="font-medium text-text-secondary">Tangent</strong> below any reply, or highlight a passage and choose <strong className="font-medium text-text-secondary">Ask in a tangent</strong>.</p>
              </div>}
              <div className="space-y-2">{tangents.data?.map(tangent => <button key={tangent.conversationId} type="button"
                className="w-full rounded-lg border border-border-subtle p-3 text-left hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                onClick={() => setSelectedId(tangent.conversationId)}>
                <span className="block truncate text-sm font-medium">{tangent.title}</span>
                <span className="mt-1 line-clamp-2 text-xs leading-relaxed text-text-muted">{tangent.selectedText}</span>
              </button>)}</div>
            </div>
          )}
        </aside>}
      </div>
    </TangentSelectionContext.Provider>
  );
}
