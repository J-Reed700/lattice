import { useState, useEffect} from 'react';

import { invoke } from '@tauri-apps/api/core';

interface LogEntry {
  timestamp: string;
  level: string;
  logger: string;
  message: string;
  context: Record<string, unknown>;
}

export function LogViewer() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [filter, setFilter] = useState('');
  const [level, setLevel] = useState('all');
  const [loading, setLoading] = useState(false);

  const loadLogs = async () => {
    setLoading(true);
    try {
      const entries = await invoke<LogEntry[]>('get_logs', { level });
      setLogs(entries);
    } catch (error) {
      console.error('Failed to load logs:', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadLogs();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [level]);

  const filteredLogs = logs.filter(log => {
    const matchesFilter =
      log.message.toLowerCase().includes(filter.toLowerCase()) ||
      JSON.stringify(log.context).toLowerCase().includes(filter.toLowerCase());
    return matchesFilter;
  });

  const getLevelColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'error':
        return 'bg-[hsl(var(--danger-muted))] border-[hsl(var(--danger-muted))]';
      case 'warning':
      case 'warn':
        return 'bg-[hsl(var(--warning-muted))] border-[hsl(var(--warning-muted))]';
      case 'info':
        return 'bg-[hsl(var(--accent-muted))] border-[hsl(var(--accent-muted))]';
      default:
        return 'bg-[hsl(var(--surface-raised))] border-[hsl(var(--border-subtle))]';
    }
  };

  const getLevelBadgeColor = (level: string) => {
    switch (level.toLowerCase()) {
      case 'error':
        return 'bg-[hsl(var(--danger-fg))] text-[hsl(var(--accent-fg))]';
      case 'warning':
      case 'warn':
        return 'bg-[hsl(var(--warning-fg))] text-[hsl(var(--accent-fg))]';
      case 'info':
        return 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))]';
      default:
        return 'bg-[hsl(var(--text-secondary))] text-[hsl(var(--accent-fg))]';
    }
  };

  return (
    <div className="log-viewer p-6 max-w-7xl mx-auto">
      <div className="mb-6">
        <h2 className="text-2xl font-bold mb-4 text-[hsl(var(--text-primary))]">
          System Logs
        </h2>

        <div className="flex gap-4">
          <input
            type="text"
            placeholder="Filter logs..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="flex-1 p-3 border rounded-lg bg-[hsl(var(--surface-raised))]
                     border-[hsl(var(--border-subtle))]
                     text-[hsl(var(--text-primary))]
                     placeholder-[hsl(var(--text-tertiary))]
                     focus:ring-2 ring-[hsl(var(--accent))] focus:border-transparent"
          />

          <select
            value={level}
            onChange={(e) => setLevel(e.target.value)}
            className="p-3 border rounded-lg bg-[hsl(var(--surface-raised))]
                     border-[hsl(var(--border-subtle))]
                     text-[hsl(var(--text-primary))]
                     focus:ring-2 ring-[hsl(var(--accent))] focus:border-transparent"
          >
            <option value="all">All Levels</option>
            <option value="error">Errors</option>
            <option value="warning">Warnings</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>

          <button
            onClick={loadLogs}
            disabled={loading}
            className="px-6 py-3 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-lg
                     hover:bg-[hsl(var(--accent-hover))] disabled:bg-[hsl(var(--surface-raised))]
                     transition-colors font-medium"
          >
            {loading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      <div className="space-y-3 max-h-[600px] overflow-y-auto pr-2">
        {filteredLogs.length === 0 ? (
          <div className="text-center py-12 text-[hsl(var(--text-secondary))]">
            {loading ? 'Loading logs...' : 'No logs found'}
          </div>
        ) : (
          filteredLogs.map((log, i) => (
            <div
              key={i}
              className={`log-entry p-4 rounded-lg border-l-4
                        ${getLevelColor(log.level)}
                        transition-all hover:shadow-md`}
            >
              <div className="flex justify-between items-start mb-2">
                <div className="flex items-center gap-3">
                  <span className={`px-3 py-1 rounded-full text-xs font-bold uppercase
                                  ${getLevelBadgeColor(log.level)}`}>
                    {log.level}
                  </span>
                  <span className="font-mono text-sm text-[hsl(var(--text-secondary))]">
                    {new Date(log.timestamp).toLocaleString()}
                  </span>
                </div>
                <span className="text-xs font-semibold text-[hsl(var(--text-secondary))]
                               bg-[hsl(var(--surface-raised))] px-2 py-1 rounded">
                  {log.logger}
                </span>
              </div>

              <div className="font-medium text-[hsl(var(--text-primary))] mb-2">
                {log.message}
              </div>

              {log.context && Object.keys(log.context).length > 0 && (
                <details className="mt-3">
                  <summary className="cursor-pointer text-sm font-medium
                                    text-[hsl(var(--text-secondary))]
                                    hover:text-[hsl(var(--accent))]
                                    transition-colors">
                    Context ({Object.keys(log.context).length} fields)
                  </summary>
                  <pre className="text-xs bg-[hsl(var(--surface-raised))]
                                p-3 rounded-lg mt-2 overflow-x-auto
                                border border-[hsl(var(--border-subtle))]
                                text-[hsl(var(--text-primary))]">
                    {JSON.stringify(log.context, null, 2)}
                  </pre>
                </details>
              )}
            </div>
          ))
        )}
      </div>

      <div className="mt-4 text-sm text-[hsl(var(--text-secondary))] text-center">
        Showing {filteredLogs.length} of {logs.length} logs
      </div>
    </div>
  );
}
