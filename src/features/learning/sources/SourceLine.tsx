import { ExternalLink, FileText } from 'lucide-react';

import type { LearningSourceDto } from '@/lib/bindings';

function safeHref(value?: string | null) {
  if (!value) return undefined;
  try { const url = new URL(value); return ['https:', 'http:'].includes(url.protocol) ? url.href : undefined; }
  catch { return undefined; }
}

function formatDateTime(value: number) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 'date unavailable' : date.toLocaleString();
}

export function SourceLine({ ids, sources }: { ids: string[]; sources: LearningSourceDto[] }) {
  const entries = ids.map((id) => sources.find((source) => source.id === id)).filter((item): item is LearningSourceDto => Boolean(item));
  if (!entries.length) return null;
  return <div className="mt-4 space-y-2">{entries.map((source) => { const href = safeHref(source.url); return <details key={source.id} className="group rounded-xl border border-border bg-background"><summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2.5 text-xs text-text-secondary"><FileText size={14} className="shrink-0 text-accent" /><span className="flex-1 truncate">{source.title}</span>{href && <a href={href} target="_blank" rel="noreferrer" aria-label={`Open ${source.title}`} onClick={(e) => e.stopPropagation()} className="text-accent hover:underline"><ExternalLink size={13} /></a>}<span className="text-text-muted">Source</span></summary><div className="border-t border-border px-4 py-3"><p className="whitespace-pre-wrap text-xs leading-5 text-text-secondary">{source.excerpt}</p><p className="mt-2 text-xs text-text-muted">Acquired {formatDateTime(source.acquiredAt)}{href && <> · <a href={href} target="_blank" rel="noreferrer" className="text-accent hover:underline">Open reference</a></>}</p></div></details>; })}</div>;
}
