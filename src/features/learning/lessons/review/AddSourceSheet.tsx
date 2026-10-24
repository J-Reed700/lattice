import { useEffect, useId, useState } from 'react';

import { FileText, Globe2, Library, LoaderCircle, Type, X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { AddSourceResult } from '@/features/learning/lessons/review/AddSourceResult';
import { ADD_SOURCE_HINT, addSourceTitle, attachSummary, progressLine, type AttachSummary } from '@/features/learning/lessons/review/claimReviewCopy';
import { useAddSource } from '@/features/learning/lessons/review/useAddSource';
import type { DroppedSource } from '@/features/learning/lessons/review/useReviewDrop';
import { LearningDocumentPicker, type LearningDocumentSelection } from '@/features/learning/sources/LearningDocumentPicker';
import { useLearningSourceVersion, useLearningSourceWorkspace } from '@/features/learning/sources/useLearningSources';
import type { AttachEvidenceDto, AttachNextStepDto, AttachScopeDto, ClaimAttachReportDto } from '@/lib/bindings';

type Mode = 'link' | 'paste' | 'file' | 'saved';

/** Where the sheet was opened: one claim, a group of the queue, or the whole queue. */
export type AddSourceTarget = {
  level: 'claim' | 'group' | 'queue';
  /** The claims to check; the first is the one the attach is recorded on. */
  claimIds: string[];
  /** On one claim: the other waiting claims of its group, checked too when the box is on. */
  others?: string[];
};

const hostOf = (url: string) => { try { return new URL(url).host; } catch { return null; } };

/** One saved source of the course, as a whole passage (the reader picks narrower ones). */
function SavedSourcePicker({ programId, onPick }: { programId: string; onPick: (evidence: AttachEvidenceDto | null) => void }) {
  const workspace = useLearningSourceWorkspace(programId);
  const [chosen, setChosen] = useState<{ sourceId: string; versionId: string } | null>(null);
  const detail = useLearningSourceVersion(chosen ? { programId, sourceId: chosen.sourceId, versionId: chosen.versionId } : null);
  useEffect(() => {
    if (!chosen || !detail.data) { onPick(null); return; }
    onPick({ kind: 'attach_saved_source', sourceVersionId: chosen.versionId, startByte: 0, endByte: new TextEncoder().encode(detail.data.fullText).length });
  }, [chosen, detail.data, onPick]);
  const sources = (workspace.data?.sources ?? []).filter((source) => source.activeVersion);
  if (workspace.isPending) return <p role="status" className="text-xs text-text-muted">Loading your sources…</p>;
  if (sources.length === 0) return <p className="text-xs text-text-muted">The course has no saved sources yet.</p>;
  return (
    <fieldset className="max-h-56 space-y-1 overflow-y-auto">
      <legend className="sr-only">Your sources</legend>
      {sources.map((source) => (
        <label key={source.id} className="row-hover flex min-h-10 cursor-pointer items-center gap-2 rounded-lg px-2 text-sm text-text-primary">
          <input type="radio" name={`saved-${programId}`} checked={chosen?.sourceId === source.id}
            onChange={() => setChosen({ sourceId: source.id, versionId: source.activeVersion!.id })} />
          <span className="min-w-0 truncate">{source.activeVersion!.title}</span>
        </label>
      ))}
    </fieldset>
  );
}

/**
 * Add a source : a link, pasted text, a library file or a saved
 * source becomes a course source and is checked against the claims in
 * scope, with each stage shown as it happens. Opened on a claim, on a group
 * or on the whole queue; also by a drop or paste anywhere on the screen.
 */
export function AddSourceSheet({ programId, lessonId, revision, target, initial, remaining, onClose, onAdded }: {
  programId: string;
  lessonId: string;
  revision: number;
  target: AddSourceTarget;
  /** What a drop or paste brought; it fills the matching input. */
  initial: DroppedSource | null;
  /** Waiting claims besides those in scope, for the result line. */
  remaining: number;
  onClose: () => void;
  /** The attach saved a source; the sheet closes and the screen announces the line. */
  onAdded: (report: ClaimAttachReportDto, summary: AttachSummary, scope: string[]) => void;
}) {
  const id = useId();
  const add = useAddSource(programId, lessonId);
  const [mode, setMode] = useState<Mode>(initial?.kind === 'text' ? 'paste' : initial?.kind === 'document' ? 'file' : 'link');
  const [url, setUrl] = useState(initial?.kind === 'link' ? initial.url : '');
  const [origin, setOrigin] = useState(initial?.kind === 'text' ? initial.origin ?? '' : '');
  const [text, setText] = useState(initial?.kind === 'text' ? initial.text : '');
  const [documents, setDocuments] = useState<LearningDocumentSelection[]>(initial?.kind === 'document' ? [{ id: initial.documentId, title: initial.name, spaceName: 'Library' }] : []);
  const [saved, setSaved] = useState<AttachEvidenceDto | null>(null);
  // A dropped file is already imported: it shows as itself, with the library picker one click away.
  const [picking, setPicking] = useState(initial?.kind !== 'document');
  const [withOthers, setWithOthers] = useState(Boolean(target.others?.length));
  const [formError, setFormError] = useState('');
  const [failed, setFailed] = useState<{ report: ClaimAttachReportDto; host: string | null } | null>(null);
  const [last, setLast] = useState<AttachEvidenceDto | null>(null);

  // A later drop or paste while the sheet is open replaces what it holds.
  useEffect(() => {
    if (!initial) return;
    if (initial.kind === 'link') { setMode('link'); setUrl(initial.url); }
    if (initial.kind === 'text') { setMode('paste'); setText(initial.text); setOrigin(initial.origin ?? ''); }
    if (initial.kind === 'document') { setMode('file'); setPicking(false); setDocuments([{ id: initial.documentId, title: initial.name, spaceName: 'Library' }]); }
  }, [initial]);

  const ids = target.level === 'claim' && withOthers ? [...target.claimIds, ...(target.others ?? [])] : target.claimIds;
  const scope: AttachScopeDto = ids.length > 1 ? { scope: 'claims', claimIds: ids } : { scope: 'claim' };
  const title = addSourceTitle(target.level, target.claimIds.length);

  const evidenceFor = (current: Mode): AttachEvidenceDto | string => {
    if (current === 'link') return url.trim() ? { kind: 'attach_link', url: url.trim() } : 'Enter the address of the page.';
    if (current === 'file') return documents[0] ? { kind: 'attach_document', documentId: documents[0].id } : 'Choose a file.';
    if (current === 'saved') return saved ?? 'Choose one of your sources.';
    return text.trim() ? { kind: 'attach_text', title: origin.trim() || 'Pasted text', text } : 'Paste the text that states the claim.';
  };

  const run = async (evidence: AttachEvidenceDto) => {
    setFormError('');
    setFailed(null);
    setLast(evidence);
    try {
      const report = await add.mutateAsync({ claimId: ids[0], expectedRevision: revision, evidence, scope });
      if (!report) return; // cancelled
      const host = evidence.kind === 'attach_link' ? hostOf(evidence.url) : null;
      if (report.failure || !report.source) { setFailed({ report, host }); return; }
      onAdded(report, attachSummary(report, ids.length, Math.max(0, remaining)), ids);
    } catch (error) {
      setFormError(error instanceof Error ? error.message : 'The source could not be added.');
    }
  };

  const submit = () => {
    const evidence = evidenceFor(mode);
    if (typeof evidence === 'string') { setFormError(evidence); return; }
    void run(evidence);
  };

  const nextStep = (step: AttachNextStepDto) => {
    setFailed(null);
    if (step.step === 'retry') { if (last) void run(last); return; }
    if (step.step === 'paste_text') { setMode('paste'); if (step.origin && !origin) setOrigin(step.origin); }
    else setMode('file');
  };

  const busy = add.isPending;
  const progress = add.progress;
  const checking = progress?.stage === 'checking' ? progress : null;
  const modes: Array<{ id: Mode; label: string; icon: typeof Globe2 }> = [
    { id: 'link', label: 'A link', icon: Globe2 },
    { id: 'paste', label: 'Paste text instead', icon: Type },
    { id: 'file', label: 'Choose a file…', icon: FileText },
    { id: 'saved', label: 'Pick a saved source…', icon: Library },
  ];

  return (
    <section aria-labelledby={`${id}-title`} aria-busy={busy} className="rounded-xl border border-border-default bg-surface p-4 shadow-xs">
      <div className="flex items-start justify-between gap-3">
        <h3 id={`${id}-title`} className="text-sm font-semibold text-text-primary">{title}</h3>
        <button type="button" aria-label="Close" onClick={() => { add.cancel(); onClose(); }} className="rounded-md p-1 text-text-muted hover:text-text-primary"><X size={16} aria-hidden="true" /></button>
      </div>
      <p className="mt-1 text-xs leading-5 text-text-secondary">{ADD_SOURCE_HINT}</p>

      <div role="group" aria-label="Kind of source" className="mt-3 flex flex-wrap gap-2">
        {modes.map(({ id: kind, label, icon: Icon }) => (
          <Button key={kind} type="button" size="sm" variant={mode === kind ? 'secondary' : 'ghost'} aria-pressed={mode === kind} disabled={busy}
            onClick={() => { setMode(kind); setFormError(''); }}><Icon aria-hidden="true" />{label}</Button>
        ))}
      </div>

      <div className="mt-3">
        {mode === 'link' && <>
          <label htmlFor={`${id}-url`} className="sr-only">Page address</label>
          <input id={`${id}-url`} type="url" inputMode="url" autoFocus value={url} disabled={busy} placeholder="https://"
            onChange={(event) => setUrl(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') submit(); }}
            className="w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus:border-accent focus:outline-hidden" />
        </>}
        {mode === 'paste' && <div className="space-y-2">
          <label htmlFor={`${id}-origin`} className="block text-xs font-medium text-text-secondary">Where it is from</label>
          <input id={`${id}-origin`} value={origin} disabled={busy} onChange={(event) => setOrigin(event.target.value)} placeholder="Title or address of the original" className="w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus:border-accent focus:outline-hidden" />
          <label htmlFor={`${id}-text`} className="block text-xs font-medium text-text-secondary">Text</label>
          <textarea id={`${id}-text`} value={text} disabled={busy} onChange={(event) => setText(event.target.value)} rows={5} className="w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus:border-accent focus:outline-hidden" />
        </div>}
        {mode === 'file' && (picking
          ? <LearningDocumentPicker selected={documents} onChange={setDocuments} limit={1} />
          : <p className="flex flex-wrap items-center gap-2 text-sm text-text-primary">
            <FileText size={14} aria-hidden="true" />{documents[0]?.title}
            <button type="button" disabled={busy} onClick={() => setPicking(true)} className="text-xs text-accent underline underline-offset-2">Choose another</button>
          </p>)}
        {mode === 'saved' && <SavedSourcePicker programId={programId} onPick={setSaved} />}
      </div>

      {target.level === 'claim' && (target.others?.length ?? 0) > 0 && <label className="mt-3 flex items-center gap-2 text-sm text-text-secondary">
        <input type="checkbox" checked={withOthers} disabled={busy} onChange={(event) => setWithOthers(event.target.checked)} />
        Also check the {target.others!.length} other {target.others!.length === 1 ? 'claim that waits' : 'claims that wait'} on this source’s subject
      </label>}

      {busy && <div role="status" className="mt-3 space-y-1.5">
        <p className="flex items-center gap-2 text-xs text-text-secondary"><LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />{progress ? progressLine(progress) : 'Starting…'}</p>
        {checking && <div role="progressbar" aria-label="Claims checked" aria-valuemin={0} aria-valuemax={checking.total} aria-valuenow={checking.done} className="h-1.5 overflow-hidden rounded-full bg-surface-sunken">
          <div className="h-full rounded-full bg-accent transition-[width]" style={{ width: `${checking.total ? (checking.done / checking.total) * 100 : 0}%` }} />
        </div>}
      </div>}

      {failed && <div className="mt-3"><AddSourceResult programId={programId} report={failed.report} line={null} host={failed.host} onNextStep={nextStep} /></div>}
      {formError && <p role="alert" className="mt-3 text-xs text-danger-fg">{formError}</p>}

      <div className="mt-4 flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={() => { add.cancel(); onClose(); }}>Cancel</Button>
        <Button type="button" disabled={busy} onClick={submit}>{busy ? 'Checking…' : 'Add and check'}</Button>
      </div>
    </section>
  );
}
