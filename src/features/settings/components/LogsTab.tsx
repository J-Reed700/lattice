import { useMemo, useState, useSyncExternalStore } from 'react';

import { Search } from 'lucide-react';

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  PageHeader,
} from '@/components/ui';
import { describeFailure } from '@/lib/errorHandling';
import { cn } from '@/lib/utils';
import { diagnostics } from '@/utils/diagnostics';

import {
  exportLogEntries,
  fromBackend,
  mergeNewestFirst,
  type LogLevel,
  type LogOrigin,
} from './logEntries';
import { SettingsSelect } from './SettingControls';
import { useBackendLogsQuery } from '../hooks/useBackendLogsQuery';
import { useJsonFileDialogs } from '../hooks/useJsonFileDialogs';

const tones: Record<LogLevel, string> = {
  error: 'text-danger-fg bg-danger-muted',
  warn: 'text-text-primary bg-surface-raised',
  info: 'text-accent bg-accent-muted',
  debug: 'text-text-muted bg-surface-raised',
  unknown: 'text-text-muted bg-surface-raised',
};
const originLabels: Record<LogOrigin, string> = { app: 'App', backend: 'Backend' };

const SOURCE_OPTIONS = [
  { value: 'all', label: 'All sources' },
  { value: 'app', label: 'App' },
  { value: 'backend', label: 'Backend' },
];

const SEVERITY_OPTIONS = [
  { value: 'all', label: 'All levels' },
  { value: 'error', label: 'Errors' },
  { value: 'warn', label: 'Warnings' },
  { value: 'info', label: 'Information' },
  { value: 'debug', label: 'Debug' },
];

export function LogsTab() {
  const appEntries = useSyncExternalStore(diagnostics.subscribe, diagnostics.getSnapshot);
  const backendLogs = useBackendLogsQuery();
  const { saveJsonFile } = useJsonFileDialogs();
  const backendEntries = useMemo(() => fromBackend(backendLogs.data ?? []), [backendLogs.data]);
  const entries = useMemo(
    () => mergeNewestFirst(appEntries, backendEntries),
    [appEntries, backendEntries],
  );
  const [search, setSearch] = useState('');
  const [level, setLevel] = useState('all');
  const [origin, setOrigin] = useState('all');
  const [clearRequested, setClearRequested] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [notice, setNotice] = useState('');
  const scoped = useMemo(
    () => entries.filter((entry) => origin === 'all' || entry.origin === origin),
    [entries, origin],
  );
  const filtered = useMemo(
    () =>
      scoped.filter(
        (entry) =>
          (level === 'all' || entry.level === level) &&
          `${entry.message} ${entry.source} ${entry.details}`
            .toLowerCase()
            .includes(search.toLowerCase()),
      ),
    [scoped, level, search],
  );

  const exportLogs = async () => {
    setExporting(true);
    setNotice('');
    try {
      const saved = await saveJsonFile(
        `lattice-logs-${new Date().toISOString().slice(0, 10)}.json`,
        exportLogEntries(entries),
      );
      if (!saved.ok) throw new Error(describeFailure(saved));
      if (saved.data) setNotice('Logs exported.');
    } catch (error) {
      diagnostics.capture(error, 'Log export');
      setNotice('Could not export logs. See the newest entry for details.');
    } finally {
      setExporting(false);
    }
  };

  const summary = backendLogs.isError
    ? 'Could not read the backend log. Try Refresh.'
    : `${filtered.length} of ${entries.length} entries · App events update live`;

  return (
    <>
      <PageHeader
        title="Logs"
        meta="Local diagnostics · Latest 300 from each source"
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              disabled={backendLogs.isFetching}
              onClick={() => {
                setNotice('');
                void backendLogs.refetch();
              }}
            >
              Refresh
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={!entries.length || exporting}
              onClick={() => {
                void exportLogs();
              }}
            >
              {exporting ? 'Exporting…' : 'Export logs'}
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={!appEntries.length}
              onClick={() => setClearRequested(true)}
            >
              Clear
            </Button>
          </>
        }
      />

      <div className="mb-6 grid grid-cols-3 gap-3">
        {(['error', 'warn', 'info'] as const).map((severity) => {
          const isSelected = level === severity;
          return (
            <button
              type="button"
              key={severity}
              aria-pressed={isSelected}
              onClick={() => setLevel(isSelected ? 'all' : severity)}
              className={cn(
                'rounded-xl border p-4 text-left transition-[border-color,background-color] duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                isSelected
                  ? 'border-accent bg-accent-muted'
                  : 'border-border-subtle bg-surface hover:border-border-strong hover:bg-surface-raised',
              )}
            >
              <span className="block text-2xl font-medium tabular-nums text-text-primary">
                {scoped
                  .filter((entry) => entry.level === severity)
                  .reduce((sum, entry) => sum + entry.count, 0)}
              </span>
              <span className="text-xs text-text-muted">
                {severity === 'error' ? 'Errors' : severity === 'warn' ? 'Warnings' : 'Information'}
              </span>
            </button>
          );
        })}
      </div>

      <p className="mb-5 text-xs leading-relaxed text-text-muted">
        App events and the backend log are stored on this device. Common secrets and home folder
        names are redacted. Review exported details before sharing.
      </p>

      <div className="mb-4 flex flex-wrap items-center gap-2">
        <div className="min-w-[12rem] flex-1">
          <Input
            aria-label="Search logs"
            placeholder="Search messages, sources, or details…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            leftIcon={<Search className="h-3.5 w-3.5" aria-hidden="true" />}
            showClearButton
            onClear={() => setSearch('')}
          />
        </div>
        <div className="w-36 shrink-0">
          <SettingsSelect
            aria-label="Log source"
            value={origin}
            onValueChange={setOrigin}
            options={SOURCE_OPTIONS}
          />
        </div>
        <div className="w-36 shrink-0">
          <SettingsSelect
            aria-label="Log severity"
            value={level}
            onValueChange={setLevel}
            options={SEVERITY_OPTIONS}
          />
        </div>
      </div>

      <p role="status" className="mb-3 text-xs text-text-muted">
        {notice || summary}
      </p>

      <div className="overflow-hidden border-t border-b border-border-subtle">
        {filtered.length === 0 ? (
          <div className="px-6 py-12 text-center">
            <p className="text-sm text-text-primary">
              {scoped.length ? 'No matching entries' : 'No events recorded'}
            </p>
            <p className="mt-1 text-xs text-text-muted">
              {scoped.length
                ? 'Try another search, source, or severity.'
                : 'New events will appear here.'}
            </p>
          </div>
        ) : (
          filtered.map((entry) => (
            <details key={entry.id} className="border-b border-border-subtle last:border-b-0">
              <summary className="cursor-pointer px-3 py-3 text-sm transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.03)]">
                <span
                  className={`ml-1 mr-2 inline-block rounded px-1.5 py-0.5 text-[10px] font-medium uppercase ${tones[entry.level]}`}
                >
                  {entry.level}
                </span>
                <span className="wrap-break-word text-text-primary">{entry.message}</span>
                {entry.count > 1 && (
                  <span className="ml-2 text-xs tabular-nums text-text-muted">×{entry.count}</span>
                )}
                <span className="mt-1 block pl-5 text-xs text-text-muted">
                  {originLabels[entry.origin]} · {entry.source}
                  {entry.lastSeen && ` · ${new Date(entry.lastSeen).toLocaleString()}`}
                </span>
              </summary>
              <div className="border-t border-border-subtle bg-surface-sunken/60 px-4 py-3">
                <p className="mb-2 text-xs text-text-muted">
                  {entry.origin === 'app'
                    ? `First seen ${new Date(entry.timestamp).toLocaleString()} · Event ${entry.id}`
                    : `Backend log line · ${entry.timestamp || 'No timestamp'}`}
                </p>
                <pre className="max-h-80 overflow-auto whitespace-pre-wrap wrap-break-word font-mono text-xs leading-relaxed text-text-secondary">
                  {entry.details || 'No additional details.'}
                </pre>
              </div>
            </details>
          ))
        )}
      </div>

      <Dialog open={clearRequested} onOpenChange={setClearRequested}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle className="text-base font-medium text-text-primary">
              Clear app logs?
            </DialogTitle>
            <DialogDescription className="text-sm text-text-secondary">
              Clear all app logs? This cannot be undone. The backend log is kept.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="mt-5">
            <Button type="button" variant="outline" onClick={() => setClearRequested(false)}>
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                diagnostics.clear();
                setClearRequested(false);
                setNotice('App logs cleared.');
              }}
            >
              Clear app logs
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
