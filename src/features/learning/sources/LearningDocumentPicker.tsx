import { useId, useState } from 'react';

import { useQuery } from '@tanstack/react-query';
import { BookOpen, Search, X } from 'lucide-react';

import { conversationKeys } from '@/hooks/queries/conversationKeys';
import VaultAPI from '@/lib/api';
import { useConversationUiStore } from '@/stores/conversationUiStore';

export type LearningDocumentSelection = { id: string; title: string; spaceName: string };

export function LearningDocumentPicker({ selected, onChange, limit }: {
  selected: LearningDocumentSelection[];
  onChange: (documents: LearningDocumentSelection[]) => void;
  limit?: number;
}) {
  const id = useId();
  const initialSpace = useConversationUiStore((state) => state.selectedSpaceId);
  const [spaceId, setSpaceId] = useState(initialSpace ?? 'space_general');
  const [search, setSearch] = useState('');
  const spaces = useQuery({
    queryKey: conversationKeys.spaces,
    queryFn: async () => {
      const result = await VaultAPI.listConversationSpaces();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
  });
  const documents = useQuery({
    queryKey: ['learning-space-documents', spaceId, search.trim()],
    queryFn: async () => {
      const result = await VaultAPI.listSpaceDocuments(spaceId, null, search.trim(), 50);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    // Never show results from the previous Space while the new one loads.
    retry: false,
  });
  const spaceName = spaces.data?.find((space) => space.id === spaceId)?.name ?? (spaceId === 'space_general' ? 'General' : 'Selected Space');
  const choices = spaces.data ?? [];
  return <div className="space-y-3">
    <div className="grid gap-3 sm:grid-cols-[minmax(140px,1fr)_minmax(0,2fr)]">
      <label htmlFor={`${id}-space`} className="text-xs font-medium text-text-secondary">Document Space
        <select data-add-focus id={`${id}-space`} value={spaceId} onChange={(event) => { setSpaceId(event.target.value); setSearch(''); }} className="mt-1.5 min-h-11 w-full rounded-xl border border-border bg-background px-3 text-sm">
          {!choices.some((space) => space.id === 'space_general') && <option value="space_general">General</option>}
          {!choices.some((space) => space.id === spaceId) && spaceId !== 'space_general' && <option value={spaceId}>{spaceName}</option>}
          {choices.map((space) => <option key={space.id} value={space.id}>{space.name}</option>)}
        </select>
      </label>
      <label htmlFor={`${id}-search`} className="text-xs font-medium text-text-secondary">Find a document
        <div className="mt-1.5 flex min-h-11 items-center gap-2 rounded-xl border border-border bg-background px-3"><Search size={15} aria-hidden="true" /><input id={`${id}-search`} value={search} onChange={(event) => setSearch(event.target.value)} placeholder={`Search ${spaceName}`} className="min-w-0 flex-1 bg-transparent text-sm outline-hidden" /></div>
      </label>
    </div>
    <p className="text-xs leading-5 text-text-muted">Showing documents available to <strong className="font-medium text-text-secondary">{spaceName}</strong>. Only checked documents are used. You can choose materials from several Spaces.</p>
    {spaces.isError && <p role="alert" className="text-xs text-rose-700">Spaces could not be loaded. <button type="button" onClick={() => void spaces.refetch()} className="underline">Retry Spaces</button></p>}
    {documents.isLoading ? <p role="status" className="py-4 text-sm text-text-muted">Loading documents from {spaceName}…</p> : documents.isError ? <p role="alert" className="rounded-xl bg-rose-500/5 p-3 text-sm text-rose-700">{documents.error.message} <button type="button" onClick={() => void documents.refetch()} className="underline">Retry documents</button></p> : <div className="max-h-60 overflow-y-auto rounded-xl border border-border bg-background/40 p-2">
      {documents.data?.length ? documents.data.map((document) => {
        const checked = selected.some((item) => item.id === document.documentId);
        return <label key={document.documentId} className="flex min-h-11 cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-sm hover:bg-background"><input type="checkbox" checked={checked} disabled={!checked && limit != null && selected.length >= limit} onChange={(event) => onChange(event.target.checked ? [...selected, { id: document.documentId, title: document.fileName, spaceName }] : selected.filter((item) => item.id !== document.documentId))} className="accent-accent" /><BookOpen size={15} className="shrink-0 text-text-muted" /><span className="min-w-0 wrap-break-word text-text-secondary">{document.fileName}</span></label>;
      }) : <p className="p-3 text-sm leading-6 text-text-muted">{search ? 'No matching documents in this Space. Try another search or Space.' : 'No indexed documents in this Space. Choose another Space, add a URL, or start from your topic.'}</p>}
    </div>}
    {documents.data?.length === 50 && <p className="text-xs text-text-muted">Showing the first 50 matches. Search to find more documents.</p>}
    {selected.length > 0 && <div aria-label="Selected course documents" className="rounded-xl border border-accent/20 bg-accent/5 p-3"><p className="mb-2 text-xs font-semibold text-text-primary">{selected.length}{limit != null ? ` of ${limit}` : ''} documents selected</p><div className="flex flex-wrap gap-2">{selected.map((document) => <button key={document.id} type="button" onClick={() => onChange(selected.filter((item) => item.id !== document.id))} aria-label={`Remove ${document.title}`} className="inline-flex min-h-9 max-w-full items-center gap-2 rounded-lg border border-border bg-surface px-3 py-1 text-left text-xs text-text-secondary"><span className="min-w-0 wrap-break-word">{document.title} <span className="text-text-muted">· {document.spaceName}</span></span><X size={12} className="shrink-0" /></button>)}</div></div>}
  </div>;
}
