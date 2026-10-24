import {
  badgeText,
  LEVEL_MEANING,
  overrideLines,
  relationLine,
  STANDING_WORD,
  trackRecordLine,
} from '@/features/learning/sources/trust/trustPresentation';
import type { LearningSourceTrustDto, LearningSourceTrustEntryDto } from '@/lib/bindings';

function day(seconds: number | null) {
  if (seconds == null) return null;
  const date = new Date(seconds * 1000);
  return Number.isNaN(date.getTime()) ? null : date.toISOString().slice(0, 10);
}

/**
 * Why a source has its level: the rules that fired, verbatim and in order,
 * then what capture recorded, its relations, the user's verdicts on its
 * origin and the overrides that touch it .
 */
export function TrustReasons({
  trust,
  entry,
  titleOf = () => null,
}: {
  trust: LearningSourceTrustDto;
  entry: LearningSourceTrustEntryDto | null;
  titleOf?: (versionId: string) => string | null;
}) {
  const provenance = entry?.provenance ?? null;
  const facts = provenance ? [
    provenance.publisher ? ['Publisher', provenance.publisher] : null,
    provenance.author ? ['Author', provenance.author] : null,
    provenance.siteName ? ['Site', provenance.siteName] : null,
    provenance.site ? ['Domain', provenance.site] : null,
    day(provenance.publishedAt) ? ['Published', day(provenance.publishedAt)!] : null,
    day(provenance.modifiedAt) ? ['Modified', day(provenance.modifiedAt)!] : null,
    provenance.archivedFrom ? ['Archived from', provenance.archivedFrom] : null,
    provenance.notExtracted ? ['Not recorded', provenance.notExtracted] : null,
  ].filter((fact): fact is [string, string] => fact !== null) : [];
  const relations = provenance?.relations.map((relation) => relationLine(relation, provenance.sourceVersionId, titleOf)) ?? [];
  const record = entry ? trackRecordLine(entry.trackRecord) : null;
  const overrides = entry ? overrideLines(entry.overrides) : [];
  return (
    <div className="space-y-3 text-xs leading-5 text-text-secondary">
      <div>
        <p className="font-semibold text-text-primary">{badgeText(trust)}</p>
        <p className="text-text-muted">{LEVEL_MEANING[trust.level]}</p>
      </div>
      <section aria-label="Why this level">
        <h4 className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Why</h4>
        <ol className="mt-1 list-decimal space-y-1 pl-4">
          {trust.reasons.map((reason, index) => <li key={`${reason.code}-${index}`}>{reason.text}</li>)}
        </ol>
      </section>
      {entry?.standing && (
        <section aria-label="Publisher standing">
          <h4 className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Publisher standing: {STANDING_WORD[entry.standing]}</h4>
          {entry.standingReasons.length > 0 && <ul className="mt-1 list-disc space-y-1 pl-4">
            {entry.standingReasons.map((reason, index) => <li key={`${reason.code}-${index}`}>{reason.text}</li>)}
          </ul>}
        </section>
      )}
      {facts.length > 0 && (
        <dl className="space-y-0.5 break-words">
          {facts.map(([label, value]) => <div key={label}><dt className="inline text-text-muted">{label}: </dt><dd className="inline">{value}</dd></div>)}
        </dl>
      )}
      {relations.length > 0 && <ul aria-label="Relations" className="list-disc space-y-1 pl-4">{relations.map((line, index) => <li key={`${line}-${index}`}>{line}</li>)}</ul>}
      {record && <p>{record}</p>}
      {overrides.length > 0 && <ul aria-label="Your overrides" className="space-y-1">{overrides.map((line) => <li key={line} className="font-medium text-text-primary">{line}</li>)}</ul>}
    </div>
  );
}
