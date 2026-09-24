import { useMemo, useState, useSyncExternalStore } from 'react';

import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';

import { diagnostics, type DiagnosticLevel } from '../../utils/diagnostics';
import { PageHeader } from '../ui';

const buttonClass = 'rounded-sm border border-border-default bg-surface px-3 py-1.5 text-sm text-text-secondary hover:bg-surface-raised disabled:opacity-50';
const tones: Record<DiagnosticLevel, string> = {
  error: 'text-danger-fg bg-danger-muted',
  warn: 'text-text-primary bg-surface-raised',
  info: 'text-accent bg-accent-muted',
  debug: 'text-text-muted bg-surface-raised',
};
export function LogsTab() {
  const entries = useSyncExternalStore(diagnostics.subscribe, diagnostics.getSnapshot);
  const [search, setSearch] = useState('');
  const [level, setLevel] = useState('all');
  const [clearRequested, setClearRequested] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [notice, setNotice] = useState('');
  const filtered = useMemo(() => entries.filter(entry =>
    (level === 'all' || entry.level === level) &&
    `${entry.message} ${entry.source} ${entry.details}`.toLowerCase().includes(search.toLowerCase())
  ), [entries, level, search]);
  const exportLogs = async () => {
    setExporting(true);
    setNotice('');
    try {
      const path = await save({ defaultPath: `lattice-logs-${new Date().toISOString().slice(0, 10)}.json`, filters: [{ name: 'JSON', extensions: ['json'] }] });
      if (path) { await writeTextFile(path, diagnostics.export()); setNotice('Logs exported.'); }
    } catch (error) {
      diagnostics.capture(error, 'Log export');
      setNotice('Could not export logs. See the newest entry for details.');
    } finally { setExporting(false); }
  };
  return (
    <>
      <PageHeader title="Logs" meta="Local diagnostics · Latest 300 entries" actions={
        <>
          <button type="button" className={buttonClass} disabled={!entries.length || exporting} onClick={() => { void exportLogs(); }}>{exporting ? 'Exporting…' : 'Export logs'}</button>
          <button type="button" className={buttonClass} disabled={!entries.length} onClick={() => setClearRequested(true)}>Clear</button>
        </>
      } />
      <div className="mb-6 grid grid-cols-3 gap-3">
        {(['error', 'warn', 'info'] as const).map(severity => (
          <button type="button" key={severity} aria-pressed={level === severity} onClick={() => setLevel(level === severity ? 'all' : severity)} className="rounded-md border border-border-subtle bg-surface p-4 text-left hover:bg-surface-raised">
            <span className="block text-2xl tabular-nums text-text-primary">{entries.filter(entry => entry.level === severity).reduce((sum, entry) => sum + entry.count, 0)}</span>
            <span className="text-xs text-text-muted">{severity === 'error' ? 'Errors' : severity === 'warn' ? 'Warnings' : 'Information'}</span>
          </button>
        ))}
      </div>
      <p className="mb-5 text-sm text-text-muted">Errors, background operations, and application events are stored on this device. Common secrets are redacted. Review exported details before sharing.</p>
      <div className="mb-4 flex gap-2">
        <input aria-label="Search logs" placeholder="Search messages, sources, or details…" value={search} onChange={event => setSearch(event.target.value)} className="min-w-0 flex-1 rounded-sm border border-border-default bg-surface px-3 py-2 text-sm text-text-primary" />
        <select aria-label="Log severity" value={level} onChange={event => setLevel(event.target.value)} className={buttonClass}>
          <option value="all">All levels</option><option value="error">Errors</option><option value="warn">Warnings</option><option value="info">Information</option><option value="debug">Debug</option>
        </select>
      </div>
      {clearRequested && <div className="mb-4 rounded-md border border-border-default bg-surface p-4">
        <p className="mb-3 text-sm text-text-primary">Clear all saved logs? This cannot be undone.</p>
        <div className="flex gap-2"><button className={buttonClass} onClick={() => { diagnostics.clear(); setClearRequested(false); setNotice('Logs cleared.'); }}>Clear all logs</button><button className={buttonClass} onClick={() => setClearRequested(false)}>Cancel</button></div>
      </div>}
      <p role="status" className="mb-3 text-xs text-text-muted">{notice || `${filtered.length} of ${entries.length} entries · Updates live`}</p>
      <div className="overflow-hidden rounded-md border border-border-subtle bg-surface">
        {filtered.length === 0 ? <div className="px-6 py-12 text-center"><p className="text-sm text-text-primary">{entries.length ? 'No matching entries' : 'No events recorded'}</p><p className="mt-1 text-xs text-text-muted">{entries.length ? 'Try another search or severity.' : 'New events will appear here automatically.'}</p></div> : filtered.map(entry => (
          <details key={entry.id} className="border-b border-border-subtle last:border-b-0">
            <summary className="cursor-pointer px-4 py-3 text-sm hover:bg-surface-raised">
              <span className={`ml-1 mr-2 inline-block rounded px-1.5 py-0.5 text-[10px] font-medium uppercase ${tones[entry.level]}`}>{entry.level}</span>
              <span className="break-words text-text-primary">{entry.message}</span>
              {entry.count > 1 && <span className="ml-2 text-xs text-text-muted">×{entry.count}</span>}
              <span className="mt-1 block pl-5 text-xs text-text-muted">{entry.source} · {new Date(entry.lastSeen).toLocaleString()}</span>
            </summary>
            <div className="border-t border-border-subtle bg-bg px-4 py-3">
              <p className="mb-2 text-xs text-text-muted">First seen {new Date(entry.timestamp).toLocaleString()} · Event {entry.id}</p>
              <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-words font-mono text-xs leading-relaxed text-text-secondary">{entry.details || 'No additional details.'}</pre>
            </div>
          </details>
        ))}
      </div>
    </>
  );
}
