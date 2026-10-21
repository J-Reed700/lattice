import { useId, useMemo, useState } from 'react';

import { ExternalLink } from 'lucide-react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { countsAs, sourceLine } from '@/features/learning/lessons/review/claimReviewCopy';
import { excerptAround } from '@/features/learning/lessons/review/claimReviewPresentation';
import { formatSourceForReader } from '@/features/learning/sources/sourceReaderFormat';
import { useLearningSourceVersion } from '@/features/learning/sources/useLearningSources';
import type { ClaimSupportDto, LearningSufficiencyDto } from '@/lib/bindings';

export type EvidenceHeading = 'BEST EVIDENCE' | 'SAYS OTHERWISE' | 'CLOSEST PASSAGE IN WHAT YOU ADDED';

/** The quoted sentence, marked, in up to a sentence of its context. */
function Excerpt({ support }: { support: ClaimSupportDto }) {
  const excerpt = excerptAround(support.passage.text, support.quoteSpan, support.quote);
  return (
    <blockquote className="mt-2 border-l-2 border-accent/40 pl-3 font-serif text-[15px] leading-7 text-text-secondary">
      {excerpt.clippedStart && '…'}{excerpt.before}
      <mark className="rounded-sm bg-accent/25 px-0.5 text-text-primary">{excerpt.quote}</mark>
      {excerpt.after}{excerpt.clippedEnd && '…'}
    </blockquote>
  );
}

/**
 * The passage behind the excerpt, rendered as the Sources tab's reader
 * renders a source, with the quote marked. Loads the version's extraction
 * only when opened.
 */
function FullPassage({ programId, support }: { programId: string; support: ClaimSupportDto }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const detail = useLearningSourceVersion(open ? { programId, sourceId: support.source.sourceId, versionId: support.source.sourceVersionId } : null);
  const extraction = detail.data?.version.extractionVersion ?? '';
  const content = useMemo(() => formatSourceForReader(support.passage.text, extraction), [extraction, support.passage.text]);
  const marks = useMemo(() => [{ sentence: support.quote, verdict: support.role === 'contradicts' ? 'contradicted' as const : 'supported' as const }], [support.quote, support.role]);
  return (
    <div className="w-full sm:w-auto">
      <button type="button" aria-expanded={open} aria-controls={id} onClick={() => setOpen((value) => !value)}
        className="inline-flex min-h-9 items-center gap-1 text-xs font-medium text-text-secondary hover:text-text-primary">
        Full passage <span aria-hidden="true" className={`transition-transform ${open ? 'rotate-90' : ''}`}>▸</span>
      </button>
      {open && <div id={id} className="mt-2 max-h-[28rem] overflow-y-auto rounded-lg border border-border bg-background px-4 py-3 text-sm">
        <TiptapViewer content={content} claims={marks} showEvidence />
      </div>}
    </div>
  );
}

/**
 * The evidence for the claim in focus (§3.4): where it is from in words, the
 * sentence that states (or contradicts) the claim in a little context, what
 * the source counts as, the full passage behind a disclosure, and the reader.
 */
export function ClaimEvidence({ programId, heading, support, also = [], sufficiency, onOpenSource }: {
  programId: string;
  heading: EvidenceHeading;
  support: ClaimSupportDto;
  /** Other sources that state the claim; choosing one shows its excerpt. */
  also?: ClaimSupportDto[];
  sufficiency: LearningSufficiencyDto | null;
  onOpenSource: (support: ClaimSupportDto) => void;
}) {
  const [chosen, setChosen] = useState<string | null>(null);
  const all = [support, ...also];
  const shown = all.find((entry) => entry.source.sourceVersionId === chosen) ?? support;
  const others = all.filter((entry) => entry !== shown);
  return (
    <section aria-label={heading.toLowerCase()} className="space-y-2">
      <h3 className="text-[11px] font-semibold uppercase tracking-[.12em] text-text-muted">{heading}</h3>
      <p className="text-sm font-medium text-text-primary">{sourceLine(shown)}</p>
      <Excerpt support={shown} />
      {heading !== 'CLOSEST PASSAGE IN WHAT YOU ADDED' && <p className="text-xs leading-5 text-text-secondary"><span className="font-medium text-text-primary">Counts as: </span>{countsAs(shown, sufficiency)}</p>}
      {others.length > 0 && shown.role === 'supports' && <p className="text-xs leading-5 text-text-secondary">
        {shown === support ? 'Also stated by: ' : 'Stated by: '}
        {others.map((entry, index) => <span key={entry.source.sourceVersionId}>
          {index > 0 && ', '}
          <button type="button" onClick={() => setChosen(entry.source.sourceVersionId)} className="text-accent underline underline-offset-2">{entry.source.title}</button>
        </span>)}
      </p>}
      <div className="flex flex-wrap items-start gap-x-4">
        <FullPassage key={shown.source.sourceVersionId} programId={programId} support={shown} />
        <button type="button" onClick={() => onOpenSource(shown)} className="inline-flex min-h-9 items-center gap-1 text-xs font-medium text-accent hover:underline">
          Open the source <ExternalLink size={12} aria-hidden="true" />
        </button>
      </div>
    </section>
  );
}
