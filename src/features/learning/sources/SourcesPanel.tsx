import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { BookOpen, Check, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Clock3, ExternalLink, FileText, Globe2, Plus, RefreshCw, Search, ShieldAlert, Upload, X } from 'lucide-react';


import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { LearningDocumentPicker, type LearningDocumentSelection } from '@/features/learning/sources/LearningDocumentPicker';
import { SourceReader } from '@/features/learning/sources/SourceReader';
import {
  useAddLearningDocumentSource,
  useAddLearningTextSource,
  useAddLearningWebSource,
  useAdoptLearningSourceVersion,
  useLearningSourceVersion,
  useLearningSourceWorkspace,
  useRefreshLearningSource,
  useSearchLearningSources,
  useUpdateLearningSourcePolicy,
} from '@/features/learning/sources/useLearningSources';
import type {
  AddLearningDocumentSourceRequestDto,
  AddLearningTextSourceRequestDto,
  AddLearningWebSourceRequestDto,
  AdoptLearningSourceVersionRequestDto,
  LearningSourceLibraryItemDto,
  LearningSourcePolicy,
  RefreshLearningSourceRequestDto,
  UpdateLearningSourcePolicyRequestDto,
} from '@/lib/bindings';

type AddKind = 'web' | 'document' | 'paste';
type Filter = 'all' | 'updates' | 'pinned' | 'attention';
type RetryOperation = { kind: string; fingerprint: string; request: Record<string, unknown> };
const EMPTY_SOURCES: LearningSourceLibraryItemDto[] = [];
/** How many sources the Library list shows at once; the choice is kept across programs and restarts. */
export const SOURCE_PAGE_SIZES = [10, 25, 50, 100] as const;
export const SOURCE_PAGE_SIZE_KEY = 'studio.sources.pageSize';
const PAGER_BUTTON = 'grid h-6 w-6 shrink-0 place-items-center rounded-lg border border-border bg-background text-text-muted hover:text-text-primary disabled:opacity-40 disabled:hover:text-text-muted';

function readPageSize(): number {
  try {
    const saved = Number(localStorage.getItem(SOURCE_PAGE_SIZE_KEY));
    return (SOURCE_PAGE_SIZES as readonly number[]).includes(saved) ? saved : SOURCE_PAGE_SIZES[0];
  } catch { return SOURCE_PAGE_SIZES[0]; }
}
function writePageSize(size: number) {
  try { localStorage.setItem(SOURCE_PAGE_SIZE_KEY, String(size)); } catch { /* The choice still holds until the panel closes. */ }
}

function newId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (char) => {
    const random = Math.floor(Math.random() * 16);
    return (char === 'x' ? random : (random & 0x3) | 0x8).toString(16);
  });
}
function safeHref(value?: string | null) {
  if (!value) return undefined;
  try { const url = new URL(value); return ['https:', 'http:'].includes(url.protocol) ? url.href : undefined; }
  catch { return undefined; }
}
function date(value: number) { const parsed = new Date(value); return Number.isNaN(parsed.getTime()) ? 'Date unavailable' : parsed.toLocaleString(); }
function sourceTitle(source: LearningSourceLibraryItemDto) { return source.activeVersion?.title ?? source.pendingVersion?.title ?? source.requestedUrl ?? (source.kind === 'document' ? 'Library document' : 'Untitled source'); }
function statusLabel(source: LearningSourceLibraryItemDto) {
  if (source.pendingVersionId) return 'Update available';
  if (source.latestCheck?.status === 'failed') return 'Check failed';
  if (source.activeVersion?.truncated || source.activeVersion?.extractionVersion.toLowerCase().includes('legacy')) return 'Incomplete capture';
  if (source.freshnessPolicy === 'fixed') return 'Pinned snapshot';
  if (source.activeVersion) return 'Stored for offline reading';
  return 'No saved version';
}
function isLegacy(source: LearningSourceLibraryItemDto) { return Boolean(source.activeVersion?.truncated || source.activeVersion?.extractionVersion.toLowerCase().includes('legacy')); }
function isCasConflict(error?: string) { return Boolean(error && /(revision|compare.and.swap|conflict|changed elsewhere|source changed; reload and retry)/i.test(error)); }
function prettyPolicy(policy: LearningSourcePolicy) { return policy === 'before_use' ? 'Before use' : policy === 'fixed' ? 'Fixed snapshot' : 'Manual checks'; }

export function SourcesPanel({ programId, active = true }: { programId: string; active?: boolean }) {
  const workspace = useLearningSourceWorkspace(programId);
  const addWeb = useAddLearningWebSource();
  const addDocument = useAddLearningDocumentSource();
  const addText = useAddLearningTextSource();
  const refresh = useRefreshLearningSource();
  const adopt = useAdoptLearningSourceVersion();
  const updatePolicy = useUpdateLearningSourcePolicy();
  const [selectedId, setSelectedId] = useState('');
  const [selectedVersionId, setSelectedVersionId] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [query, setQuery] = useState('');
  const [searchText, setSearchText] = useState('');
  const [pageSize, setPageSize] = useState(readPageSize);
  // The page belongs to one filter and search; changing either starts again at page 1.
  const [listPage, setListPage] = useState(() => ({ key: `${filter}\n${query}`, page: 1 }));
  const [addOpen, setAddOpen] = useState(false);
  const [addKind, setAddKind] = useState<AddKind>('web');
  const [webUrl, setWebUrl] = useState('');
  const [webPolicy, setWebPolicy] = useState<LearningSourcePolicy>('manual');
  const [selectedDocuments, setSelectedDocuments] = useState<LearningDocumentSelection[]>([]);
  const documentId = selectedDocuments[0]?.id ?? '';
  const setDocumentId = () => setSelectedDocuments([]);
  const [pasteTitle, setPasteTitle] = useState('');
  const [pastePublisher, setPastePublisher] = useState('');
  const [pasteText, setPasteText] = useState('');
  const [formError, setFormError] = useState('');
  const [retryOperation, setRetryOperation] = useState<RetryOperation | null>(null);
  const [operationMessage, setOperationMessage] = useState('');
  const [reloadRequired, setReloadRequired] = useState(false);
  const retryRef = useRef<RetryOperation | null>(null);
  const preparedAddRef = useRef<RetryOperation | null>(null);
  const [mutationBusy, setMutationBusy] = useState(false);
  const mutationBusyRef = useRef(false);
  const addTriggerRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => { const timer = window.setTimeout(() => setQuery(searchText.trim()), 220); return () => window.clearTimeout(timer); }, [searchText]);
  useLayoutEffect(() => {
    if (!addOpen) return;
    // A new material type puts focus on its first field.
    dialogRef.current?.querySelector<HTMLElement>('[data-add-focus]')?.focus();
  }, [addOpen, addKind]);
  const search = useSearchLearningSources(query ? { programId, query, limit: 30 } : null);
  const sources = useMemo(() => workspace.data?.sources ?? EMPTY_SOURCES, [workspace.data?.sources]);
  useEffect(() => {
    if (!sources.length) { setSelectedId(''); setSelectedVersionId(''); return; }
    if (!sources.some((item) => item.id === selectedId)) {
      const first = sources[0]; setSelectedId(first.id); setSelectedVersionId(first.activeVersionId ?? first.pendingVersionId ?? '');
    }
  }, [sources, selectedId]);
  const selected = sources.find((source) => source.id === selectedId) ?? null;
  const selectedVersion = selected?.versions.find((item) => item.id === selectedVersionId) ?? null;
  const detailRequest = active && selected && selectedVersionId ? { programId, sourceId: selected.id, versionId: selectedVersionId } : null;
  const detail = useLearningSourceVersion(detailRequest);
  const visibleSources = useMemo(() => {
    const haystack = query ? new Set((search.data ?? []).map((result) => result.sourceId)) : null;
    return sources.filter((source) => {
      if (filter === 'updates' && !source.pendingVersionId) return false;
      if (filter === 'pinned' && source.freshnessPolicy !== 'fixed') return false;
      if (filter === 'attention' && source.latestCheck?.status !== 'failed' && !source.pendingVersionId && !isLegacy(source)) return false;
      return !haystack || haystack.has(source.id);
    });
  }, [filter, query, search.data, sources]);
  const listKey = `${filter}\n${query}`;
  if (listPage.key !== listKey) setListPage({ key: listKey, page: 1 });
  const pageCount = Math.max(1, Math.ceil(visibleSources.length / pageSize));
  const page = Math.min(listPage.key === listKey ? listPage.page : 1, pageCount);
  const pageStart = (page - 1) * pageSize;
  const pageSources = visibleSources.slice(pageStart, pageStart + pageSize);
  const goToPage = (next: number) => setListPage({ key: listKey, page: Math.min(Math.max(1, next), pageCount) });
  const changePageSize = (size: number) => {
    writePageSize(size);
    setPageSize(size);
    // Keep the first source on screen in view rather than jumping back to the start.
    setListPage({ key: listKey, page: Math.floor(pageStart / size) + 1 });
  };

  const counts = {
    downloaded: sources.filter((source) => Boolean(source.activeVersionId)).length,
    pinned: sources.filter((source) => source.freshnessPolicy === 'fixed').length,
    updates: sources.filter((source) => Boolean(source.pendingVersionId)).length,
    failed: sources.filter((source) => source.latestCheck?.status === 'failed').length,
    legacy: sources.filter(isLegacy).length,
  };

  const clearRetry = () => { retryRef.current = null; setRetryOperation(null); setOperationMessage(''); setReloadRequired(false); };
  const perform = async <T extends Record<string, unknown>>(
    kind: string,
    payload: Omit<T, 'operationId'> | T,
    call: (request: T) => Promise<unknown>,
  ) => {
    if (mutationBusyRef.current) return false;
    const { operationId: providedOperationId, ...requestPayload } = payload as T & { operationId?: string };
    const fingerprint = JSON.stringify(requestPayload);
    const previous = retryRef.current;
    const retry = previous?.kind === kind && previous.fingerprint === fingerprint
      ? previous
      : { kind, fingerprint, request: { ...requestPayload, operationId: providedOperationId ?? newId() } };
    retryRef.current = retry;
    setRetryOperation(retry);
    mutationBusyRef.current = true;
    setMutationBusy(true);
    setOperationMessage(''); setReloadRequired(false);
    try {
      await call(retry.request as T);
      clearRetry();
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOperationMessage(message);
      setReloadRequired(isCasConflict(message));
      return false;
    } finally {
      mutationBusyRef.current = false;
      setMutationBusy(false);
    }
  };

  const addRequest = <T extends Record<string, unknown>>(
    kind: string,
    input: Omit<T, 'operationId' | 'sourceId' | 'versionId'>,
  ) => {
    const fingerprint = JSON.stringify(input);
    let operation = preparedAddRef.current;
    if (!operation?.kind || operation.kind !== kind || operation.fingerprint !== fingerprint) {
      operation = { kind, fingerprint, request: { ...input, sourceId: newId(), versionId: newId(), operationId: newId() } };
      preparedAddRef.current = operation;
    }
    retryRef.current = { ...operation, fingerprint: JSON.stringify(Object.fromEntries(Object.entries(operation.request).filter(([key]) => key !== 'operationId'))) };
    return operation.request as T;
  };

  const openSource = (sourceId: string, versionId?: string) => {
    const item = sources.find((entry) => entry.id === sourceId);
    if (!item) return;
    setSelectedId(sourceId);
    setSelectedVersionId(versionId ?? item.activeVersionId ?? item.pendingVersionId ?? '');
    // Opened from a search match: turn the list to the page that holds it.
    const index = visibleSources.findIndex((entry) => entry.id === sourceId);
    if (index >= 0) goToPage(Math.floor(index / pageSize) + 1);
  };

  const submitAdd = async () => {
    setFormError('');
    if (addKind === 'web') {
      let url: URL;
      try { url = new URL(webUrl.trim()); } catch { setFormError('Enter a complete web address, such as https://example.org/article.'); return; }
      if (!['http:', 'https:'].includes(url.protocol)) { setFormError('Use an http or https web address.'); return; }
      const request = addRequest<AddLearningWebSourceRequestDto>('add-web', { programId, url: url.href, freshnessPolicy: webPolicy });
      const ok = await perform<AddLearningWebSourceRequestDto>('add-web', request, (body) => addWeb.mutateAsync(body as AddLearningWebSourceRequestDto));
      if (ok) { preparedAddRef.current = null; setWebUrl(''); setAddOpen(false); }
      return;
    }
    if (addKind === 'document') {
      if (!documentId) { setFormError('Choose a document from your Library.'); return; }
      const request = addRequest<AddLearningDocumentSourceRequestDto>('add-document', { programId, documentId });
      const ok = await perform<AddLearningDocumentSourceRequestDto>('add-document', request, (body) => addDocument.mutateAsync(body as AddLearningDocumentSourceRequestDto));
      if (ok) { preparedAddRef.current = null; setDocumentId(); setAddOpen(false); }
      return;
    }
    if (!pasteTitle.trim()) { setFormError('Give this saved text a title.'); return; }
    if (!pasteText.trim()) { setFormError('Paste some text to save.'); return; }
    const request = addRequest<AddLearningTextSourceRequestDto>('add-text', { programId, title: pasteTitle.trim(), publisher: pastePublisher.trim() || null, text: pasteText });
    const ok = await perform<AddLearningTextSourceRequestDto>('add-text', request, (body) => addText.mutateAsync(body as AddLearningTextSourceRequestDto));
    if (ok) { preparedAddRef.current = null; setPasteTitle(''); setPastePublisher(''); setPasteText(''); setAddOpen(false); }
  };

  const runRefresh = async (source: LearningSourceLibraryItemDto) => {
    const request: Omit<RefreshLearningSourceRequestDto, 'operationId'> = { programId, sourceId: source.id, expectedRevision: source.revision };
    await perform<RefreshLearningSourceRequestDto>(`refresh:${source.id}`, request, (body) => refresh.mutateAsync(body as RefreshLearningSourceRequestDto));
  };
  const runAdopt = async (source: LearningSourceLibraryItemDto) => {
    if (!source.pendingVersionId) return;
    const request: Omit<AdoptLearningSourceVersionRequestDto, 'operationId'> = { programId, sourceId: source.id, versionId: source.pendingVersionId, expectedRevision: source.revision };
    const ok = await perform<AdoptLearningSourceVersionRequestDto>(`adopt:${source.id}`, request, (body) => adopt.mutateAsync(body as AdoptLearningSourceVersionRequestDto));
    if (ok) setSelectedVersionId(source.pendingVersionId);
  };
  const runPolicy = async (source: LearningSourceLibraryItemDto, policy: LearningSourcePolicy) => {
    const request: Omit<UpdateLearningSourcePolicyRequestDto, 'operationId'> = { programId, sourceId: source.id, freshnessPolicy: policy, expectedRevision: source.revision };
    await perform<UpdateLearningSourcePolicyRequestDto>(`policy:${source.id}`, request, (body) => updatePolicy.mutateAsync(body as UpdateLearningSourcePolicyRequestDto));
  };

  if (workspace.isLoading) return <div role="status" className="rounded-2xl border border-border bg-surface p-8 text-sm text-text-muted">Opening the source library…</div>;
  if (workspace.error) return <section className="rounded-2xl border border-border bg-surface p-8"><h3 className="font-serif text-2xl text-text-primary">The source library could not be opened</h3><p role="alert" className="mt-2 text-sm text-rose-700">{workspace.error.message}</p><button type="button" onClick={() => void workspace.refetch()} className="mt-4 rounded-full border border-accent/40 px-4 py-2 text-sm text-accent">Retry</button></section>;

  return <div className="min-w-0" data-testid="learning-sources-workspace">
    <header className="mb-4 flex flex-wrap items-end justify-between gap-4 rounded-2xl border border-[#d7c9b8] bg-[#eee8df] p-5 sm:p-7 dark:border-white/10 dark:bg-[#28251f]">
      <div><p className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Program materials</p><h2 className="mt-1 font-serif text-2xl text-text-primary sm:text-3xl">Sources</h2><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Build the reference collection used to write and check your lessons. Saved versions remain available for offline reading.</p></div>
      <button ref={addTriggerRef} type="button" disabled={mutationBusy} onClick={() => { setFormError(''); setOperationMessage(''); setAddOpen(true); }} className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-sm font-semibold text-accent-fg disabled:opacity-50"><Plus size={16} /> Add material</button>
      <div className="grid w-full grid-cols-2 gap-2 sm:grid-cols-5">{[
        ['Downloaded', counts.downloaded], ['Pinned', counts.pinned], ['Update available', counts.updates], ['Failed checks', counts.failed], ['Incomplete captures', counts.legacy],
      ].map(([label, count]) => <div key={label} className="rounded-xl border border-white/70 bg-white/50 px-3 py-2.5 dark:border-white/10 dark:bg-black/10"><div className="text-[9px] font-semibold uppercase tracking-wider text-text-muted">{label}</div><div className="mt-1 font-serif text-xl tabular-nums text-text-primary">{count}</div></div>)}</div>
    </header>

    {(operationMessage || reloadRequired) && <div role="alert" className="mb-4 rounded-xl border border-rose-500/20 bg-rose-500/5 p-4 text-sm text-rose-800"><p>{reloadRequired ? 'This source changed elsewhere. Reload the latest version before trying again.' : operationMessage}</p><div className="mt-3 flex flex-wrap gap-2">{retryOperation && !reloadRequired && <button type="button" onClick={() => void replay()} className="rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg">Retry same operation</button>}{reloadRequired && <button type="button" onClick={() => { clearRetry(); void workspace.refetch(); }} className="rounded-full border border-rose-700/30 px-4 py-2 text-xs font-semibold">Reload sources</button>}</div></div>}

    <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(230px,0.36fr)_minmax(0,1fr)]">
      <aside className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-5">
        <div className="flex flex-wrap items-center justify-between gap-2"><h3 className="font-serif text-xl text-text-primary">Library</h3><span className="text-xs text-text-muted">{sources.length} {sources.length === 1 ? 'source' : 'sources'}</span></div>
        <label className="mt-4 flex items-center gap-2 rounded-xl border border-border bg-background px-3 py-2.5"><Search size={15} className="shrink-0 text-text-muted" /><input aria-label="Search saved source text" value={searchText} onChange={(event) => setSearchText(event.target.value)} placeholder="Search saved text" className="min-w-0 flex-1 bg-transparent text-sm outline-hidden placeholder:text-text-muted" /></label>
        {query && <p className="mt-2 text-[10px] text-text-muted">Searching saved versions only · never opens the web</p>}
        <div className="mt-3 flex flex-wrap gap-1.5" aria-label="Source filters">{([['all', 'All'], ['updates', 'Updates'], ['pinned', 'Pinned'], ['attention', 'Attention']] as const).map(([value, label]) => <button key={value} type="button" aria-pressed={filter === value} onClick={() => setFilter(value)} className={`rounded-full px-3 py-1.5 text-[10px] font-medium ${filter === value ? 'bg-accent text-accent-fg' : 'bg-background text-text-muted hover:text-text-primary'}`}>{label}</button>)}</div>
        {query && search.isLoading && <p role="status" className="mt-4 text-xs text-text-muted">Searching saved text…</p>}
        {query && search.error && <div className="mt-4 rounded-lg bg-rose-500/5 p-3 text-xs text-rose-700"><p role="alert">Search failed: {search.error.message}</p><button type="button" onClick={() => void search.refetch()} className="mt-2 underline">Retry search</button></div>}
        {visibleSources.length > SOURCE_PAGE_SIZES[0] && <div className="mt-3 flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
          <span role="status" className="text-[11px] tabular-nums text-text-muted">{pageStart + 1}–{pageStart + pageSources.length} of {visibleSources.length}</span>
          <nav aria-label="Source pages" className="flex items-center gap-0.5">
            <button type="button" aria-label="First page" title="First page" disabled={page === 1} onClick={() => goToPage(1)} className={PAGER_BUTTON}><ChevronsLeft size={14} /></button>
            <button type="button" aria-label="Previous page" title="Previous page" disabled={page === 1} onClick={() => goToPage(page - 1)} className={PAGER_BUTTON}><ChevronLeft size={14} /></button>
            <button type="button" aria-label="Next page" title="Next page" disabled={page === pageCount} onClick={() => goToPage(page + 1)} className={PAGER_BUTTON}><ChevronRight size={14} /></button>
            <button type="button" aria-label="Last page" title="Last page" disabled={page === pageCount} onClick={() => goToPage(pageCount)} className={PAGER_BUTTON}><ChevronsRight size={14} /></button>
          </nav>
          <label className="ml-auto flex items-center gap-1.5 text-[11px] text-text-muted">Show<select aria-label="Sources per page" value={pageSize} onChange={(event) => changePageSize(Number(event.target.value))} className="rounded-lg border border-border bg-background px-1.5 py-1 text-[11px] text-text-primary">{SOURCE_PAGE_SIZES.map((size) => <option key={size} value={size}>{size}</option>)}</select>per page</label>
        </div>}
        <div className="mt-3 space-y-1.5" aria-label="Source list">{pageSources.map((source) => {
          const title = sourceTitle(source); const status = statusLabel(source); const active = selectedId === source.id;
          return <button key={source.id} type="button" onClick={() => openSource(source.id)} aria-current={active ? 'true' : undefined} className={`w-full min-w-0 rounded-xl border px-3 py-3 text-left transition ${active ? 'border-accent/45 bg-accent/5' : 'border-transparent hover:border-border hover:bg-background'}`}><span className="flex min-w-0 items-center gap-2"><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-background text-accent">{source.kind === 'web' ? <Globe2 size={15} /> : source.kind === 'document' ? <BookOpen size={15} /> : <FileText size={15} />}</span><span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium text-text-primary">{title}</span><span className="mt-1 block truncate text-[10px] text-text-muted">{status}</span></span><ChevronRight size={14} className="shrink-0 text-text-muted" /></span></button>;
        })}{!visibleSources.length && <p className="rounded-xl bg-background p-4 text-xs leading-5 text-text-muted">{sources.length ? query ? 'No saved text matches this search and filter.' : 'No sources match this filter.' : 'No materials have been saved to this program yet.'}</p>}</div>
      </aside>

      <main className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-6">
        {!selected ? <div className="grid min-h-64 place-items-center rounded-xl border border-dashed border-border bg-background p-8 text-center"><div><BookOpen className="mx-auto text-accent" /><h3 className="mt-3 font-serif text-2xl text-text-primary">A library for this course</h3><p className="mt-2 max-w-sm text-sm leading-6 text-text-muted">Add a web page, a Library document, or your own text to build a source record with readable history.</p><button type="button" onClick={() => setAddOpen(true)} className="mt-4 rounded-full bg-accent px-4 py-2 text-sm font-semibold text-accent-fg">Add first material</button></div></div> : <>
          <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2 text-[10px] font-semibold uppercase tracking-[.14em] text-accent">{selected.kind} <span className="text-text-muted">· {prettyPolicy(selected.freshnessPolicy)}</span></div><h3 className="mt-1 wrap-break-word font-serif text-2xl text-text-primary">{selectedVersion?.title ?? sourceTitle(selected)}</h3>{selectedVersion?.publisher && <p className="mt-1 text-xs text-text-secondary">{selectedVersion.publisher}</p>}</div><div className="flex flex-wrap items-center gap-2">{safeHref(selectedVersion?.resolvedUrl ?? selected.requestedUrl) && <a href={safeHref(selectedVersion?.resolvedUrl ?? selected.requestedUrl)} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-2 text-xs text-text-secondary"><ExternalLink size={13} /> Open reference</a>}{selected.kind === 'web' && <button type="button" disabled={mutationBusy || selected.freshnessPolicy === 'fixed'} onClick={() => void runRefresh(selected)} className="inline-flex items-center gap-1.5 rounded-full border border-accent/40 px-3 py-2 text-xs font-medium text-accent disabled:opacity-45"><RefreshCw size={13} /> Check for changes</button>}</div></div>
          {selected.kind === 'web' && <div className="mt-4 flex min-w-0 flex-wrap items-center gap-3 rounded-xl bg-background p-3"><label htmlFor="source-policy" className="w-full text-xs font-medium text-text-secondary sm:w-auto">Freshness policy</label><select id="source-policy" disabled={mutationBusy} value={selected.freshnessPolicy} onChange={(event) => void runPolicy(selected, event.target.value as LearningSourcePolicy)} className="w-full min-w-0 max-w-full rounded-lg border border-border bg-surface px-3 py-2 text-xs disabled:opacity-50 sm:w-auto"><option value="fixed">Fixed snapshot · pinned</option><option value="manual">Manual checks</option><option value="before_use">Before use</option></select><span className="min-w-0 flex-1 text-[10px] leading-4 text-text-muted">Fixed pins this edition. Manual lets you decide when to check. Before use checks web sources before lesson preparation. A check cannot prove when the publisher last changed a page.</span></div>}
          {selected.pendingVersion && <section aria-label="Available update" className="mt-4 rounded-xl border border-amber-500/25 bg-amber-500/5 p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="text-[10px] font-semibold uppercase tracking-wider text-amber-800">Update available · saved edition stays active</div><p className="mt-1 text-sm leading-5 text-text-secondary">Compare the saved text before choosing whether future lesson preparation should use this version.</p></div><button type="button" disabled={mutationBusy} onClick={() => setSelectedVersionId(selected.pendingVersionId!)} className="rounded-full border border-amber-700/25 px-3 py-2 text-xs font-medium text-amber-900 disabled:opacity-50">Review update</button></div><div className="mt-3 grid gap-3 sm:grid-cols-2"><div className="rounded-lg border border-border bg-surface p-3"><div className="text-[9px] font-semibold uppercase tracking-wider text-text-muted">Current edition · v{selected.activeVersion?.versionNumber ?? '—'}</div><p className="mt-2 line-clamp-4 whitespace-pre-wrap text-xs leading-5 text-text-secondary">{selected.activeVersion?.excerpt ?? 'No active version'}</p></div><div className="rounded-lg border border-amber-600/20 bg-surface p-3"><div className="text-[9px] font-semibold uppercase tracking-wider text-amber-800">New representation · v{selected.pendingVersion.versionNumber}</div><p className="mt-2 line-clamp-4 whitespace-pre-wrap text-xs leading-5 text-text-secondary">{selected.pendingVersion.excerpt}</p></div></div>{selectedVersionId === selected.pendingVersion.id && <button type="button" disabled={mutationBusy} onClick={() => void runAdopt(selected)} className="mt-3 inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-50"><Check size={14} /> Adopt this version</button>}</section>}
          {selected.latestCheck && <p className={`mt-3 flex items-start gap-2 rounded-lg p-3 text-xs leading-5 ${selected.latestCheck.status === 'failed' ? 'bg-rose-500/5 text-rose-800' : 'bg-background text-text-muted'}`}><Clock3 size={14} className="mt-0.5 shrink-0" /><span>{selected.latestCheck.status === 'failed' ? `Last check failed · ${selected.latestCheck.message ?? 'The saved version was kept.'}` : selected.latestCheck.status === 'update_available' ? selected.pendingVersionId ? 'A newer representation is waiting for your review.' : 'A changed representation was adopted; this is the result of that earlier check.' : 'Last check found the same saved text.'} Checked {date(selected.latestCheck.checkedAt)}. This is an acquisition time, not a publisher update time.</span>{selected.latestCheck.status === 'failed' && selected.kind === 'web' && <button type="button" disabled={mutationBusy} onClick={() => void runRefresh(selected)} className="shrink-0 underline disabled:opacity-50">Retry</button>}</p>}
          {isLegacy(selected) && <p className="mt-3 flex gap-2 rounded-lg bg-amber-500/5 p-3 text-xs leading-5 text-amber-900"><ShieldAlert size={14} className="mt-0.5 shrink-0" />This source began as a legacy bounded excerpt. The original complete text was not preserved, so this record may be truncated.</p>}
          <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3"><div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-text-muted">Saved edition</div><div className="mt-1 text-xs text-text-secondary">{selectedVersion ? `Version ${selectedVersion.versionNumber} · captured ${date(selectedVersion.acquiredAt)}` : 'Choose a saved version'}</div></div><label className="flex items-center gap-2 text-xs text-text-muted">Version history<select aria-label="Source version history" value={selectedVersionId} onChange={(event) => setSelectedVersionId(event.target.value)} className="max-w-[210px] rounded-lg border border-border bg-background px-2 py-2 text-xs text-text-primary">{selected.versions.map((version) => <option key={version.id} value={version.id}>v{version.versionNumber} · {version.title}</option>)}</select></label></div>
          {detail.isLoading ? <p role="status" className="py-8 text-sm text-text-muted">Opening saved version…</p> : detail.error ? <div className="py-8"><p role="alert" className="text-sm text-rose-700">This saved version could not be opened: {detail.error.message}</p><button type="button" onClick={() => void detail.refetch()} className="mt-3 text-xs text-accent underline">Retry</button></div> : detail.data ? <>
            {detail.data.version.truncated && <p className="mt-4 rounded-lg bg-amber-500/5 p-3 text-xs text-amber-900">This saved text is bounded and may omit material after the captured limit.</p>}
            <SourceReader key={detail.data.version.id} fullText={detail.data.fullText} extractionVersion={detail.data.version.extractionVersion} wordCount={detail.data.version.wordCount} />
            <div className="mt-4 grid gap-3 sm:grid-cols-2"><section className="rounded-xl bg-background p-4"><h4 className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Provenance</h4><dl className="mt-2 space-y-1.5 text-xs text-text-secondary"><div><dt className="inline text-text-muted">Requested: </dt><dd className="inline break-all">{selected.requestedUrl ?? selected.origin}</dd></div><div><dt className="inline text-text-muted">Resolved: </dt><dd className="inline break-all">{detail.data.version.resolvedUrl ?? 'Not applicable'}</dd></div><div><dt className="inline text-text-muted">Captured: </dt><dd className="inline">{date(detail.data.version.acquiredAt)}</dd></div><div><dt className="inline text-text-muted">Words: </dt><dd className="inline">{detail.data.version.wordCount.toLocaleString()}</dd></div><div><dt className="inline text-text-muted">SHA-256: </dt><dd className="inline break-all font-mono text-[10px]">{detail.data.version.contentSha256}</dd></div><div><dt className="inline text-text-muted">Extractor: </dt><dd className="inline">{detail.data.version.extractionVersion}</dd></div></dl></section><section className="rounded-xl bg-background p-4"><h4 className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Used in this program</h4>{detail.data.usage.length ? <ul className="mt-2 space-y-2">{detail.data.usage.map((usage, index) => <li key={`${usage.lessonId}-${usage.referenceKind}-${index}`} className="border-l-2 border-accent/35 pl-3"><p className="text-xs font-medium text-text-primary">{usage.lessonTitle}</p><p className="mt-0.5 text-[10px] text-text-muted">{usage.referenceKind}: {usage.referenceTitle}</p></li>)}</ul> : <p className="mt-2 text-xs leading-5 text-text-muted">No current lesson or assessment references this version.</p>}</section></div>
          </> : <p className="py-8 text-sm text-text-muted">Select a version to read.</p>}
        </>}
      </main>
    </div>

    {query && search.data?.length ? <section className="mt-4 rounded-2xl border border-border bg-surface p-4 sm:p-5"><h3 className="text-xs font-semibold uppercase tracking-wider text-text-muted">Matches in saved text</h3><div className="mt-3 grid gap-2 md:grid-cols-2">{search.data.map((result) => <button type="button" key={`${result.sourceId}-${result.versionId}`} onClick={() => openSource(result.sourceId, result.versionId)} className="rounded-xl border border-border bg-background p-3 text-left hover:border-accent/40"><span className="block text-xs font-medium text-text-primary">{result.title}</span><span className="mt-1 line-clamp-3 block whitespace-pre-wrap text-xs leading-5 text-text-secondary">{result.excerpt}</span></button>)}</div></section> : null}

    <Dialog open={addOpen} onOpenChange={(open) => { if (!open) setAddOpen(false); }}><DialogContent ref={dialogRef} unstyled hideClose aria-describedby={undefined} tabIndex={-1} overlayClassName="fixed inset-0 z-50 bg-black/45" className="fixed bottom-0 left-1/2 z-50 max-h-[92dvh] w-full max-w-2xl overflow-auto rounded-t-2xl border border-border bg-surface p-5 shadow-2xl [translate:-50%_0] sm:bottom-auto sm:top-1/2 sm:w-[calc(100%-32px)] sm:rounded-2xl sm:p-7 sm:[translate:-50%_-50%]" onOpenAutoFocus={(event) => { event.preventDefault(); dialogRef.current?.querySelector<HTMLElement>('[data-add-focus]')?.focus(); }} onCloseAutoFocus={(event) => { event.preventDefault(); addTriggerRef.current?.focus(); }} onEscapeKeyDown={(event) => { if (mutationBusyRef.current) event.preventDefault(); }} onPointerDownOutside={(event) => { if (mutationBusyRef.current) event.preventDefault(); }}><div className="flex items-start justify-between gap-4"><div><div className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Add to this program</div><DialogTitle asChild><h3 className="mt-1 font-serif text-2xl text-text-primary">Bring in a source</h3></DialogTitle><p className="mt-2 text-xs leading-5 text-text-muted">Sources are saved as immutable versions. Reading and search use the captured text.</p></div><button type="button" aria-label="Close add material" disabled={mutationBusy} onClick={() => setAddOpen(false)} className="rounded-full p-2 text-text-muted hover:bg-background disabled:opacity-50"><X size={17} /></button></div>
      <div className="mt-5 grid grid-cols-3 gap-2" role="group" aria-label="Material type">{([['web', 'Web page'], ['document', 'Library document'], ['paste', 'Paste text']] as const).map(([kind, label]) => <button key={kind} type="button" disabled={mutationBusy} aria-pressed={addKind === kind} onClick={() => { setAddKind(kind); setFormError(''); }} className={`rounded-xl border px-2 py-3 text-xs font-medium disabled:opacity-50 ${addKind === kind ? 'border-accent/40 bg-accent/5 text-accent' : 'border-border bg-background text-text-muted'}`}>{label}</button>)}</div>
      {addKind === 'web' && <div className="mt-5 space-y-4"><p className="rounded-xl bg-background p-3 text-xs leading-5 text-text-secondary">Lattice captures readable page text for this program. The reader never fetches the page in your browser; you can open the canonical reference separately.</p><label className="block text-xs font-medium text-text-secondary" htmlFor="source-url">Web address<input id="source-url" data-add-focus type="url" value={webUrl} onChange={(event) => setWebUrl(event.target.value)} placeholder="https://example.org/article" className="mt-2 w-full rounded-xl border border-border bg-background px-3 py-3 text-sm" /></label><label className="block text-xs font-medium text-text-secondary" htmlFor="new-source-policy">Freshness policy<select id="new-source-policy" value={webPolicy} onChange={(event) => setWebPolicy(event.target.value as LearningSourcePolicy)} className="mt-2 w-full min-w-0 max-w-full rounded-xl border border-border bg-background px-3 py-3 text-sm"><option value="fixed">Fixed snapshot · intentionally pinned</option><option value="manual">Manual · I decide when to check</option><option value="before_use">Before use · check before lesson preparation</option></select></label></div>}
      {addKind === 'document' && <div className="mt-5 space-y-3"><p className="rounded-xl bg-background p-3 text-xs leading-5 text-text-secondary">Choose an indexed document from a Space. Studio saves a snapshot for this course; later changes to the document do not rewrite it.</p><LearningDocumentPicker selected={selectedDocuments} onChange={setSelectedDocuments} limit={1} /></div>}
      {addKind === 'paste' && <div className="mt-5 space-y-3"><p className="rounded-xl bg-background p-3 text-xs leading-5 text-text-secondary">Paste notes, excerpts, or other material you have permission to use. The complete text is saved as a fixed snapshot and searched in passages when preparing lessons.</p><label className="block text-xs font-medium text-text-secondary" htmlFor="paste-source-title">Title<input id="paste-source-title" data-add-focus value={pasteTitle} onChange={(event) => setPasteTitle(event.target.value)} maxLength={180} className="mt-1.5 w-full rounded-xl border border-border bg-background px-3 py-2.5 text-sm" /></label><label className="block text-xs font-medium text-text-secondary" htmlFor="paste-source-publisher">Publisher or origin <span className="font-normal text-text-muted">(optional)</span><input id="paste-source-publisher" value={pastePublisher} onChange={(event) => setPastePublisher(event.target.value)} maxLength={180} className="mt-1.5 w-full rounded-xl border border-border bg-background px-3 py-2.5 text-sm" /></label><label className="block text-xs font-medium text-text-secondary" htmlFor="paste-source-text">Saved text<textarea id="paste-source-text" value={pasteText} onChange={(event) => setPasteText(event.target.value)} rows={8} className="mt-1.5 w-full resize-y rounded-xl border border-border bg-background px-3 py-2.5 text-sm leading-6" /></label><div className="text-right text-[10px] tabular-nums text-text-muted">{Array.from(pasteText).length.toLocaleString()} characters</div></div>}
      {formError && <p role="alert" className="mt-4 rounded-lg bg-rose-500/5 p-3 text-xs text-rose-700">{formError}</p>}{operationMessage && retryOperation?.kind.startsWith('add-') && <p role="alert" className="mt-3 text-xs text-rose-700">{operationMessage} You can retry the same submission without creating a duplicate.</p>}
      <div className="mt-6 flex flex-wrap justify-end gap-2 border-t border-border pt-4"><button type="button" disabled={mutationBusy} onClick={() => setAddOpen(false)} className="rounded-full border border-border px-4 py-2.5 text-xs text-text-secondary disabled:opacity-50">Cancel</button><button type="button" disabled={mutationBusy} onClick={() => void submitAdd()} className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-50"><Upload size={14} />{mutationBusy ? 'Saving…' : retryOperation?.kind.startsWith('add-') ? 'Retry save source' : 'Save source'}</button></div>
    </DialogContent></Dialog>
  </div>;

  async function replay() {
    const item = retryRef.current;
    if (!item || mutationBusyRef.current) return;
    mutationBusyRef.current = true;
    setMutationBusy(true);
    try {
      const sourceId = String(item.request.sourceId ?? '');
      const table: Record<string, (request: Record<string, unknown>) => Promise<unknown>> = {
        'add-web': (request) => addWeb.mutateAsync(request as AddLearningWebSourceRequestDto),
        'add-document': (request) => addDocument.mutateAsync(request as AddLearningDocumentSourceRequestDto),
        'add-text': (request) => addText.mutateAsync(request as AddLearningTextSourceRequestDto),
        [`refresh:${sourceId}`]: (request) => refresh.mutateAsync(request as unknown as RefreshLearningSourceRequestDto),
        [`adopt:${sourceId}`]: (request) => adopt.mutateAsync(request as unknown as AdoptLearningSourceVersionRequestDto),
        [`policy:${sourceId}`]: (request) => updatePolicy.mutateAsync(request as unknown as UpdateLearningSourcePolicyRequestDto),
      };
      const call = table[item.kind];
      if (!call) return;
      await call(item.request);
      clearRetry();
      if (item.kind.startsWith('add-')) setAddOpen(false);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setOperationMessage(message); setReloadRequired(isCasConflict(message));
    } finally {
      mutationBusyRef.current = false;
      setMutationBusy(false);
    }
  }
}
