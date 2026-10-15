import type { BackendLogEntry } from '@/lib/bindings';
import { redact, type DiagnosticEntry, type DiagnosticLevel } from '@/utils/diagnostics';

/** Which side of the app wrote an entry. */
export type LogOrigin = 'app' | 'backend';
/** Backend lines that do not match the log format have no level. */
export type LogLevel = DiagnosticLevel | 'unknown';
/** A diagnostics entry tagged with its origin: what the Logs tab shows and exports. */
export interface LogEntry extends Omit<DiagnosticEntry, 'level'> {
  level: LogLevel;
  origin: LogOrigin;
}

/** Backend lines as entries, oldest first, every text field redacted. Trace folds into debug. */
export function fromBackend(lines: readonly BackendLogEntry[]): LogEntry[] {
  return lines.map((line, index) => {
    const timestamp = line.timestamp ?? '';
    return {
      id: `backend-${index}-${timestamp}`,
      timestamp,
      lastSeen: timestamp,
      count: 1,
      level: line.level === 'trace' ? 'debug' : line.level,
      source: redact(line.target ?? 'Backend'),
      message: redact(line.message).slice(0, 2000),
      details: '',
      origin: 'backend',
    };
  });
}

/**
 * Both origins in one list, newest first. A backend line without a timestamp
 * (a continuation) sorts with the timestamped line before it in the file.
 */
export function mergeNewestFirst(app: readonly DiagnosticEntry[], backend: readonly LogEntry[]): LogEntry[] {
  const times = new Map<LogEntry, number>();
  const appEntries = app.map((entry): LogEntry => ({ ...entry, origin: 'app' }));
  for (const entry of appEntries) times.set(entry, Date.parse(entry.lastSeen) || 0);
  let carried = 0;
  for (const entry of backend) {
    carried = Date.parse(entry.timestamp) || carried;
    times.set(entry, carried);
  }
  return [...appEntries, ...[...backend].reverse()]
    .sort((a, b) => (times.get(b) ?? 0) - (times.get(a) ?? 0));
}

/** The diagnostics export shape, with entries from both origins. */
export function exportLogEntries(entries: readonly LogEntry[]): string {
  return JSON.stringify({ version: 1, exportedAt: new Date().toISOString(), entries }, null, 2);
}
