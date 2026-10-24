import { useState } from 'react';

import { useMutation, useQuery } from '@tanstack/react-query';

import { LEARNING_CHECKER_ACCURACY_KEY } from '@/features/learning/api/learningQueryKeys';
import VaultAPI from '@/lib/api';
import type { CheckerAccuracyDto, ExportedLearningCheckerExamplesDto } from '@/lib/bindings';
import type { ApiResult } from '@/types';

function unwrap<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

const percent = (rate: number) => `${(rate * 100).toFixed(1)}%`;

/** One checker model's record on the user's own labelled claims (§15); data, no verdict. */
export function accuracyLine(row: CheckerAccuracyDto): string {
  return [
    `${row.labelled} labelled ${row.labelled === 1 ? 'claim' : 'claims'}`,
    row.falseFlagRate != null ? `false flags ${percent(row.falseFlagRate)}` : null,
    row.falsePassRate != null ? `false passes ${percent(row.falsePassRate)} (from ${row.verifierLabelled} Verifier ${row.verifierLabelled === 1 ? 'audit' : 'audits'})` : null,
    `retrieval misses ${row.retrievalMisses}`,
  ].filter(Boolean).join(' · ');
}

/**
 * How often the checker was wrong on the user's lessons, per checker model,
 * and the export that lets the claim-pairs eval score another checker on them.
 */
export function CheckerAccuracy({ programId }: { programId: string }) {
  const [open, setOpen] = useState(false);
  const accuracy = useQuery({
    queryKey: LEARNING_CHECKER_ACCURACY_KEY,
    queryFn: async () => unwrap(await VaultAPI.getLearningCheckerAccuracy()),
    enabled: open,
    retry: false,
  });
  const exported = useMutation<ExportedLearningCheckerExamplesDto | null, Error, void>({
    retry: false,
    mutationFn: async () => unwrap(await VaultAPI.exportLearningCheckerExamples({ programId })),
  });
  return (
    <details className="mt-4 text-xs" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary className="cursor-pointer font-medium text-text-secondary">Checker accuracy on your lessons</summary>
      <div className="mt-2 space-y-1.5 leading-5 text-text-secondary">
        {accuracy.isFetching && <p role="status" className="text-text-muted">Loading…</p>}
        {accuracy.error && <p role="alert" className="text-rose-700">{accuracy.error.message}</p>}
        {accuracy.data?.length === 0 && <p>No labelled claims yet. Each confirmation, removal, edit or added source labels one.</p>}
        {accuracy.data?.map((row) => <p key={row.checkerModel}><span className="font-medium text-text-primary">{row.checkerModel}: </span>{accuracyLine(row)}</p>)}
        <button type="button" disabled={exported.isPending} onClick={() => exported.mutate()} className="min-h-8 rounded-full border border-border px-3 font-medium text-text-primary disabled:opacity-50">
          {exported.isPending ? 'Exporting…' : 'Export this course’s examples for evaluation'}
        </button>
        {exported.data && <p role="status">Saved {exported.data.pairs} claim {exported.data.pairs === 1 ? 'pair' : 'pairs'} to {exported.data.path}.</p>}
        {exported.isSuccess && exported.data === null && <p role="status">Export cancelled.</p>}
        {exported.error && <p role="alert" className="text-rose-700">{exported.error.message}</p>}
      </div>
    </details>
  );
}
