import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { settingsFieldClass } from '@/components/ui/SettingsSection';
import VaultAPI from '@/lib/api';
import { getErrorMessage } from '@/lib/errorUtils';
import type { DocumentMetadata } from '@/types/fileBrowser';

export function LibraryFilePicker({ onAdd, disabled }: { onAdd: (paths: string[]) => void; disabled: boolean }) {
  const [documents, setDocuments] = useState<DocumentMetadata[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const load = async () => {
    setLoading(true); setError('');
    try {
      const result = await VaultAPI.listAllDocuments(10000);
      if (!result.ok) throw new Error(result.error);
      setDocuments(result.data.filter(document => document.filePath && !/^https?:/.test(document.filePath)));
    } catch (err) { setError(getErrorMessage(err)); } finally { setLoading(false); }
  };
  const visible = documents?.filter(doc => `${doc.fileName} ${doc.filePath}`.toLowerCase().includes(filter.toLowerCase())) ?? [];
  return <div className="mt-3 space-y-3">
    <Button variant="ghost" disabled={disabled || loading} onClick={() => { if (documents) setDocuments(null); else void load(); }}>
      {loading ? 'Loading library…' : documents ? 'Close library picker' : 'Choose files already in library'}
    </Button>
    {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
    {documents && <div className="space-y-3 rounded border border-border-subtle p-3">
      <p className="text-sm text-text-secondary">Update related-source settings and rebuild search using the saved files. Existing documents and their spaces stay linked.</p>
      <input aria-label="Filter library files" placeholder="Find files by name" value={filter} onChange={event => setFilter(event.target.value)} className={`${settingsFieldClass} w-full`} />
      <div className="flex items-center justify-between text-xs">
        <span>{visible.length} files · {selected.size} selected (up to 100)</span>
        <button type="button" disabled={disabled || visible.length > 100} className="text-accent" onClick={() => setSelected(new Set(visible.map(doc => doc.filePath)))}>Select matching files</button>
      </div>
      <div className="max-h-60 overflow-y-auto">
        {visible.map(doc => <label key={doc.id} className="flex items-center gap-2 py-2 text-sm">
          <input type="checkbox" checked={selected.has(doc.filePath)} disabled={disabled || (!selected.has(doc.filePath) && selected.size >= 100)} onChange={event => setSelected(current => {
            const next = new Set(current); if (event.target.checked) next.add(doc.filePath); else next.delete(doc.filePath); return next;
          })} />
          <span className="truncate" title={doc.filePath}>{doc.fileName}</span>
        </label>)}
      </div>
      <Button disabled={disabled || selected.size === 0} onClick={() => { onAdd(Array.from(selected)); setDocuments(null); setSelected(new Set()); }}>Use {selected.size} saved {selected.size === 1 ? 'file' : 'files'}</Button>
    </div>}
  </div>;
}
