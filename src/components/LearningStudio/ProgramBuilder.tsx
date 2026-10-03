import { type FormEvent, useState } from 'react';

import { ArrowRight, BookOpen, Link2, Sparkles } from 'lucide-react';

import { useLibraryDocumentsQuery } from '@/hooks/queries/useLibraryDocumentsQuery';
import type { GenerateLearningProgramRequestDto } from '@/lib/bindings';

export function ProgramBuilder({ pending, error, onGenerate, onCancel }: {
  pending: boolean; error?: string; onGenerate: (request: GenerateLearningProgramRequestDto) => void; onCancel: () => void;
}) {
  const { documents, isLoading, error: documentsError, refreshFiles } = useLibraryDocumentsQuery();
  const [goal, setGoal] = useState('');
  const [prior, setPrior] = useState('');
  const [minutes, setMinutes] = useState(30);
  const [urls, setUrls] = useState('');
  const [documentIds, setDocumentIds] = useState<string[]>([]);
  const [formError, setFormError] = useState('');

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const sourceUrls = urls.split(/[\n,]/).map((url) => url.trim()).filter(Boolean);
    if (!goal.trim()) return setFormError('Describe what you want to be able to do.');
    if (goal.trim().length > 500) return setFormError('Keep your learning goal to 500 characters or fewer.');
    if (documentIds.length > 8) return setFormError('Choose no more than 8 library documents.');
    if (sourceUrls.length > 8) return setFormError('Add no more than 8 reference URLs.');
    if (documentIds.length + sourceUrls.length > 12) return setFormError('Choose no more than 12 sources in total.');
    if (!documentIds.length && !sourceUrls.length) return setFormError('Choose at least one document or reference URL to ground the program.');
    for (const url of sourceUrls) {
      try { if (!['http:', 'https:'].includes(new URL(url).protocol)) throw new Error(); }
      catch { return setFormError(`Use a complete http or https URL: ${url}`); }
    }
    setFormError('');
    onGenerate({ goal: goal.trim(), priorKnowledge: prior.trim(), minutesPerSession: minutes, documentIds, sourceUrls });
  };

  return (
    <div className="mx-auto grid max-w-6xl gap-8 xl:grid-cols-[minmax(0,1fr)_360px]">
      <form onSubmit={submit} className="rounded-2xl border border-border/70 bg-surface p-7 shadow-sm sm:p-9">
        <button type="button" onClick={onCancel} className="mb-8 text-sm text-text-muted hover:text-text-primary">← Back to programs</button>
        <div className="mb-7 flex items-center gap-3 text-accent"><Sparkles size={18} /><span className="text-xs font-semibold uppercase tracking-[.18em]">Build a learning program</span></div>
        <h2 className="font-serif text-3xl leading-tight text-text-primary sm:text-[2.5rem]">What would you like to learn?</h2>
        <p className="mt-3 max-w-xl text-sm leading-6 text-text-secondary">Studio will shape your sources into a course outline you can review before any lesson is prepared.</p>

        <label className="mt-8 block text-sm font-medium text-text-primary" htmlFor="learning-goal">Your goal</label>
        <textarea id="learning-goal" value={goal} onChange={(e) => setGoal(e.target.value)} rows={3} maxLength={500} placeholder="For example: understand the core ideas in these materials and use them to build a small project" className="mt-2 w-full resize-y rounded-xl border border-border bg-background px-4 py-3 text-sm leading-6 outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/15" /><div className="mt-1 text-right text-[10px] tabular-nums text-text-muted">{goal.length}/500</div>
        <label className="mt-5 block text-sm font-medium text-text-primary" htmlFor="prior-knowledge">What do you already know?</label>
        <textarea id="prior-knowledge" value={prior} onChange={(e) => setPrior(e.target.value)} rows={2} maxLength={1200} placeholder="A little context helps set a useful starting point. You can leave this blank." className="mt-2 w-full resize-y rounded-xl border border-border bg-background px-4 py-3 text-sm leading-6 outline-none focus:border-accent focus:ring-2 focus:ring-accent/15" />
        <label className="mt-5 block text-sm font-medium text-text-primary" htmlFor="session-time">Time for each session</label>
        <div className="mt-2 flex items-center gap-3"><input id="session-time" type="range" min="15" max="90" step="5" value={minutes} onChange={(e) => setMinutes(Number(e.target.value))} className="w-52 accent-accent" /><span className="text-sm tabular-nums text-text-secondary">{minutes} minutes</span></div>

        <div className="mt-8 border-t border-border pt-6">
          <h3 className="text-sm font-semibold text-text-primary">Choose your materials</h3>
          <p className="mt-1 text-xs leading-5 text-text-muted">Program content will be grounded in sources you select here.</p>
          {documentsError && <div role="alert" className="mt-3 flex items-center justify-between rounded-lg bg-rose-500/10 p-3 text-xs text-rose-700"><span>Library could not be loaded: {documentsError}</span><button type="button" onClick={() => void refreshFiles()} className="underline">Retry</button></div>}
          {isLoading ? <p className="mt-4 text-sm text-text-muted">Loading your library…</p> : documents.length ? (
            <div className="mt-3 max-h-52 space-y-1 overflow-y-auto rounded-xl border border-border p-2">
              {documents.map((document) => <label key={document.id} className="flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-sm hover:bg-background"><input type="checkbox" checked={documentIds.includes(document.id)} disabled={!documentIds.includes(document.id) && documentIds.length >= 8} onChange={(e) => setDocumentIds((ids) => e.target.checked ? [...ids, document.id] : ids.filter((id) => id !== document.id))} className="accent-accent" /><BookOpen size={15} className="shrink-0 text-text-muted" /><span className="truncate text-text-secondary">{document.fileName}</span></label>)}
            </div>
          ) : !documentsError ? <p className="mt-3 rounded-lg bg-background p-3 text-xs text-text-muted">No library documents yet. Add a reference URL below to continue.</p> : null}
          {documentIds.length >= 8 && <p className="mt-2 text-[10px] text-text-muted">Maximum 8 selected documents. Remove one to choose another.</p>}
          <label className="mt-4 block text-xs font-medium text-text-secondary" htmlFor="reference-urls">Reference URLs <span className="font-normal text-text-muted">(one per line)</span></label>
          <div className="mt-2 flex items-start gap-2 rounded-xl border border-border bg-background px-3 py-2"><Link2 size={15} className="mt-1 shrink-0 text-text-muted" /><textarea id="reference-urls" value={urls} onChange={(e) => setUrls(e.target.value)} rows={2} placeholder="https://…" className="w-full resize-y bg-transparent text-sm outline-none placeholder:text-text-muted" /></div>
        </div>
        {(formError || error) && <p role="alert" className="mt-5 rounded-lg bg-rose-500/10 px-4 py-3 text-sm text-rose-700">{formError || `The outline could not be generated: ${error}`}</p>}
        <div className="mt-7 flex items-center justify-between gap-4"><span className="text-xs text-text-muted">Your outline starts as a draft. No progress is recorded yet.</span><button type="submit" disabled={pending} className="inline-flex items-center gap-2 rounded-full bg-accent px-5 py-3 text-sm font-semibold text-accent-fg shadow-sm transition hover:brightness-95 disabled:cursor-wait disabled:opacity-60">{pending ? 'Building outline…' : 'Create outline'} {!pending && <ArrowRight size={16} />}</button></div>
      </form>
      <aside className="hidden rounded-2xl bg-[#eee8df] p-7 xl:block dark:bg-white/5">
        <div className="text-xs font-semibold uppercase tracking-[.16em] text-accent">A considered course</div><h3 className="mt-5 font-serif text-2xl leading-snug text-text-primary">Made from what matters to you.</h3><p className="mt-4 text-sm leading-6 text-text-secondary">First, review the complete module journey and outcomes. Then prepare lessons one at a time, when you are ready to work through them.</p>
        <div className="mt-8 space-y-4 border-t border-black/10 pt-5 dark:border-white/10">{['Specific outcomes', 'Sources in context', 'Your pace, your call'].map((item, i) => <div className="flex gap-3" key={item}><span className="grid h-6 w-6 shrink-0 place-items-center rounded-full bg-white/70 font-serif text-sm text-accent dark:bg-white/10">0{i + 1}</span><span className="pt-1 text-sm text-text-secondary">{item}</span></div>)}</div>
      </aside>
    </div>
  );
}
