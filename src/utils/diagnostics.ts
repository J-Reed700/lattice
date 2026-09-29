/** Local, bounded diagnostics. Never let reporting break the operation being reported. */
export type DiagnosticLevel = 'debug' | 'info' | 'warn' | 'error';
export interface DiagnosticEntry {
  id: string;
  timestamp: string;
  lastSeen: string;
  count: number;
  level: DiagnosticLevel;
  source: string;
  message: string;
  details: string;
}
const KEY = 'lattice-diagnostics-v1';
const LIMIT = 300;
const listeners = new Set<() => void>();

export function redact(text: string): string {
  return text
    .replace(/("(?:api[_-]?key|token|password|secret|authorization|content|prompt|query|body)"\s*:\s*)"(?:\\.|[^"\\])*"/gi, '$1"[REDACTED]"')
    .replace(/\b(Bearer\s+)[^\s"']+/gi, '$1[REDACTED]')
    .replace(/((?:api[_-]?key|token|password|secret|authorization)\s*[=:]\s*)[^\s,;"']+/gi, '$1[REDACTED]')
    .replace(/([?&](?:token|key|api_key|secret|password)=)[^&#\s]+/gi, '$1[REDACTED]')
    .replace(/(?:\/Users\/|\/home\/)[^/\s]+/g, '/[USER]')
    .replace(/[A-Z]:\\Users\\[^\\\s]+/gi, '[USER]');
}

function serialize(value: unknown): string {
  const seen = new WeakSet<object>();
  try {
    return redact(JSON.stringify(value, (key, item: unknown) => {
      if (/password|secret|token|authorization|api.?key|content|prompt|query|body/i.test(key)) return '[REDACTED]';
      if (typeof item === 'bigint') return String(item);
      if (item && typeof item === 'object') {
        if (seen.has(item)) return '[Circular]';
        seen.add(item);
        if (item instanceof Error) return { ...item, name: item.name, message: item.message, stack: item.stack, cause: 'cause' in item ? item.cause : undefined };
      }
      return item;
    }, 2) ?? '').slice(0, 8000);
  } catch { return '[Unserializable details]'; }
}
function read(): DiagnosticEntry[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((entry): entry is DiagnosticEntry => !!entry &&
      typeof entry.id === 'string' && typeof entry.message === 'string' &&
      typeof entry.details === 'string' && typeof entry.source === 'string' &&
      typeof entry.timestamp === 'string' && typeof entry.lastSeen === 'string' &&
      Number.isFinite(entry.count) && ['debug', 'info', 'warn', 'error'].includes(entry.level)
    ).slice(0, LIMIT).map(entry => ({ ...entry, message: redact(entry.message).slice(0, 2000), details: redact(entry.details).slice(0, 8000) }));
  } catch { return []; }
}
let entries = read();
let persistTimer: ReturnType<typeof setTimeout> | undefined;
function persist() {
  try { localStorage.setItem(KEY, JSON.stringify(entries)); } catch { /* Memory log remains available when storage is full or disabled. */ }
}
function changed() {
  for (const listener of listeners) { try { listener(); } catch { /* Reporting must not throw. */ } }
  if (persistTimer === undefined) {
    persistTimer = setTimeout(() => { persistTimer = undefined; persist(); }, 250);
  }
}
export const diagnostics = {
  subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
  getSnapshot: () => entries,
  record(level: DiagnosticLevel, message: string, source = 'Application', details?: unknown) {
    try {
      const safeMessage = redact(message).slice(0, 2000);
      const safeDetails = serialize(details);
      const now = new Date().toISOString();
      const previous = entries[0];
      if (previous?.level === level && previous.source === source && previous.message === safeMessage && previous.details === safeDetails && Date.now() - Date.parse(previous.lastSeen) < 5000) {
        entries = [{ ...previous, count: previous.count + 1, lastSeen: now }, ...entries.slice(1)];
      } else {
        entries = [{ id: crypto.randomUUID(), timestamp: now, lastSeen: now, count: 1, level, source: redact(source), message: safeMessage, details: safeDetails }, ...entries].slice(0, LIMIT);
      }
      changed();
    } catch { /* Even exotic thrown values must not crash the logger. */ }
  },
  capture(error: unknown, source: string, context?: unknown) {
    let message = 'Unknown error';
    try {
      message = typeof error === 'string' ? error : error && typeof error === 'object' && 'message' in error ? String(error.message) : String(error);
    } catch { /* Use fallback for hostile objects. */ }
    this.record('error', message, source, { error, context });
  },
  clear() { entries = []; changed(); persist(); },
  flush: persist,
  export: () => JSON.stringify({ version: 1, exportedAt: new Date().toISOString(), entries }, null, 2),
};
