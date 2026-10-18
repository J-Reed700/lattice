import { useMemo, useState } from 'react';

import { ExternalLink, FileSearch, ShieldCheck } from 'lucide-react';

import { useLearningLessonEvidence, type LessonEvidenceQuery } from '@/features/learning/api/evidenceQueries';
import { passageKey, passageNumbersFor } from '@/features/learning/model/evidencePresentation';
import { TrustLevelChip } from '@/features/learning/sources/trust/TrustBadge';
import { sufficiencyNote, verifiedByLevel } from '@/features/learning/sources/trust/trustPresentation';
import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import {
  fromLearningEvidencePassage,
  type ResolvedReaderTarget,
} from '@/features/reading/model/sourceAdapter';
import type {
  LearningClaimEvidenceDto,
  LearningEvidencePassageDto,
  LearningLessonEvidenceDto,
} from '@/lib/bindings';

function safeUrl(value: string | null) {
  try {
    const url = new URL(value ?? '');
    return ['https:', 'http:'].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined;
  }
}

/** One compared passage; opens in the source reader. */
export function Passage({
  passage,
  number,
  onOpenPassage,
  provenance,
  defaultOpen = false,
  readerAlways = false,
}: {
  passage: LearningEvidencePassageDto;
  number: number;
  onOpenPassage?: (passage: LearningEvidencePassageDto, number: number) => void;
  /** "Added by you", "Found by research"…; who brought the source in. */
  provenance?: string | null;
  defaultOpen?: boolean;
  /** Offer the reader for passages with no web page too (pasted text, documents). */
  readerAlways?: boolean;
}) {
  const url = safeUrl(passage.url);
  return (
    <details className="rounded-lg bg-background p-3" open={defaultOpen}>
      <summary className="cursor-pointer text-xs font-medium text-text-secondary">
        <span className="mr-2 inline-grid h-5 min-w-5 place-items-center rounded bg-accent/10 px-1 text-[10px] font-semibold text-accent">
          {number}
        </span>
        {passage.title}
        {provenance && <span className="ml-2 rounded-full border border-border px-2 py-0.5 text-[10px] font-medium text-text-secondary">{provenance}</span>}
        {passage.trust && <span className="ml-2"><TrustLevelChip trust={passage.trust} /></span>}
      </summary>
      {passage.trust && passage.trust.reasons.length > 0 && (
        <ul aria-label="Why this level" className="mt-2 list-disc space-y-0.5 pl-4 text-[10px] leading-4 text-text-muted">
          {passage.trust.reasons.map((reason, index) => <li key={`${reason.code}-${index}`}>{reason.text}</li>)}
        </ul>
      )}
      <p className="mt-2 break-all text-[10px] text-text-muted">
        Saved version {passage.sourceVersionId} · bytes {passage.startByte}–{passage.endByte} · {passage.retrievalKind.replace(/_/g, ' ')}
      </p>
      <blockquote className="mt-2 whitespace-pre-wrap wrap-anywhere border-l-2 border-accent/40 pl-3 font-serif text-sm leading-6 text-text-primary">
        {passage.text}
      </blockquote>
      {(url || readerAlways) && onOpenPassage && (
        <button
          type="button"
          onClick={() => onOpenPassage(passage, number)}
          className="mt-2 inline-flex items-center gap-1 text-xs text-accent underline"
        >
          {url ? 'Open original page' : 'Open in reader'} <ExternalLink size={12} />
        </button>
      )}
    </details>
  );
}

/** No saved reference supports or contradicts the claim; it was kept unchecked. */
const GENERAL_KNOWLEDGE = 'general_knowledge';

function isGeneralKnowledge(claim: LearningClaimEvidenceDto) {
  return claim.resolution === 'general_knowledge' || claim.verdict === GENERAL_KNOWLEDGE;
}

/** How the claim was settled, in words (§5.6): confirmed never reads as verified. */
function ResolutionChip({ claim }: { claim: LearningClaimEvidenceDto }) {
  if (isGeneralKnowledge(claim)) {
    return <span className="rounded-full border border-border px-2 py-1 text-[10px] font-medium text-text-secondary">General knowledge · not checked against a source</span>;
  }
  if (claim.resolution === 'confirmed_by_you') {
    return <span className="rounded-full border border-border bg-background px-2 py-1 text-[10px] font-medium text-text-secondary">Confirmed by you</span>;
  }
  if (claim.resolution === 'source_removed') {
    return <span className="rounded-full border border-amber-600/40 px-2 py-1 text-[10px] font-medium text-text-primary">Source removed · review</span>;
  }
  return (
    <span className="rounded-full bg-emerald-700/10 px-2 py-1 text-[9px] font-semibold uppercase tracking-wider text-emerald-800 dark:text-emerald-300">
      {claim.verdict || 'checked'}
    </span>
  );
}

/** "12 claims verified against sources · 3 confirmed by you · 2 general knowledge" (§5.6). */
export function evidenceSummary(report: LearningLessonEvidenceDto): string {
  const verified = Math.max(0, report.claimCount - report.generalKnowledgeCount - report.confirmedCount);
  const byLevel = verifiedByLevel(report.teachingClaims);
  return [
    ...(byLevel.length ? byLevel : [`${verified} ${verified === 1 ? 'claim' : 'claims'} verified against sources`]),
    report.confirmedCount > 0 ? `${report.confirmedCount} confirmed by you` : null,
    report.generalKnowledgeCount > 0 ? `${report.generalKnowledgeCount} general knowledge, not checked against a source` : null,
  ].filter(Boolean).join(' · ');
}

export function sourceRemovedCount(report: LearningLessonEvidenceDto | null | undefined): number {
  return report?.teachingClaims.filter((claim) => claim.resolution === 'source_removed').length ?? 0;
}

function ClaimEvidence({
  claim,
  passageNumbers,
  onOpenPassage,
}: {
  claim: LearningClaimEvidenceDto;
  passageNumbers: Map<string, number>;
  onOpenPassage?: (passage: LearningEvidencePassageDto, number: number) => void;
}) {
  return (
    <article className="wrap-anywhere rounded-lg border border-border p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <p className="text-sm font-medium leading-6 text-text-primary">{claim.claim}</p>
        <ResolutionChip claim={claim} />
      </div>
      {claim.resolution === 'confirmed_by_you' && claim.note && (
        <p className="mt-2 text-xs leading-5 text-text-secondary"><span className="font-medium text-text-primary">Your note: </span>{claim.note}</p>
      )}
      {claim.resolution === 'source_removed' && (
        <p className="mt-2 text-xs leading-5 text-text-secondary">The source that supported this claim was deleted. The lesson stays readable; review the claim to attach another source or confirm it.</p>
      )}
      {claim.contentQuote && (
        <blockquote className="mt-2 border-l-2 border-border pl-3 text-xs leading-5 text-text-muted">
          <span className="font-medium text-text-secondary">In the lesson: </span>
          {claim.contentQuote}
        </blockquote>
      )}
      {claim.sufficiency && <p className="mt-2 text-xs leading-5 text-text-primary">{sufficiencyNote(claim.sufficiency)}</p>}
      <p className="mt-2 text-xs leading-5 text-text-secondary">{claim.reason}</p>
      {claim.supportingQuote && (
        <blockquote className="mt-2 border-l-2 border-accent pl-3 text-xs leading-5 text-text-primary">
          <span className="font-medium">Supporting passage: </span>
          {claim.supportingQuote}
        </blockquote>
      )}
      <div className="mt-3 space-y-2">
        {claim.passages.map((passage) => (
          <Passage
            key={passageKey(passage)}
            passage={passage}
            number={passageNumbers.get(passageKey(passage)) ?? 0}
            onOpenPassage={onOpenPassage}
          />
        ))}
      </div>
    </article>
  );
}

/** Audit-level metadata for the whole prepared lesson. Collapsed by default. */
export function LessonEvidencePanel({
  programId,
  lessonId,
  programRevision = 0,
  report: suppliedReport,
}: {
  programId: string;
  lessonId: string;
  programRevision?: number;
  report?: LessonEvidenceQuery;
}) {
  const [open, setOpen] = useState(false);
  const [previewTarget, setPreviewTarget] = useState<ResolvedReaderTarget | null>(null);
  const ownedReport = useLearningLessonEvidence(programId, lessonId, programRevision, !suppliedReport && open);
  const report = suppliedReport ?? ownedReport;
  const data = report.data;
  const passageNumbers = useMemo(() => passageNumbersFor(data), [data]);

  const handleOpenPassage = (passage: LearningEvidencePassageDto, number: number) => {
    setPreviewTarget(fromLearningEvidencePassage(passage, number - 1));
  };

  return (
    <>
      <section className="mb-5 rounded-xl border border-border bg-surface p-4" aria-label="Lesson evidence">
        <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="flex min-h-10 w-full items-center gap-2 text-left text-sm font-semibold text-accent">
          <FileSearch size={17} />
          {open ? 'Hide lesson evidence' : 'View lesson evidence'}
          {data && <span className="ml-auto text-xs font-normal text-text-muted">{data.teachingClaims.length} teaching {data.teachingClaims.length === 1 ? 'claim' : 'claims'}</span>}
        </button>
        {open && (
          <div className="mt-3 space-y-4">
            {report.isFetching && <p role="status" className="text-sm text-text-muted">Loading the saved verification report…</p>}
            {report.error && (
              <div role="alert" className="text-sm text-rose-700">
                <p>{report.error.message}</p>
                <button type="button" onClick={() => void report.refetch()} className="mt-2 underline">Retry evidence</button>
              </div>
            )}
            {report.isSuccess && !data && <p className="text-sm text-text-secondary">This lesson has no saved verification report. New lessons prepared with references record their evidence here.</p>}
            {data && (
              <>
                <p className="text-sm leading-6 text-text-secondary">
                  {evidenceSummary(data)} · {data.executedExamples} examples executed. These checks reduce errors; they do not guarantee that a lesson is correct or complete.
                </p>
                {sourceRemovedCount(data) > 0 && <p role="status" className="rounded-lg border border-amber-600/30 p-3 text-sm text-text-primary">{sourceRemovedCount(data)} {sourceRemovedCount(data) === 1 ? 'claim lost its source' : 'claims lost their source'} — review {sourceRemovedCount(data) === 1 ? 'it' : 'them'} in the claim review.</p>}
                <p className="text-xs leading-5 text-text-muted">
                  {data.retrievalMode === 'hybrid' ? 'Semantic and keyword retrieval' : data.retrievalMode === 'lexical_fallback' ? 'Keyword retrieval — embeddings were unavailable' : 'Earlier verification method'} · Checked {new Date(data.checkedAt).toLocaleString()} · {data.checkerModel}
                </p>
                {data.unexecutedLanguages.length > 0 && <p className="rounded-lg bg-amber-500/10 p-3 text-sm text-text-secondary">Examples in {data.unexecutedLanguages.join(', ')} were checked against references but were not compiled or executed.</p>}
                {!data.sourcesCurrent && <p role="status" className="rounded-lg bg-amber-500/10 p-3 text-sm text-text-secondary">The reference collection has changed since this check. The citations below still open the exact source versions used for this saved lesson.</p>}
                <details>
                  <summary className="cursor-pointer text-xs text-text-muted">Verification details</summary>
                  <dl className="mt-2 space-y-1 break-all text-xs text-text-secondary">
                    <div><dt className="inline font-medium">Policy: </dt><dd className="inline">{data.policy}</dd></div>
                    <div><dt className="inline font-medium">Lesson fingerprint: </dt><dd className="inline">{data.contentSha256}</dd></div>
                    {data.embeddingModel && <div><dt className="inline font-medium">Embedding model: </dt><dd className="inline">{data.embeddingModel}</dd></div>}
                  </dl>
                </details>
                <div className="space-y-2">
                  {data.teachingClaims.map((claim, index) => (
                    <ClaimEvidence
                      key={`${claim.sectionIndex}:${claim.claim}:${index}`}
                      claim={claim}
                      passageNumbers={passageNumbers}
                      onOpenPassage={handleOpenPassage}
                    />
                  ))}
                </div>
              </>
            )}
          </div>
        )}
      </section>

      <FilePreviewModal
        isOpen={previewTarget !== null}
        onClose={() => setPreviewTarget(null)}
        source={previewTarget?.source ?? null}
        initialLocator={previewTarget?.locator ?? null}
      />
    </>
  );
}

/** Contextual evidence for one generated lesson block. */
export function LessonSectionEvidence({
  report,
  sectionIndex,
}: {
  report: LessonEvidenceQuery;
  sectionIndex: number;
}) {
  const [previewTarget, setPreviewTarget] = useState<ResolvedReaderTarget | null>(null);
  const data = report.data;
  const claims = data?.teachingClaims.filter((claim) => claim.sectionIndex === sectionIndex) ?? [];
  const passageNumbers = useMemo(() => passageNumbersFor(data), [data]);
  const sectionPassages = new Set(claims.flatMap((claim) => claim.passages.map(passageKey)));
  const general = claims.filter(isGeneralKnowledge).length;
  const confirmed = claims.filter((claim) => claim.resolution === 'confirmed_by_you').length;
  const checked = claims.length - general - confirmed;

  const handleOpenPassage = (passage: LearningEvidencePassageDto, number: number) => {
    setPreviewTarget(fromLearningEvidencePassage(passage, number - 1));
  };

  if (report.isFetching && !data) {
    return <p role="status" className="mt-4 text-xs text-text-muted">Loading citations for this section…</p>;
  }
  if (report.isError || !data || claims.length === 0) return null;

  return (
    <>
      <details className="mt-4 border-t border-border pt-3">
        <summary className="flex cursor-pointer list-none flex-wrap items-center gap-2 text-xs font-medium text-accent">
          <ShieldCheck size={14} />
          Evidence · {checked} checked {checked === 1 ? 'claim' : 'claims'}
          {confirmed > 0 && ` · ${confirmed} confirmed by you`}
          {general > 0 && ` · ${general} general knowledge`} · {sectionPassages.size} {sectionPassages.size === 1 ? 'passage' : 'passages'}
          <span className="ml-auto flex gap-1" aria-label="Citations in this section">
            {[...sectionPassages].map((key) => <span key={key} className="inline-grid h-5 min-w-5 place-items-center rounded bg-accent/10 px-1 text-[10px] font-semibold text-accent">{passageNumbers.get(key)}</span>)}
          </span>
        </summary>
        <div className="mt-3 space-y-2">
          {claims.map((claim, index) => (
            <ClaimEvidence
              key={`${claim.claim}:${index}`}
              claim={claim}
              passageNumbers={passageNumbers}
              onOpenPassage={handleOpenPassage}
            />
          ))}
        </div>
      </details>

      <FilePreviewModal
        isOpen={previewTarget !== null}
        onClose={() => setPreviewTarget(null)}
        source={previewTarget?.source ?? null}
        initialLocator={previewTarget?.locator ?? null}
      />
    </>
  );
}
