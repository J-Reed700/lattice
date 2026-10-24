import { useId, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { ACCEPT_LEVELS, claimsCount, kindWithArticle, standingWords } from '@/features/learning/lessons/review/claimReviewCopy';
import { useSetSourceTrustOverride, useSourceTrust } from '@/features/learning/sources/trust/useSourceTrust';
import type { ClaimSupportDto, LearningTrustChangeResultDto, LearningTrustLevel } from '@/lib/bindings';

type Props = {
  programId: string;
  /** The source to trust; null closes the dialog. */
  support: ClaimSupportDto | null;
  /** Waiting claims the queue shows on this source, until the trust read answers. */
  sharing: number;
  onClose: () => void;
  onTrusted: (result: LearningTrustChangeResultDto, support: ClaimSupportDto) => void;
};

function AcceptSourceBody({ programId, support, sharing, onClose, onTrusted }: Props & { support: ClaimSupportDto }) {
  const id = useId();
  const [level, setLevel] = useState<LearningTrustLevel>('established');
  const [note, setNote] = useState('');
  const set = useSetSourceTrustOverride();
  const sourceId = support.source.sourceId;
  const trust = useSourceTrust({ programId, sourceIds: [sourceId], claim: null, includeImpact: true });
  const impact = trust.data?.sources.find((entry) => entry.sourceId === sourceId)?.impact?.source.claims;

  const confirm = async () => {
    try {
      const result = await set.mutateAsync({ scope: 'source', programId, sourceId, level, note: note.trim() || null });
      onTrusted(result, support);
    } catch {
      // The error is shown in the dialog; it stays open.
    }
  };

  const count = impact ?? sharing;
  return (
    <DialogContent className="max-w-lg rounded-2xl border border-border-default bg-surface p-5 shadow-sheet">
      <DialogTitle className="font-serif text-xl text-text-primary">Accept this source for the course</DialogTitle>
      <DialogDescription className="mt-1 text-sm leading-6 text-text-secondary">
        <span className="font-medium text-text-primary">{support.source.title}</span>, {kindWithArticle(support.kind)}{support.site ? ` from ${support.site}` : ''}.
        {' '}{claimsCount(count)} rest on this source in this course. Each is checked again with the source trusted; any it states are verified.
      </DialogDescription>
      <fieldset className="mt-4">
        <legend className="text-xs font-medium text-text-secondary">Treat it as</legend>
        {ACCEPT_LEVELS.map((choice) => (
          <label key={choice} className="mt-2 flex min-h-9 items-center gap-2 text-sm text-text-primary">
            <input type="radio" name={`${id}-level`} value={choice} checked={level === choice} onChange={() => setLevel(choice)} />
            {standingWords(choice)}{choice === 'established' && <span className="text-xs text-text-muted"> (usual)</span>}
          </label>
        ))}
      </fieldset>
      <label htmlFor={`${id}-note`} className="mt-4 block text-xs font-medium text-text-secondary">Note (optional)</label>
      <textarea id={`${id}-note`} value={note} onChange={(event) => setNote(event.target.value)} rows={2} className="mt-1 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus:border-accent focus:outline-hidden" />
      {set.error && <p role="alert" className="mt-2 text-xs text-danger-fg">{set.error.message}</p>}
      <div className="mt-5 flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onClose}>Cancel</Button>
        <Button type="button" disabled={set.isPending} onClick={() => void confirm()}>{set.isPending ? 'Checking the claims…' : `Accept for ${claimsCount(count)}`}</Button>
      </div>
    </DialogContent>
  );
}

/**
 * "Accept this source for all N" : trust the source for this course at
 * a level the user picks, so every claim it states is decided again. The
 * impact count comes from the trust read; one Undo clears the override.
 */
export function AcceptSourceDialog(props: Props) {
  const { support, onClose } = props;
  return (
    <Dialog open={support !== null} onOpenChange={(open) => { if (!open) onClose(); }}>
      {support && <AcceptSourceBody key={support.source.sourceId} {...props} support={support} />}
    </Dialog>
  );
}
