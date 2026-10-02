import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { AlertCircle, ArrowDownToLine, Check, ChevronDown, CircleHelp, Clock3, FileJson2, ImageDown, LoaderCircle, Plus, RotateCcw, Save, Sparkles, Text } from 'lucide-react';

import type { JsonValue, LearningCanvasDto , LearningCanvasSnapshotDto } from '@/lib/bindings';
import { registerPendingSave } from '@/lib/pendingSaves';

import { canvasTextOutline, parseCanvasScene, serializeCanvasScene } from './canvasScene';
import { useCreateLearningCanvas, useCreateLearningCanvasSnapshot, useLearningCanvas, useRestoreLearningCanvasSnapshot, useSaveLearningCanvas } from './useLearningCanvas';

import type { CanvasSurfaceHandle } from './CanvasSurface';
import type { Theme } from '@excalidraw/excalidraw/element/types';

const LazyCanvasSurface = lazy(() => import('./CanvasSurface'));

type SceneUpdate = { sceneJson: string; outline: string[]; elementCount: number };
type Editable = { title: string; description: string; sceneJson: string; outline: string[]; elementCount: number };
type StableSave = { request: import('@/lib/bindings').SaveLearningCanvasRequestDto; payload: Editable };

function sceneString(value: JsonValue): string { return JSON.stringify(value); }
function makeId() { return crypto.randomUUID(); }
function initialEditable(canvas: LearningCanvasDto): Editable {
  const sceneJson = sceneString(canvas.sceneJson);
  try { const scene = parseCanvasScene(sceneJson); return { title: canvas.title, description: canvas.description, sceneJson, outline: canvasTextOutline(scene.elements), elementCount: canvas.elementCount }; }
  catch { return { title: canvas.title, description: canvas.description, sceneJson, outline: [], elementCount: canvas.elementCount }; }
}
function samePayload(a: Editable | null, b: Editable) { return Boolean(a?.title.trim() === b.title.trim() && a.description.trim() === b.description.trim() && a.sceneJson === b.sceneJson); }
function timestamp(value: number) { const date = new Date(value); return Number.isNaN(date.getTime()) ? 'Date unavailable' : date.toLocaleString(); }
function saveConflict(error: unknown) { return /revision|conflict|changed since|stale/i.test(error instanceof Error ? error.message : String(error)); }

export default function CanvasPanel({ programId, lessonId, lessonTitle, theme, enabled }: { programId: string; lessonId?: string; lessonTitle: string; theme: Theme; enabled: boolean }) {
  const query = useLearningCanvas(programId);
  const create = useCreateLearningCanvas();
  const save = useSaveLearningCanvas();
  const checkpoint = useCreateLearningCanvasSnapshot();
  const restore = useRestoreLearningCanvasSnapshot();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraftState] = useState<Editable | null>(null);
  const draftRef = useRef<Editable | null>(null);
  const savedRef = useRef<Editable | null>(null);
  const revisionRef = useRef<number>(0);
  const selectedRef = useRef<string | null>(null);
  const surfaceRef = useRef<CanvasSurfaceHandle | null>(null);
  const drainRef = useRef<() => Promise<boolean>>(async () => true);
  const inFlight = useRef<Promise<boolean> | null>(null);
  const retrySave = useRef<StableSave | null>(null);
  const sceneErrorRef = useRef<string | null>(null);
  const conflictRef = useRef(false);
  const restoreCancelRef = useRef<HTMLButtonElement | null>(null);
  const createRetry = useRef<import('@/lib/bindings').CreateLearningCanvasRequestDto | null>(null);
  const checkpointRetry = useRef<import('@/lib/bindings').CreateLearningCanvasSnapshotRequestDto | null>(null);
  const restoreRetry = useRef<import('@/lib/bindings').RestoreLearningCanvasSnapshotRequestDto | null>(null);
  const [createTitle, setCreateTitle] = useState('');
  const [createDescription, setCreateDescription] = useState('');
  const [creationOpen, setCreationOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [sceneError, setSceneError] = useState<string | null>(null);
  const [imageBlocked, setImageBlocked] = useState(false);
  const [saveState, setSaveState] = useState<'saved' | 'saving' | 'error'>('saved');
  const [checkpointName, setCheckpointName] = useState('');
  const [confirmRestore, setConfirmRestore] = useState<LearningCanvasSnapshotDto | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [surfaceVersion, setSurfaceVersion] = useState(0);
  const [outlineOpen, setOutlineOpen] = useState(true);

  const workspace = query.data;
  const canvases = useMemo(() => workspace?.canvases ?? [], [workspace?.canvases]);
  const selected = canvases.find((canvas) => canvas.id === selectedId) ?? null;
  selectedRef.current = selectedId;

  const setDraft = useCallback((value: Editable | null) => { draftRef.current = value; setDraftState(value); }, []);
  const saveDraft = useCallback((next: Partial<Editable>) => {
    const current = draftRef.current;
    if (!current) return;
    const changed = Object.entries(next).some(([key, value]) => {
      const previous = current[key as keyof Editable];
      return Array.isArray(value) && Array.isArray(previous)
        ? value.length !== previous.length || value.some((item, index) => item !== previous[index])
        : value !== previous;
    });
    if (!changed) return;
    const value = { ...current, ...next };
    draftRef.current = value;
    setDraftState(value);
    checkpointRetry.current = null;
    setError(null);
  }, []);
  const reportSceneError = useCallback((message: string) => { sceneErrorRef.current = message; setSceneError(message); setSaveState('error'); }, []);
  const clearSceneError = useCallback(() => { sceneErrorRef.current = null; setSceneError(null); }, []);
  const handleSceneChange = useCallback((scene: SceneUpdate) => { clearSceneError(); saveDraft(scene); }, [clearSceneError, saveDraft]);
  const handleImageBlocked = useCallback(() => setImageBlocked(true), []);

  const drain = useCallback((): Promise<boolean> => {
    if (inFlight.current) return inFlight.current;
    const work = (async () => {
      while (true) {
        const current = draftRef.current;
        const canvasId = selectedRef.current;
        if (!current || !canvasId) return true;
        if (conflictRef.current) { setSaveState('error'); return false; }
        if (sceneErrorRef.current) { setSaveState('error'); return false; }
        if (!retrySave.current && samePayload(savedRef.current, current)) { setSaveState('saved'); return true; }

        let operation = retrySave.current;
        if (!operation) {
          const request = {
            operationId: makeId(), programId, canvasId, expectedRevision: revisionRef.current,
            title: current.title.trim(), description: current.description.trim(), sceneJson: JSON.parse(current.sceneJson) as JsonValue,
          };
          if (!request.title) { setError('Add a title before saving this canvas.'); setSaveState('error'); return false; }
          operation = { request, payload: { ...current, title: request.title, description: request.description } };
          retrySave.current = operation;
        }
        setSaveState('saving'); setError(null);
        try {
          const result = await save.mutateAsync(operation.request);
          const latest = result.canvases.find((canvas) => canvas.id === canvasId);
          if (!latest) throw new Error('The saved canvas was not returned by the workspace.');
          if (latest.revision !== operation.request.expectedRevision + 1) {
            retrySave.current = null;
            conflictRef.current = true;
            setConflict(true); setSaveState('error');
            setError('This canvas advanced to a newer version while this save was being confirmed. Reload the latest version before continuing.');
            return false;
          }
          revisionRef.current = latest.revision;
          savedRef.current = operation.payload;
          retrySave.current = null;
          if (sceneErrorRef.current) { setSaveState('error'); return false; }
          // Edits made during the write remain in draftRef and are drained as a
          // second serialized save using the revision returned above.
          if (!samePayload(savedRef.current, draftRef.current ?? operation.payload)) continue;
          setSaveState('saved');
          return true;
        } catch (failure) {
          setSaveState('error'); setError(failure instanceof Error ? failure.message : 'Canvas could not be saved.');
          if (saveConflict(failure)) { conflictRef.current = true; setConflict(true); }
          return false;
        }
      }
    })();
    inFlight.current = work;
    return work.finally(() => { if (inFlight.current === work) inFlight.current = null; });
  }, [programId, save]);
  drainRef.current = drain;

  useEffect(() => {
    const unregister = registerPendingSave(() => drainRef.current());
    return () => { void drainRef.current().finally(unregister); };
  }, []);
  useEffect(() => {
    if (!confirmRestore) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    restoreCancelRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { restoreRetry.current = null; setConfirmRestore(null); }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => { document.removeEventListener('keydown', onKeyDown); previous?.focus(); };
  }, [confirmRestore]);
  useEffect(() => {
    if (!enabled) return;
    if (!selectedId && canvases.length) setSelectedId(canvases[0].id);
  }, [enabled, canvases, selectedId]);
  useEffect(() => {
    if (!selected) return;
    const value = initialEditable(selected);
    setDraft(value); savedRef.current = value; revisionRef.current = selected.revision;
    retrySave.current = null; setSaveState('saved'); setError(null); setConflict(false); conflictRef.current = false; setSceneError(null); sceneErrorRef.current = null;
    setSurfaceVersion((version) => version + 1);
  // Selection is the only synchronization boundary: cache updates during autosave
  // must not overwrite edits typed while a previous revision was being written.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);
  useEffect(() => {
    if (!selected || !draft || samePayload(savedRef.current, draft) || sceneError) return;
    const timer = window.setTimeout(() => { void drainRef.current(); }, 650);
    return () => window.clearTimeout(timer);
  }, [selectedId, draft, selected, sceneError]);

  const flush = useCallback(async () => {
    if (inFlight.current) {
      const first = await inFlight.current;
      if (!first) return false;
    }
    return drainRef.current();
  }, []);

  const doCreate = async () => {
    setError(null);
    if (selectedId && !await flush()) return;
    const title = createTitle.trim();
    if (!title) { setError('Give this canvas a title before creating it.'); return; }
    const sceneJson = serializeCanvasScene([], {}, {});
    const request = createRetry.current ?? {
      operationId: makeId(), canvasId: makeId(), programId, lessonId: lessonId ?? null,
      title, description: createDescription.trim(), sceneJson: JSON.parse(sceneJson) as JsonValue,
    };
    createRetry.current = request;
    try {
      const result = await create.mutateAsync(request);
      const created = result.canvases.find((canvas) => canvas.id === request.canvasId);
      if (!created) throw new Error('The new canvas was not returned by the workspace.');
      createRetry.current = null; setCreationOpen(false); setCreateTitle(''); setCreateDescription(''); setSelectedId(created.id);
    } catch (failure) { setError(failure instanceof Error ? failure.message : 'Canvas could not be created.'); }
  };

  const switchCanvas = async (id: string) => {
    if (id === selectedId) return;
    if (!await flush()) return;
    setSelectedId(id); setError(null); setImageBlocked(false); setExportError(null);
  };
  const reloadLatest = async () => {
    const result = await query.refetch();
    const latest = result.data?.canvases.find((canvas) => canvas.id === selectedId);
    if (!latest) return;
    const value = initialEditable(latest);
    setDraft(value); savedRef.current = value; revisionRef.current = latest.revision;
    retrySave.current = null; setConflict(false); conflictRef.current = false; setError(null); setSceneError(null); sceneErrorRef.current = null; setSaveState('saved'); setSurfaceVersion((v) => v + 1);
  };

  const makeCheckpoint = async () => {
    if (!selected || !draft || !await flush()) return;
    const name = checkpointName.trim();
    if (!name) { setError('Name this checkpoint first.'); return; }
    const request = checkpointRetry.current ?? { operationId: makeId(), snapshotId: makeId(), programId, canvasId: selected.id, expectedRevision: revisionRef.current, name };
    checkpointRetry.current = request;
    try { await checkpoint.mutateAsync(request); checkpointRetry.current = null; setCheckpointName(''); setError(null); }
    catch (failure) { setError(failure instanceof Error ? failure.message : 'Checkpoint could not be created.'); }
  };

  const restoreSnapshot = async () => {
    const snapshot = confirmRestore;
    if (!selected || !snapshot || !await flush()) return;
    const request = restoreRetry.current ?? { operationId: makeId(), preRestoreSnapshotId: makeId(), programId, canvasId: selected.id, snapshotId: snapshot.id, expectedRevision: revisionRef.current };
    restoreRetry.current = request;
    try {
      const result = await restore.mutateAsync(request);
      const latest = result.canvases.find((canvas) => canvas.id === selected.id);
      if (!latest) throw new Error('The restored canvas was not returned by the workspace.');
      const value = initialEditable(latest);
      setDraft(value); savedRef.current = value; revisionRef.current = latest.revision; restoreRetry.current = null;
      setSurfaceVersion((version) => version + 1); setConfirmRestore(null); setError(null); setSaveState('saved'); setConflict(false); conflictRef.current = false;
    } catch (failure) { setError(failure instanceof Error ? failure.message : 'Checkpoint could not be restored.'); }
  };

  const exportFile = async (kind: 'json' | 'svg' | 'png') => {
    setExportError(null);
    try {
      const blob = await surfaceRef.current?.exportScene(kind);
      if (!blob) throw new Error('The drawing surface is still opening. Try again shortly.');
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a'); anchor.href = url; anchor.download = `${(draft?.title || 'learning-canvas').replace(/[^a-z0-9-_]+/gi, '-').toLowerCase()}.${kind === 'json' ? 'excalidraw' : kind}`;
      anchor.click(); URL.revokeObjectURL(url);
    } catch (failure) { setExportError(failure instanceof Error ? failure.message : 'This canvas could not be exported.'); }
  };

  const outline = draft?.outline ?? [];
  const checkpointItems = selected?.snapshots ?? [];
  const emptyScene = (draft?.elementCount ?? 0) === 0;
  const creationError = create.isError ? create.error.message : null;

  if (query.isLoading) return <section className="rounded-2xl border border-border bg-surface p-8"><p role="status" className="flex items-center gap-2 text-sm text-text-muted"><LoaderCircle size={16} className="animate-spin" /> Loading your canvases…</p></section>;
  if (query.isError) return <section className="rounded-2xl border border-border bg-surface p-8"><h3 className="font-serif text-xl text-text-primary">Canvas could not load</h3><p className="mt-2 text-sm text-text-secondary">{query.error.message}</p><button type="button" onClick={() => void query.refetch()} className="mt-4 rounded-full bg-accent px-4 py-2 text-sm font-semibold text-accent-fg">Retry</button></section>;

  return <section aria-label="Learning canvas workspace" className="overflow-hidden rounded-2xl border border-border bg-surface">
    <header className="flex flex-wrap items-start justify-between gap-4 border-b border-border bg-[#f6f1e9] px-5 py-5 dark:bg-[#24221e] sm:px-7"><div><div className="flex items-center gap-2 text-[10px] font-semibold uppercase tracking-[.16em] text-accent"><Sparkles size={13} /> Studio Canvas</div><h2 className="mt-1 font-serif text-2xl text-text-primary">Learning canvas</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-text-secondary">Sketch ideas alongside {lessonTitle}. Shapes, connectors, and text are saved privately with this program.</p></div><button type="button" onClick={() => { setCreationOpen((open) => !open); setError(null); }} className="inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg shadow-sm"><Plus size={14} /> Create canvas</button></header>
    {error && <div role="alert" className="mx-5 mt-4 flex items-start justify-between gap-3 rounded-xl border border-rose-500/25 bg-rose-500/5 p-3 text-sm text-rose-700 sm:mx-7"><span>{error}</span>{!conflictRef.current && <button type="button" onClick={() => { if (createRetry.current) void doCreate(); else if (checkpointRetry.current) void makeCheckpoint(); else if (restoreRetry.current) void restoreSnapshot(); else void flush(); }} className="shrink-0 rounded-full border border-current/30 px-3 py-1 text-xs font-semibold">Retry</button>}</div>}
    {creationOpen && <div className="grid gap-3 border-b border-border bg-background/60 p-5 sm:grid-cols-[1fr_1fr_auto] sm:items-end"><label className="grid gap-1.5 text-xs font-medium text-text-secondary">Canvas title<input autoFocus aria-label="Canvas title" maxLength={120} value={createTitle} onChange={(e) => { if (createRetry.current && e.target.value.trim() !== createRetry.current.title) createRetry.current = null; setCreateTitle(e.target.value); }} placeholder="e.g. A map of the argument" className="rounded-lg border border-border bg-surface px-3 py-2 text-sm text-text-primary outline-none focus:border-accent" /></label><label className="grid gap-1.5 text-xs font-medium text-text-secondary">Describe your drawing or its meaning<textarea aria-label="Describe your drawing or its meaning" rows={2} maxLength={2000} value={createDescription} onChange={(e) => { if (createRetry.current && e.target.value.trim() !== createRetry.current.description) createRetry.current = null; setCreateDescription(e.target.value); }} placeholder="A written alternative that explains what this canvas shows" className="resize-y rounded-lg border border-border bg-surface px-3 py-2 text-sm text-text-primary outline-none focus:border-accent" /></label><button type="button" disabled={create.isPending || !createTitle.trim()} onClick={() => void doCreate()} className="inline-flex items-center justify-center gap-2 rounded-lg bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-45">{create.isPending ? <LoaderCircle size={14} className="animate-spin" /> : <Plus size={14} />} Create</button>{creationError && <p role="alert" className="text-xs text-rose-700 sm:col-span-3">{creationError} Use Retry to safely repeat this creation.</p>}</div>}

    {canvases.length === 0 ? <div className="grid min-h-64 place-items-center px-6 py-12 text-center"><div className="max-w-md"><div className="mx-auto grid h-12 w-12 place-items-center rounded-2xl bg-accent/10 text-accent"><PenToolIcon /></div><h3 className="mt-4 font-serif text-xl text-text-primary">A visual page for your thinking</h3><p className="mt-2 text-sm leading-6 text-text-secondary">Start a canvas to connect ideas, sketch a process, or make a written visual note for this lesson.</p><button type="button" onClick={() => setCreationOpen(true)} className="mt-4 rounded-full border border-accent/40 px-4 py-2 text-xs font-semibold text-accent">Create your first canvas</button></div></div> : <>
      <div className="flex gap-2 overflow-x-auto border-b border-border px-4 py-3" aria-label="Canvases">{canvases.map((canvas) => <button type="button" key={canvas.id} aria-pressed={canvas.id === selectedId} onClick={() => void switchCanvas(canvas.id)} className={`max-w-[230px] shrink-0 truncate rounded-full border px-3.5 py-2 text-xs ${canvas.id === selectedId ? 'border-accent/50 bg-accent/10 font-semibold text-accent' : 'border-border text-text-secondary hover:border-accent/35'}`}>Canvas: {canvas.title}</button>)}</div>
      {selected && draft && <div className="p-4 sm:p-6">
        <div className="mb-4 grid gap-3 sm:grid-cols-[minmax(180px,.75fr)_minmax(240px,1.5fr)_auto] sm:items-start"><label className="grid gap-1 text-[10px] font-semibold uppercase tracking-[.12em] text-text-muted">Title<input aria-label="Canvas title" value={draft.title} maxLength={120} onChange={(e) => saveDraft({ title: e.target.value })} className="rounded-lg border border-border bg-background px-3 py-2 text-sm font-medium normal-case tracking-normal text-text-primary outline-none focus:border-accent" /></label><label className="grid gap-1 text-[10px] font-semibold uppercase tracking-[.12em] text-text-muted">Written description<textarea aria-label="Describe your drawing or its meaning" rows={2} maxLength={2000} value={draft.description} onChange={(e) => saveDraft({ description: e.target.value })} placeholder="Explain this drawing in words for someone who cannot see it" className="resize-y rounded-lg border border-border bg-background px-3 py-2 text-sm font-normal normal-case leading-5 tracking-normal text-text-primary outline-none focus:border-accent" /></label><div className="flex items-center gap-2 pt-1 text-xs text-text-muted" aria-live="polite">{saveState === 'saving' ? <><LoaderCircle size={14} className="animate-spin" /> Saving</> : saveState === 'error' ? <><AlertCircle size={14} className="text-rose-600" /> Not saved</> : <><Check size={14} className="text-emerald-700" /> Saved</>}</div></div>
        {sceneError && <div role="alert" className="mb-3 rounded-lg border border-amber-600/25 bg-amber-500/5 p-3 text-xs text-amber-800 dark:text-amber-200">{sceneError} Saving is paused until the scene is simplified or corrected.</div>}
        {imageBlocked && <p role="status" className="mb-3 rounded-lg bg-accent/5 px-3 py-2 text-xs text-text-secondary">Image attachments aren’t available on Canvas yet. You can still draw shapes, lines, and text.</p>}
        {conflict && <div role="alert" className="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-600/25 bg-amber-500/5 p-3 text-xs text-text-secondary"><span>This canvas changed elsewhere. Reload the latest version to continue; unsaved local edits will be replaced.</span><button type="button" onClick={() => void reloadLatest()} className="rounded-full border border-amber-600/30 px-3 py-1.5 font-semibold text-amber-800 dark:text-amber-200">Reload latest</button></div>}
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_270px]"><div className="relative z-10 min-w-0"><Suspense fallback={<div role="status" className="grid min-h-[460px] place-items-center rounded-xl border border-border bg-background text-sm text-text-muted">Opening drawing tools…</div>}><LazyCanvasSurface key={`${selected.id}:${surfaceVersion}`} ref={surfaceRef} sceneJson={draft.sceneJson} theme={theme} onSceneChange={handleSceneChange} onImageBlocked={handleImageBlocked} onSceneError={reportSceneError} /></Suspense>
          <div className="relative z-20 mt-3 flex flex-wrap items-center gap-2"><span className="mr-1 text-[10px] text-text-muted">Export a copy</span><button type="button" disabled={emptyScene} onClick={() => void exportFile('json')} className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1.5 text-[10px] text-text-secondary disabled:opacity-40"><FileJson2 size={12} /> .excalidraw</button><button type="button" disabled={emptyScene} onClick={() => void exportFile('svg')} className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1.5 text-[10px] text-text-secondary disabled:opacity-40"><ArrowDownToLine size={12} /> SVG</button><button type="button" disabled={emptyScene} onClick={() => void exportFile('png')} className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1.5 text-[10px] text-text-secondary disabled:opacity-40"><ImageDown size={12} /> PNG</button><span className="basis-full text-left text-[10px] text-text-muted sm:ml-auto sm:basis-auto sm:text-right">{emptyScene ? 'Add an element to enable export' : `${draft.elementCount} ${draft.elementCount === 1 ? 'element' : 'elements'}`}</span></div>{exportError && <p role="alert" className="mt-2 text-xs text-rose-700">{exportError}</p>}</div>
          <aside className="relative z-0 space-y-3"><section className="rounded-xl border border-border bg-background p-4"><button type="button" aria-expanded={outlineOpen} onClick={() => setOutlineOpen((open) => !open)} className="flex w-full items-center justify-between text-left"><span className="flex items-center gap-2 text-xs font-semibold text-text-primary"><Text size={14} className="text-accent" /> Canvas element outline</span><ChevronDown size={14} className={`transition-transform ${outlineOpen ? '' : '-rotate-90'}`} /></button>{outlineOpen && <div className="mt-3">{outline.length ? <ol className="space-y-1.5 text-xs leading-5 text-text-secondary">{outline.map((line, index) => <li key={`${index}-${line}`} className="flex gap-2"><span className="w-5 shrink-0 text-right tabular-nums text-text-muted">{index + 1}.</span><span className="break-words">{line}</span></li>)}</ol> : <p className="text-xs leading-5 text-text-muted">Your text and drawing elements will appear here in reading order.</p>}</div>}</section>
            <section className="rounded-xl border border-border bg-background p-4"><h3 className="flex items-center gap-2 text-xs font-semibold text-text-primary"><Clock3 size={14} className="text-accent" /> Checkpoints</h3><p className="mt-1 text-[10px] leading-4 text-text-muted">Named, immutable versions of this canvas.</p><div className="mt-3 flex gap-2"><input aria-label="Checkpoint name" maxLength={120} value={checkpointName} onChange={(e) => { if (checkpointRetry.current && e.target.value.trim() !== checkpointRetry.current.name) checkpointRetry.current = null; setCheckpointName(e.target.value); }} placeholder="Name this moment" className="min-w-0 flex-1 rounded-lg border border-border bg-surface px-2.5 py-2 text-xs text-text-primary outline-none focus:border-accent" /><button type="button" aria-label="Create checkpoint" disabled={!checkpointName.trim() || checkpoint.isPending || saveState === 'error'} onClick={() => void makeCheckpoint()} className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-accent text-accent-fg disabled:opacity-40"><Save size={14} /></button></div>{checkpointItems.length ? <ul className="mt-3 max-h-64 space-y-2 overflow-y-auto">{checkpointItems.map((item) => <li key={item.id} className="rounded-lg border border-border/70 bg-surface p-2.5"><div className="truncate text-xs font-medium text-text-primary">{item.name}</div><div className="mt-1 text-[9px] text-text-muted">{item.elementCount} {item.elementCount === 1 ? 'element' : 'elements'} · {timestamp(item.createdAt)}</div><button type="button" aria-label={`Restore checkpoint ${item.name}`} onClick={() => { restoreRetry.current = null; setConfirmRestore(item); }} className="mt-2 inline-flex items-center gap-1 text-[10px] font-semibold text-accent hover:underline"><RotateCcw size={11} /> Restore</button></li>)}</ul> : <p className="mt-3 text-[10px] text-text-muted">No checkpoints yet.</p>}</section>
            <p className="rounded-xl bg-[#f3ede4] p-3 text-[10px] leading-4 text-text-secondary dark:bg-white/5"><CircleHelp size={12} className="mr-1 inline text-accent" /> Canvas describes your thinking; it does not change lesson completion or assessment results.</p>
          </aside></div>
        <div className="mt-4 flex flex-wrap items-center justify-between gap-x-4 gap-y-1 border-t border-border pt-3 text-[10px] text-text-muted"><span>Updated {timestamp(selected.updatedAt)} · Revision {revisionRef.current}</span><span className="text-left sm:text-right">Private to this learning program</span></div>
      </div>}
    </>}
    {confirmRestore && <div role="presentation" className="fixed inset-0 z-50 grid place-items-center bg-black/45 p-4" onMouseDown={(event) => { if (event.target === event.currentTarget) { restoreRetry.current = null; setConfirmRestore(null); } }}><section role="alertdialog" aria-modal="true" aria-labelledby="canvas-restore-title" className="w-full max-w-md rounded-2xl border border-border bg-surface p-6 shadow-2xl"><h3 id="canvas-restore-title" className="font-serif text-xl text-text-primary">Restore “{confirmRestore.name}”?</h3><p className="mt-2 text-sm leading-6 text-text-secondary">This replaces the current canvas. A checkpoint of the current version will be created first so you can return to it.</p><div className="mt-5 flex justify-end gap-2"><button type="button" ref={restoreCancelRef} onClick={() => { restoreRetry.current = null; setConfirmRestore(null); }} className="rounded-full border border-border px-4 py-2 text-xs text-text-secondary">Cancel</button><button type="button" disabled={restore.isPending} onClick={() => void restoreSnapshot()} className="rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-50">{restore.isPending ? 'Restoring…' : 'Restore checkpoint'}</button></div></section></div>}
  </section>;
}

function PenToolIcon() { return <span aria-hidden="true" className="font-serif text-2xl leading-none">✎</span>; }
