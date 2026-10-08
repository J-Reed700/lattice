import { useRef, useState } from 'react';

import { open } from '@tauri-apps/plugin-dialog';
import { Archive, Check, Download, FolderOpen, LockKeyhole, RotateCcw, ShieldCheck, Upload } from 'lucide-react';

import {
  useApplyLearningPackImport,
  useCancelLearningPackImportPreview,
  useExportLearningPack,
  useLearningPortability,
  usePreviewLearningPackImport,
} from '@/features/learning/portability/useLearningPortability';
import VaultAPI from '@/lib/api';
import type { LearningPackConflictPolicy, LearningPackImportPreviewDto } from '@/lib/bindings';


type RetryEntry = { fingerprint: string; request: Record<string, unknown> };
const conflictPolicies: { value: LearningPackConflictPolicy; label: string; description: string }[] = [
  { value: 'create_copy', label: 'Import as a separate copy', description: 'Creates a new local copy and remaps incoming IDs so existing local history stays untouched.' },
  { value: 'merge_safe', label: 'Import only if conflict-free', description: 'Keeps every incoming ID and blocks the import if the program or any included record conflicts locally.' },
  { value: 'replace_after_backup', label: 'Replace after backup', description: 'Creates a backup before replacing conflicting program data.' },
];

function uuid() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (char) => {
    const random = Math.floor(Math.random() * 16);
    return (char === 'x' ? random : (random & 3) | 8).toString(16);
  });
}
function dateLabel(value: number) { const date = new Date(value); return Number.isNaN(date.getTime()) ? 'Date unavailable' : date.toLocaleString(); }

export function PortabilityPanel({ programId, programTitle }: { programId: string; programTitle: string }) {
  const workspace = useLearningPortability(programId);
  const exportPack = useExportLearningPack(programId);
  const previewPack = usePreviewLearningPackImport(programId);
  const applyPack = useApplyLearningPackImport(programId);
  const cancelPreview = useCancelLearningPackImportPreview(programId);
  const retry = useRef<Record<string, RetryEntry>>({});
  const [fileName, setFileName] = useState(programTitle.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'learning-program');
  const [includeEvidence, setIncludeEvidence] = useState(true);
  const [includeArtifacts, setIncludeArtifacts] = useState(false);
  const [includeBodies, setIncludeBodies] = useState(false);
  const [confirmBodies, setConfirmBodies] = useState(false);
  const [conflictPolicy, setConflictPolicy] = useState<LearningPackConflictPolicy>('create_copy');
  const [previewId, setPreviewId] = useState('');
  const [previewSnapshot, setPreviewSnapshot] = useState<LearningPackImportPreviewDto | null>(null);
  const [applyConfirm, setApplyConfirm] = useState(false);
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');
  const [lastDestination, setLastDestination] = useState('');

  const exportRequest = async () => {
    const payload = { programId, fileName: fileName.trim().replace(/\.lattice-learning$/i, ''), includeEvidence, includePracticalArtifacts: includeArtifacts, includeSourceBodies: includeBodies, sourceBodyRedistributionConfirmed: includeBodies && confirmBodies };
    const fingerprint = JSON.stringify(payload);
    let entry = retry.current.export;
    if (entry?.fingerprint !== fingerprint) entry = { fingerprint, request: { ...payload, operationId: uuid() } };
    retry.current.export = entry;
    setError(''); setSuccess('');
    try {
      const updated = await exportPack.mutateAsync(entry.request as never);
      delete retry.current.export;
      const exported = updated.exports.at(0);
      if (exported?.destinationPath) setLastDestination(exported.destinationPath);
      setSuccess(exported ? `Pack saved to ${exported.destinationPath}` : 'Pack export completed.');
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'The pack could not be exported.'); }
  };

  const performPreview = async (entry: RetryEntry) => {
    setError(''); setSuccess(''); setApplyConfirm(false);
    try {
      const updated = await previewPack.mutateAsync(entry.request as never);
      const found = updated.importPreviews.find((item) => item.id === (entry.request.previewId as string));
      const selected = found ?? updated.importPreviews.at(-1) ?? null;
      setPreviewSnapshot(selected);
      setPreviewId(selected?.id ?? '');
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'The selected pack could not be previewed.'); }
  };

  const choosePack = async () => {
    setError(''); setSuccess(''); setApplyConfirm(false);
    try {
      const chosen = await open({ multiple: false, directory: false, title: 'Choose a Learning Studio pack', filters: [{ name: 'Learning Studio pack', extensions: ['lattice-learning'] }] });
      if (!chosen || Array.isArray(chosen)) return;
      const payload = { sourcePath: chosen, conflictPolicy };
      const fingerprint = JSON.stringify(payload);
      let entry = retry.current.preview;
      if (entry?.fingerprint !== fingerprint) entry = { fingerprint, request: { ...payload, operationId: uuid(), previewId: uuid() } };
      retry.current.preview = entry;
      await performPreview(entry);
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'The selected pack could not be previewed.'); }
  };

  const selectedPreview = previewSnapshot?.id === previewId ? previewSnapshot : (workspace.data?.importPreviews ?? []).find((item) => item.id === previewId)
    ?? workspace.data?.importPreviews.find((item) => item.status === 'pending')
    ?? null;
  const runApply = async () => {
    if (!selectedPreview?.canApply) return;
    const payload = { previewId: selectedPreview.id, expectedRootSha256: selectedPreview.manifest.rootSha256 };
    const fingerprint = JSON.stringify(payload);
    let entry = retry.current.apply;
    if (entry?.fingerprint !== fingerprint) entry = { fingerprint, request: { ...payload, operationId: uuid() } };
    retry.current.apply = entry;
    setError(''); setSuccess(''); setApplyConfirm(false);
    try {
      const updated = await applyPack.mutateAsync(entry.request as never);
      delete retry.current.apply;
      setPreviewSnapshot(updated.importPreviews.find((item) => item.id === selectedPreview.id) ?? null);
      const result = updated.imports.at(0);
      setSuccess(result ? `Imported program ${result.importedProgramId}.` : 'Import applied.');
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'The import could not be applied.'); }
  };
  const abandonPreview = async () => {
    if (!selectedPreview || cancelPreview.isPending) return;
    const payload = { previewId: selectedPreview.id };
    let entry = retry.current.cancel;
    if (entry?.fingerprint !== JSON.stringify(payload)) entry = { fingerprint: JSON.stringify(payload), request: { ...payload, operationId: uuid() } };
    retry.current.cancel = entry;
    try { const updated = await cancelPreview.mutateAsync(entry.request as never); delete retry.current.cancel; setPreviewSnapshot(updated.importPreviews.find((item) => item.id === selectedPreview.id) ?? null); setPreviewId(''); setApplyConfirm(false); }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'The preview could not be cancelled.'); }
  };

  const busy = exportPack.isPending || previewPack.isPending || applyPack.isPending || cancelPreview.isPending;
  const showInFolder = async (requestedPath?: string) => {
    const path = requestedPath ?? lastDestination ?? workspace.data?.exports.at(0)?.destinationPath;
    if (!path) return;
    const result = await VaultAPI.showInFolder(path);
    if (!result.ok) setError(result.error);
  };

  if (workspace.isLoading) return <div role="status" className="rounded-2xl border border-border bg-surface p-6 text-sm text-text-muted">Loading bundle history…</div>;
  if (workspace.error) return <section className="rounded-2xl border border-border bg-surface p-6"><h3 className="font-serif text-xl text-text-primary">Portability history is unavailable</h3><p role="alert" className="mt-2 text-sm text-rose-700">{workspace.error.message}</p><button type="button" onClick={() => void workspace.refetch()} className="mt-3 rounded-full border border-border px-4 py-2 text-sm">Retry</button></section>;
  const currentPolicy = conflictPolicies.find((item) => item.value === conflictPolicy)!;

  return <div className="space-y-5" data-testid="learning-portability-workspace">
    <header className="rounded-2xl border border-[#d7c9b8] bg-[#eee8df] p-5 dark:border-white/10 dark:bg-[#28251f] sm:p-7">
      <div className="flex items-start gap-3"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent"><Archive size={18} /></span><div><p className="text-[10px] font-semibold uppercase tracking-[.16em] text-accent">Portable learning</p><h3 className="mt-1 font-serif text-2xl text-text-primary sm:text-3xl">Keep a safe copy of your program</h3><p className="mt-2 max-w-2xl text-sm leading-6 text-text-secondary">Export a versioned, checksummed bundle or inspect every proposed change before importing one. No write happens during preview.</p></div></div>
      <div className="mt-5 grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-white/70 bg-white/55 p-3 dark:border-white/10 dark:bg-black/10"><ShieldCheck size={15} className="text-emerald-700" /><p className="mt-2 text-xs font-medium text-text-primary">Private by default</p><p className="mt-1 text-[10px] leading-4 text-text-muted">Credentials and Lattice chat outside this program are never included.</p></div><div className="rounded-xl border border-white/70 bg-white/55 p-3 dark:border-white/10 dark:bg-black/10"><LockKeyhole size={15} className="text-accent" /><p className="mt-2 text-xs font-medium text-text-primary">Exact source versions</p><p className="mt-1 text-[10px] leading-4 text-text-muted">The manifest records content checksums and source version IDs.</p></div><div className="rounded-xl border border-white/70 bg-white/55 p-3 dark:border-white/10 dark:bg-black/10"><Archive size={15} className="text-text-muted" /><p className="mt-2 text-xs font-medium text-text-primary">Review before writing</p><p className="mt-1 text-[10px] leading-4 text-text-muted">Conflicts and actions are listed before you choose Apply.</p></div></div>
    </header>

    {error && <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-rose-500/20 bg-rose-500/5 px-4 py-3 text-sm text-rose-700"><span>{error}</span>{retry.current.export && <button type="button" disabled={busy} onClick={() => void exportRequest()} className="inline-flex items-center gap-1 text-xs font-semibold underline"><RotateCcw size={12} /> Retry same export</button>}{retry.current.preview && <button type="button" disabled={busy} onClick={() => void performPreview(retry.current.preview!)} className="inline-flex items-center gap-1 text-xs font-semibold underline"><RotateCcw size={12} /> Retry same preview</button>}{retry.current.apply && <button type="button" disabled={busy} onClick={() => void runApply()} className="inline-flex items-center gap-1 text-xs font-semibold underline"><RotateCcw size={12} /> Retry apply</button>}</div>}
    {success && <div role="status" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-emerald-600/20 bg-emerald-500/5 px-4 py-3 text-sm text-emerald-800"><span className="min-w-0 wrap-break-word">{success}</span>{lastDestination || workspace.data?.exports.length ? <button type="button" onClick={() => void showInFolder()} className="inline-flex shrink-0 items-center gap-1.5 text-xs font-semibold underline"><FolderOpen size={13} /> Reveal file</button> : null}</div>}

    <div className="grid min-w-0 gap-5 xl:grid-cols-2">
      <section className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-6"><div className="flex items-center gap-2"><Download size={16} className="text-accent" /><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">Export</p><h4 className="font-serif text-xl text-text-primary">Create a learning pack</h4></div></div><p className="mt-2 text-xs leading-5 text-text-secondary">The app writes a validated bundle in its managed exports folder. You can reveal the destination after it is created.</p>
        <label className="mt-4 block text-xs font-medium text-text-secondary">Pack file name<input value={fileName} onChange={(event) => setFileName(event.target.value)} maxLength={120} className="mt-1.5 block w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2.5 text-sm" /></label>
        <p className="mt-3 rounded-lg bg-amber-500/10 p-3 text-[10px] leading-4 text-amber-800">Assessment answer keys are included in the pack so restored questions still work. They are not shown in normal app views, but anyone who can inspect the pack file can read them. Hidden practical evaluator files are excluded.</p><div className="mt-4 space-y-2"><label className="flex gap-2 rounded-lg border border-border bg-background p-3 text-xs text-text-secondary"><input type="checkbox" checked={includeEvidence} onChange={(event) => setIncludeEvidence(event.target.checked)} className="mt-0.5 accent-accent" /><span><strong className="font-medium text-text-primary">Include learner evidence</strong><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">Includes submitted responses, recall and review history, program tutor turns, workbench history, and Canvas documents. Restorable assessments include their model-authored answer keys; hidden evaluators remain out.</span></span></label><label className="flex gap-2 rounded-lg border border-border bg-background p-3 text-xs text-text-secondary"><input type="checkbox" checked={includeArtifacts} onChange={(event) => setIncludeArtifacts(event.target.checked)} className="mt-0.5 accent-accent" /><span><strong className="font-medium text-text-primary">Include practical work artifacts</strong><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">Includes authored activities and saved learner files; hidden evaluator files remain out.</span></span></label><label className="flex gap-2 rounded-lg border border-border bg-background p-3 text-xs text-text-secondary"><input type="checkbox" checked={includeBodies} onChange={(event) => { setIncludeBodies(event.target.checked); if (!event.target.checked) setConfirmBodies(false); }} className="mt-0.5 accent-accent" /><span><strong className="font-medium text-text-primary">Include full source text</strong><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">Only select this if you have rights to redistribute every included source.</span></span></label>{includeBodies && <label className="ml-6 flex items-start gap-2 rounded-lg bg-amber-500/10 p-3 text-xs text-text-secondary"><input type="checkbox" checked={confirmBodies} onChange={(event) => setConfirmBodies(event.target.checked)} className="mt-0.5 accent-accent" /><span>I confirm that full source bodies in this pack may be redistributed.</span></label>}<p className="rounded-lg border border-border bg-background/60 p-3 text-[10px] leading-4 text-text-muted">The privacy manifest records whether answer keys, hidden evaluators, learner evidence, practical artifacts, and full source bodies are included. Credentials and Lattice chat outside this program are excluded.</p></div>
        <button type="button" disabled={busy || !fileName.trim() || (includeBodies && !confirmBodies)} onClick={() => void exportRequest()} className="mt-4 inline-flex items-center gap-2 rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-50">{exportPack.isPending ? 'Creating pack…' : 'Export program pack'}</button>
      </section>

      <section className="min-w-0 rounded-2xl border border-border bg-surface p-4 sm:p-6"><div className="flex items-center gap-2"><Upload size={16} className="text-accent" /><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-accent">Import</p><h4 className="font-serif text-xl text-text-primary">Preview a learning pack</h4></div></div><label className="mt-4 block text-xs font-medium text-text-secondary">If records conflict<select value={conflictPolicy} onChange={(event) => { setConflictPolicy(event.target.value as LearningPackConflictPolicy); retry.current.preview = undefined as never; }} className="mt-1.5 block w-full min-w-0 max-w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm"><option value="create_copy">Import as a separate copy</option><option value="merge_safe">Import only if conflict-free</option><option value="replace_after_backup">Replace after backup</option></select></label><p className="mt-2 text-[10px] leading-4 text-text-muted">{currentPolicy.description}</p><button type="button" disabled={busy} onClick={() => void choosePack()} className="mt-4 inline-flex items-center gap-2 rounded-full border border-accent/35 px-4 py-2.5 text-xs font-semibold text-accent disabled:opacity-50">{previewPack.isPending ? 'Inspecting bundle…' : 'Choose pack and preview'}</button>
        {selectedPreview && <article className="mt-5 min-w-0 rounded-xl border border-border bg-background p-3 sm:p-4"><div className="flex flex-wrap items-start justify-between gap-2"><div><p className="text-[10px] font-semibold uppercase tracking-[.13em] text-accent">Dry run · {selectedPreview.status.replace(/_/g, ' ')}</p><h5 className="mt-1 wrap-break-word font-serif text-lg text-text-primary">{selectedPreview.incomingProgramTitle}</h5><p className="mt-1 break-all text-[10px] text-text-muted">{selectedPreview.sourcePath}</p></div><span className="rounded-full bg-accent/10 px-2.5 py-1 text-[9px] font-medium text-accent">{selectedPreview.conflictPolicy.replace(/_/g, ' ')}</span></div>
          <div className="mt-4 grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-border p-3"><span className="block text-[10px] text-text-muted">Checksum</span><code className="mt-1 block break-all text-[10px] text-text-secondary">{selectedPreview.manifest.rootSha256}</code></div><div className="rounded-lg border border-border p-3"><span className="block text-[10px] text-text-muted">Pack version</span><span className="mt-1 block text-text-secondary">{selectedPreview.manifest.format} · v{selectedPreview.manifest.version}</span></div></div>
          <section aria-label="Pack privacy manifest" className="mt-3 min-w-0 rounded-lg border border-border p-3"><h6 className="text-xs font-semibold text-text-primary">Pack privacy manifest</h6><dl className="mt-3 grid min-w-0 gap-2 text-[10px] sm:grid-cols-2">{[['Answer keys included', selectedPreview.manifest.privacy.includesAnswerKeys], ['Hidden evaluators included', selectedPreview.manifest.privacy.includesHiddenEvaluators], ['Learner evidence included', selectedPreview.manifest.privacy.includesLearnerEvidence], ['Practical artifacts included', selectedPreview.manifest.privacy.includesPracticalArtifacts], ['Full source bodies included', selectedPreview.manifest.privacy.includesFullSourceBodies], ['Source redistribution confirmed', selectedPreview.manifest.privacy.sourceBodyRedistributionConfirmed], ['Private chat included', selectedPreview.manifest.privacy.includesPrivateChat], ['Credentials included', selectedPreview.manifest.privacy.includesCredentials]].map(([label, value]) => <div key={String(label)} className="flex min-w-0 items-center justify-between gap-2 rounded bg-surface px-2.5 py-2"><dt className="min-w-0 text-text-muted">{label}</dt><dd className={`shrink-0 ${value ? 'font-semibold text-amber-700' : 'text-emerald-700'}`}>{value ? 'Yes' : 'No'}</dd></div>)}</dl><div className="mt-3 rounded-lg bg-background p-2.5"><p className="text-[10px] font-medium text-text-primary">Omitted items</p>{selectedPreview.manifest.privacy.omittedItems.length > 0 ? <ul className="mt-1 list-disc space-y-1 wrap-break-word pl-4 text-[10px] leading-4 text-text-muted">{selectedPreview.manifest.privacy.omittedItems.map((item, index) => <li key={`${item}:${index}`}>{item}</li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">None listed.</p>}</div></section>
          <div className="mt-4"><h6 className="text-xs font-semibold text-text-primary">Conflicts · {selectedPreview.conflicts.length}</h6>{selectedPreview.conflicts.length ? <ul className="mt-2 space-y-2">{selectedPreview.conflicts.map((item, index) => <li key={`${item.entityKind}:${item.incomingId}:${index}`} className="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3 text-[10px]"><div className="font-medium text-text-primary">{item.entityKind} · {item.incomingTitle}</div><div className="mt-1 text-text-muted">Local: {item.existingTitle} · Resolution: {item.resolution.replace(/_/g, ' ')}</div></li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">No ID or content conflicts were found during preview.</p>}</div>
          <div className="mt-4"><h6 className="text-xs font-semibold text-text-primary">Proposed changes · {selectedPreview.changes.length}</h6>{selectedPreview.changes.length ? <ul className="mt-2 max-h-40 space-y-2 overflow-auto">{selectedPreview.changes.map((item, index) => <li key={`${item.entityKind}:${item.entityId}:${index}`} className="rounded-lg border border-border p-2.5 text-[10px]"><span className="font-medium text-accent">{item.action.replace(/_/g, ' ')}</span><span className="ml-2 text-text-muted">{item.entityKind}</span><p className="mt-1 text-text-secondary">{item.description}</p></li>)}</ul> : <p className="mt-1 text-[10px] text-text-muted">No write actions are proposed.</p>}</div>
          {selectedPreview.warnings.length > 0 && <div className="mt-4 rounded-lg bg-amber-500/10 p-3"><h6 className="text-xs font-semibold text-amber-800">Review these warnings</h6><ul className="mt-2 list-disc space-y-1 pl-4 text-[10px] leading-4 text-amber-800">{selectedPreview.warnings.map((warning, index) => <li key={`${warning}:${index}`}>{warning}</li>)}</ul></div>}
          {selectedPreview.status === 'pending' && <div className="mt-4 flex flex-wrap gap-2"><button type="button" disabled={busy || !selectedPreview.canApply} onClick={() => setApplyConfirm(true)} className="inline-flex items-center gap-1.5 rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:opacity-40"><Check size={13} />Apply these changes</button><button type="button" disabled={busy} onClick={() => void abandonPreview()} className="rounded-full border border-border px-4 py-2 text-xs text-text-secondary disabled:opacity-50">Cancel preview</button>{!selectedPreview.canApply && <span className="self-center text-[10px] text-amber-700">This preview is not eligible to apply. Review its warnings and conflicts.</span>}</div>}
          {applyConfirm && <div role="dialog" aria-modal="true" aria-labelledby="apply-pack-confirm" className="mt-4 rounded-xl border border-rose-500/25 bg-rose-500/5 p-4"><h6 id="apply-pack-confirm" className="font-medium text-text-primary">Apply the reviewed import?</h6><p className="mt-1 text-xs leading-5 text-text-secondary">This will write {selectedPreview.changes.length} listed change{selectedPreview.changes.length === 1 ? '' : 's'} under the selected conflict policy. The pack checksum will be checked again before writing.</p><div className="mt-3 flex flex-wrap gap-2"><button type="button" autoFocus disabled={busy} onClick={() => void runApply()} className="rounded-full bg-rose-700 px-4 py-2 text-xs font-semibold text-white disabled:opacity-50">{applyPack.isPending ? 'Applying…' : 'Confirm import'}</button><button type="button" disabled={busy} onClick={() => setApplyConfirm(false)} className="rounded-full border border-border px-4 py-2 text-xs text-text-secondary">Keep reviewing</button></div></div>}
        </article>}
      </section>
    </div>

    <section className="rounded-2xl border border-border bg-surface p-4 sm:p-6"><div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-semibold uppercase tracking-[.14em] text-text-muted">History</p><h4 className="font-serif text-xl text-text-primary">Recent bundles</h4></div><span className="rounded-full bg-background px-3 py-1 text-[10px] text-text-muted">{workspace.data?.exports.length ?? 0} exports · {workspace.data?.imports.length ?? 0} imports</span></div>
      {(workspace.data?.exports.length ?? 0) === 0 && (workspace.data?.imports.length ?? 0) === 0 ? <p className="mt-3 rounded-xl border border-dashed border-border p-4 text-xs text-text-muted">Export and import records will appear here after they are created.</p> : <div className="mt-4 grid gap-3 lg:grid-cols-2">{workspace.data?.exports.slice().reverse().map((item) => <article key={item.id} className="min-w-0 rounded-xl border border-border bg-background p-4"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="text-xs font-medium text-text-primary">{item.manifest.title}</p><p className="mt-1 break-all text-[10px] text-text-muted">{item.destinationPath}</p><p className="mt-2 text-[10px] text-text-muted">Created {dateLabel(item.createdAt)} · v{item.manifest.version} · {item.manifest.entries.length} entries</p></div><button type="button" onClick={() => void VaultAPI.showInFolder(item.destinationPath)} className="shrink-0 rounded-full border border-border p-2 text-text-muted" aria-label={`Reveal ${item.manifest.title} in folder`}><FolderOpen size={13} /></button></div></article>)}{workspace.data?.imports.slice().reverse().map((item) => <article key={item.id} className="min-w-0 rounded-xl border border-border bg-background p-4"><p className="text-xs font-medium text-text-primary">Imported program</p><p className="mt-1 break-all text-[10px] text-text-muted">{item.importedProgramId}</p><p className="mt-2 text-[10px] text-text-muted">{dateLabel(item.importedAt)} · {item.appliedChanges.length} applied changes{item.backupId ? ` · backup ${item.backupId}` : ''}</p></article>)}</div>}</section>
  </div>;
}
