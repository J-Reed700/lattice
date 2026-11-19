import { useState } from 'react';

import { Trash2 } from 'lucide-react';

import { useDeleteLearningSourceV2 } from '@/features/learning/portability/useLearningPortability';
import type { LearningRemovableSourceDto } from '@/lib/bindings';

function newId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (char) => {
    const random = Math.floor(Math.random() * 16);
    return (char === 'x' ? random : (random & 0x3) | 0x8).toString(16);
  });
}

const RESEARCH_HEADING = 'Added by research for a statement that no longer needs it';

/** The removal's recorded reason: why the gate refused the page, else that research added it for nothing now. */
function reasonFor(item: LearningRemovableSourceDto) {
  return item.reason ? `Not a source: ${item.reason}` : RESEARCH_HEADING;
}

function RemovableGroup({ programId, heading, note, noun, items }: {
  programId: string;
  heading: string;
  note: string;
  noun: string;
  items: LearningRemovableSourceDto[];
}) {
  const remove = useDeleteLearningSourceV2(programId);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const count = `${items.length} ${noun}${items.length === 1 ? '' : 's'}`;

  const removeAll = async () => {
    setBusy(true);
    setError('');
    let removed = 0;
    try {
      for (const item of items) {
        await remove.mutateAsync({ operationId: newId(), programId, sourceId: item.sourceId, expectedRevision: item.revision, reason: reasonFor(item) });
        removed += 1;
      }
      setConfirming(false);
    } catch (cause) {
      setError(`${removed} of ${items.length} removed. ${cause instanceof Error ? cause.message : 'The rest could not be removed.'}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-label={heading} className="rounded-2xl border border-border bg-surface p-4 sm:p-5">
      <h3 className="text-xs font-semibold uppercase tracking-wider text-text-muted">{heading}</h3>
      <p className="mt-1 max-w-3xl text-xs leading-5 text-text-secondary">{note}</p>
      <ul className="mt-3 max-h-64 space-y-1.5 overflow-auto">
        {items.map((item) => (
          <li key={item.sourceId} className="min-w-0 rounded-lg bg-background px-3 py-2">
            <span className="block truncate text-xs font-medium text-text-primary">{item.title}</span>
            {item.url && <span className="block truncate text-[10px] text-text-muted">{item.url}</span>}
            {item.reason && <span className="mt-1 inline-block rounded-full border border-border px-1.5 py-px text-[10px] text-text-secondary">{item.reason}</span>}
          </li>
        ))}
      </ul>
      {error && <p role="alert" className="mt-3 text-xs text-rose-700">{error}</p>}
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {confirming ? (
          <>
            <span className="text-xs text-text-secondary">Remove {count}? Each keeps its saved history and can be brought back later.</span>
            <button type="button" disabled={busy} onClick={() => void removeAll()} className="rounded-full bg-rose-700 px-3.5 py-2 text-[10px] font-semibold text-white disabled:opacity-50">{busy ? 'Removing…' : 'Confirm removal'}</button>
            <button type="button" disabled={busy} onClick={() => setConfirming(false)} className="rounded-full border border-border px-3.5 py-2 text-[10px] text-text-secondary disabled:opacity-50">Keep them</button>
          </>
        ) : (
          <button type="button" onClick={() => setConfirming(true)} className="inline-flex items-center gap-1.5 rounded-full border border-rose-500/30 px-3.5 py-2 text-[10px] font-semibold text-rose-700"><Trash2 size={12} />Remove {count}</button>
        )}
      </div>
    </section>
  );
}

/** What the library offers to remove: the pages the admission check refused and the research no lesson cites now (claim classes design 3.6). Nothing is removed until the user confirms. */
export function SourceCleanup({ programId, notSources, unusedResearch }: {
  programId: string;
  notSources: LearningRemovableSourceDto[];
  unusedResearch: LearningRemovableSourceDto[];
}) {
  if (notSources.length === 0 && unusedResearch.length === 0) return null;
  return (
    <div className="mb-4 space-y-3" data-testid="source-cleanup">
      {notSources.length > 0 && (
        <RemovableGroup
          programId={programId}
          heading="Pages that are not sources"
          note="The course saved these pages on its own, and the admission check found nothing in them a statement could rest on. Lessons no longer compare statements with them. They stay here until you remove them."
          noun="page"
          items={notSources}
        />
      )}
      {unusedResearch.length > 0 && (
        <RemovableGroup
          programId={programId}
          heading={RESEARCH_HEADING}
          note="Research saved these for statements that have since been rewritten or removed, and no lesson cites them now. They stay here until you remove them."
          noun="source"
          items={unusedResearch}
        />
      )}
    </div>
  );
}
