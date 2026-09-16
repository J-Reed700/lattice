import { useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { PageHeader } from '@/components/ui/PageHeader';
import { settingsFieldClass } from '@/components/ui/SettingsSection';

import { StudyGenerations } from './StudyGenerations';
import { useGenerateStudyDeck, useStudyDocuments, useStudyGenerations } from './useStudy';

export function NewStudyDeck({ onCreated, onCancel }: { onCreated: (id: string) => void; onCancel: () => void }) {
  const documents = useStudyDocuments();
  const generate = useGenerateStudyDeck();
  const generations = useStudyGenerations();
  const [title, setTitle] = useState('New study deck');
  const [focus, setFocus] = useState('');
  const [studyGoal, setStudyGoal] = useState('');
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [count, setCount] = useState(6);
  const available = useMemo(() => (documents.data ?? []).filter(doc => `${doc.fileName} ${doc.filePath}`.toLowerCase().includes(search.toLowerCase())).sort((a, b) => a.fileName.localeCompare(b.fileName)), [documents.data, search]);
  if (!generate.isPending && generations.some(item => item.status === 'pending')) return <>
    <PageHeader title="Study" /><StudyGenerations /><Button variant="ghost" onClick={onCancel}>All decks</Button>
  </>;
  return <>
    <PageHeader title="New deck" />
    <form className="space-y-6" onSubmit={event => {
      event.preventDefault();
      if (!generate.isPending) generate.mutate({ title, focus, studyGoal, documentIds: selected, count }, { onSuccess: deck => onCreated(deck.id) });
    }}>
      <div>
        <label htmlFor="study-title" className="mb-2 block text-sm text-text-primary">Deck title</label>
        <input id="study-title" className={`${settingsFieldClass} w-full`} required maxLength={120} value={title} onChange={e => setTitle(e.target.value)} disabled={generate.isPending} />
      </div>
      <div>
        <label htmlFor="study-focus" className="mb-2 block text-sm text-text-primary">Topic or section <span className="text-text-muted">· optional</span></label>
        <input id="study-focus" className={`${settingsFieldClass} w-full`} placeholder="e.g. photosynthesis, chapter 3, project planning" maxLength={200} value={focus} onChange={e => setFocus(e.target.value)} disabled={generate.isPending} />
      </div>
      <div>
        <label htmlFor="study-goal" className="mb-2 block text-sm text-text-primary">Learning goal <span className="text-text-muted">· optional</span></label>
        <input id="study-goal" className={`${settingsFieldClass} w-full`} placeholder="e.g. exam preparation, professional training, general understanding" maxLength={300} value={studyGoal} onChange={e => setStudyGoal(e.target.value)} disabled={generate.isPending} />
        <p className="mt-2 text-xs text-text-muted">Guides the question style and difficulty. Answers still come from your documents.</p>
      </div>
      <fieldset disabled={generate.isPending}>
        <legend className="mb-2 text-sm text-text-primary">Source documents · {selected.length} of 3 selected</legend>
        <p className="mb-3 text-xs text-text-muted">Choose up to three documents per deck. Documents appear as their imports finish.</p>
        <input aria-label="Search source documents" className={`${settingsFieldClass} mb-3 w-full`} placeholder="Search documents" value={search} onChange={e => setSearch(e.target.value)} />
        {documents.isPending && <p className="py-4 text-sm text-text-muted">Loading documents…</p>}
        {documents.isError && <p role="alert" className="text-sm text-danger-fg">{documents.error.message}</p>}
        <div className="max-h-64 overflow-y-auto border-t border-border-subtle">
          {available.map(doc => <label key={doc.id} className="flex cursor-pointer items-center gap-3 border-b border-border-subtle py-3 text-sm text-text-primary">
            <input type="checkbox" className="h-4 w-4 accent-accent" checked={selected.includes(doc.id)} disabled={!selected.includes(doc.id) && selected.length >= 3} onChange={e => setSelected(prev => e.target.checked ? [...prev, doc.id] : prev.filter(id => id !== doc.id))} />
            <span className="min-w-0 truncate">{doc.fileName}</span>
          </label>)}
          {!documents.isPending && !available.length && <p className="py-4 text-sm text-text-muted">{search ? 'No matching documents.' : 'No indexed documents yet.'}</p>}
        </div>
      </fieldset>
      <div className="flex items-center justify-between gap-4 border-y border-border-subtle py-3">
        <label htmlFor="study-count" className="text-sm text-text-primary">Questions</label>
        <select id="study-count" className={settingsFieldClass} value={count} disabled={generate.isPending} onChange={e => setCount(Number(e.target.value))}>
          {[2, 4, 6, 8, 10, 12].map(n => <option key={n} value={n}>{n}</option>)}
        </select>
      </div>
      <p className="text-xs leading-relaxed text-text-muted">AI-generated practice from a sample of passages in your selected documents. Each question includes its source so you can check the answer in context.</p>
      {generate.isError && <p role="alert" className="text-sm text-danger-fg">{generate.error.message}</p>}
      {generate.isPending && <p role="status" className="text-sm text-text-secondary">Reading passages and writing questions… You can keep using Lattice while this finishes.</p>}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onCancel} disabled={generate.isPending}>Back</Button>
        <Button type="submit" disabled={generate.isPending || !title.trim() || selected.length === 0}>{generate.isPending ? 'Generating…' : 'Generate deck'}</Button>
      </div>
    </form>
  </>;
}
